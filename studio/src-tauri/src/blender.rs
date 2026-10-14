use crate::{models::StudioSettings, storage::{StudioResult,StudioState}};
use serde_json::{json,Value};
use std::{fs,env,path::{Path,PathBuf}};
use tauri::State;
use tokio::process::Command;
use uuid::Uuid;

const LOD_SCRIPT:&str=include_str!("../../../blender/prepare_lods.py");

pub fn find_blender(settings: &StudioSettings) -> Option<String> {
    if !settings.blender_path.is_empty() && Path::new(&settings.blender_path).is_file() {
        return Some(settings.blender_path.clone());
    }
    let mut matches=Vec::new();
    for envvar in ["ProgramFiles","ProgramFiles(x86)","LOCALAPPDATA"] {
        if let Ok(prefix)=env::var(envvar) {
            let bases=if envvar=="LOCALAPPDATA" {
                vec![PathBuf::from(&prefix).join("Programs").join("Blender Foundation"),
                    PathBuf::from(prefix).join("Blender Foundation")]
            } else { vec![PathBuf::from(prefix).join("Blender Foundation")] };
            for base in bases {
                if !base.exists(){continue;}
                if let Ok(entries)=fs::read_dir(base) {
                    for entry in entries.flatten() {
                        let p=entry.path().join("blender.exe");
                        if p.is_file(){matches.push(p);}
                    }
                }
            }
        }
    }
    matches.sort_by(|a,b|b.cmp(a));
    matches.first().map(|x|x.to_string_lossy().into_owned())
}

pub(crate) fn validate_lod_filename(name:&str) -> StudioResult<&str> {
    if name.len()<5 || name.len()>80 || !name.ends_with(".glb") ||
        !name.chars().all(|c|c.is_ascii_alphanumeric() || matches!(c,'_'|'-'|'.')) ||
        name.starts_with('.') || name.contains("..") {
        return Err("LOD dosya adı güvenli değil".into());
    }
    Ok(name)
}

fn lod_dir(asset_path:&str) -> StudioResult<PathBuf> {
    let root=Path::new(asset_path).parent().ok_or("Geçersiz asset arşiv yolu")?;
    Ok(root.join("lods"))
}

#[tauri::command]
pub fn blender_detect(state:State<'_,StudioState>)->StudioResult<Option<String>> {
    Ok(find_blender(&state.current_settings()?))
}

#[tauri::command]
pub async fn blender_generate_lods(asset_id:String,state:State<'_,StudioState>)->StudioResult<Value> {
    let asset=state.asset_by_id(&asset_id)?;
    let exe=find_blender(&state.current_settings()?).ok_or("Blender bulunamadı. Ayarlar > Blender.exe yolunu seç.")?;
    let worker_dir=state.app_dir.join("workers");
    fs::create_dir_all(&worker_dir).map_err(|e|e.to_string())?;
    let worker=worker_dir.join("prepare_lods_v02.py");
    // Pinned script shipped with application; never overwrite user scripts.
    if !worker.is_file() { fs::write(&worker,LOD_SCRIPT).map_err(|e|e.to_string())?; }
    // Studio stores the immutable source model; export names are descriptive and safe.
    let clean=asset.title.chars().map(|c|if c.is_ascii_alphanumeric() || c=='_' || c=='-' {c} else {'_'})
        .take(48).collect::<String>();
    let trimmed=clean.trim_matches('_');
    let name=if trimmed.is_empty(){"Asset".to_string()}else{trimmed.to_string()};
    let output=lod_dir(&asset.file_path)?.join(Uuid::new_v4().to_string());
    fs::create_dir_all(&output).map_err(|e|e.to_string())?;
    let execution=Command::new(&exe).arg("-b").arg("--python").arg(&worker)
        .arg("--").arg("--input").arg(&asset.file_path)
        .arg("--outdir").arg(&output)
        .arg("--name").arg(&name)
        .output().await.map_err(|e|format!("Blender başlatılamadı: {e}"))?;
    let log=format!("{}\n{}",String::from_utf8_lossy(&execution.stdout),String::from_utf8_lossy(&execution.stderr));
    fs::write(output.join("Blender_Run.log"),&log).map_err(|e|e.to_string())?;
    if !execution.status.success() {
        return Err(format!("Blender LOD hatası. Log klasörü: {}. {}",output.display(),log.chars().rev().take(450).collect::<String>().chars().rev().collect::<String>()));
    }
    let report:Value=serde_json::from_slice(&fs::read(output.join("Blender_LOD_Report.json")).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    for idx in 0..3 {
        let raw=report.pointer(&format!("/lods/{idx}/file")).and_then(Value::as_str)
            .ok_or_else(||format!("Blender LOD{idx} dosya adı raporda eksik"))?;
        let safe=validate_lod_filename(raw)?;
        if !output.join(safe).is_file() {
            return Err(format!("Blender LOD{idx} eksik: {raw}; {}",output.display()));
        }
    }
    let summary=json!({"assetId":asset_id,"lodDirectory":output,"report":report,
        "generatedAt":chrono::Utc::now().to_rfc3339(),"normalRebaked":false});
    fs::write(lod_dir(&asset.file_path)?.join("latest.json"),serde_json::to_vec_pretty(&summary).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    Ok(summary)
}

#[tauri::command]
pub fn blender_lod_report(asset_id:String,state:State<'_,StudioState>)->StudioResult<Option<Value>> {
    let asset=state.asset_by_id(&asset_id)?;
    let marker=lod_dir(&asset.file_path)?.join("latest.json");
    if !marker.is_file(){return Ok(None);}
    Ok(Some(serde_json::from_slice(&fs::read(marker).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?))
}

#[tauri::command]
pub fn blender_read_lod(asset_id:String,index:u8,state:State<'_,StudioState>)->StudioResult<String> {
    use base64::{engine::general_purpose::STANDARD,Engine};
    if index>2{return Err("LOD index 0-2 olmalı".into());}
    let report=blender_lod_report(asset_id,state)?.ok_or("LOD henüz oluşturulmadı")?;
    let root=report.get("lodDirectory").and_then(Value::as_str).ok_or("LOD rapor yolu eksik")?;
    let raw=report.pointer(&format!("/report/lods/{index}/file")).and_then(Value::as_str)
        .ok_or("LOD rapor dosya adı eksik")?;
    let name=validate_lod_filename(raw)?;
    let file=Path::new(root).join(name);
    let bytes=fs::read(file).map_err(|e|e.to_string())?;
    if bytes.len()>100_000_000 {return Err("LOD GLB önizleme sınırını aşıyor".into());}
    Ok(STANDARD.encode(bytes))
}
