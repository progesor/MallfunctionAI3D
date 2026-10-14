//! Non-destructive PBR map exports to Studio's managed archive.
//! The source GLB is never edited or re-encoded. Only local managed asset IDs
//! can be used and every export creates a new immutable directory.
use crate::{storage::{StudioResult, StudioState}};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, io::Read, path::{Path, PathBuf}};
use tauri::State;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureFileInput {
    filename: String,
    role: String,
    material_index: u32,
    source_image_index: u32,
    source_channel: String,
    width: u32,
    height: u32,
    content_base64: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureFileOutput {
    filename: String,
    role: String,
    width: u32,
    height: u32,
    file_bytes: usize,
    sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureExportResult {
    output_dir: String,
    manifest_path: String,
    files: Vec<TextureFileOutput>,
    source_untouched: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureExportSummary {
    output_dir: String,
    preset: String,
    generated_at: String,
    map_count: usize,
}

fn valid_role(role: &str) -> bool {
    matches!(role, "baseColor" | "normal" | "roughness" | "metallic" | "occlusion" | "emissive")
}

fn check_png(data: &[u8], width: u32, height: u32) -> StudioResult<()> {
    if data.len() < 33 || !data.starts_with(b"\x89PNG\r\n\x1a\n") || &data[12..16] != b"IHDR" {
        return Err("Texture PNG başlığı geçersiz.".into());
    }
    let w = u32::from_be_bytes(data[16..20].try_into().map_err(|_| "PNG genişliği okunamadı")?);
    let h = u32::from_be_bytes(data[20..24].try_into().map_err(|_| "PNG yüksekliği okunamadı")?);
    if w != width || h != height || w == 0 || h == 0 {
        return Err("PNG boyutu bildirilenden farklı.".into());
    }
    Ok(())
}

fn owned_asset_dir(source: &str, asset_id: &str) -> StudioResult<PathBuf> {
    Uuid::parse_str(asset_id).map_err(|_| "Geçersiz arşiv asset kimliği")?;
    let file = Path::new(source);
    if file.file_name().and_then(|x| x.to_str()) != Some("original.glb") {
        return Err("Yalnızca Studio arşiv GLB dosyalarından texture export yapılabilir.".into());
    }
    let dir = file.parent().ok_or("Asset klasörü eksik")?;
    if dir.file_name().and_then(|x| x.to_str()) != Some(asset_id) ||
        dir.parent().and_then(|p| p.file_name()).and_then(|x| x.to_str()) != Some("assets") {
        return Err("Asset arşiv yolu beklenen biçimde değil.".into());
    }
    let canonical = dir.canonicalize().map_err(|e| e.to_string())?;
    let assets_parent = dir.parent().ok_or("Arşiv üst klasörü eksik")?
        .canonicalize().map_err(|e| e.to_string())?;
    if canonical.parent() != Some(assets_parent.as_path()) || !canonical.join("original.glb").is_file() {
        return Err("Asset klasör sahipliği doğrulanamadı.".into());
    }
    Ok(canonical)
}

fn verify_sha256(path: &Path, expected: &str) -> StudioResult<()> {
    let mut source = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut chunk = [0u8; 65_536];
    loop {
        let len = source.read(&mut chunk).map_err(|e| e.to_string())?;
        if len == 0 { break; }
        hash.update(&chunk[..len]);
    }
    let current = format!("{:x}", hash.finalize());
    if current != expected {
        return Err("Kaynak GLB arşiv checksum değeriyle eşleşmiyor; dışa aktarma durduruldu.".into());
    }
    Ok(())
}

fn validate_input(file: &TextureFileInput, preset: &str) -> StudioResult<()> {
    if !valid_role(&file.role) || file.material_index > 63 || file.source_image_index > 255 {
        return Err("Desteklenmeyen texture haritası veya indeks.".into());
    }
    if !matches!(file.source_channel.as_str(), "rgb" | "r" | "g" | "b") ||
        ((file.role == "roughness" || file.role == "metallic" || file.role == "occlusion") && file.source_channel == "rgb") {
        return Err("Packed PBR texture kanal bilgisi geçersiz.".into());
    }
    let expected = format!("m{}_{}.png", file.material_index, file.role);
    if file.filename != expected {
        return Err("Texture dosya adı yalnızca güvenli PBR rol biçiminde olabilir.".into());
    }
    if file.width < 1 || file.height < 1 || file.width > 8192 || file.height > 8192 ||
        (file.width as u64) * (file.height as u64) > 32_000_000 {
        return Err("Texture boyut sınırı aşıldı.".into());
    }
    if preset != "source" && file.width.max(file.height) > preset.parse::<u32>().unwrap_or(0) {
        return Err("Texture ölçüsü seçili preset sınırını aşıyor.".into());
    }
    if file.content_base64.len() > 24_000_000 {
        return Err("Texture dosyası 18 MB sınırını aşıyor.".into());
    }
    Ok(())
}

#[tauri::command]
pub fn export_asset_textures(asset_id: String, preset: String, files: Vec<TextureFileInput>,
                             state: State<'_, StudioState>) -> StudioResult<TextureExportResult> {
    if !matches!(preset.as_str(), "source" | "1024" | "512" | "256") {
        return Err("Geçersiz texture boyutlandırma preset'i.".into());
    }
    if files.is_empty() || files.len() > 16 {
        return Err("Bir seferde 1–16 texture haritası dışa aktarılabilir.".into());
    }
    let asset = state.asset_by_id(&asset_id)?;
    let dir = owned_asset_dir(&asset.file_path, &asset_id)?;
    verify_sha256(Path::new(&asset.file_path), &asset.sha256)?;
    let mut names = HashSet::new();
    let mut staged = Vec::with_capacity(files.len());
    let mut total_bytes = 0usize;
    // Decode and validate *all* data before creating an output directory.
    for file in files {
        validate_input(&file, &preset)?;
        if !names.insert(file.filename.clone()) {
            return Err("Aynı PBR haritası birden fazla kez gönderildi.".into());
        }
        let data = STANDARD.decode(&file.content_base64).map_err(|_| "PNG base64 geçersiz")?;
        check_png(&data, file.width, file.height)?;
        total_bytes = total_bytes.checked_add(data.len()).ok_or("Texture toplam boyutu geçersiz")?;
        if total_bytes > 72_000_000 { return Err("Texture çıktı toplamı 72 MB sınırını aşıyor.".into()); }
        let summary = TextureFileOutput {
            filename: file.filename, role: file.role, width: file.width, height: file.height,
            file_bytes: data.len(), sha256: format!("{:x}", Sha256::digest(&data)),
        };
        staged.push((summary, data, file.material_index, file.source_image_index, file.source_channel));
    }
    let root = dir.join("texture_exports");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let export_dir = root.join(format!("pbr_{}_{}", Utc::now().format("%Y%m%d_%H%M%S"), Uuid::new_v4()));
    fs::create_dir(&export_dir).map_err(|e| e.to_string())?;
    let outcome = (|| -> StudioResult<TextureExportResult> {
        let metadata = staged.iter().map(|(info,_, material, source_image, channel)| json!({
            "filename": info.filename, "role": info.role,
            "width": info.width, "height": info.height, "file_bytes": info.file_bytes,
            "sha256": info.sha256, "materialIndex": material,
            "sourceImageIndex": source_image, "sourceChannel": channel,
        })).collect::<Vec<_>>();
        for (info, data, _, _, _) in &staged {
            fs::write(export_dir.join(&info.filename), data).map_err(|e| e.to_string())?;
        }
        let manifest = json!({
            "formatVersion": 1, "type": "mallfunction-ai3d-pbr-export",
            "assetId": asset_id, "assetTitle": asset.title,
            "sourceGlbSha256": asset.sha256, "source_untouched": true,
            "preset": preset, "generatedAt": Utc::now().to_rfc3339(),
            "maps": metadata,
            "notes": "PNG map files are non-destructive derivatives; original GLB is never modified. Packed ORM channels have been split by role. Color maps use sRGB; linear roles are linear grayscale."
        });
        let manifest_path = export_dir.join("texture_export_manifest.json");
        fs::write(&manifest_path,serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(TextureExportResult {
            output_dir: export_dir.to_string_lossy().into_owned(),
            manifest_path: manifest_path.to_string_lossy().into_owned(),
            files: staged.into_iter().map(|(info,_,_,_,_)|info).collect(),
            source_untouched: true,
        })
    })();
    if outcome.is_err() { let _ = fs::remove_dir_all(&export_dir); }
    outcome
}

/// Lists previously completed exports. Invalid/partial manifests are ignored.
#[tauri::command]
pub fn list_asset_texture_exports(asset_id: String, state: State<'_, StudioState>) -> StudioResult<Vec<TextureExportSummary>> {
    let asset = state.asset_by_id(&asset_id)?;
    let root = owned_asset_dir(&asset.file_path, &asset_id)?.join("texture_exports");
    if !root.exists() { return Ok(vec![]); }
    let mut results = Vec::new();
    for entry in fs::read_dir(root).map_err(|e|e.to_string())? {
        let entry = entry.map_err(|e|e.to_string())?;
        if !entry.file_type().map_err(|e|e.to_string())?.is_dir() { continue; }
        let directory = entry.path();
        if !entry.file_name().to_string_lossy().starts_with("pbr_") { continue; }
        let file = directory.join("texture_export_manifest.json");
        let bytes = match fs::read(&file) { Ok(b) if b.len() <= 128_000 => b, _ => continue };
        let value: serde_json::Value = match serde_json::from_slice(&bytes) { Ok(v) => v, Err(_) => continue };
        if value.get("assetId").and_then(|x|x.as_str()) != Some(asset_id.as_str()) ||
            value.get("sourceGlbSha256").and_then(|x|x.as_str()) != Some(asset.sha256.as_str()) { continue; }
        let preset = value.get("preset").and_then(|x|x.as_str()).unwrap_or("?").to_owned();
        let generated_at = value.get("generatedAt").and_then(|x|x.as_str()).unwrap_or("").to_owned();
        let map_count = value.get("maps").and_then(|x|x.as_array()).map(|v|v.len()).unwrap_or(0);
        results.push(TextureExportSummary {
            output_dir: directory.to_string_lossy().into_owned(), preset, generated_at, map_count,
        });
    }
    results.sort_by(|a,b| b.generated_at.cmp(&a.generated_at));
    results.truncate(20);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file() -> TextureFileInput { TextureFileInput {
        filename:"m0_roughness.png".into(),role:"roughness".into(),material_index:0,
        source_image_index:1,source_channel:"g".into(),width:512,height:256,
        content_base64:"AA==".into(),
    }}
    #[test]
    fn validates_role_and_presets() {
        assert!(validate_input(&file(),"512").is_ok());
        assert!(validate_input(&file(),"256").is_err());
        let mut changed=file(); changed.filename="../other.png".into();
        assert!(validate_input(&changed,"source").is_err());
    }
    #[test]
    fn confirms_managed_path_and_does_not_allow_arbitrary_sources() {
        let id = Uuid::new_v4().to_string();
        let base = std::env::temp_dir().join(format!("ai3d_texture_export_test_{}", Uuid::new_v4()));
        let owned = base.join("assets").join(&id);
        fs::create_dir_all(&owned).unwrap();
        let original = owned.join("original.glb");
        fs::write(&original, b"SOURCE MUST NOT CHANGE").unwrap();
        assert_eq!(owned_asset_dir(original.to_str().unwrap(), &id).unwrap(), owned.canonicalize().unwrap());
        assert!(owned_asset_dir(original.to_str().unwrap(), "wrong-id").is_err());
        assert!(owned_asset_dir(base.join("other.glb").to_str().unwrap(), &id).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"SOURCE MUST NOT CHANGE");
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn rejects_mismatched_or_incomplete_png() {
        assert!(check_png(b"not a png",512,256).is_err());
        let mut bytes = vec![0u8;33];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&2u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&3u32.to_be_bytes());
        assert!(check_png(&bytes,2,3).is_ok());
        assert!(check_png(&bytes,3,2).is_err());
    }
}
