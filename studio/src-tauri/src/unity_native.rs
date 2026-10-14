//! Unity-native FBX/URP/HDRP pipeline. Source GLB, Blender LODs and PBR
//! export versions are read-only. An explicit user-selected Unity project is
//! the only writable target, under Assets/MallfunctionAI3D/Imported/<id>/<version>.
use crate::blender::{find_blender, validate_lod_filename};
use crate::storage::{StudioResult, StudioState};
use crate::texture_export::{owned_asset_dir, verify_sha256};
use crate::unity_export::{
    existing_lod_dir, existing_texture_dir, geometry_only_glb, parse_glb,
    png_bytes, png_map, read_json, sum_sha,
};
use chrono::Utc;
use image::{Rgba, RgbaImage};
use serde::Serialize;
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}, process::Command};
use tauri::State;
use uuid::Uuid;

const FBX_WORKER: &str = include_str!("../../../blender/export_unity_fbx.py");
const UNITY_IMPORTER: &str = include_str!("../../../unity/Editor/MallfunctionAI3DImporter.cs");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnityProjectInfo {
    pub project_path: String,
    pub editor_version: String,
    pub pipeline: String,
    pub urp_installed: bool,
    pub hdrp_installed: bool,
    // Only Unity's actual GraphicsSettings/Quality pipeline can prove this.
    pub active_pipeline_verified: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeExportResult {
    pub output_dir: String,
    pub unity_folder: String,
    pub manifest_path: String,
    pub pipeline: String,
    pub lod_triangles: [u64; 3],
    pub status: String,
    pub source_untouched: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnityImportStatus {
    pub status: String,
    pub prefab_path: Option<String>,
    pub message: Option<String>,
}

fn detect_project(raw: &str) -> StudioResult<UnityProjectInfo> {
    let input = Path::new(raw);
    if raw.trim().is_empty() || !input.is_absolute() {
        return Err("Unity proje kökünü tam dosya yolu olarak seç.".into());
    }
    let root = input.canonicalize().map_err(|e| format!("Unity proje klasörü bulunamadı: {e}"))?;
    let assets = root.join("Assets").canonicalize()
        .map_err(|_| "Unity projesinde Assets klasörü bulunamadı")?;
    if assets.parent() != Some(root.as_path()) {
        return Err("Assets klasörü dışarıya yönlendirilmiş; güvenli Unity export reddedildi".into());
    }
    let version_content = fs::read_to_string(root.join("ProjectSettings/ProjectVersion.txt"))
        .map_err(|_| "ProjectSettings/ProjectVersion.txt bulunamadı")?;
    let version = version_content.lines().find_map(|line| line.strip_prefix("m_EditorVersion: "))
        .ok_or("Unity editor sürümü okunamadı")?.trim().to_string();
    if version.split('.').next().and_then(|x| x.parse::<u32>().ok()).is_none_or(|v| v < 2022) {
        return Err("Unity 2022.3+ / Unity 6 bekleniyor".into());
    }
    let packages = read_json(&root.join("Packages/manifest.json"))?;
    let dependencies = packages.get("dependencies").and_then(Value::as_object)
        .ok_or("Unity Packages manifest.json dependencies eksik")?;
    let urp = dependencies.contains_key("com.unity.render-pipelines.universal");
    let hdrp = dependencies.contains_key("com.unity.render-pipelines.high-definition");
    if !urp && !hdrp {
        return Err("Proje URP veya HDRP paketi içermiyor; Built-in pipeline desteklenmiyor".into());
    }
    // Presence of a package is not proof it is the ACTIVE RenderPipelineAsset.
    // The installed Unity Editor importer performs that authoritative check.
    let pipeline = if urp && !hdrp {"URP"} else if hdrp && !urp {"HDRP"} else {"Ambiguous"};
    Ok(UnityProjectInfo {
        project_path: root.to_string_lossy().into_owned(),
        editor_version: version,
        pipeline: pipeline.into(),
        urp_installed: urp,
        hdrp_installed: hdrp,
        active_pipeline_verified: false,
    })
}
#[tauri::command]
pub fn inspect_unity_project(project_path: String) -> StudioResult<UnityProjectInfo> {
    detect_project(&project_path)
}

pub(crate) fn pack_mask(
    metal: &RgbaImage, rough: &RgbaImage, ao: &RgbaImage,
    metallic_factor: f32, roughness_factor: f32,
) -> StudioResult<RgbaImage> {
    if !metallic_factor.is_finite() || !roughness_factor.is_finite() ||
        !(0.0..=1.0).contains(&metallic_factor) || !(0.0..=1.0).contains(&roughness_factor) {
        return Err("GLB material metallic/roughness factor 0..1 olmalı".into());
    }
    let dims = metal.dimensions();
    if dims != rough.dimensions() || dims != ao.dimensions() {
        return Err("Mask girdilerinin çözünürlükleri aynı olmalı; ortak preset ile yeniden Texture Export oluştur".into());
    }
    let mut out = RgbaImage::new(dims.0, dims.1);
    for (x, y, p) in metal.enumerate_pixels() {
        let m = (p[0] as f32 * metallic_factor).round() as u8;
        let rough_scaled = (rough.get_pixel(x, y)[0] as f32 * roughness_factor).round() as u8;
        // URP:  R=Metal G=AO B=unused A=Smoothness
        // HDRP: R=Metal G=AO B=DetailMask(0: no detail) A=Smoothness
        out.put_pixel(x, y, Rgba([m, ao.get_pixel(x, y)[0], 0, 255 - rough_scaled]));
    }
    Ok(out)
}
fn write_new(path: &Path, content: &[u8]) -> StudioResult<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)
        .map_err(|e| format!("Mevcut dosya asla ezilmeyecek ({}): {e}", path.display()))?;
    file.write_all(content).map_err(|e| e.to_string())
}
fn assert_no_links(path: &Path, base: &Path) -> StudioResult<()> {
    let relative = path.strip_prefix(base).map_err(|_| "Unity hedefi Assets klasörü dışında")?;
    let mut current = base.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if let Ok(meta) = fs::symlink_metadata(&current) {
            if meta.file_type().is_symlink() {
                return Err(format!("Unity export symlink/junction yolu reddedildi: {}", current.display()));
            }
        }
    }
    Ok(())
}
fn choose_profile(profile: &str, project: &UnityProjectInfo) -> StudioResult<String> {
    let chosen = if profile == "Auto" {project.pipeline.as_str()} else {profile};
    if chosen != "URP" && chosen != "HDRP" {
        return Err("Render pipeline belirsiz: açıkça URP veya HDRP seç".into());
    }
    if (chosen == "URP" && !project.urp_installed) || (chosen == "HDRP" && !project.hdrp_installed) {
        return Err(format!("{chosen} paketi seçili Unity projesinde kurulu değil"));
    }
    Ok(chosen.into())
}
fn source_material(source: &Path) -> StudioResult<(String, Value, f32, f32, f64, f64)> {
    let raw = fs::read(source).map_err(|e| e.to_string())?;
    geometry_only_glb(&raw)?; // reject skins, extensions and malformed geometry
    let (root, _) = parse_glb(&raw)?;
    let materials = root.get("materials").and_then(Value::as_array).ok_or("GLB material listesi eksik")?;
    if materials.len() != 1 {return Err("Unity-native v1 yalnızca tek statik PBR materyal kabul eder".into());}
    let m = &materials[0];
    if m.get("emissiveTexture").is_some() || m.get("extensions").is_some() ||
        m.get("alphaMode").and_then(Value::as_str).is_some_and(|s| s != "OPAQUE") {
        return Err("Emission/alpha/material extension v1 kapsamında değil".into());
    }
    for pointer in ["/pbrMetallicRoughness/baseColorTexture", "/pbrMetallicRoughness/metallicRoughnessTexture",
        "/normalTexture", "/occlusionTexture"] {
        if m.pointer(pointer).is_none() { return Err(format!("GLB zorunlu PBR slotu eksik: {pointer}")); }
    }
    let metallic_factor = m.pointer("/pbrMetallicRoughness/metallicFactor").and_then(Value::as_f64).unwrap_or(1.0) as f32;
    let roughness_factor = m.pointer("/pbrMetallicRoughness/roughnessFactor").and_then(Value::as_f64).unwrap_or(1.0) as f32;
    let factor = m.pointer("/pbrMetallicRoughness/baseColorFactor")
        .cloned().unwrap_or(json!([1.0, 1.0, 1.0, 1.0]));
    let channels = factor.as_array().ok_or("GLB baseColorFactor geçersiz")?;
    if channels.len() != 4 || channels.iter().any(|x| x.as_f64().is_none_or(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))) {
        return Err("GLB baseColorFactor RGBA 0..1 olmalı".into());
    }
    let normal_scale = m.pointer("/normalTexture/scale").and_then(Value::as_f64).unwrap_or(1.0);
    let ao_strength = m.pointer("/occlusionTexture/strength").and_then(Value::as_f64).unwrap_or(1.0);
    if !normal_scale.is_finite() || !(0.0..=4.0).contains(&normal_scale) ||
        !ao_strength.is_finite() || !(0.0..=1.0).contains(&ao_strength) {
        return Err("Normal scale veya occlusion strength geçersiz".into());
    }
    Ok(("PBR".into(), factor, metallic_factor, roughness_factor, normal_scale, ao_strength))
}

fn native_export(
    asset_dir: PathBuf, asset_id: String, source_sha: String,
    texture_dir: String, unity_project: String, profile: String, blender: String,
) -> StudioResult<NativeExportResult> {
    Uuid::parse_str(&asset_id).map_err(|_| "Asset ID geçerli UUID olmalı")?;
    let project = detect_project(&unity_project)?;
    let pipeline = choose_profile(&profile, &project)?;
    let unity_root = PathBuf::from(&project.project_path);
    let assets = unity_root.join("Assets").canonicalize().map_err(|e|e.to_string())?;
    let original = asset_dir.join("original.glb");
    verify_sha256(&original, &source_sha)?;
    let (_kind, tint, metal_factor, rough_factor, normal_scale, ao_strength) = source_material(&original)?;
    let lod_root = existing_lod_dir(&asset_dir, &asset_id)?;
    let lod_report = read_json(&lod_root.join("Blender_LOD_Report.json"))?;
    let mut lods = Vec::new();
    let mut triangles = [0_u64; 3];
    for (i, count) in triangles.iter_mut().enumerate() {
        let name = lod_report.pointer(&format!("/lods/{i}/file")).and_then(Value::as_str)
            .ok_or_else(|| format!("LOD{i} Blender raporunda dosya yok"))?;
        let path = lod_root.join(validate_lod_filename(name)?);
        if path.canonicalize().map_err(|e|e.to_string())?.parent() != Some(lod_root.as_path()) {
            return Err(format!("LOD{i} arşiv dışına işaret ediyor"));
        }
        let raw = fs::read(&path).map_err(|e|e.to_string())?;
        let (_, tris) = geometry_only_glb(&raw)?;
        *count = tris;
        lods.push(path);
    }
    if triangles[0] < triangles[1] || triangles[1] < triangles[2] || triangles[2] == 0 {
        return Err("LOD üçgen dizilimi geçersiz".into());
    }
    let (map_dir, map_manifest) = existing_texture_dir(&asset_dir, &asset_id, &source_sha, &texture_dir)?;
    let base = png_map(&map_dir, &map_manifest, "m0_baseColor.png")?;
    let normal = png_map(&map_dir, &map_manifest, "m0_normal.png")?;
    let metal = png_map(&map_dir, &map_manifest, "m0_metallic.png")?;
    let rough = png_map(&map_dir, &map_manifest, "m0_roughness.png")?;
    let ao = png_map(&map_dir, &map_manifest, "m0_occlusion.png")?;
    let packed = pack_mask(&metal, &rough, &ao, metal_factor, rough_factor)?;
    let images = vec![
        ("BaseColor.png", png_bytes(&base)?),
        ("Normal.png", png_bytes(&normal)?),
        (if pipeline == "URP" {"URP_Mask.png"} else {"HDRP_Mask.png"}, png_bytes(&packed)?),
    ];
    let run = Uuid::new_v4();
    let version_id = format!("v{}", run.simple());
    let archive_root = asset_dir.join("unity_exports");
    fs::create_dir_all(&archive_root).map_err(|e|e.to_string())?;
    let out = archive_root.join(format!("native_{}_{}_{}", pipeline.to_lowercase(), Utc::now().format("%Y%m%d_%H%M%S"), run));
    fs::create_dir(&out).map_err(|e|e.to_string())?;
    let result = (|| -> StudioResult<NativeExportResult> {
        let meshes = out.join("Meshes");
        let textures = out.join("Textures");
        fs::create_dir(&meshes).map_err(|e|e.to_string())?;
        fs::create_dir(&textures).map_err(|e|e.to_string())?;
        let fbx = meshes.join("Asset_LODs.fbx");
        let report = out.join("Blender_Unity_FBX_Report.json");
        // Script is embedded in the application. Source GLBs remain immutable.
        let script = out.join("export_unity_fbx.py");
        write_new(&script, FBX_WORKER.as_bytes())?;
        let run_blender = Command::new(blender)
            .args(["-b", "--python"]).arg(&script).arg("--")
            .arg("--lod0").arg(&lods[0]).arg("--lod1").arg(&lods[1])
            .arg("--lod2").arg(&lods[2]).arg("--out").arg(&fbx)
            .arg("--report").arg(&report).output()
            .map_err(|e| format!("Blender FBX başlatılamadı: {e}"))?;
        let log = format!("{}\n{}", String::from_utf8_lossy(&run_blender.stdout),
            String::from_utf8_lossy(&run_blender.stderr));
        write_new(&out.join("Blender_Unity_FBX.log"), log.as_bytes())?;
        if !run_blender.status.success() {
            return Err(format!("Blender FBX başarısız: {}", log.chars().rev().take(900).collect::<String>().chars().rev().collect::<String>()));
        }
        let fbx_report = read_json(&report)?;
        if fbx_report.get("axisUp").and_then(Value::as_str) != Some("Y") ||
            fbx_report.get("materialSlots").and_then(Value::as_u64) != Some(0) {
            return Err("FBX Blender raporu axis/material ayarlarını doğrulamadı".into());
        }
        for (i, expected) in triangles.iter().enumerate() {
            let actual = fbx_report.pointer(&format!("/lods/{i}/triangles")).and_then(Value::as_u64);
            let uv = fbx_report.pointer(&format!("/lods/{i}/uvLayers")).and_then(Value::as_u64);
            if actual != Some(*expected) || uv.is_none_or(|x|x==0) {
                return Err(format!("FBX LOD{i} üçgen veya UV0 doğrulaması başarısız"));
            }
        }
        if fs::metadata(&fbx).map_err(|e|e.to_string())?.len() > 150_000_000 {
            return Err("FBX 150 MB üst sınırını aşıyor".into());
        }
        verify_sha256(&original, &source_sha)?;
        let mut files = Vec::new();
        let fbx_data = fs::read(&fbx).map_err(|e|e.to_string())?;
        files.push(json!({"path":"Meshes/Asset_LODs.fbx","sha256":sum_sha(&fbx_data)}));
        for (name, bytes) in &images {
            write_new(&textures.join(name), bytes)?;
            files.push(json!({"path":format!("Textures/{name}"),"sha256":sum_sha(bytes)}));
        }
        let mask_name = images[2].0;
        let manifest = json!({
            "schemaVersion":2, "type":"mallfunction-ai3d-unity-native",
            "assetId":asset_id,"versionId":version_id,"pipeline":pipeline,
            "editorVersion":project.editor_version,
            "sourceGlbSha256":source_sha, "sourceUntouched":true,
            "sourceTexturePreset":map_manifest.get("preset").cloned().unwrap_or(Value::Null),
            "geometry":{"path":"Meshes/Asset_LODs.fbx","lodNames":["LOD0","LOD1","LOD2"],
                "triangles":triangles,"screenHeights":[0.60,0.28,0.08],"collider":"Box"},
            "textures":{"baseColor":"Textures/BaseColor.png","normal":"Textures/Normal.png",
                "packedMask":format!("Textures/{mask_name}")},
            "material":{"baseColorFactor":tint,"normalScale":normal_scale,"occlusionStrength":ao_strength},
            "packedChannels":{"R":"metallic","G":"occlusion","B":if pipeline == "URP" {"unused"} else {"detail mask (no detail: 0)"},"A":"smoothness = 1 - roughness"},
            "linearPackedMask":true, "sharedMaterialAcrossLods":true,
            "files":files, "importStatus":"STAGED_PENDING_UNITY_EDITOR"
        });
        let json_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e|e.to_string())?;
        let archive_manifest = out.join("unity_asset_manifest.json");
        write_new(&archive_manifest, &json_bytes)?;
        // Explicit user-selected project only. Never copy to an inferred game repo.
        let relative = PathBuf::from("MallfunctionAI3D").join("Imported").join(&asset_id).join(&version_id);
        let target = assets.join(&relative);
        assert_no_links(&target, &assets)?;
        if target.exists() {return Err("Unity hedef sürüm klasörü zaten mevcut, üzerine yazılmayacak".into());}
        fs::create_dir_all(&target).map_err(|e|e.to_string())?;
        let copy_result = (|| -> StudioResult<()> {
            fs::create_dir(target.join("Meshes")).map_err(|e|e.to_string())?;
            fs::create_dir(target.join("Textures")).map_err(|e|e.to_string())?;
            fs::copy(&fbx, target.join("Meshes/Asset_LODs.fbx")).map_err(|e|e.to_string())?;
            for (name, _) in &images {
                fs::copy(textures.join(name), target.join("Textures").join(name)).map_err(|e|e.to_string())?;
            }
            write_new(&target.join("unity_asset_manifest.json"), &json_bytes)?;
            // Install importer ONCE; an incompatible existing copy is never overwritten.
            let editor = assets.join("MallfunctionAI3D").join("Editor");
            assert_no_links(&editor, &assets)?;
            fs::create_dir_all(&editor).map_err(|e|e.to_string())?;
            let importer = editor.join("MallfunctionAI3DImporter.cs");
            if importer.is_file() {
                let previous = fs::read(&importer).map_err(|e|e.to_string())?;
                if previous != UNITY_IMPORTER.as_bytes() {
                    return Err("Unity importer mevcut fakat farklı sürümde. Elle doğrulanmadan üzerine yazılmadı".into());
                }
            } else {
                write_new(&importer, UNITY_IMPORTER.as_bytes())?;
            }
            // Save an explicit, immutable project pointer for status polling.
            let pointer = serde_json::to_vec_pretty(&json!({"folder":target,"project":unity_root,
                "assetId":asset_id,"versionId":version_id,"pipeline":pipeline}))
                .map_err(|e|e.to_string())?;
            write_new(&out.join("unity_project_target.json"), &pointer)?;
            // Marker last: Unity auto-importer only sees a complete, immutable payload.
            write_new(&target.join("AI3D_READY.txt"), b"Mallfunction AI3D import request v2\n")?;
            Ok(())
        })();
        if let Err(err) = copy_result {
            let _ = fs::remove_dir_all(&target); // Only newly-created version folder.
            return Err(err);
        }
        Ok(NativeExportResult {
            output_dir:out.to_string_lossy().into_owned(),
            unity_folder:target.to_string_lossy().into_owned(),
            manifest_path:archive_manifest.to_string_lossy().into_owned(),
            pipeline: pipeline.clone(), lod_triangles:triangles,
            status:"STAGED_PENDING_UNITY_EDITOR".into(), source_untouched:true,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&out);
    }
    result
}
#[tauri::command]
pub async fn export_unity_native(
    asset_id: String, texture_export_dir: String, unity_project_path: String, profile: String,
    state: State<'_,StudioState>,
) -> StudioResult<NativeExportResult> {
    let asset = state.asset_by_id(&asset_id)?;
    let archive = owned_asset_dir(&asset.file_path, &asset_id)?;
    verify_sha256(Path::new(&asset.file_path), &asset.sha256)?;
    let blender = find_blender(&state.current_settings()?)
        .ok_or("Blender bulunamadı: Settings > Blender executable yolunu seç")?;
    tauri::async_runtime::spawn_blocking(move ||
        native_export(archive, asset_id, asset.sha256, texture_export_dir,
            unity_project_path, profile, blender)
    ).await.map_err(|e| format!("Unity-native worker hatası: {e}"))?
}
#[tauri::command]
pub fn check_unity_import(asset_id:String, output_dir:String, state:State<'_,StudioState>) -> StudioResult<UnityImportStatus> {
    let asset = state.asset_by_id(&asset_id)?;
    let archive = owned_asset_dir(&asset.file_path, &asset_id)?.join("unity_exports")
        .canonicalize().map_err(|e|e.to_string())?;
    let output = Path::new(&output_dir).canonicalize().map_err(|e|e.to_string())?;
    if output.parent()!=Some(archive.as_path()) || !output.file_name().and_then(|n|n.to_str()).is_some_and(|x| x.starts_with("native_")) {
        return Err("Unity import kontrolü yalnızca bu asset'in kendi native export sürümü için geçerli".into());
    }
    let manifest = read_json(&output.join("unity_asset_manifest.json"))?;
    if manifest.get("assetId").and_then(Value::as_str)!=Some(asset_id.as_str()) ||
        manifest.get("sourceGlbSha256").and_then(Value::as_str)!=Some(asset.sha256.as_str()) {
        return Err("Unity export sürümü ile orijinal GLB hash uyuşmuyor".into());
    }
    // A report can only be read from the explicitly recorded, versioned project path.
    let marker = read_json(&output.join("unity_project_target.json"))?;
    let path = marker.get("folder").and_then(Value::as_str).ok_or("Unity target path missing")?;
    let folder = Path::new(path);
    let canonical_assets = folder.ancestors().find(|p|p.file_name().and_then(|x|x.to_str()) == Some("Assets"))
        .ok_or("Unity target must be below Assets")?;
    if folder.file_name().and_then(|n|n.to_str())!=manifest.get("versionId").and_then(Value::as_str) ||
        folder.parent().and_then(|p|p.file_name()).and_then(|n|n.to_str())!=Some(asset_id.as_str()) ||
        folder.parent().and_then(Path::parent).and_then(|p|p.file_name()).and_then(|n|n.to_str())!=Some("Imported") ||
        !canonical_assets.join("../ProjectSettings/ProjectVersion.txt").is_file() {
        return Err("Unity hedef rapor yolu beklenen asset/version/project yapısında değil".into());
    }
    let report_path = folder.join("unity_import_report.json");
    if !report_path.is_file() {
        return Ok(UnityImportStatus { status:"STAGED_PENDING_UNITY_EDITOR".into(),
            prefab_path:None, message:Some("Unity Editor proje açıldığında importer işleyecek".into()) });
    }
    let result = read_json(&report_path)?;
    if result.get("assetId").and_then(Value::as_str)!=Some(asset_id.as_str()) ||
        result.get("versionId")!=manifest.get("versionId") {
        return Err("Unity importer raporunda asset/version uyuşmuyor".into());
    }
    let status = result.get("status").and_then(Value::as_str).unwrap_or("ImportFailed");
    if status!="UnityPrefabReady" && status!="ImportFailed" {
        return Err("Unity importer raporunda bilinmeyen durum".into());
    }
    if status=="UnityPrefabReady" {
        let expected = manifest.pointer("/geometry/triangles").ok_or("Unity manifest LOD sayıları eksik")?;
        if result.get("lodTriangles")!=Some(expected) ||
            result.get("rendererCount").and_then(Value::as_u64)!=Some(3) ||
            result.get("sharedMaterial").and_then(Value::as_bool)!=Some(true) ||
            result.get("colliderExists").and_then(Value::as_bool)!=Some(true) ||
            result.get("pipeline")!=manifest.get("pipeline") {
            return Err("Unity prefab kabul raporunda materyal/LOD/collider doğrulaması başarısız".into());
        }
        let prefix = format!("Assets/MallfunctionAI3D/Imported/{asset_id}/{}/Prefabs/", 
            manifest.get("versionId").and_then(Value::as_str).unwrap_or(""));
        let prefab = result.get("prefabPath").and_then(Value::as_str).ok_or("Prefab path raporda eksik")?;
        let name = prefab.strip_prefix(&prefix).ok_or("Prefab yolunda beklenmeyen dizin")?;
        if name!=format!("{asset_id}.prefab") || !folder.join("Prefabs").join(name).is_file() {
            return Err("Unity prefab dosyası beklenen sürümde bulunamadı".into());
        }
        verify_sha256(Path::new(&asset.file_path), &asset.sha256)?;
    }
    Ok(UnityImportStatus {status:status.into(),
        prefab_path:result.get("prefabPath").and_then(Value::as_str).map(str::to_owned),
        message:result.get("error").and_then(Value::as_str).map(str::to_owned)})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn channel_pack_and_factors() {
        let m=RgbaImage::from_pixel(1,1,Rgba([77,0,0,255]));
        let r=RgbaImage::from_pixel(1,1,Rgba([64,0,0,255]));
        let a=RgbaImage::from_pixel(1,1,Rgba([204,0,0,255]));
        assert_eq!(pack_mask(&m,&r,&a,1.,1.).unwrap().get_pixel(0,0).0,[77,204,0,191]);
        assert_eq!(pack_mask(&m,&r,&a,0.5,0.5).unwrap().get_pixel(0,0).0,[39,204,0,223]);
        assert!(pack_mask(&m,&RgbaImage::new(2,2),&a,1.,1.).is_err());
        assert!(pack_mask(&m,&r,&a,f32::NAN,1.).is_err());
    }
    #[test] fn profile_is_conservative() {
        let p=UnityProjectInfo {project_path:"a".into(),editor_version:"6000.0".into(),
            pipeline:"Ambiguous".into(),urp_installed:true,hdrp_installed:true,
            active_pipeline_verified:false};
        assert!(choose_profile("Auto",&p).is_err());
        assert_eq!(choose_profile("URP",&p).unwrap(),"URP");
        assert!(choose_profile("BuiltIn",&p).is_err());
        let q=UnityProjectInfo {hdrp_installed:false,..p};
        assert!(choose_profile("HDRP",&q).is_err());
    }
}
