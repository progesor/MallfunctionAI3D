//! Non-destructive automatic UV atlas repair + independent PBR channel rebake.
//! A successful repair is registered as a NEW catalog asset. ComfyUI, original
//! GLB, Blender LODs, existing texture exports and Unity prefabs are immutable.
use crate::blender::find_blender;
use crate::models::Asset;
use crate::storage::{inspect_glb, StudioResult, StudioState};
use crate::texture_export::{owned_asset_dir, verify_sha256};
use crate::unity_export::{existing_texture_dir, geometry_only_glb, parse_glb, png_map, png_bytes, read_json, sum_sha};
use chrono::Utc;
use image::{ImageFormat, Rgba, RgbaImage};
use serde::Serialize;
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}, process::Command};
use tauri::State;
use uuid::Uuid;

const WORKER: &str = include_str!("../../../blender/repair_uv_pbr.py");
const ROLES: [&str;5] = ["baseColor", "normal", "metallic", "roughness", "occlusion"];

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct UvRepairResult {
    pub asset: Asset,
    pub version_dir: String,
    pub report_path: String,
    pub working_blend: String,
    pub uv_layout: String,
    pub strategy: String,
    pub old_islands: u64,
    pub new_islands: u64,
    pub source_untouched: bool,
}

struct PreparedRepair {
    source: Vec<u8>,
    report: Value,
    version: PathBuf,
}

fn read_baked_png(version:&Path, role:&str, resolution:u32) -> StudioResult<(RgbaImage,Vec<u8>)> {
    if !ROLES.contains(&role) {return Err("Bilinmeyen bake rolü".into());}
    let path=version.join("Textures").join(format!("{role}.png"));
    let baked_dir=version.join("Textures").canonicalize().map_err(|e|
        format!("Blender texture çıkış klasörü bulunamadı: {}: {e}. Log: {}",
            version.join("Textures").display(),version.join("Repair_Run.log").display()))?;
    let canonical=path.canonicalize().map_err(|e|
        format!("Blender {role} map üretmedi: {}: {e}. Log: {}",
            path.display(),version.join("Repair_Run.log").display()))?;
    if canonical.parent() != Some(baked_dir.as_path()) {
        return Err("Bake texture path traversal reddedildi".into());
    }
    let raw=fs::read(&path).map_err(|e|
        format!("Bake texture okunamadı: {}: {e}",path.display()))?;
    if raw.len()>30_000_000 {return Err("Bake PNG 30 MB sınırını aşıyor".into());}
    let img=image::load_from_memory_with_format(&raw,ImageFormat::Png)
        .map_err(|e|format!("{role} bake PNG bozuk: {e}"))?.to_rgba8();
    if img.dimensions()!=(resolution,resolution) {
        return Err(format!("{role} bake PNG çözünürlüğü beklenen {resolution}x{resolution} değil"));
    }
    Ok((img,raw))
}

fn gltf_factors(source:&[u8]) -> StudioResult<(Value,f32,f32,f32,f32,bool)> {
    // Require a single opaque, standard PBR material, not an unsafe conversion.
    geometry_only_glb(source)?;
    let (root,_)=parse_glb(source)?;
    let materials=root.get("materials").and_then(Value::as_array).ok_or("GLB PBR materyal listesi eksik")?;
    if materials.len()!=1 {return Err("UV Repair v1 yalnızca tek materyalli statik GLB destekler".into());}
    let material=&materials[0];
    if material.get("extensions").is_some() || material.get("emissiveTexture").is_some() ||
       material.get("alphaMode").and_then(Value::as_str).is_some_and(|x|x!="OPAQUE") {
        return Err("UV Repair v1 emission/alpha/uzantı desteklemiyor; eksik doku oluşturulmadı".into());
    }
    for path in ["/pbrMetallicRoughness/baseColorTexture","/pbrMetallicRoughness/metallicRoughnessTexture",
                 "/normalTexture","/occlusionTexture"] {
        if material.pointer(path).is_none() {return Err(format!("Zorunlu PBR haritası kaynak GLB'de yok: {path}"));}
    }
    let factor=material.pointer("/pbrMetallicRoughness/baseColorFactor").cloned().unwrap_or(json!([1.,1.,1.,1.]));
    let cs=factor.as_array().ok_or("baseColorFactor RGBA dizisi değil")?;
    if cs.len()!=4 || cs.iter().any(|v|v.as_f64().is_none_or(|x|!x.is_finite()||!(0.0..=1.0).contains(&x))) {
        return Err("Geçersiz baseColorFactor; GLB işlemi durduruldu".into());
    }
    let read_factor=|path:&str,default:f32| -> StudioResult<f32> {
        let f=material.pointer(path).and_then(Value::as_f64).unwrap_or(default as f64) as f32;
        if !f.is_finite() || !(0.0..=1.0).contains(&f) {
            Err(format!("GLB material factor aralık dışında: {path}"))
        } else {Ok(f)}
    };
    let metal=read_factor("/pbrMetallicRoughness/metallicFactor",1.)?;
    let rough=read_factor("/pbrMetallicRoughness/roughnessFactor",1.)?;
    let ao=read_factor("/occlusionTexture/strength",1.)?;
    let normal=material.pointer("/normalTexture/scale").and_then(Value::as_f64).unwrap_or(1.) as f32;
    if !normal.is_finite() || normal<0. || normal>4. {return Err("GLB normal scale geçersiz".into());}
    Ok((factor,metal,rough,ao,normal,
        material.get("doubleSided").and_then(Value::as_bool).unwrap_or(false)))
}

/// PBR contract: glTF metallic=ORM.B, roughness=ORM.G, AO=ORM.R.
/// Input PNGs are separate grayscale maps from the existing Texture Export.
/// The optional plush profile is explicit; the Preserve profile changes no
/// material factor or channel value.
fn make_orm(
    metallic:&RgbaImage, roughness:&RgbaImage, occlusion:&RgbaImage,
    metallic_factor:f32, roughness_factor:f32, preset:&str,
) -> StudioResult<(RgbaImage,f32,f32)> {
    if !matches!(preset,"preserve"|"plush") {return Err("UV Repair preset preserve/plush olmalı".into());}
    if metallic.dimensions()!=roughness.dimensions() || metallic.dimensions()!=occlusion.dimensions(){
        return Err("Bake map boyutları uyumsuz".into());
    }
    let mut out=RgbaImage::new(metallic.width(),metallic.height());
    for (x,y,p) in metallic.enumerate_pixels() {
        let mut m=p[0];
        let mut r=roughness.get_pixel(x,y)[0];
        if preset=="plush" {
            // For a soft fabric/plush asset, remove unwanted metallic flashes.
            // Intentionally a GLOBAL material override, not AI part segmentation.
            m=0;
            let physically_rough=(r as f32/255.*roughness_factor).clamp(0.,1.);
            r=(physically_rough.max(0.82)*255.).round() as u8;
        }
        out.put_pixel(x,y,Rgba([occlusion.get_pixel(x,y)[0],r,m,255]));
    }
    Ok((out,if preset=="plush"{1.}else{metallic_factor},
        if preset=="plush"{1.}else{roughness_factor}))
}

fn glb_chunk(dst:&mut Vec<u8>, tag:&[u8;4], data:&[u8], fill:u8)->StudioResult<()> {
    let length=(data.len()+3)&!3;
    if length>u32::MAX as usize{return Err("GLB chunk çok büyük".into());}
    dst.extend_from_slice(&(length as u32).to_le_bytes());
    dst.extend_from_slice(tag);
    dst.extend_from_slice(data);
    dst.resize(dst.len()+length-data.len(),fill);
    Ok(())
}

fn build_repaired_glb(
    geometry:&[u8], base:&[u8], orm:&[u8], normal:&[u8],
    base_factor:Value, metal_factor:f32, rough_factor:f32, ao_strength:f32, normal_scale:f32,
    double_sided:bool,
) -> StudioResult<Vec<u8>> {
    let (clean,_) = geometry_only_glb(geometry)?;
    let (mut root,bin)=parse_glb(&clean)?;
    let mut buffer=bin.to_vec();
    let views=root.get_mut("bufferViews").and_then(Value::as_array_mut).ok_or("GLB bufferViews eksik")?;
    let mut images=Vec::new();
    for (name,data) in [("BaseColor",base),("ORM",orm),("Normal",normal)] {
        while buffer.len()%4!=0 {buffer.push(0);}
        let offset=buffer.len();
        buffer.extend_from_slice(data);
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":data.len()}));
        images.push(json!({"bufferView":views.len()-1,"mimeType":"image/png","name":name}));
    }
    while buffer.len()%4!=0{buffer.push(0);}
    root["buffers"]=json!([{"byteLength":buffer.len()}]);
    root["images"]=json!(images);
    root["textures"]=json!([{"source":0},{"source":1},{"source":2}]);
    root["materials"]=json!([{"name":"AI3D_UV_Repaired","pbrMetallicRoughness":{
        "baseColorTexture":{"index":0},"metallicRoughnessTexture":{"index":1},
        "metallicFactor":metal_factor,"roughnessFactor":rough_factor,
        "baseColorFactor":base_factor},
        "normalTexture":{"index":2,"scale":normal_scale},
        "occlusionTexture":{"index":1,"strength":ao_strength},
        "alphaMode":"OPAQUE","doubleSided":double_sided
    }]);
    let meshes=root.get_mut("meshes").and_then(Value::as_array_mut).ok_or("GLB mesh yok")?;
    for mesh in meshes {
        let primitives=mesh.get_mut("primitives").and_then(Value::as_array_mut).ok_or("GLB primitive yok")?;
        for primitive in primitives {
            primitive.as_object_mut().ok_or("Primitive nesne değil")?.insert("material".into(),json!(0));
        }
    }
    let json_raw=serde_json::to_vec(&root).map_err(|e|e.to_string())?;
    let total=12+8+((json_raw.len()+3)&!3)+8+buffer.len();
    if total>u32::MAX as usize||total>150_000_000{return Err("Repair GLB 150 MB sınırını aşıyor".into());}
    let mut out=Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    glb_chunk(&mut out,b"JSON",&json_raw,b' ')?;
    glb_chunk(&mut out,b"BIN\0",&buffer,0)?;
    if out.len()!=total{return Err("Repaired GLB bytes length mismatch".into());}
    Ok(out)
}

/// Blender may return exit=0 when a Python script throws unless
/// --python-exit-code is supplied. Missing Repair_Report.json must therefore
/// become a precise worker error, never a bare Windows "os error 2".
fn read_blender_result(version:&Path, exit_success:bool, exit_code:Option<i32>, log:&str) -> StudioResult<Value> {
    let report_path=version.join("Repair_Report.json");
    if !exit_success || !report_path.is_file() {
        let tail=log.lines().rev().take(32).collect::<Vec<_>>()
            .into_iter().rev().collect::<Vec<_>>().join("\n");
        let stage=log.lines().rev().find(|line|line.contains("AI3D_UV_STAGE:"))
            .unwrap_or("Blender aşaması belirlenemedi");
        let reason=if !report_path.is_file() {
            format!("Blender başarı raporunu üretmedi: {}",report_path.display())
        } else {"Blender Python betiği başarısız oldu".to_string()};
        return Err(format!(
            "{reason} (Blender çıkış kodu: {exit_code:?}, son aşama: {stage}). \
             Ayrıntılı log: {}\n{}",
            version.join("Repair_Run.log").display(),tail,
        ));
    }
    read_json(&report_path).map_err(|e|
        format!("Blender raporu okunamadı: {}: {e}. Log: {}",
            report_path.display(),version.join("Repair_Run.log").display()))
}

/// Validate the Blender report without silently accepting topology changes.
/// Keep exact observed values in the UI error so a completed five-map bake
/// can be diagnosed from its existing Repair_Report.json without rerunning it.
fn verify_repair_report(report:&Value, source_triangles:u64, version:&Path) -> StudioResult<(u64,u64)> {
    let untouched=report.get("sourceUntouched").and_then(Value::as_bool);
    let before=report.get("trianglesBefore").and_then(Value::as_u64);
    let after=report.get("trianglesAfter").and_then(Value::as_u64);
    let old=report.pointer("/uv/oldUVIslands").and_then(Value::as_u64);
    let new=report.pointer("/uv/newUVIslands").and_then(Value::as_u64);
    let mut failures=Vec::new();
    if untouched!=Some(true) {failures.push("sourceUntouched != true");}
    if before!=Some(source_triangles) {failures.push("Blender input triangle count != source GLB");}
    if after!=Some(source_triangles) {failures.push("Blender output triangle count != source GLB");}
    if old.is_none() {failures.push("uv.oldUVIslands missing or not a nonnegative integer");}
    if new.is_none() {failures.push("uv.newUVIslands missing or not a nonnegative integer");}
    if old==Some(0) {failures.push("source has zero UV islands");}
    if new==Some(0) {failures.push("repaired atlas has zero UV islands");}
    if matches!((old,new),(Some(a),Some(b)) if b>a) {
        failures.push("new UV island count is greater than source count");
    }
    if !failures.is_empty() {
        return Err(format!(
            "UV/PBR kabul kontrolü: {}. Kaynak GLB triangles={source_triangles}; \
             Blender trianglesBefore={before:?}, trianglesAfter={after:?}; \
             sourceUntouched={untouched:?}; UV oldUVIslands={old:?}, newUVIslands={new:?}. \
             Geometriyi değiştiren sonuç otomatik kabul edilmedi. \
             Mevcut rapor: {}. Beş haritayı yeniden bake etmeden önce raporu paylaş.",
            failures.join("; "),version.join("Repair_Report.json").display()
        ));
    }
    Ok((old.expect("checked oldUVIslands"),new.expect("checked newUVIslands")))
}

fn background_repair(
    asset_dir:PathBuf, file_path:PathBuf, source_sha:String, texture_version:String,
    blender:String, resolution:u32, angle:u32, preset:String, weld:bool,
) -> StudioResult<PreparedRepair> {
    verify_sha256(&file_path,&source_sha).map_err(|e|
        format!("Kaynak GLB doğrulanamadı: {}: {e}",file_path.display()))?;
    let original=fs::read(&file_path).map_err(|e|
        format!("Kaynak GLB okunamadı: {}: {e}",file_path.display()))?;
    let (base_factor,source_metal,source_rough,source_ao,source_normal,double_sided)=gltf_factors(&original)?;
    let (map_dir,manifest)=existing_texture_dir(
        &asset_dir, asset_dir.file_name().and_then(|x|x.to_str()).ok_or("Asset klasörü geçersiz")?,
        &source_sha,&texture_version
    )?;
    // Validate all 5 user-selected, immutable Texture Export maps before Blender.
    for role in ROLES {
        let filename=format!("m0_{role}.png");
        png_map(&map_dir,&manifest,&filename).map_err(|e|format!(
            "UV onarımı başlamadan PBR kaynak haritası doğrulanamadı: {}: {e}. \
             Seçilen Texture Export sürümünü kontrol et veya yeniden oluştur.",
            map_dir.join(&filename).display()))?;
    }
    let version=asset_dir.join("uv_repairs").join(Uuid::new_v4().to_string());
    fs::create_dir_all(&version).map_err(|e|
        format!("Yeni UV onarım klasörü açılamadı: {}: {e}",version.display()))?;
    let inputs=version.join("SourceMaps");
    fs::create_dir(&inputs).map_err(|e|
        format!("SourceMaps klasörü açılamadı: {}: {e}",inputs.display()))?;
    for role in ROLES {
        let source=map_dir.join(format!("m0_{role}.png"));
        let destination=inputs.join(format!("m0_{role}.png"));
        fs::copy(&source,&destination).map_err(|e|
            format!("PBR kaynak harita kopyalanamadı: {} → {}: {e}",
                source.display(),destination.display()))?;
    }
    let worker=version.join("repair_uv_pbr.py");
    fs::write(&worker,WORKER).map_err(|e|
        format!("Blender UV worker yazılamadı: {}: {e}",worker.display()))?;
    // Do not clean up failed version automatically: Blender log/checkpoints
    // are the diagnostic evidence. No original files are modified.
    let execution=Command::new(&blender).arg("-b")
        .arg("--python-exit-code").arg("23")
        .arg("--python").arg(&worker)
        .arg("--").arg("--input").arg(&file_path)
        .arg("--source-maps").arg(&inputs)
        .arg("--output").arg(&version)
        .arg("--resolution").arg(resolution.to_string())
        .arg("--angle").arg(angle.to_string())
        .arg("--margin").arg("24")
        .arg("--weld").arg(if weld {"1"} else {"0"})
        .arg("--preset").arg(&preset)
        .output().map_err(|e|format!(
            "Blender UV/PBR başlatılamadı. Executable: {}. Script: {}. OS: {e}",
            blender,worker.display()))?;
    let log=format!("{}\n{}",String::from_utf8_lossy(&execution.stdout),String::from_utf8_lossy(&execution.stderr));
    let log_path=version.join("Repair_Run.log");
    fs::write(&log_path,&log).map_err(|e|
        format!("Blender logu yazılamadı: {}: {e}",log_path.display()))?;
    let report=read_blender_result(&version,execution.status.success(),execution.status.code(),&log)?;
    let source_triangles=inspect_glb(&original)?.0;
    let (old_islands,new_islands)=verify_repair_report(&report,source_triangles,&version)?;
    let (_,base)=read_baked_png(&version,"baseColor",resolution)?;
    let (_,normal)=read_baked_png(&version,"normal",resolution)?;
    let (metal,_)=read_baked_png(&version,"metallic",resolution)?;
    let (rough,_)=read_baked_png(&version,"roughness",resolution)?;
    let (ao,_)=read_baked_png(&version,"occlusion",resolution)?;
    let (orm,metal_factor,rough_factor)=make_orm(
        &metal,&rough,&ao,source_metal,source_rough,&preset)?;
    let orm_bytes=png_bytes(&orm)?;
    let orm_path=version.join("Textures/ORM.png");
    fs::write(&orm_path,&orm_bytes).map_err(|e|
        format!("Yeni ORM haritası yazılamadı: {}: {e}",orm_path.display()))?;
    let geometry_path=version.join("geometry.glb");
    let geometry=fs::read(&geometry_path).map_err(|e|format!(
        "Blender geometry.glb okunamadı: {}: {e}. Log: {}",
        geometry_path.display(),log_path.display()))?;
    let output=build_repaired_glb(
        &geometry,&base,&orm_bytes,&normal,
        base_factor,metal_factor,rough_factor,source_ao,source_normal,double_sided,
    )?;
    let (out_triangles,_,mats,tex)=inspect_glb(&output)?;
    if out_triangles!=source_triangles || mats!=1 || tex!=3 {
        return Err(format!("New UV GLB PBR/mesh doğrulaması geçemedi: {}",version.display()));
    }
    verify_sha256(&file_path,&source_sha)?;
    fs::write(version.join("Repaired.glb"),&output).map_err(|e|e.to_string())?;
    let hashes=json!({"sourceGlbSha256":source_sha,
        "repairedGlbSha256":sum_sha(&output),
        "sourceTextureExport":map_dir,
        "sourceUntouched":true,
        "textureRoles":["baseColor","normal","metallic","roughness","occlusion"],
        "outputResolution":resolution,"preset":preset,
        "completedAt":Utc::now().to_rfc3339()});
    fs::write(version.join("Repair_Manifest.json"),
        serde_json::to_vec_pretty(&hashes).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    Ok(PreparedRepair {source:output,report,version})
}

#[tauri::command]
pub async fn repair_asset_uv(
    asset_id:String, texture_export_dir:String, preset:String,
    resolution:u32, seam_angle:u32, weld:bool, state:State<'_,StudioState>
) -> StudioResult<UvRepairResult> {
    if !matches!(preset.as_str(),"preserve"|"plush") {
        return Err("Yalnız preserve/plush onarım profili desteklenir".into());
    }
    if !matches!(resolution,512|1024|2048) || !(35..=88).contains(&seam_angle) {
        return Err("UV çözünürlük/seam angle seçeneği geçersiz".into());
    }
    let asset=state.asset_by_id(&asset_id)?;
    let dir=owned_asset_dir(&asset.file_path,&asset_id)?;
    verify_sha256(Path::new(&asset.file_path),&asset.sha256)?;
    if asset.material_count!=1 {return Err("UV Repair v1 yalnız tek materyalli modelleri destekler".into());}
    let exe=find_blender(&state.current_settings()?)
        .ok_or("Blender bulunamadı; Ayarlarda Blender.exe dosyasını seç")?;
    let sha=asset.sha256.clone();
    let original_path=PathBuf::from(&asset.file_path);
    let prepared=tauri::async_runtime::spawn_blocking(move ||
        background_repair(dir,original_path,sha,texture_export_dir,exe,resolution,seam_angle,preset.clone(),weld)
    ).await.map_err(|e|format!("UV Repair worker hatası: {e}"))??;
    let report=&prepared.report;
    let new_asset=state.store_asset(&prepared.source,
        &format!("{} · UV Clean",asset.title), &asset.engine,
        json!({"origin":"uv-repair","parentAssetId":asset_id,"parentSha256":asset.sha256,
               "repairVersion":prepared.version,
               "repairReport":report,
               "manualUvBlend":"Repair_Working.blend",
               "originalImmutable":true}))?;
    let uv=report.get("uv").ok_or("UV report missing")?;
    Ok(UvRepairResult {
        asset:new_asset,
        version_dir:prepared.version.to_string_lossy().into_owned(),
        report_path:prepared.version.join("Repair_Report.json").to_string_lossy().into_owned(),
        working_blend:prepared.version.join("Repair_Working.blend").to_string_lossy().into_owned(),
        uv_layout:prepared.version.join("UV_Layout.svg").to_string_lossy().into_owned(),
        strategy:uv.get("strategy").and_then(Value::as_str).unwrap_or("unknown").into(),
        old_islands:uv.get("oldUVIslands").and_then(Value::as_u64).unwrap_or(0),
        new_islands:uv.get("newUVIslands").and_then(Value::as_u64).unwrap_or(0),
        source_untouched:true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn missing_blender_report_explains_actual_failure_instead_of_os_error_2() {
        let dir=std::env::temp_dir().join(format!("uv_report_missing_{}",Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let error=read_blender_result(&dir,true,Some(0),
            "AI3D_UV_STAGE:source_import\nTraceback (most recent call last):\nRuntimeError: unsupported Blender command")
            .unwrap_err();
        assert!(error.contains("Repair_Report.json"));
        assert!(error.contains("Repair_Run.log"));
        assert!(error.contains("source_import"));
        assert!(error.contains("RuntimeError: unsupported Blender command"));
        let error=read_blender_result(&dir,false,Some(23),"AI3D_UV_STAGE:bake_normal\nError").unwrap_err();
        assert!(error.contains("bake_normal"));
        fs::write(dir.join("Repair_Report.json"),b"{\"success\":true}").unwrap();
        assert!(read_blender_result(&dir,true,Some(0),"done").is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]fn rejects_bad_blender_report_with_exact_observed_values() {
        let path=Path::new("example/uv_repairs/uuid");
        let good=json!({"sourceUntouched":true,"trianglesBefore":7998,
            "trianglesAfter":7998,"uv":{"oldUVIslands":168,"newUVIslands":18}});
        assert_eq!(verify_repair_report(&good,7998,path).unwrap(),(168,18));
        let bad=json!({"sourceUntouched":true,"trianglesBefore":7996,
            "trianglesAfter":7996,"uv":{"oldUVIslands":168,"newUVIslands":18}});
        let error=verify_repair_report(&bad,7998,path).unwrap_err();
        assert!(error.contains("Kaynak GLB triangles=7998"));
        assert!(error.contains("trianglesBefore=Some(7996)"));
        assert!(error.contains("trianglesAfter=Some(7996)"));
        assert!(error.contains("Repair_Report.json"));
        let missing=json!({"sourceUntouched":true,"trianglesBefore":7998,
            "trianglesAfter":7998,"uv":{"oldUVIslands":168}});
        assert!(verify_repair_report(&missing,7998,path).unwrap_err()
            .contains("newUVIslands missing"));
        let worse=json!({"sourceUntouched":true,"trianglesBefore":7998,
            "trianglesAfter":7998,"uv":{"oldUVIslands":13,"newUVIslands":40}});
        assert!(verify_repair_report(&worse,7998,path).unwrap_err()
            .contains("greater than source"));
    }
    #[test]fn orm_preserves_gltf_semantics_and_plush_cleanup() {
        let metal=RgbaImage::from_pixel(1,1,Rgba([150,150,150,255]));
        let rough=RgbaImage::from_pixel(1,1,Rgba([35,35,35,255]));
        let ao=RgbaImage::from_pixel(1,1,Rgba([210,210,210,255]));
        let (raw,met_f,rou_f)=make_orm(&metal,&rough,&ao,0.7,0.5,"preserve").unwrap();
        assert_eq!(raw.get_pixel(0,0).0,[210,35,150,255]);
        assert_eq!(met_f,0.7);assert_eq!(rou_f,0.5);
        let (plush,mf,rf)=make_orm(&metal,&rough,&ao,0.7,0.5,"plush").unwrap();
        assert_eq!(plush.get_pixel(0,0).0,[210,209,0,255]);
        assert_eq!(mf,1.);assert_eq!(rf,1.);
        assert!(make_orm(&metal,&rough,&RgbaImage::new(2,2),1.,1.,"preserve").is_err());
        assert!(make_orm(&metal,&rough,&ao,1.,1.,"unknown").is_err());
    }
    #[test]fn pbr_injection_preserves_triangle_accessors() {
        // Three indexed vertices, UV0 and normal accessors, zero-filled dummy geometry.
        let meta=json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":104}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},
                           {"buffer":0,"byteOffset":36,"byteLength":6},
                           {"buffer":0,"byteOffset":44,"byteLength":36},
                           {"buffer":0,"byteOffset":80,"byteLength":24}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                         {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"},
                         {"bufferView":2,"componentType":5126,"count":3,"type":"VEC3"},
                         {"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":2,"TEXCOORD_0":3},"indices":1}]}],
            "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
        let mut j=serde_json::to_vec(&meta).unwrap();
        while j.len()%4!=0{j.push(b' ');}
        let mut b=Vec::new();
        let total=12+8+j.len()+8+104;
        b.extend_from_slice(b"glTF");b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(&(total as u32).to_le_bytes());
        glb_chunk(&mut b,b"JSON",&j,b' ').unwrap();
        glb_chunk(&mut b,b"BIN\0",&[0u8;104],0).unwrap();
        let png=png_bytes(&RgbaImage::from_pixel(1,1,Rgba([128,64,32,255]))).unwrap();
        let modified=build_repaired_glb(&b,&png,&png,&png,json!([1.,1.,1.,1.]),1.,1.,1.,1.,false).unwrap();
        let (tr,_,mats,tex)=inspect_glb(&modified).unwrap();
        assert_eq!((tr,mats,tex),(1,1,3));
        let (v,_) = parse_glb(&modified).unwrap();
        assert_eq!(v["materials"][0]["pbrMetallicRoughness"]["metallicRoughnessTexture"]["index"],json!(1));
        assert_eq!(v["materials"][0]["occlusionTexture"]["index"],json!(1));
        assert_eq!(v["images"].as_array().unwrap().len(),3);
    }
}
