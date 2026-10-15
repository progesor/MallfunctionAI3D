//! Non-destructive automatic UV atlas repair + independent PBR channel rebake.
//! A successful repair is registered as a NEW catalog asset. ComfyUI, original
//! GLB, Blender LODs, existing texture exports and Unity prefabs are immutable.
use crate::blender::find_blender;
use crate::uv_triangle_audit::{audit_source_triangles, SourceTriangles};
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
const FIDELITY_WORKER: &str = include_str!("../../../blender/uv_texture_fidelity.py");
const UV_PLANNER_WORKER: &str = include_str!("../../../blender/uv_planner.py");
const TOPOLOGY_WORKER: &str = include_str!("../../../blender/uv_topology.py");
const TOPOLOGY_ADAPTER_WORKER: &str =
    include_str!("../../../blender/uv_topology_blender.py");
const XATLAS_ADAPTER_WORKER: &str =
    include_str!("../../../blender/uv_provider_xatlas.py");
const ROLES: [&str;5] = ["baseColor", "normal", "metallic", "roughness", "occlusion"];

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct UvRepairResult {
    pub asset: Asset,
    pub version_dir: String,
    pub report_path: String,
    pub working_blend: String,
    pub uv_layout: String,
    pub uv_stress_layout: String,
    pub strategy: String,
    pub old_islands: u64,
    pub new_islands: u64,
    pub source_untouched: bool,
    pub quality: Value,
    pub mesh_health: Value,
    pub review_only: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all="camelCase")]
pub struct RecoverableUvRepair {
    pub version_dir: String,
    pub profile: String,
    pub resolution: u32,
    pub old_islands: u64,
    pub new_islands: u64,
    pub input_triangles: u64,
    pub quality_status: String,
    pub quality_issues: Vec<String>,
}

struct PreparedRepair {
    source: Vec<u8>,
    report: Value,
    version: PathBuf,
    review_only: bool,
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
    if log.contains("UV_PREFLIGHT_REJECTED:") {
        let stage=log.lines().rev().find(|x|x.contains("UV_PREFLIGHT_REJECTED:"))
            .unwrap_or("UV_PREFLIGHT_REJECTED");
        return Err(format!(
            "UV atlası bake öncesinde reddedildi; hiçbir PBR haritası              yeniden bake edilmedi. {}. Plan: {}.              UV_Stress.svg ve UV_Plan_Working.blend aynı klasördedir.              Eski source GLB ve Unity prefabları korunuyor.",
            stage.trim(),version.join("UV_Plan.json").display()
        ));
    }
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
/// Acceptance does not confuse an import-time difference with UV mesh loss.
/// Redundant faces are proven from the source accessor bytes (same oriented
/// indices within a primitive). An otherwise unexplained single missing face
/// may be accepted ONLY by an explicit opt-in on a completed old bake; it
/// remains flagged as an import-time loss in the immutable manifest.
fn verify_repair_report_mode(
    report:&Value, audit:&SourceTriangles, version:&Path,
    approve_one_import_face_loss:bool, review_only:bool,
) -> StudioResult<(u64,u64,u64,&'static str)> {
    let untouched=report.get("sourceUntouched").and_then(Value::as_bool);
    let before=report.get("trianglesBefore").and_then(Value::as_u64);
    let after=report.get("trianglesAfter").and_then(Value::as_u64);
    let old=report.pointer("/uv/oldUVIslands").and_then(Value::as_u64);
    let new=report.pointer("/uv/newUVIslands").and_then(Value::as_u64);
    let accepted=match before {
        Some(n) if n==audit.declared=>Some((n,"exact-source-match")),
        Some(n) if (audit.zero_area>0 || audit.duplicate_faces>0) &&
            n==audit.effective=>Some((n,"audited-redundant-source-face")),
        Some(n) if approve_one_import_face_loss && n>0 &&
            audit.zero_area==0 && audit.duplicate_faces==0 &&
            audit.declared.checked_sub(n)==Some(1)=>
            Some((n,"explicit-user-approved-one-import-face-loss")),
        _=>None,
    };
    let mut failures=Vec::new();
    if untouched!=Some(true) {failures.push("sourceUntouched != true");}
    if accepted.is_none() {
        failures.push("Blender import triangle count is not explained by source audit; explicit one-face review may be required");
    }
    // IMPORTANT: Compare before and after directly. Earlier builds used
    // accepted!=after, spuriously reporting UV loss when accepted=None.
    if before!=after || before.is_none() {
        failures.push("Blender actually changed triangle count DURING UV repair");
    }
    if old.is_none() {failures.push("uv.oldUVIslands missing or not an integer");}
    if new.is_none() {failures.push("uv.newUVIslands missing or not an integer");}
    if old==Some(0) {failures.push("source has zero UV islands");}
    if new==Some(0) {failures.push("repaired atlas has zero UV islands");}
    if !review_only && matches!((old,new),(Some(a),Some(b)) if b>a) {
        failures.push("new UV island count is greater than source count");
    }
    if !failures.is_empty() {
        return Err(format!(
            "UV/PBR kabul kontrolü: {}. Kaynak indeks üçgeni={}; \
             doğrulanmış sıfır alanlı={}; yinelenen aynı yüz={}; \
             audit sonrası kalan={}; Blender import before={before:?}, \
             UV sonrası after={after:?}; UV old={old:?} new={new:?}. \
             Tek üçgenlik açık onay yalnızca tamamlanmış bir bake kurtarılırken \
             geçerlidir; kaynak GLB değiştirilmez. Rapor: {}.",
            failures.join("; "),audit.declared,audit.zero_area,
            audit.duplicate_faces,audit.effective,
            version.join("Repair_Report.json").display()
        ));
    }
    let (tris,reason)=accepted.expect("accepted checked");
    Ok((old.unwrap(),new.unwrap(),tris,reason))
}


// Compatibility wrapper is exercised only by the in-module regression tests.
// Production validation calls verify_repair_report_mode directly.
#[cfg(test)]
fn verify_repair_report(
    report:&Value, audit:&SourceTriangles, version:&Path,
    approve_one_import_face_loss:bool,
) -> StudioResult<(u64,u64,u64,&'static str)> {
    verify_repair_report_mode(
        report,audit,version,approve_one_import_face_loss,false
    )
}

/// Fail closed if a good UV island count hides poor physical texture transfer.
/// The Blender-independent fidelity module compares source and baked textures
/// at matching surface barycentric sample positions, not atlas background.
fn check_texture_quality(
    report:&Value,folder:&Path,allow_review_quality_failure:bool
) -> StudioResult<Value> {
    let path=folder.join("Texture_Quality.json");
    if !path.is_file() {
        return Err(format!(
            "Bu UV sürümünün kalite raporu yok. Kaynak model korunuyor. \
             Yeni kalite kontrolü için taze UV onarımı başlat: {}",path.display()
        ));
    }
    let saved=read_json(&path)?;
    if report.get("textureFidelity")!=Some(&saved) {
        return Err(format!(
            "Blender Repair_Report.json ve Texture_Quality.json uyuşmuyor: {}",
            path.display()
        ));
    }
    if saved.get("schemaVersion").and_then(Value::as_u64)!=Some(2) ||
       saved.get("evaluated").and_then(Value::as_bool)!=Some(true) ||
       saved.get("method").and_then(Value::as_str)!=Some(
           "same-3d-surface-barycentric-UV-reprojection") {
        return Err(format!(
            "Eski texture kalite sözleşmesi M2.5.1 için yetersiz. \
             UV gerilme / yerel renk hatası ölçümü olmayan eski sürüm \
             otomatik onaylanamaz: {}",path.display()
        ));
    }
    let provenance=saved.get("sourceUvProvenance")
        .ok_or("Texture QA kaynak yüz/UV eşleştirme kanıtı eksik")?;
    let faces=provenance.get("matchedTriangleCount").and_then(Value::as_u64)
        .ok_or("Texture QA eşleşen kaynak üçgen sayısı eksik")?;
    if faces==0 ||
       Some(faces)!=report.get("trianglesBefore").and_then(Value::as_u64) ||
       provenance.get("originalUvProvenanceIntact").and_then(Value::as_bool)!=Some(true) ||
       provenance.get("originalCornerUvDriftCount").and_then(Value::as_u64)!=Some(0) {
        return Err(format!(
            "Blender mesh/UV kaynak yüz eşleşmesi bozuk: {}. Weld'i kapatıp \
             yeniden dene; orijinal GLB korunuyor.",path.display()
        ));
    }
    let metric=|object:&Value,name:&str| -> StudioResult<f64> {
        let v=object.get(name).and_then(Value::as_f64)
            .ok_or_else(||format!("Texture QA sayısal alanı eksik: {name}"))?;
        if !v.is_finite() || v<0.0 {
            return Err(format!("Texture QA alanı geçersiz: {name}"));
        }
        Ok(v)
    };
    // The first two metrics were already checked in M2.5. M2.5.1 also
    // checks each role's P99 and the fraction of extreme local errors.
    let specs:[(&str,f64,f64,f64,f64);4]=[
        ("baseColor",0.065,0.180,0.250,0.010),
        ("metallic",0.075,0.210,0.270,0.015),
        ("roughness",0.075,0.210,0.270,0.015),
        ("occlusion",0.080,0.220,0.280,0.015),
    ];
    let mut failures=Vec::new();
    let total=saved.get("sampleCount").and_then(Value::as_u64)
        .ok_or("Texture QA toplam sampleCount eksik")?;
    if total<4 || total!=faces*4 {
        return Err("Texture QA toplam örnek sayısı/üçgen sayısı uyuşmuyor".into());
    }
    for (role,max_mean,max_p95,max_p99,max_severe) in specs {
        let item=saved.pointer(&format!("/maps/{role}")).ok_or_else(
            ||format!("Texture QA rolü eksik: {role}"))?;
        let mean=metric(item,"meanAbsoluteError")?;
        let p95=metric(item,"p95AbsoluteError")?;
        let p99=metric(item,"p99AbsoluteError")?;
        let severe=metric(item,"severeSampleFraction")?;
        if item.get("sampleCount").and_then(Value::as_u64)!=Some(total) ||
           mean>1.0 || p95>1.0 || p99>1.0 || severe>1.0 {
            return Err(format!("Texture QA rolü ölçü aralığı/örnek sayısı geçersiz: {role}"));
        }
        let ok=mean<=max_mean && p95<=max_p95 && p99<=max_p99 && severe<=max_severe;
        if item.get("passed").and_then(Value::as_bool)!=Some(ok) {
            return Err(format!("Texture QA {role} başarı durumu verilerle uyuşmuyor"));
        }
        if !ok {
            failures.push(format!(
                "{role}: mean={mean:.4}/{max_mean:.3}, P95={p95:.4}/{max_p95:.3}, \
                 P99={p99:.4}/{max_p99:.3}, yüksek-hata=%{:.2}/{:.2}",
                severe*100.0,max_severe*100.0
            ));
        }
    }
    let stretch=saved.get("uvStretch").ok_or("UV stretch kalite alanı eksik")?;
    let source=stretch.get("source").ok_or("Kaynak UV stretch raporu eksik")?;
    let rebuilt=stretch.get("rebuilt").ok_or("Yeni UV stretch raporu eksik")?;
    if source.get("trianglesSampled").and_then(Value::as_u64)!=Some(faces) ||
       rebuilt.get("trianglesSampled").and_then(Value::as_u64)!=Some(faces) {
        return Err("UV stretch triangle counts do not match source face provenance".into());
    }
    let source_p95=metric(source,"p95Anisotropy")?;
    let source_max=metric(source,"maximumAnisotropy")?;
    let rebuilt_p95=metric(rebuilt,"p95Anisotropy")?;
    let rebuilt_max=metric(rebuilt,"maximumAnisotropy")?;
    if source_p95<1.0 || source_max<1.0 || rebuilt_p95<1.0 || rebuilt_max<1.0 {
        return Err("UV stretch anisotropy < 1 is invalid".into());
    }
    let p95_allowed=(source_p95*1.80).max(1.9);
    let max_allowed=(source_max*3.0).max(24.0);
    let degenerate_uv=rebuilt.get("degenerateUvTriangles")
        .and_then(Value::as_u64)
        .ok_or("Yeni UV atlasının sıfır UV-kenar üçgen sayısı eksik")?;
    let stretch_ok=rebuilt_p95<=p95_allowed+0.00001 &&
        rebuilt_max<=max_allowed+0.00001 && degenerate_uv==0;
    let stretch_report=saved.get("uvStretchGate").ok_or(
        "Blender UV distortion gate missing")?;
    if stretch_report.get("passed").and_then(Value::as_bool)!=Some(stretch_ok) {
        return Err("Blender UV distortion gate status doesn't match measured geometry".into());
    }
    let uv=report.get("uv").ok_or("UV unwrap report missing")?;
    let old_islands=uv.get("oldUVIslands").and_then(Value::as_u64)
        .ok_or("Old UV island count missing")?;
    let new_islands=uv.get("newUVIslands").and_then(Value::as_u64)
        .ok_or("New UV island count missing")?;
    let simplified=old_islands>0 && new_islands>0 && new_islands<old_islands;
    if saved.get("uvAtlasSimplified").and_then(Value::as_bool)!=Some(simplified) {
        return Err("UV atlas simplification claim doesn't match island counts".into());
    }
    if !stretch_ok {
        failures.push(format!(
            "UV gerilmesi: P95 {rebuilt_p95:.3} (kaynak {source_p95:.3}, \
             izin verilen {p95_allowed:.3}); en kötü {rebuilt_max:.3} \
             (kaynak {source_max:.3}, izin verilen {max_allowed:.3}); \
             sıfır UV kenarlı üçgen={degenerate_uv}. \
             Kuyruk gibi yüzeylerde ciddi bantlaşma riski"
        ));
    }
    if !simplified {
        failures.push(format!(
            "UV atlası daha kolay düzenlenebilir olmadı: \
             eski {old_islands} ada → yeni {new_islands} ada"
        ));
    }
    let accepted=failures.is_empty();
    if saved.get("passed").and_then(Value::as_bool)!=Some(accepted) {
        return Err(format!(
            "Blender kalite sonuçları ve Rust yeniden hesaplaması uyuşmuyor: {}",
            path.display()
        ));
    }
    if !accepted && !allow_review_quality_failure {
        return Err(format!(
            "UV/PBR kalite kapısı başarısız: {}. Model otomatik kabul edilmedi, \
             kaynak GLB ve eski Unity prefablara dokunulmadı. \
             Kaydedilmiş Texture_Quality.json: {}. UV_Stress.svg: {}",
            failures.join(" | "),path.display(),
            folder.join("UV_Stress.svg").display()
        ));
    }
    Ok(saved)
}

/// Legacy strict entrypoint stays strict; a separate, explicitly requested
/// review-only path may inspect failed output without claiming acceptance.
fn verify_texture_quality(report:&Value,folder:&Path) -> StudioResult<Value> {
    check_texture_quality(report,folder,false)
}

fn quality_issues(quality:&Value) -> Vec<String> {
    let mut problems=Vec::new();
    for role in ROLES {
        if quality.pointer(&format!("/maps/{role}/passed"))
            .and_then(Value::as_bool)==Some(false) {
            problems.push(format!("{role} PBR kalite kaybı"));
        }
    }
    if quality.pointer("/uvStretchGate/passed").and_then(Value::as_bool)==Some(false) {
        problems.push("UV gerilmesi yüksek".into());
    }
    if quality.get("uvAtlasSimplified").and_then(Value::as_bool)==Some(false) {
        problems.push("UV ada sayısı azaltılamadı".into());
    }
    problems
}


/// Finalize Blender's completed outputs, preserving the original five bakes.
/// This is also called when resuming a previously rejected completed attempt.
fn finalize_repair(
    asset_dir:&Path, file_path:&Path, source_sha:&str,
    texture_version:&str, version:&Path, preset:&str, resolution:u32,
    approve_one_import_face_loss:bool,
    review_only:bool,
) -> StudioResult<PreparedRepair> {
    let base=asset_dir.join("uv_repairs").canonicalize()
        .map_err(|e|format!("UV onarım arşivi bulunamadı: {e}"))?;
    let folder=version.canonicalize().map_err(|e|
        format!("Tamamlanacak UV klasörü bulunamadı: {}: {e}",version.display()))?;
    if folder.parent()!=Some(base.as_path()) ||
        !folder.file_name().and_then(|x|x.to_str())
            .is_some_and(|x|Uuid::parse_str(x).is_ok()) {
        return Err("UV kurtarma sürümü seçili kaynak modelin arşivine ait değil".into());
    }
    let manifest_path=folder.join("Repair_Manifest.json");
    if manifest_path.exists(){
        return Err("Bu UV sürümü önceden tamamlanmış; tekrar arşive eklenmeyecek".into());
    }
    verify_sha256(file_path,source_sha)?;
    let original=fs::read(file_path).map_err(|e|e.to_string())?;
    let (tint,metal_factor_src,rough_factor_src,ao_factor,normal_scale,double_sided)=gltf_factors(&original)?;
    let asset_id=asset_dir.file_name().and_then(|s|s.to_str()).ok_or("Asset UUID missing")?;
    let (map_dir,map_manifest)=existing_texture_dir(
        asset_dir,asset_id,source_sha,texture_version)?;
    // The prior attempt must have baked the EXACT currently selected export
    // inputs; arbitrary files in an old version may not be rebound silently.
    let source_dir=folder.join("SourceMaps").canonicalize()
        .map_err(|e|format!("SourceMaps kaydı bulunamadı: {e}"))?;
    for role in ROLES {
        let name=format!("m0_{role}.png");
        png_map(&map_dir,&map_manifest,&name)?;
        let snapshot=source_dir.join(&name);
        let checked=snapshot.canonicalize().map_err(|e|
            format!("Kurtarma haritası kayıp: {}: {e}",snapshot.display()))?;
        if checked.parent()!=Some(source_dir.as_path()) {
            return Err("UV kurtarma SourceMaps yolu güvenli değil".into());
        }
        if fs::read(&checked).map_err(|e|e.to_string())? !=
           fs::read(map_dir.join(&name)).map_err(|e|e.to_string())? {
            return Err(format!(
                "Önceki bake {} adlı farklı Texture Export sürümünü kullanmış. \
                 Kaynak PBR sürümünü değiştir veya yeniden bake et.",name
            ));
        }
    }
    let log_path=folder.join("Repair_Run.log");
    let log=fs::read_to_string(&log_path).map_err(|e|
        format!("Tamamlanmış Blender run logu kayıp: {}: {e}",log_path.display()))?;
    if !log.contains("AI3D_UV_STAGE:complete") ||
       !log.contains("AI3D_REPAIR_SUCCESS:") {
        return Err(format!(
            "Blender bake+GLB export bu klasörde tamamlanmamış: {}",
            log_path.display()
        ));
    }
    let report=read_blender_result(&folder,true,Some(0),&log)?;
    let reported_profile=report.get("profile").and_then(Value::as_str);
    let reported_resolution=report.get("resolution").and_then(Value::as_u64);
    let map_roles=report.get("mapRoles").and_then(Value::as_array);
    let expected_roles=ROLES.iter().map(|x|Value::String((*x).into())).collect::<Vec<_>>();
    if report.get("schemaVersion").and_then(Value::as_u64)!=Some(1) ||
       report.get("normalRebakedInTangentSpace").and_then(Value::as_bool)!=Some(true) ||
       reported_profile!=Some(preset) || reported_resolution!=Some(resolution as u64) ||
       map_roles!=Some(&expected_roles) {
        return Err(format!(
            "Kurtarılacak Blender raporu seçili profil/çözünürlük/5 PBR map \
             değerleriyle uyuşmuyor: profile={reported_profile:?}, \
             resolution={reported_resolution:?}. Rapor: {}",
            folder.join("Repair_Report.json").display()
        ));
    }
    // Evaluate texture quality FIRST so a bad tail/color never hides behind
    // the separately documented one-face import-time geometry mismatch.
    let quality=if review_only {
        check_texture_quality(&report,&folder,true)?
    } else {
        verify_texture_quality(&report,&folder)?
    };
    let audit=audit_source_triangles(&original)?;
    let declared=inspect_glb(&original)?.0;
    if audit.declared!=declared{
        return Err(format!("GLB triangle audit mismatch: {} vs {declared}",audit.declared));
    }
    let (old_islands,new_islands,accepted_triangles,geometry_policy)=
        verify_repair_report_mode(
            &report,&audit,&folder,approve_one_import_face_loss,review_only
        )?;
    let (_,base_color)=read_baked_png(&folder,"baseColor",resolution)?;
    let (_,normal)=read_baked_png(&folder,"normal",resolution)?;
    let (metal,_)=read_baked_png(&folder,"metallic",resolution)?;
    let (rough,_)=read_baked_png(&folder,"roughness",resolution)?;
    let (ao,_)=read_baked_png(&folder,"occlusion",resolution)?;
    if !folder.join("UV_Layout.svg").is_file() ||
       !folder.join("UV_Stress.svg").is_file() ||
       !folder.join("Repair_Working.blend").is_file() {
        return Err("Tamamlanmış Blender çalışma dosyası veya UV atlası eksik".into());
    }
    let (orm,metal_factor,rough_factor)=make_orm(
        &metal,&rough,&ao,metal_factor_src,rough_factor_src,preset)?;
    let orm_bytes=png_bytes(&orm)?;
    let orm_path=folder.join("Textures/ORM.png");
    if orm_path.exists() {
        if fs::read(&orm_path).map_err(|e|e.to_string())?!=orm_bytes {
            return Err(format!("Kurtarma ORM.png önceki sürümle farklı: {}",orm_path.display()));
        }
    } else {
        use std::io::Write;
        fs::OpenOptions::new().write(true).create_new(true).open(&orm_path)
            .map_err(|e|e.to_string())?
            .write_all(&orm_bytes).map_err(|e|e.to_string())?;
    }
    let geometry_path=folder.join("geometry.glb");
    let checked_geometry=geometry_path.canonicalize().map_err(|e|
        format!("Blender geometry.glb kayıp: {}: {e}",geometry_path.display()))?;
    if checked_geometry.parent()!=Some(folder.as_path()) {
        return Err("UV geometry.glb arşiv dışına çıkıyor".into());
    }
    let geometry=fs::read(&checked_geometry).map_err(|e|e.to_string())?;
    let geometry_tris=inspect_glb(&geometry)?.0;
    if geometry_tris!=accepted_triangles {
        return Err(format!(
            "Blender geometry.glb triangle count={geometry_tris}, \
             report triangle count={accepted_triangles}; kaynak değiştirilmedi"
        ));
    }
    let output=build_repaired_glb(
        &geometry,&base_color,&orm_bytes,&normal,
        tint,metal_factor,rough_factor,ao_factor,normal_scale,double_sided,
    )?;
    let (out_triangles,_,mats,tex)=inspect_glb(&output)?;
    if out_triangles!=accepted_triangles || mats!=1 || tex!=3 {
        return Err(format!(
            "Yeni UV GLB PBR/geometri kabulü başarısız: \
             output triangles={out_triangles}, Blender triangles={accepted_triangles}, \
             material={mats}, textures={tex}"
        ));
    }
    verify_sha256(file_path,source_sha)?;
    let repaired=folder.join("Repaired.glb");
    if repaired.exists() {
        if fs::read(&repaired).map_err(|e|e.to_string())?!=output{
            return Err(format!("Mevcut Repaired.glb değiştirilmeyecek: {}",repaired.display()));
        }
    } else {
        use std::io::Write;
        fs::OpenOptions::new().write(true).create_new(true).open(&repaired)
            .map_err(|e|e.to_string())?.write_all(&output)
            .map_err(|e|e.to_string())?;
    }
    let manifest=json!({
        "sourceGlbSha256":source_sha,
        "repairedGlbSha256":sum_sha(&output),
        "sourceTextureExport":map_dir,
        "sourceUntouched":true,
        "sourceDeclaredTriangles":audit.declared,
        "sourceZeroAreaTriangles":audit.zero_area,
        "sourceDuplicateFaces":audit.duplicate_faces,
        "blenderAcceptedTriangles":accepted_triangles,
        "sourceImportTriangleDelta":audit.declared-accepted_triangles,
        "surfaceTextureQuality":quality,
        "qualityGate":"surface-fidelity-local-distortion-v2",
        "reviewOnly":review_only,
        "qualityGatePassed":quality.get("passed").and_then(Value::as_bool)==Some(true),
        "qualityIssues":quality_issues(&quality),
        "qualityAcceptancePolicy":if review_only {
            "explicit-review-only-exception"
        } else {"strict"},

        "uvStressLayout":"UV_Stress.svg",
        "geometryAcceptancePolicy":geometry_policy,
        "explicitOneFaceImportLossApproval":geometry_policy=="explicit-user-approved-one-import-face-loss",
        "uv":{"oldIslands":old_islands,"newIslands":new_islands},
        "textureRoles":["baseColor","normal","metallic","roughness","occlusion"],
        "outputResolution":resolution,"preset":preset,
        "completedAt":Utc::now().to_rfc3339()
    });
    let manifest_data=serde_json::to_vec_pretty(&manifest).map_err(|e|e.to_string())?;
    {
        use std::io::Write;
        fs::OpenOptions::new().write(true).create_new(true).open(&manifest_path)
            .map_err(|e|format!("Repair_Manifest.json üzerine yazılmadı: {e}"))?
            .write_all(&manifest_data).map_err(|e|e.to_string())?;
    }
    Ok(PreparedRepair{source:output,report,version:folder,review_only})
}

fn background_repair(
    asset_dir:PathBuf, file_path:PathBuf, source_sha:String, texture_version:String,
    blender:String, resolution:u32, angle:u32, preset:String,
    weld:bool, safe_cleanup:bool,uv_engine:String,
) -> StudioResult<PreparedRepair> {
    verify_sha256(&file_path,&source_sha).map_err(|e|
        format!("Kaynak GLB doğrulanamadı: {}: {e}",file_path.display()))?;
    let original=fs::read(&file_path).map_err(|e|
        format!("Kaynak GLB okunamadı: {}: {e}",file_path.display()))?;
    gltf_factors(&original)?; // fail early before starting Blender
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
    // Bundle the exact tested offline quality module alongside the pinned
    // worker. No external pip package or Blender addon installation required.
    let quality_worker=version.join("uv_texture_fidelity.py");
    fs::write(&quality_worker,FIDELITY_WORKER).map_err(|e|format!(
        "Blender Texture Fidelity worker yazılamadı: {}: {e}",
        quality_worker.display()))?;
    let planner_worker=version.join("uv_planner.py");
    fs::write(&planner_worker,UV_PLANNER_WORKER).map_err(|e|format!(
        "UV plan ön kontrol modülü yazılamadı: {}: {e}",
        planner_worker.display()))?;
    // Provider registry is version-pinned with each UV repair output.
    // Built-in Blender topo adapter is always available; MIT xatlas is
    // optional and never downloaded or installed by the worker.
    for (name,content) in [
        ("uv_topology.py",TOPOLOGY_WORKER),
        ("uv_topology_blender.py",TOPOLOGY_ADAPTER_WORKER),
        ("uv_provider_xatlas.py",XATLAS_ADAPTER_WORKER),
    ] {
        let destination=version.join(name);
        fs::write(&destination,content).map_err(|e|format!(
            "UV motoru dosyası yazılamadı: {}: {e}",destination.display()))?;
    }
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
        .arg("--safe-cleanup").arg(if safe_cleanup {"1"} else {"0"})
        .arg("--preset").arg(&preset)
        .arg("--uv-engine").arg(&uv_engine)
        .output().map_err(|e|format!(
            "Blender UV/PBR başlatılamadı. Executable: {}. Script: {}. OS: {e}",
            blender,worker.display()))?;
    let log=format!("{}\n{}",String::from_utf8_lossy(&execution.stdout),String::from_utf8_lossy(&execution.stderr));
    let log_path=version.join("Repair_Run.log");
    fs::write(&log_path,&log).map_err(|e|
        format!("Blender logu yazılamadı: {}: {e}",log_path.display()))?;
    read_blender_result(&version,execution.status.success(),execution.status.code(),&log)?;
    // The worker has finished; exact validations and packing are also reused
    // by the no-bake recovery path.
    finalize_repair(&asset_dir,&file_path,&source_sha,&texture_version,
                    &version,&preset,resolution,false,false)
}

fn register_repair(
    state:&StudioState, asset:&Asset, prepared:PreparedRepair
) -> StudioResult<UvRepairResult> {
    let report=&prepared.report;
    let title=if prepared.review_only {
        format!("{} · UV REVIEW (QA FAIL)",asset.title)
    } else {format!("{} · UV Clean",asset.title)};
    let new_asset=state.store_asset(
        &prepared.source,&title,&asset.engine,
        json!({
            "origin":"uv-repair",
            "parentAssetId":&asset.id,
            "parentSha256":&asset.sha256,
            "repairVersion":&prepared.version,
            "repairReport":report,
            "manualUvBlend":"Repair_Working.blend",
            "originalImmutable":true,
            "reviewOnly":prepared.review_only,
            "productionExportBlocked":prepared.review_only,
            "qualityGatePassed":report.pointer("/textureFidelity/passed")
                .and_then(Value::as_bool)==Some(true),
            "qualityIssues":quality_issues(&report["textureFidelity"])
        }),
    )?;
    let uv=report.get("uv").ok_or("UV report missing")?;
    Ok(UvRepairResult {
        asset:new_asset,
        version_dir:prepared.version.to_string_lossy().into_owned(),
        report_path:prepared.version.join("Repair_Report.json").to_string_lossy().into_owned(),
        working_blend:prepared.version.join("Repair_Working.blend").to_string_lossy().into_owned(),
        uv_layout:prepared.version.join("UV_Layout.svg").to_string_lossy().into_owned(),
        uv_stress_layout:prepared.version.join("UV_Stress.svg").to_string_lossy().into_owned(),
        strategy:uv.get("strategy").and_then(Value::as_str).unwrap_or("unknown").into(),
        old_islands:uv.get("oldUVIslands").and_then(Value::as_u64).unwrap_or(0),
        new_islands:uv.get("newUVIslands").and_then(Value::as_u64).unwrap_or(0),
        source_untouched:true,
        quality:report.get("textureFidelity").cloned().unwrap_or(Value::Null),
        mesh_health:report.get("meshHealth").cloned().unwrap_or(Value::Null),
        review_only:prepared.review_only,
    })
}

/// Finds completed Blender bakes that were rejected by Studio's previous
/// over-strict raw index count check. Listing never mutates or retries Blender.
#[tauri::command]
pub fn list_uv_repair_attempts(
    asset_id:String, state:State<'_,StudioState>
) -> StudioResult<Vec<RecoverableUvRepair>> {
    let asset=state.asset_by_id(&asset_id)?;
    let dir=owned_asset_dir(&asset.file_path,&asset_id)?;
    let root=dir.join("uv_repairs");
    if !root.is_dir() {return Ok(Vec::new());}
    let mut found=Vec::new();
    for entry in fs::read_dir(&root).map_err(|e|e.to_string())? {
        let entry=entry.map_err(|e|e.to_string())?;
        let ty=entry.file_type().map_err(|e|e.to_string())?;
        if !ty.is_dir() || ty.is_symlink(){continue;}
        let version=entry.path();
        if !version.file_name().and_then(|s|s.to_str())
            .is_some_and(|s|Uuid::parse_str(s).is_ok()) {continue;}
        if version.join("Repair_Manifest.json").exists() ||
            !version.join("Repair_Run.log").is_file() ||
            !version.join("Texture_Quality.json").is_file() ||
            !version.join("geometry.glb").is_file() ||
            !version.join("Repair_Working.blend").is_file() ||
            !version.join("UV_Layout.svg").is_file() ||
            ROLES.iter().any(|role|!version.join("Textures").join(format!("{role}.png")).is_file()) {
            continue;
        }
        let report=match read_json(&version.join("Repair_Report.json")) {
            Ok(v)=>v,Err(_)=>continue,
        };
        let quality=match read_json(&version.join("Texture_Quality.json")) {
            Ok(v)=>v,Err(_)=>continue,
        };
        if quality.get("schemaVersion").and_then(Value::as_u64)!=Some(2) ||
           quality.get("evaluated").and_then(Value::as_bool)!=Some(true) ||
           quality.get("passed").and_then(Value::as_bool).is_none() ||
           report.get("textureFidelity")!=Some(&quality) {
            continue; // Invalid or unscored runs are not reviewable.
        }
        if report.get("schemaVersion").and_then(Value::as_u64)!=Some(1) ||
            report.get("sourceUntouched").and_then(Value::as_bool)!=Some(true){
            continue;
        }
        let profile=match report.get("profile").and_then(Value::as_str){
            Some(s @ ("preserve"|"plush"))=>s.to_owned(),_=>continue
        };
        let resolution=match report.get("resolution").and_then(Value::as_u64){
            Some(x @ (512|1024|2048))=>x as u32,_=>continue
        };
        let old_islands=report.pointer("/uv/oldUVIslands").and_then(Value::as_u64)
            .unwrap_or(0);
        let new_islands=report.pointer("/uv/newUVIslands").and_then(Value::as_u64)
            .unwrap_or(0);
        let triangles=report.get("trianglesAfter").and_then(Value::as_u64)
            .unwrap_or(0);
        if old_islands==0 || new_islands==0 || triangles==0 {
            continue;
        }
        // Limit reading log size; this endpoint must remain cheap for UI.
        let log_path=version.join("Repair_Run.log");
        if log_path.metadata().map(|m|m.len()).unwrap_or(u64::MAX)>5_000_000 {
            continue;
        }
        let log=fs::read_to_string(&log_path).unwrap_or_default();
        if !log.contains("AI3D_UV_STAGE:complete") ||
           !log.contains("AI3D_REPAIR_SUCCESS:"){continue;}
        let when=entry.metadata().ok().and_then(|m|m.modified().ok())
            .unwrap_or(std::time::UNIX_EPOCH);
        found.push((when,RecoverableUvRepair{
            version_dir:version.to_string_lossy().into_owned(),
            profile,resolution,old_islands,new_islands,input_triangles:triangles,
            quality_status:if quality["passed"]==Value::Bool(true) {
                "passed".to_owned()
            } else {"needs-review".to_owned()},
            quality_issues:quality_issues(&quality),
        }));
    }
    found.sort_by(|a,b|b.0.cmp(&a.0));
    found.truncate(16);
    Ok(found.into_iter().map(|(_,data)|data).collect())
}

/// Complete a previously successful five-map bake WITHOUT launching Blender.
/// The version must be inside this exact source asset; all five input snapshots,
/// source SHA, report, geometry and texture packing are revalidated.
#[tauri::command]
pub async fn resume_uv_repair(
    asset_id:String, version_dir:String, texture_export_dir:String,
    preset:String, resolution:u32, approve_one_import_face_loss:bool,
    state:State<'_,StudioState>
) -> StudioResult<UvRepairResult> {
    if !matches!(preset.as_str(),"preserve"|"plush") ||
       !matches!(resolution,512|1024|2048) {
        return Err("UV kurtarma profili veya çözünürlük geçersiz".into());
    }
    let asset=state.asset_by_id(&asset_id)?;
    if asset.material_count!=1{
        return Err("UV kurtarma tek materyalli kaynakla sınırlı".into());
    }
    let dir=owned_asset_dir(&asset.file_path,&asset_id)?;
    let original=PathBuf::from(&asset.file_path);
    let source_sha=asset.sha256.clone();
    let prepared=tauri::async_runtime::spawn_blocking(move ||
        finalize_repair(&dir,&original,&source_sha,&texture_export_dir,
                        Path::new(&version_dir),&preset,resolution,
                        approve_one_import_face_loss,false)
    ).await.map_err(|e|format!("UV kurtarma görevi başarısız: {e}"))??;
    register_repair(&state,&asset,prepared)
}

/// Explicit user choice to keep a failed but fully baked UV asset for
/// INSPECTION. The result is permanently flagged reviewOnly/QA FAIL,
/// blocked from Unity exports and never labelled game-ready.
#[tauri::command]
pub async fn save_uv_review(
    asset_id:String,version_dir:String,texture_export_dir:String,
    preset:String,resolution:u32,approve_one_import_face_loss:bool,
    accept_failed_quality:bool,state:State<'_,StudioState>,
) -> StudioResult<UvRepairResult> {
    if !accept_failed_quality {
        return Err("QA başarısız sürüm yalnız açık kullanıcı onayıyla inceleme taslağı olarak kaydedilebilir".into());
    }
    if !matches!(preset.as_str(),"preserve"|"plush") ||
       !matches!(resolution,512|1024|2048) {
        return Err("İnceleme taslağı PBR profili / çözünürlüğü geçersiz".into());
    }
    let asset=state.asset_by_id(&asset_id)?;
    if asset.material_count!=1 {
        return Err("İnceleme taslağı yalnız tek materyalli modeller içindir".into());
    }
    let dir=owned_asset_dir(&asset.file_path,&asset_id)?;
    let src=PathBuf::from(&asset.file_path);
    let sha=asset.sha256.clone();
    let prepared=tauri::async_runtime::spawn_blocking(move ||
        finalize_repair(&dir,&src,&sha,&texture_export_dir,
                        Path::new(&version_dir),&preset,resolution,
                        approve_one_import_face_loss,true)
    ).await.map_err(|e|format!("UV inceleme taslağı kurtarma görevi: {e}"))??;
    register_repair(&state,&asset,prepared)
}

#[tauri::command]
pub async fn repair_asset_uv(
    asset_id:String, texture_export_dir:String, preset:String,
    resolution:u32, seam_angle:u32, weld:bool, safe_cleanup:bool,
    uv_engine:String, state:State<'_,StudioState>
) -> StudioResult<UvRepairResult> {
    if !matches!(uv_engine.as_str(),"both"|"blender"|"xatlas"){
        return Err("UV engine must be both, blender or xatlas".into());
    }
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
        background_repair(dir,original_path,sha,texture_export_dir,exe,resolution,seam_angle,
                          preset.clone(),weld,safe_cleanup,uv_engine)
    ).await.map_err(|e|format!("UV Repair worker hatası: {e}"))??;
    register_repair(&state,&asset,prepared)
}

/// All game export entrypoints must reject manually saved, QA-rejected
/// review-only assets. Normal (original or quality-passed) assets are unchanged.
pub(crate) fn ensure_unity_exportable(asset_path:&Path)->StudioResult<()> {
    let owner=asset_path.parent().ok_or("UV varlık arşiv yolu geçersiz")?;
    let manifest=crate::unity_export::read_json(&owner.join("manifest.json"))?;
    if manifest.pointer("/provenance/reviewOnly").and_then(Value::as_bool)==Some(true)
      || manifest.pointer("/provenance/productionExportBlocked")
          .and_then(Value::as_bool)==Some(true) {
        return Err("Bu model QA FAIL · UV REVIEW inceleme taslağıdır. \
            Unity için kaynak veya kalite testinden geçmiş modeli seç. \
            Orijinal GLB korunmaktadır.".into());
    }
    Ok(())
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

    #[test]fn rejects_unscored_or_low_fidelity_uv_repairs() {
        let root=std::env::temp_dir().join(format!("uv_qa_{}",Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        assert!(verify_texture_quality(&json!({}),&root).unwrap_err()
            .contains("kalite raporu yok"));
        let mut quality=json!({
            "schemaVersion":2,"evaluated":true,
            "method":"same-3d-surface-barycentric-UV-reprojection",
            "passed":true,"sampleCount":320,"uvAtlasSimplified":true,
            "sourceUvProvenance":{"matchedTriangleCount":80,
                "originalCornerUvDriftCount":0,
                "originalUvProvenanceIntact":true},
            "uvStretch":{"source":{"trianglesSampled":80,"p95Anisotropy":1.2,
                          "maximumAnisotropy":7.0},
                         "rebuilt":{"trianglesSampled":80,"p95Anisotropy":1.5,
                          "maximumAnisotropy":9.0,"degenerateUvTriangles":0}},
            "uvStretchGate":{"passed":true},
            "maps":{
                "baseColor":{"meanAbsoluteError":0.025,"p95AbsoluteError":0.07,
                    "p99AbsoluteError":0.1,"severeSampleFraction":0.001,
                    "sampleCount":320,"passed":true},
                "metallic":{"meanAbsoluteError":0.01,"p95AbsoluteError":0.02,
                    "p99AbsoluteError":0.04,"severeSampleFraction":0.0,
                    "sampleCount":320,"passed":true},
                "roughness":{"meanAbsoluteError":0.01,"p95AbsoluteError":0.02,
                    "p99AbsoluteError":0.04,"severeSampleFraction":0.0,
                    "sampleCount":320,"passed":true},
                "occlusion":{"meanAbsoluteError":0.01,"p95AbsoluteError":0.02,
                    "p99AbsoluteError":0.04,"severeSampleFraction":0.0,
                    "sampleCount":320,"passed":true}
            }
        });
        let report=|q:&Value|json!({"textureFidelity":q,"trianglesBefore":80,
            "uv":{"oldUVIslands":254,"newUVIslands":169}});
        fs::write(root.join("Texture_Quality.json"),
            serde_json::to_vec_pretty(&quality).unwrap()).unwrap();
        assert_eq!(verify_texture_quality(&report(&quality),&root)
            .unwrap()["passed"],json!(true));
        quality["maps"]["baseColor"]["p99AbsoluteError"]=json!(0.41);
        quality["maps"]["baseColor"]["passed"]=json!(false);
        quality["passed"]=json!(false);
        fs::write(root.join("Texture_Quality.json"),
            serde_json::to_vec_pretty(&quality).unwrap()).unwrap();
        assert!(verify_texture_quality(&report(&quality),&root).unwrap_err()
            .contains("baseColor"));
        quality["maps"]["baseColor"]["p99AbsoluteError"]=json!(0.1);
        quality["maps"]["baseColor"]["passed"]=json!(true);
        quality["uvStretch"]["rebuilt"]["p95Anisotropy"]=json!(3.67474);
        quality["uvStretch"]["rebuilt"]["maximumAnisotropy"]=json!(88.76794);
        quality["uvStretchGate"]["passed"]=json!(false);
        fs::write(root.join("Texture_Quality.json"),
            serde_json::to_vec_pretty(&quality).unwrap()).unwrap();
        assert!(verify_texture_quality(&report(&quality),&root).unwrap_err()
            .contains("UV gerilmesi"));
        // Explicit inspection-only recovery may preserve QA-failed geometry
        // but cannot mutate the measured quality or claim it passed.
        assert_eq!(
            check_texture_quality(&report(&quality),&root,true)
                .unwrap()["passed"],json!(false)
        );
        let mut fake_good=quality.clone();
        fake_good["passed"]=json!(true);
        assert!(check_texture_quality(&report(&fake_good),&root,true).is_err());
        quality["schemaVersion"]=json!(1);
        assert!(verify_texture_quality(&report(&quality),&root).unwrap_err()
            .contains("Eski texture kalite sözleşmesi"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]fn failed_atlas_is_reviewable_without_relaxing_real_geometry_safety() {
        let audited=SourceTriangles{declared:7994,zero_area:0,
            duplicate_faces:0,effective:7994};
        let report=json!({"sourceUntouched":true,"trianglesBefore":7993,
            "trianglesAfter":7993,"uv":{"oldUVIslands":254,"newUVIslands":269}});
        let path=Path::new("asset/uv_repairs/rejected");
        assert!(verify_repair_report(&report,&audited,path,true).is_err());
        let permitted=verify_repair_report_mode(&report,&audited,path,true,true).unwrap();
        assert_eq!(permitted.0,254);
        assert_eq!(permitted.1,269);
        assert_eq!(permitted.2,7993);
        assert_eq!(permitted.3,"explicit-user-approved-one-import-face-loss");

        let later_loss=json!({"sourceUntouched":true,"trianglesBefore":7994,
            "trianglesAfter":7993,"uv":{"oldUVIslands":254,"newUVIslands":269}});
        assert!(verify_repair_report_mode(&later_loss,&audited,path,true,true)
            .unwrap_err().contains("actually changed triangle count DURING UV repair"));
    }
    #[test]fn review_only_asset_cannot_be_exported_as_game_ready() {
        let folder=std::env::temp_dir().join(format!("uv_review_block_{}",Uuid::new_v4()));
        fs::create_dir_all(&folder).unwrap();
        let asset_path=folder.join("original.glb");
        fs::write(&asset_path,b"only manifest evaluated").unwrap();
        let manifest=folder.join("manifest.json");
        fs::write(&manifest,serde_json::to_vec(&json!({"provenance":{
            "reviewOnly":true,"qualityGatePassed":false
        }})).unwrap()).unwrap();
        assert!(ensure_unity_exportable(&asset_path).unwrap_err()
            .contains("QA FAIL"));
        fs::write(&manifest,serde_json::to_vec(&json!({"provenance":{
            "reviewOnly":false,"qualityGatePassed":true
        }})).unwrap()).unwrap();
        assert!(ensure_unity_exportable(&asset_path).is_ok());
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]fn triangle_acceptance_distinguishes_pre_import_from_in_repair_loss() {
        let path=Path::new("example/uv_repairs/uuid");
        let fox=json!({"sourceUntouched":true,"trianglesBefore":7993,
            "trianglesAfter":7993,"uv":{"oldUVIslands":254,"newUVIslands":116}});
        let no_proof=SourceTriangles{
            declared:7994,zero_area:0,duplicate_faces:0,effective:7994
        };
        let err=verify_repair_report(&fox,&no_proof,path,false).unwrap_err();
        assert!(err.contains("Kaynak indeks üçgeni=7994"));
        assert!(err.contains("yinelenen aynı yüz=0"));
        assert!(!err.contains("actually changed triangle count DURING UV repair"));
        // Human explicitly reviewed the visually intact one-face import loss;
        // this permission applies ONLY to a resume call and is permanently
        // recorded in the repair manifest.
        assert_eq!(
            verify_repair_report(&fox,&no_proof,path,true).unwrap(),
            (254,116,7993,"explicit-user-approved-one-import-face-loss")
        );
        let duplicate=SourceTriangles{
            declared:7994,zero_area:0,duplicate_faces:1,effective:7993
        };
        assert_eq!(
            verify_repair_report(&fox,&duplicate,path,false).unwrap(),
            (254,116,7993,"audited-redundant-source-face")
        );
        let zero=SourceTriangles{
            declared:7994,zero_area:1,duplicate_faces:0,effective:7993
        };
        assert_eq!(
            verify_repair_report(&fox,&zero,path,false).unwrap().2,7993
        );
        let actual_loss=json!({"sourceUntouched":true,"trianglesBefore":7994,
            "trianglesAfter":7993,"uv":{"oldUVIslands":254,"newUVIslands":116}});
        assert!(verify_repair_report(&actual_loss,&no_proof,path,true)
            .unwrap_err().contains("actually changed triangle count DURING UV repair"));
        let two_lost=json!({"sourceUntouched":true,"trianglesBefore":7992,
            "trianglesAfter":7992,"uv":{"oldUVIslands":254,"newUVIslands":116}});
        assert!(verify_repair_report(&two_lost,&no_proof,path,true).is_err());
        let invalid_uv=json!({"sourceUntouched":true,"trianglesBefore":7993,
            "trianglesAfter":7993,"uv":{"oldUVIslands":116,"newUVIslands":254}});
        assert!(verify_repair_report(&invalid_uv,&no_proof,path,true)
            .unwrap_err().contains("greater than source count"));
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
