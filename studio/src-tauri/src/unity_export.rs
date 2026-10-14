//! Unity URP staging export. No source model, LOD or prior texture export is edited.
//! Compacts each source LOD GLB to geometry-only GLB and writes ONE shared URP
//! material set. Unity importer / prefab creation is deliberately a later step.
use crate::storage::{StudioResult, StudioState};
use crate::texture_export::{owned_asset_dir, verify_sha256};
use chrono::Utc;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap,BTreeSet}, fs, io::Cursor, path::{Path,PathBuf}};
use tauri::State;
use uuid::Uuid;

const MAX_GLB: usize = 150_000_000;
const MAX_MAP_BYTES: u64 = 32_000_000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct GameExportResult {
    pub output_dir: String,
    pub manifest_path: String,
    pub lod_triangles: [u64; 3],
    pub source_lod_bytes: u64,
    pub geometry_lod_bytes: u64,
    pub shared_texture_bytes: u64,
    pub source_untouched: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct GameExportSummary {
    pub output_dir: String,
    pub generated_at: String,
    pub triangles: [u64; 3],
}

fn get_obj<'a>(v: &'a Value, key: &str) -> StudioResult<&'a serde_json::Map<String, Value>> {
    v.get(key).and_then(Value::as_object).ok_or_else(|| format!("GLB alanı geçersiz: {key}"))
}
fn get_arr<'a>(v: &'a Value, key: &str) -> StudioResult<&'a Vec<Value>> {
    v.get(key).and_then(Value::as_array).ok_or_else(|| format!("GLB alanı geçersiz: {key}"))
}
fn u64_at(v:&Value, key:&str) -> StudioResult<u64> {
    v.get(key).and_then(Value::as_u64).ok_or_else(|| format!("GLB sayısal alanı geçersiz: {key}"))
}

fn parse_glb(src:&[u8]) -> StudioResult<(Value,&[u8])> {
    if src.len()<28 || src.len()>MAX_GLB || &src[..4]!=b"glTF" {return Err("GLB 2.0 dosyası geçersiz".into());}
    if u32::from_le_bytes(src[4..8].try_into().unwrap())!=2 || u32::from_le_bytes(src[8..12].try_into().unwrap()) as usize!=src.len() {
        return Err("GLB sürümü veya toplam boyutu geçersiz".into());
    }
    let mut offset=12usize;
    let mut json_data=None;
    let mut binary=None;
    while offset<src.len() {
        if src.len()-offset<8 {return Err("GLB chunk başlığı eksik".into());}
        let size=u32::from_le_bytes(src[offset..offset+4].try_into().unwrap()) as usize;
        let tag=&src[offset+4..offset+8];
        offset+=8;
        if size%4!=0 || size>src.len()-offset {return Err("GLB chunk sınırı geçersiz".into());}
        if tag==b"JSON" && json_data.is_none() {json_data=Some(&src[offset..offset+size]);}
        if tag==b"BIN\0" && binary.is_none() {binary=Some(&src[offset..offset+size]);}
        offset+=size;
    }
    let value:Value=serde_json::from_slice(json_data.ok_or("GLB JSON chunk yok")?).map_err(|e|e.to_string())?;
    if !value.is_object(){return Err("GLB JSON kökü geçersiz".into());}
    Ok((value,binary.ok_or("GLB binary chunk yok")?))
}
fn push_chunk(out:&mut Vec<u8>, chunk:&[u8], kind:&[u8;4], padding:u8) -> StudioResult<()> {
    let size=(chunk.len()+3)&!3;
    if size>u32::MAX as usize {return Err("GLB chunk çok büyük".into());}
    out.extend_from_slice(&(size as u32).to_le_bytes());out.extend_from_slice(kind);
    out.extend_from_slice(chunk);out.resize(out.len()+(size-chunk.len()),padding);
    Ok(())
}

/// Keeps all accessor-backed geometry (POSITION, indices, UV, normal, tangents).
/// Image bufferViews are eliminated; no material or image data is copied.
/// Unsupported compression / animations / skins are rejected, never silently damaged.
pub fn geometry_only_glb(src:&[u8]) -> StudioResult<(Vec<u8>,u64)> {
    let (mut root,bin)=parse_glb(src)?;
    if root.get("extensionsUsed").and_then(Value::as_array).is_some_and(|x| !x.is_empty()) ||
       root.get("extensionsRequired").and_then(Value::as_array).is_some_and(|x| !x.is_empty()) ||
       root.get("animations").is_some() || root.get("skins").is_some() {
        return Err("Bu GLB animasyon/skin veya desteklenmeyen uzantı içeriyor; güvenli game export durduruldu".into());
    }
    let meshes=get_arr(&root,"meshes")?;
    if meshes.is_empty(){return Err("GLB mesh içermiyor".into());}
    let accessors=get_arr(&root,"accessors")?.clone();
    let old_views=get_arr(&root,"bufferViews")?.clone();
    let buffers=get_arr(&root,"buffers")?;
    if buffers.len()!=1 {return Err("Yalnızca tek gömülü BIN buffer destekleniyor".into());}
    let mut triangles=0u64;
    for mesh in meshes {
        let primitives=get_arr(mesh,"primitives")?;
        if primitives.is_empty(){return Err("Boş mesh primitive".into());}
        for primitive in primitives {
            if primitive.get("mode").and_then(Value::as_u64).unwrap_or(4)!=4 {return Err("Triangle olmayan primitive desteklenmiyor".into());}
            let attrs=get_obj(primitive,"attributes")?;
            if !attrs.contains_key("POSITION") || !attrs.contains_key("TEXCOORD_0") || !attrs.contains_key("NORMAL") {
                return Err("Unity LOD için POSITION, UV0 ve NORMAL gereklidir".into());
            }
            for ix in attrs.values() {
                if ix.as_u64().is_none_or(|i|i as usize>=accessors.len()) {return Err("Geçersiz primitive accessor".into());}
            }
            let index=u64_at(primitive,"indices")? as usize;
            let indices=accessors.get(index).ok_or("GLB index accessor eksik")?;
            if u64_at(indices,"count")?%3!=0 {return Err("Triangle index sayısı üçe bölünmüyor".into());}
            triangles+=u64_at(indices,"count")?/3;
        }
    }
    if triangles==0 {return Err("GLB triangle sayısı sıfır".into());}
    let mut used=BTreeSet::new();
    for a in &accessors {
        if a.get("sparse").is_some() {return Err("Sparse accessor henüz desteklenmiyor".into());}
        let old=u64_at(a,"bufferView")? as usize;
        if old>=old_views.len(){return Err("GLB accessor bufferView indeks hatası".into());}
        used.insert(old);
    }
    let mut mapping=BTreeMap::new();
    let mut new_views=Vec::with_capacity(used.len());
    let mut new_bin=Vec::new();
    for old in used {
        let mut view=old_views[old].clone();
        if u64_at(&view,"buffer")?!=0 {return Err("Harici buffer desteklenmiyor".into());}
        let start=view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let len=u64_at(&view,"byteLength")? as usize;
        if len==0 || start>bin.len() || len>bin.len()-start {return Err("BufferView sınırı geçersiz".into());}
        while new_bin.len()%4!=0 {new_bin.push(0);}
        let new_offset=new_bin.len();
        new_bin.extend_from_slice(&bin[start..start+len]);
        view.as_object_mut().ok_or("Geçersiz bufferView")?.insert("byteOffset".into(),json!(new_offset));
        mapping.insert(old,new_views.len());new_views.push(view);
    }
    for accessor in root["accessors"].as_array_mut().ok_or("GLB accessor dizisi eksik")? {
        let old=u64_at(accessor,"bufferView")? as usize;
        accessor.as_object_mut().ok_or("Accessor nesne değil")?.insert("bufferView".into(),json!(mapping[&old]));
    }
    root["bufferViews"]=json!(new_views);
    while new_bin.len()%4!=0 {new_bin.push(0);}
    root["buffers"]=json!([{"byteLength":new_bin.len()}]);
    for mesh in root["meshes"].as_array_mut().ok_or("GLB meshes dizisi eksik")? {
        for primitive in mesh["primitives"].as_array_mut().ok_or("GLB primitive dizisi eksik")? {
            primitive.as_object_mut().ok_or("GLB primitive nesnesi eksik")?.remove("material");
        }
    }
    let obj=root.as_object_mut().ok_or("GLB kökü geçersiz")?;
    for name in ["materials","textures","images","samplers"] {obj.remove(name);}
    let json_bytes=serde_json::to_vec(&root).map_err(|e|e.to_string())?;
    let reserve=12+8+((json_bytes.len()+3)&!3)+8+new_bin.len();
    if reserve>u32::MAX as usize {return Err("Game export GLB çok büyük".into());}
    let mut out=Vec::with_capacity(reserve);
    out.extend_from_slice(b"glTF");out.extend_from_slice(&2u32.to_le_bytes());out.extend_from_slice(&(reserve as u32).to_le_bytes());
    push_chunk(&mut out,&json_bytes,b"JSON",b' ')?;
    push_chunk(&mut out,&new_bin,b"BIN\0",0)?;
    if out.len()!=reserve{return Err("Game export GLB uzunluğu tutarsız".into());}
    Ok((out,triangles))
}

fn png_map(dir:&Path, manifest:&Value, name:&str) -> StudioResult<RgbaImage> {
    let entries=get_arr(manifest,"maps")?;
    let row=entries.iter().find(|v|v.get("filename").and_then(Value::as_str)==Some(name))
        .ok_or_else(||format!("Texture Export içindeki harita eksik: {name}"))?;
    let expected=row.get("sha256").and_then(Value::as_str).ok_or("Texture SHA256 eksik")?;
    if !name.starts_with("m0_") || !name.ends_with(".png") || name.contains('/') || name.contains('\\') {
        return Err("PBR dosya adı geçersiz".into());
    }
    let file=dir.join(name);
    let canon=file.canonicalize().map_err(|e|e.to_string())?;
    if canon.parent()!=Some(dir){return Err("PBR harita arşiv klasörü dışına çıkıyor".into());}
    if fs::metadata(&canon).map_err(|e|e.to_string())?.len()>MAX_MAP_BYTES {return Err("Texture dosyası çok büyük".into());}
    verify_sha256(&canon,expected)?;
    let bytes=fs::read(&canon).map_err(|e|e.to_string())?;
    let image=image::load_from_memory_with_format(&bytes,ImageFormat::Png)
        .map_err(|e|format!("Geçersiz PBR PNG {name}: {e}"))?.to_rgba8();
    if image.width()==0 || image.height()==0 || image.width()>4096 || image.height()>4096 {return Err("PBR boyutu geçersiz".into());}
    if image.width() as u64*image.height() as u64>16_000_000 {return Err("PBR piksel limiti aşıldı".into());}
    let w=u64_at(row,"width")?;let h=u64_at(row,"height")?;
    if w!=image.width() as u64 || h!=image.height() as u64 {return Err(format!("PBR manifest boyutu uyuşmuyor: {name}"));}
    Ok(image)
}

/// Unity URP Lit packing: metallic in R, smoothness (1 - roughness) in A;
/// occlusion is stored in G of a separate texture. glTF +Y normal is copied
/// unmodified; importer must set Normal Map, orientation should be checked.
fn pack_urp(metal:&RgbaImage, rough:&RgbaImage, ao:&RgbaImage, metallic_factor:f32, roughness_factor:f32) -> StudioResult<(RgbaImage,RgbaImage)> {
    if !metallic_factor.is_finite() || !roughness_factor.is_finite() || metallic_factor<0.0 || metallic_factor>1.0 || roughness_factor<0.0 || roughness_factor>1.0 {
        return Err("Geçersiz glTF metallic/roughness faktörü".into());
    }
    let dims=metal.dimensions();
    if dims!=rough.dimensions() || dims!=ao.dimensions() {return Err("Packed mask kaynaklarının boyutları eşit olmalı; ortak 512/1024 preset ile yeniden export et".into());}
    let mut metal_smooth=RgbaImage::new(dims.0,dims.1);
    let mut occlusion=RgbaImage::new(dims.0,dims.1);
    for (x,y,p) in metal.enumerate_pixels() {
        let m=(p[0] as f32*metallic_factor).round() as u8;
        let r=(rough.get_pixel(x,y)[0] as f32*roughness_factor).round() as u8;
        let a=ao.get_pixel(x,y)[0];
        metal_smooth.put_pixel(x,y,Rgba([m,0,0,255-r]));
        occlusion.put_pixel(x,y,Rgba([255,a,255,255]));
    }
    Ok((metal_smooth,occlusion))
}
fn png_bytes(img:&RgbaImage) -> StudioResult<Vec<u8>> {
    let mut target=Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img.clone()).write_to(&mut target,ImageFormat::Png)
        .map_err(|e|e.to_string())?;
    Ok(target.into_inner())
}
fn sum_sha(bytes:&[u8]) -> String {format!("{:x}",Sha256::digest(bytes))}
fn read_json(path:&Path) -> StudioResult<Value> {
    let raw=fs::read(path).map_err(|e|e.to_string())?;
    if raw.len()>512_000 {return Err("Manifest boyutu çok büyük".into());}
    serde_json::from_slice(&raw).map_err(|e|e.to_string())
}
fn existing_lod_dir(asset_dir:&Path, asset_id:&str) -> StudioResult<PathBuf> {
    let root=asset_dir.join("lods").canonicalize().map_err(|_|"Bu asset için LOD üretimi henüz yapılmamış")?;
    let latest=read_json(&root.join("latest.json"))?;
    if latest.get("assetId").and_then(Value::as_str)!=Some(asset_id) {return Err("LOD asset kimliği uyuşmuyor".into());}
    let raw=latest.get("lodDirectory").and_then(Value::as_str).ok_or("LOD rapor klasörü bulunamadı")?;
    let dir=PathBuf::from(raw).canonicalize().map_err(|e|e.to_string())?;
    if dir.parent()!=Some(root.as_path()) {return Err("LOD yolu arşiv dışına çıkıyor".into());}
    if !dir.join("Blender_LOD_Report.json").is_file(){return Err("Blender LOD raporu bulunamadı".into());}
    Ok(dir)
}
fn existing_texture_dir(asset_dir:&Path, asset_id:&str, original_sha:&str, supplied:&str) -> StudioResult<(PathBuf,Value)> {
    let exports=asset_dir.join("texture_exports").canonicalize().map_err(|_|"Önce Texture Export oluştur")?;
    let p=PathBuf::from(supplied).canonicalize().map_err(|_|"Seçili texture export klasörü bulunamadı")?;
    if p.parent()!=Some(exports.as_path()) || !p.file_name().and_then(|x|x.to_str()).is_some_and(|s|s.starts_with("pbr_")) {
        return Err("Sadece bu asset'in kendi texture export sürümleri kullanılabilir".into());
    }
    let manifest=read_json(&p.join("texture_export_manifest.json"))?;
    if manifest.get("assetId").and_then(Value::as_str)!=Some(asset_id) ||
       manifest.get("sourceGlbSha256").and_then(Value::as_str)!=Some(original_sha) {
        return Err("Texture export kimliği veya kaynak hash uyuşmuyor".into());
    }
    Ok((p,manifest))
}

fn build_pack(asset_dir:&Path, asset_id:&str, asset_title:&str, original_sha:&str, texture_dir:&str) -> StudioResult<GameExportResult> {
    let lod_root=existing_lod_dir(asset_dir,asset_id)?;
    let (map_root,map_manifest)=existing_texture_dir(asset_dir,asset_id,original_sha,texture_dir)?;
    // First version only allows one material. It must never silently lose materials.
    let original=fs::read(asset_dir.join("original.glb")).map_err(|e|e.to_string())?;
    let (header,_) = parse_glb(&original)?;
    if header.get("materials").and_then(Value::as_array).map(|a|a.len()) != Some(1) {
        return Err("Unity paketlemenin ilk sürümü tek materyalli GLB destekler; çoklu materyali atlamıyoruz".into());
    }
    let material=&header["materials"][0];
    if material.get("emissiveTexture").is_some() || material.get("extensions").is_some() ||
       material.get("alphaMode").and_then(Value::as_str).is_some_and(|m|m!="OPAQUE") {
        return Err("Bu materyalde henüz desteklenmeyen emission, extension veya transparency var; export eksik malzeme üretmek yerine durduruldu".into());
    }
    let metallic_factor=material.pointer("/pbrMetallicRoughness/metallicFactor").and_then(Value::as_f64).unwrap_or(1.0) as f32;
    let roughness_factor=material.pointer("/pbrMetallicRoughness/roughnessFactor").and_then(Value::as_f64).unwrap_or(1.0) as f32;
    let base_factor=material.pointer("/pbrMetallicRoughness/baseColorFactor").cloned().unwrap_or(json!([1.0,1.0,1.0,1.0]));
    let normal_scale=material.pointer("/normalTexture/scale").and_then(Value::as_f64).unwrap_or(1.0);
    let occlusion_strength=material.pointer("/occlusionTexture/strength").and_then(Value::as_f64).unwrap_or(1.0);
    let base=png_map(&map_root,&map_manifest,"m0_baseColor.png")?;
    let normal=png_map(&map_root,&map_manifest,"m0_normal.png")?;
    let metal=png_map(&map_root,&map_manifest,"m0_metallic.png")?;
    let rough=png_map(&map_root,&map_manifest,"m0_roughness.png")?;
    let ao=png_map(&map_root,&map_manifest,"m0_occlusion.png")?;
    let (mask,occ)=pack_urp(&metal,&rough,&ao, metallic_factor, roughness_factor)?;
    let textures=[("BaseColor.png",png_bytes(&base)?),("Normal.png",png_bytes(&normal)?),
        ("MetallicSmoothness.png",png_bytes(&mask)?),("Occlusion.png",png_bytes(&occ)?)];
    let mut source_lod_bytes=0u64;let mut geometry_lod_bytes=0u64;
    let mut triangles=[0u64;3];let mut geometry_files=Vec::new();
    let lod_report=read_json(&lod_root.join("Blender_LOD_Report.json"))?;
    for i in 0..3 {
        let rel=lod_report.pointer(&format!("/lods/{i}/file")).and_then(Value::as_str)
            .ok_or_else(||format!("LOD{i} rapor dosya adı eksik"))?;
        let safe=crate::blender::validate_lod_filename(rel)?;
        let src_path=lod_root.join(safe);
        let src=fs::read(&src_path).map_err(|e|format!("LOD{i} dosyası eksik: {e}"))?;
        let (geometry,tris)=geometry_only_glb(&src)?;
        source_lod_bytes+=src.len() as u64;geometry_lod_bytes+=geometry.len() as u64;triangles[i]=tris;
        geometry_files.push((format!("LOD{i}.glb"),geometry));
    }
    if triangles[0]<triangles[1] || triangles[1]<triangles[2] || triangles[2]==0 {
        return Err("LOD triangle sayıları LOD0>=LOD1>=LOD2 olmalı".into());
    }
    let output_root=asset_dir.join("unity_exports");
    fs::create_dir_all(&output_root).map_err(|e|e.to_string())?;
    let output_dir=output_root.join(format!("urp_{}_{}",Utc::now().format("%Y%m%d_%H%M%S"),Uuid::new_v4()));
    fs::create_dir(&output_dir).map_err(|e|e.to_string())?;
    let result=(||->StudioResult<GameExportResult>{
        let meshes=output_dir.join("Meshes");let maps=output_dir.join("Textures");
        fs::create_dir(&meshes).map_err(|e|e.to_string())?;
        fs::create_dir(&maps).map_err(|e|e.to_string())?;
        let mut manifest_files=Vec::new();
        for (name,data) in &geometry_files {
            fs::write(meshes.join(name),data).map_err(|e|e.to_string())?;
            manifest_files.push(json!({"path":format!("Meshes/{name}"),"sha256":sum_sha(data),"bytes":data.len()}));
        }
        let mut shared_texture_bytes=0u64;
        for (name,data) in &textures {
            fs::write(maps.join(name),data).map_err(|e|e.to_string())?;
            shared_texture_bytes+=data.len() as u64;
            manifest_files.push(json!({"path":format!("Textures/{name}"),"sha256":sum_sha(data),"bytes":data.len()}));
        }
        let now=Utc::now().to_rfc3339();
        let json_data=json!({
            "formatVersion":1,"type":"mallfunction-ai3d-unity-urp-staging", "assetId":asset_id,
            "assetTitle":asset_title,"createdAt":now,"sourceGlbSha256":original_sha,
            "sourceUntouched":true,"lodTriangles":triangles,"sourceLodBytes":source_lod_bytes,
            "geometryLodBytes":geometry_lod_bytes,"sharedTextureBytes":shared_texture_bytes,
            "sourceTexturePreset":map_manifest.get("preset").cloned().unwrap_or(Value::Null),
            "files":manifest_files,"materialCount":1,"textureMaps":{
                "_BaseMap":"Textures/BaseColor.png",
                "_BumpMap":"Textures/Normal.png",
                "_MetallicGlossMap":"Textures/MetallicSmoothness.png",
                "_OcclusionMap":"Textures/Occlusion.png"
            },
            "channelRules":{"MetallicSmoothness.R":"metallic","MetallicSmoothness.A":"1 - roughness",
                "Occlusion.G":"ambient occlusion","Normal":"glTF OpenGL +Y unchanged"},
            "unitySettings":{"renderPipeline":"URP","shader":"Universal Render Pipeline/Lit",
                "metallic":1.0,"smoothnessTextureChannel":"Metallic Alpha","occlusionStrength":1.0,
                "normalTextureType":"Normal Map","normalGreenInverted":false,
                "baseColorFactor":base_factor,"normalScale":normal_scale,"occlusionStrength":occlusion_strength,
                "lodTransitionHeights":[0.60,0.28,0.08]},
            "status":"STAGING_ONLY_NOT_IMPORTED_IN_UNITY"
        });
        let manifest_path=output_dir.join("unity_asset_manifest.json");
        fs::write(&manifest_path,serde_json::to_vec_pretty(&json_data).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        fs::write(output_dir.join("UNITY_URP_SETUP.txt"),
            "Mallfunction AI3D - Unity URP LOD staging\n\n1. Import the Meshes/*.glb via a glTF importer such as glTFast (Unity does not import .glb directly by default).\n2. Create ONE URP/Lit material and assign the same material to all LOD renderers.\n3. BaseMap -> Textures/BaseColor.png (sRGB); Normal -> Textures/Normal.png (Texture Type: Normal map; validate +/-Y orientation); Metallic -> Textures/MetallicSmoothness.png (R=Metal, A=Smoothness); Occlusion -> Textures/Occlusion.png (G=AO, enable occlusion).\n4. Place LOD0, LOD1, LOD2 in identical local transforms; create LODGroup (suggested relative heights 0.60/0.28/0.08; tune in Unity scene). Do not render all LODs simultaneously.\n5. Add one simple collider and assign your item definition; inspect normal and seams at real shelf camera distance.\n6. This is a portable import staging folder, NOT a validated Unity prefab. Source GLBs and exported PBR maps were not modified.\n")
            .map_err(|e|e.to_string())?;
        Ok(GameExportResult {output_dir:output_dir.to_string_lossy().into_owned(),
            manifest_path:manifest_path.to_string_lossy().into_owned(),lod_triangles:triangles,
            source_lod_bytes,geometry_lod_bytes,shared_texture_bytes,source_untouched:true})
    })();
    if result.is_err(){let _=fs::remove_dir_all(&output_dir);}
    result
}

#[tauri::command]
pub async fn export_unity_urp(asset_id:String, texture_export_dir:String,
    state:State<'_,StudioState>) -> StudioResult<GameExportResult> {
    let asset=state.asset_by_id(&asset_id)?;
    let owned=owned_asset_dir(&asset.file_path,&asset_id)?;
    verify_sha256(Path::new(&asset.file_path),&asset.sha256)?;
    tauri::async_runtime::spawn_blocking(move ||
        build_pack(&owned,&asset_id,&asset.title,&asset.sha256,&texture_export_dir)
    ).await.map_err(|e|format!("Unity export worker hatası: {e}"))?
}

#[tauri::command]
pub fn list_unity_exports(asset_id:String,state:State<'_,StudioState>) -> StudioResult<Vec<GameExportSummary>> {
    let asset=state.asset_by_id(&asset_id)?;
    let dir=owned_asset_dir(&asset.file_path,&asset_id)?.join("unity_exports");
    if !dir.exists(){return Ok(Vec::new());}
    let mut result=Vec::new();
    for entry in fs::read_dir(dir).map_err(|e|e.to_string())? {
        let entry=entry.map_err(|e|e.to_string())?;
        if !entry.file_type().map_err(|e|e.to_string())?.is_dir(){continue;}
        if !entry.file_name().to_string_lossy().starts_with("urp_"){continue;}
        let p=entry.path();
        let manifest=match read_json(&p.join("unity_asset_manifest.json")){Ok(v)=>v,Err(_)=>continue};
        if manifest.get("assetId").and_then(Value::as_str)!=Some(asset_id.as_str()) ||
            manifest.get("sourceGlbSha256").and_then(Value::as_str)!=Some(asset.sha256.as_str()) {continue;}
        let Some(array)=manifest.get("lodTriangles").and_then(Value::as_array) else {continue;};
        if array.len()!=3{continue;}
        let Some(nums)=array.iter().map(Value::as_u64).collect::<Option<Vec<u64>>>() else {continue;};
        result.push(GameExportSummary {output_dir:p.to_string_lossy().into_owned(),
            generated_at:manifest.get("createdAt").and_then(Value::as_str).unwrap_or("").to_owned(),
            triangles:[nums[0],nums[1],nums[2]]});
    }
    result.sort_by(|a,b|b.generated_at.cmp(&a.generated_at));result.truncate(20);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn glb(json_data:Value,bytes:&[u8])->Vec<u8>{
        let mut b=Vec::new();let mut j=serde_json::to_vec(&json_data).unwrap();while j.len()%4!=0 {j.push(b' ');}
        let mut bin=bytes.to_vec();while bin.len()%4!=0 {bin.push(0);}
        let size=12+8+j.len()+8+bin.len();
        b.extend_from_slice(b"glTF");b.extend_from_slice(&2u32.to_le_bytes());b.extend_from_slice(&(size as u32).to_le_bytes());
        push_chunk(&mut b,&j,b"JSON",b' ').unwrap();push_chunk(&mut b,&bin,b"BIN\0",0).unwrap();b
    }
    #[test]fn removes_embedded_images_and_preserves_mesh_attributes(){
        let original=json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":240}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6},
                           {"buffer":0,"byteOffset":42,"byteLength":198}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                         {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":0,"TEXCOORD_0":0},"indices":1,"material":0}]}],
            "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0,
            "images":[{"bufferView":2,"mimeType":"image/png"}],"textures":[{"source":0}],
            "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]});
        let blob=glb(original,&[77;240]);
        let (clean,triangles)=geometry_only_glb(&blob).unwrap();
        assert_eq!(triangles,1);
        let (metadata,bin)=parse_glb(&clean).unwrap();
        assert!(metadata.get("images").is_none());assert!(metadata.get("materials").is_none());
        assert_eq!(metadata["bufferViews"].as_array().unwrap().len(),2);
        assert_eq!(bin.len(),44);
        assert!(clean.len()<blob.len());
        assert!(metadata["meshes"][0]["primitives"][0].get("material").is_none());
    }
    #[test]fn preserves_urp_channels(){
        let metal=RgbaImage::from_pixel(1,1,Rgba([30,30,30,255]));
        let rough=RgbaImage::from_pixel(1,1,Rgba([65,65,65,255]));
        let ao=RgbaImage::from_pixel(1,1,Rgba([142,142,142,255]));
        let (mask,occ)=pack_urp(&metal,&rough,&ao,1.0,1.0).unwrap();
        assert_eq!(mask.get_pixel(0,0).0,[30,0,0,190]);
        assert_eq!(occ.get_pixel(0,0).0,[255,142,255,255]);
        assert!(pack_urp(&metal,&RgbaImage::new(2,2),&ao,1.0,1.0).is_err());
        let (scaled,_)=pack_urp(&metal,&rough,&ao,0.5,0.5).unwrap();
        assert_eq!(scaled.get_pixel(0,0).0,[15,0,0,222]);
    }

    #[test]fn writes_immutable_package_with_owned_sources_only(){
        let base=std::env::temp_dir().join(format!("ai3d_unity_pack_test_{}",Uuid::new_v4()));
        let id=Uuid::new_v4().to_string();
        let asset_dir=base.join("assets").join(&id);
        let lod_folder=asset_dir.join("lods").join("test_v1");
        let texture_folder=asset_dir.join("texture_exports").join("pbr_v1");
        fs::create_dir_all(&lod_folder).unwrap();
        fs::create_dir_all(&texture_folder).unwrap();
        let source_obj=json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":240}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},
                           {"buffer":0,"byteOffset":36,"byteLength":6},
                           {"buffer":0,"byteOffset":44,"byteLength":36},
                           {"buffer":0,"byteOffset":80,"byteLength":24},
                           {"buffer":0,"byteOffset":104,"byteLength":136}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                         {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"},
                         {"bufferView":2,"componentType":5126,"count":3,"type":"VEC3"},
                         {"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":2,"TEXCOORD_0":3},"indices":1,"material":0}]}],
            "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0,
            "images":[{"bufferView":4,"mimeType":"image/png"}],"textures":[{"source":0}],
            "materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0},
                "metallicFactor":1.0,"roughnessFactor":1.0}}]});
        let original=glb(source_obj,&[0;240]);
        let sha=sum_sha(&original);
        fs::write(asset_dir.join("original.glb"),&original).unwrap();
        for i in 0..3 {fs::write(lod_folder.join(format!("PLAYBOX_FoxPlush_LOD{i}.glb")),&original).unwrap();}
        let report=json!({"lods":[{"file":"PLAYBOX_FoxPlush_LOD0.glb"},{"file":"PLAYBOX_FoxPlush_LOD1.glb"},{"file":"PLAYBOX_FoxPlush_LOD2.glb"}]});
        fs::write(lod_folder.join("Blender_LOD_Report.json"),serde_json::to_vec(&report).unwrap()).unwrap();
        let lod=json!({"assetId":id,"lodDirectory":lod_folder});
        fs::write(asset_dir.join("lods").join("latest.json"),serde_json::to_vec(&lod).unwrap()).unwrap();
        let mut metadata=Vec::new();
        for (role,value) in [("baseColor",200),("normal",128),("metallic",30),("roughness",65),("occlusion",142)] {
            let name=format!("m0_{role}.png");
            let png=png_bytes(&RgbaImage::from_pixel(2,2,Rgba([value,value,value,255]))).unwrap();
            fs::write(texture_folder.join(&name),&png).unwrap();
            metadata.push(json!({"filename":name,"width":2,"height":2,"sha256":sum_sha(&png)}));
        }
        let manifest=json!({"assetId":id,"sourceGlbSha256":sha,"preset":"512","maps":metadata});
        fs::write(texture_folder.join("texture_export_manifest.json"),serde_json::to_vec(&manifest).unwrap()).unwrap();
        let folder_string=texture_folder.to_string_lossy().into_owned();
        let result=build_pack(&asset_dir,&id,"Fox Plush",&sha,&folder_string).unwrap();
        assert_eq!(result.lod_triangles,[1,1,1]);
        assert_eq!(result.source_untouched,true);
        assert!(result.geometry_lod_bytes<result.source_lod_bytes);
        assert_eq!(fs::read(asset_dir.join("original.glb")).unwrap(),original);
        let dir=Path::new(&result.output_dir);
        assert!(dir.join("Textures/MetallicSmoothness.png").is_file());
        assert!(dir.join("Textures/Occlusion.png").is_file());
        for i in 0..3 {
            let lod_bytes=fs::read(lod_folder.join(format!("PLAYBOX_FoxPlush_LOD{i}.glb"))).unwrap();
            let (g,t)=geometry_only_glb(&lod_bytes).unwrap();
            assert_eq!(t,1);
            assert_eq!(fs::read(dir.join(format!("Meshes/LOD{i}.glb"))).unwrap(),g);
        }
        let bad=base.join("alien");fs::create_dir_all(&bad).unwrap();
        assert!(existing_texture_dir(&asset_dir,&id,&sha,bad.to_str().unwrap()).is_err());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]fn rejects_bad_or_unsupported_glb(){
        assert!(geometry_only_glb(b"garbage").is_err());
        let obj=json!({"asset":{"version":"2.0"},"extensionsUsed":["EXT_meshopt_compression"],
            "buffers":[{"byteLength":4}],"bufferViews":[],"accessors":[],"meshes":[]});
        assert!(geometry_only_glb(&glb(obj,&[0;4])).is_err());
    }
}
