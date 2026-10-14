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
    let worker=worker_dir.join("prepare_lods_v01.py");
    // Pinned script shipped with application; never overwrite user scripts.
    if !worker.is_file() { fs::write(&worker,LOD_SCRIPT).map_err(|e|e.to_string())?; }
    let output=lod_dir(&asset.file_path)?.join(Uuid::new_v4().to_string());
    fs::create_dir_all(&output).map_err(|e|e.to_string())?;
    let mut command=Command::new(&exe);
    command.kill_on_drop(true).arg("-b").arg("--python").arg(&worker)
        .arg("--").arg("--input").arg(&asset.file_path).arg("--outdir").arg(&output);
    let execution=tokio::time::timeout(std::time::Duration::from_secs(900),command.output())
        .await.map_err(|_|"Blender 15 dakikalık zaman sınırını aştı; işlem durduruldu.".to_owned())?
        .map_err(|e|format!("Blender başlatılamadı: {e}"))?;
    let log=format!("{}\n{}",String::from_utf8_lossy(&execution.stdout),String::from_utf8_lossy(&execution.stderr));
    fs::write(output.join("Blender_Run.log"),&log).map_err(|e|e.to_string())?;
    if !execution.status.success() {
        return Err(format!("Blender LOD hatası. Log klasörü: {}. {}",output.display(),log.chars().rev().take(450).collect::<String>().chars().rev().collect::<String>()));
    }
    let report:Value=serde_json::from_slice(&fs::read(output.join("Blender_LOD_Report.json")).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    for idx in 0..3 {
        if !output.join(format!("PLAYBOX_FoxPlush_LOD{idx}.glb")).is_file() {
            return Err(format!("Blender LOD{idx} eksik; {}",output.display()));
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
    let file=Path::new(root).join(format!("PLAYBOX_FoxPlush_LOD{index}.glb"));
    let bytes=fs::read(file).map_err(|e|e.to_string())?;
    if bytes.len()>100_000_000 {return Err("LOD GLB önizleme sınırını aşıyor".into());}
    Ok(STANDARD.encode(bytes))
}
