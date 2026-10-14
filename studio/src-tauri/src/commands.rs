use crate::{models::{Asset, Environment, StudioSettings},storage::{StudioResult,StudioState}};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json,Value};
use std::{fs,path::Path};
use tauri::State;

#[tauri::command]
pub fn get_environment(state:State<'_,StudioState>) -> StudioResult<Environment> {
    Ok(Environment{settings:state.current_settings()?,
        app_data_path:state.app_dir.to_string_lossy().into_owned(),
        detected_blender:crate::blender::find_blender(&state.current_settings()?),
        source:"local desktop / user-configured ComfyUI".into()})
}

#[tauri::command]
pub fn save_settings(settings:StudioSettings,state:State<'_,StudioState>) -> StudioResult<StudioSettings> {
    state.save_settings(settings)
}

#[tauri::command]
pub fn list_assets(state:State<'_,StudioState>) -> StudioResult<Vec<Asset>> {
    state.list_assets()
}

#[tauri::command]
pub fn import_model_file(path:String,state:State<'_,StudioState>) -> StudioResult<Asset> {
    let original=Path::new(&path);
    if !original.is_file() || original.extension().and_then(|s|s.to_str()).map(|s|s.to_ascii_lowercase())!=Some("glb".to_owned()) {
        return Err("Dosya bulunamadı veya .glb değil.".into());
    }
    let size=fs::metadata(original).map_err(|e|e.to_string())?.len();
    if size==0 || size>250_000_000 { return Err("GLB dosyası 1 byte–250 MB arasında olmalı.".into()); }
    let bytes=fs::read(original).map_err(|e|e.to_string())?;
    let title=original.file_stem().and_then(|s|s.to_str()).unwrap_or("Imported GLB");
    state.store_asset(&bytes,title,"Imported GLB",json!({"sourcePath":path,"imported":true}))
}

#[tauri::command]
pub fn read_asset_glb(asset_id:String,state:State<'_,StudioState>) -> StudioResult<String> {
    let asset=state.asset_by_id(&asset_id)?;
    let bytes=fs::read(&asset.file_path).map_err(|e|format!("Arşiv dosyası okunamadı: {e}"))?;
    if bytes.len()>100_000_000 {return Err("Önizleme için bu GLB 100 MB limitini aşıyor.".into());}
    Ok(STANDARD.encode(bytes))
}

#[tauri::command]
pub fn get_asset_manifest(asset_id:String,state:State<'_,StudioState>) -> StudioResult<Value> {
    let asset=state.asset_by_id(&asset_id)?;
    let path=Path::new(&asset.file_path).parent().ok_or("Arşiv yolu geçersiz")?.join("manifest.json");
    serde_json::from_slice(&fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
}

#[tauri::command]
pub async fn gpu_snapshot() -> StudioResult<serde_json::Value> {
    use tokio::process::Command;
    let argv=["--query-gpu=name,index,memory.total,memory.used,utilization.gpu,temperature.gpu",
              "--format=csv,noheader,nounits"];
    let result=match Command::new("nvidia-smi").args(argv).output().await {
        Ok(output)=>output,
        Err(first)=>{
            #[cfg(target_os="windows")]
            {
                let windows=std::env::var("WINDIR").unwrap_or_else(|_|"C:\\Windows".into());
                let fallback=std::path::Path::new(&windows).join("System32").join("nvidia-smi.exe");
                Command::new(fallback).args(argv).output().await
                    .map_err(|e|format!("nvidia-smi bulunamadı: {first}; {e}"))?
            }
            #[cfg(not(target_os="windows"))]
            { return Err(format!("nvidia-smi bulunamadı: {first}")); }
        }
    };
    if !result.status.success(){return Err("nvidia-smi GPU bilgisini döndüremedi.".into());}
    let raw=String::from_utf8_lossy(&result.stdout);
    let fields:Vec<_>=raw.lines().next().unwrap_or("").split(',').map(str::trim).collect();
    if fields.len()<6 {return Err("nvidia-smi çıktısı beklenen biçimde değil.".into());}
    Ok(json!({"name":fields[0],"index":fields[1].parse::<i32>().unwrap_or(0),
        "vramTotalMiB":fields[2].parse::<f64>().ok(),
        "vramUsedMiB":fields[3].parse::<f64>().ok(),
        "utilPercent":fields[4].parse::<f64>().ok(),
        "temperatureC":fields[5].parse::<f64>().ok()}))
}
