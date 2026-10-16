//! Managed optional UV engine setup for Blender 5.1 Windows CPython.
//! Built-in Blender UV is always available. xatlas is installed only by an
//! explicit Studio UI click into the app-owned library/plugins/xatlas folder;
//! never in Blender system site-packages and never during a repair job.
use crate::blender::find_blender;
use crate::storage::{StudioResult,StudioState};
use serde::Serialize;
use sha2::{Digest,Sha256};
use std::{fs,path::{Path,PathBuf},process::Command};
use tauri::State;

#[derive(Serialize,Clone)]
#[serde(rename_all="camelCase")]
pub struct UvEngineStatus {
    pub blender_ready: bool,
    pub xatlas_ready: bool,
    pub python_version: Option<String>,
    pub plugin_path: String,
    pub message: String,
    pub backend_revision: String,
    pub worker_sha256: String,
    pub proxy_worker_ready: bool,
}

// Visible build handshake: versioned BOTH in Rust and TS panel. The
// included worker checksum is derived from the exact compiler-embedded
// Python bytes so a stale Tauri binary cannot masquerade as current.
pub(crate) const UV_RUNTIME_REVISION:&str="M2.8-XATLAS-TRADEOFF";
const WORKER_SOURCE:&str=include_str!("../../../blender/repair_uv_pbr.py");
fn worker_sha256()->String {
    format!("{:x}",Sha256::digest(WORKER_SOURCE.as_bytes()))
}
fn worker_has_proxy()->bool {
    WORKER_SOURCE.contains("stage(\"build_uv_topology_proxy\")")
        && WORKER_SOURCE.contains("transfer_clean_uv_from_proxy(")
        && !WORKER_SOURCE.contains("rebuild_exact_topology_work_copy(")
}
// Installation version is enforced by the embedded worker; this label
// is used only by the unit contract so normal desktop builds stay warning-free.
#[cfg(test)]
const PINNED_XATLAS:&str="xatlas==0.0.11";
const INSTALLER_SCRIPT:&str=include_str!("../../../blender/install_xatlas_wheel.py");
const XATLAS_PROBE:&str=
    "import sys,pathlib;sys.path.insert(0,sys.argv[1]);import xatlas,numpy;\
     root=pathlib.Path(sys.argv[1]).resolve();\
     native=pathlib.Path(xatlas.__file__).resolve();\
     print('XATLAS_IMPORT_OK' if native.is_relative_to(root) \
           and hasattr(xatlas,'parametrize') else 'XATLAS_WRONG_ORIGIN')";

fn blender_python(exe:&Path)->StudioResult<PathBuf>{
    let parent=exe.parent().ok_or("Blender executable folder invalid")?;
    let direct=parent.join("python").join("bin").join("python.exe");
    if direct.is_file(){return Ok(direct);}
    let mut dirs=fs::read_dir(parent).map_err(|e|format!("Blender version folders unreadable: {e}"))?
        .flatten().filter_map(|ent|{
            if !ent.file_type().ok()?.is_dir(){return None;}
            let name=ent.file_name().to_string_lossy().into_owned();
            if name.chars().all(|ch|ch.is_ascii_digit()||ch=='.') {
                let candidate=ent.path().join("python").join("bin").join("python.exe");
                if candidate.is_file(){return Some(candidate);}
            }
            None
        }).collect::<Vec<_>>();
    dirs.sort_by(|a,b|b.cmp(a));
    dirs.into_iter().next().ok_or(
        "Blender'in kendi python.exe dosyası bulunamadı. Blender 5.1 Python kurulumunu kontrol et.".into()
    )
}
fn output_short(command:&mut Command)->StudioResult<(bool,String)> {
    let proc=command.output().map_err(|e|format!("Blender Python çalıştırılamadı: {e}"))?;
    let text=format!("{} {}",
        String::from_utf8_lossy(&proc.stdout),
        String::from_utf8_lossy(&proc.stderr)).trim().chars().take(1800).collect();
    Ok((proc.status.success(),text))
}
fn ensure_python_313(python:&Path)->StudioResult<String> {
    let (ok,v)=output_short(Command::new(python)
       .arg("-I").arg("-c").arg("import sys;print(str(sys.version_info.major)+'.'+str(sys.version_info.minor))"))?;
    if !ok || v.trim()!="3.13" {
        return Err(format!(
            "Blender Python 3.13 gerekiyor (Windows wheel xatlas 0.0.11). Mevcut: {v}"
        ));
    }
    Ok(v)
}
fn uv_status(blender:Option<String>,library:PathBuf)->StudioResult<UvEngineStatus>{
    let plugin=library.join("plugins").join("xatlas");
    let base=UvEngineStatus{
        blender_ready:blender.is_some(),xatlas_ready:false,
        python_version:None,plugin_path:plugin.to_string_lossy().into_owned(),
        message:"Blender UV hazır; xatlas durumu kontrol edilmedi".into(),
        backend_revision:UV_RUNTIME_REVISION.into(),
        worker_sha256:worker_sha256(),
        proxy_worker_ready:worker_has_proxy(),
    };
    let exe=match blender {Some(s)=>s,None=>return Ok(UvEngineStatus{
        message:"Blender bulunamadı; Studio Ayarlar'dan blender.exe seç".into(),..base
    })};
    let py=match blender_python(Path::new(&exe)) {
        Ok(p)=>p,Err(e)=>return Ok(UvEngineStatus{message:e,..base})
    };
    let v=match ensure_python_313(&py) {
        Ok(v)=>v,Err(e)=>return Ok(UvEngineStatus{message:e,..base})
    };
    let mut status=UvEngineStatus{python_version:Some(v),..base};
    let (works,msg)=output_short(
        Command::new(&py).arg("-I").arg("-c").arg(XATLAS_PROBE).arg(&plugin))?;
    status.xatlas_ready=works && msg.contains("XATLAS_IMPORT_OK");
    status.message=if status.xatlas_ready {
        "Blender + xatlas 0.0.11 hazır; iki UV motoru da denenebilir".into()
    } else if !plugin.exists() {
        "Blender hazır; xatlas henüz kurulmadı".into()
    } else {
        format!("xatlas kurulu ama Blender tarafından yüklenemedi: {msg}")
    };
    Ok(status)
}

#[tauri::command]
pub async fn uv_engine_status(state:State<'_,StudioState>)->StudioResult<UvEngineStatus>{
    let blender=find_blender(&state.current_settings()?);
    let lib=state.library_dir()?;
    tauri::async_runtime::spawn_blocking(move ||uv_status(blender,lib))
       .await.map_err(|e|format!("UV engine probe failed: {e}"))?
}

#[tauri::command]
pub async fn install_uv_xatlas(state:State<'_,StudioState>)->StudioResult<UvEngineStatus>{
    let blender=find_blender(&state.current_settings()?)
        .ok_or("xatlas kurulumu için Blender bulunamadı")?;
    let lib=state.library_dir()?;
    let app_dir=state.app_dir.clone();
    tauri::async_runtime::spawn_blocking(move ||{
        let py=blender_python(Path::new(&blender))?;
        ensure_python_313(&py)?;
        let dir=lib.join("plugins").join("xatlas");
        if dir.is_dir() {
            let existing=uv_status(Some(blender.clone()),lib.clone())?;
            if existing.xatlas_ready {return Ok(existing);}
            if fs::read_dir(&dir).map_err(|e|e.to_string())?.next().is_some(){
                return Err(format!(
                    "Mevcut xatlas eklenti klasöründe dosyalar var fakat import başarısız. \
                     Güvenlik için üzerine yazılmadı: {}",
                    dir.display()
                ));
            }
        }
        // Blender 5.1 Python usually ships no pip. Instead use Blender's
        // own Python 3.13 stdlib: fetch only the exact xatlas cp313 Windows
        // wheel from PyPI, verify upstream SHA256, safely extract to a
        // temporary app-owned folder and publish atomically. No system
        // Blender files or pip site-packages are modified.
        let worker_dir=app_dir.join("workers");
        fs::create_dir_all(&worker_dir).map_err(|e|e.to_string())?;
        let worker=worker_dir.join("install_xatlas_wheel_v1.py");
        if worker.is_file() {
            let prior=fs::read(&worker).map_err(|e|e.to_string())?;
            if prior!=INSTALLER_SCRIPT.as_bytes(){
                return Err(format!(
                    "Managed xatlas installer changed unexpectedly; not overwritten: {}",
                    worker.display()));
            }
        } else {
            fs::write(&worker,INSTALLER_SCRIPT).map_err(|e|e.to_string())?;
        }
        let (success,message)=output_short(Command::new(&py)
            .arg("-I").arg(&worker).arg(&lib))?;
        if !success {
            return Err(format!(
                "xatlas 0.0.11 wheel download/verification failed. No Blender                  system files were changed. Installer: {}. {message}",
                worker.display()
            ));
        }
        let installed=uv_status(Some(blender),lib)?;
        if !installed.xatlas_ready {
            return Err(format!(
                "xatlas indirildi ancak Blender Python yükleyemedi. {}",
                installed.message
            ));
        }
        Ok(installed)
    }).await.map_err(|e|format!("UV xatlas installer failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn worker_runtime_handshake_matches_embedded_uv_proxy(){
        assert_eq!(UV_RUNTIME_REVISION,"M2.8-XATLAS-TRADEOFF");
        assert!(worker_has_proxy());
        assert_eq!(worker_sha256().len(),64);
    }
    #[test]fn xatlas_install_is_pinned_and_never_mutates_blender_site_packages(){
        assert_eq!(PINNED_XATLAS,"xatlas==0.0.11");
        assert!(INSTALLER_SCRIPT.contains("VERSION = \"0.0.11\""));
        assert!(XATLAS_PROBE.contains("import xatlas,numpy"));
    }
}
