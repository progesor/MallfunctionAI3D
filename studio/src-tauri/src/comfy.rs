use crate::{models::{JobStatus, QueueRequest, WorkflowInfo}, storage::{StudioResult, StudioState}};
use serde_json::{json, Value};
use std::{fs, path::Path};
use tauri::State;
use uuid::Uuid;

const PIXAL: &str=include_str!("../../../workflows/Mallfunction_PLAYBOX_FoxPlush_pixal3d_pilot_v01.json");
const TRELLIS: &str=include_str!("../../../workflows/Mallfunction_PLAYBOX_FoxPlush_trellis2_pilot_v01.json");

pub fn workflow_json(id: &str) -> StudioResult<Value> {
    let raw=match id { "pixal3d-fox"=>PIXAL, "trellis2-fox"=>TRELLIS,
        _=>return Err("Bilinmeyen workflow kimliği.".into()) };
    serde_json::from_str(raw).map_err(|e|e.to_string())
}

#[tauri::command]
pub fn get_workflows() -> Vec<WorkflowInfo> {
    vec![
        WorkflowInfo { id:"pixal3d-fox".into(),title:"Pixal3D • Image → PBR Model".into(),
            description:"Resmî ComfyUI grafından türetilmiş PLAYBOX pilot profili".into(),
            family:"Pixal3D".into(),version:"pilot-v01".into() },
        WorkflowInfo { id:"trellis2-fox".into(),title:"TRELLIS.2 • Image → PBR Model".into(),
            description:"Aynı resmî graf, alternatif geometri/PBR motoru".into(),
            family:"TRELLIS.2".into(),version:"pilot-v01".into() },
    ]
}

#[tauri::command]
pub fn get_workflow_json(id: String) -> StudioResult<Value> { workflow_json(&id) }

fn endpoint(state: &StudioState, route: &str) -> StudioResult<String> {
    Ok(format!("{}{}",state.current_settings()?.comfy_url.trim_end_matches('/'),route))
}

#[tauri::command]
pub async fn comfy_health(state:State<'_,StudioState>) -> StudioResult<Value> {
    let response=state.client.get(endpoint(&state,"/system_stats")?)
        .send().await.map_err(|e|format!("ComfyUI bağlantısı kurulamadı: {e}"))?
        .error_for_status().map_err(|e|format!("ComfyUI HTTP hatası: {e}"))?;
    response.json().await.map_err(|e|e.to_string())
}

#[tauri::command]
pub async fn comfy_node_info(state:State<'_,StudioState>) -> StudioResult<Value> {
    let response=state.client.get(endpoint(&state,"/object_info")?).send().await
        .map_err(|e|e.to_string())?.error_for_status().map_err(|e|e.to_string())?;
    response.json().await.map_err(|e|e.to_string())
}

#[tauri::command]
pub async fn comfy_upload_image(path:String,state:State<'_,StudioState>) -> StudioResult<String> {
    let file=Path::new(&path);
    if !file.is_file() { return Err("Referans dosyası bulunamadı.".into()); }
    let ext=file.extension().and_then(|v|v.to_str()).unwrap_or("").to_ascii_lowercase();
    let mime=match ext.as_str() { "png"=>"image/png", "jpg" | "jpeg"=>"image/jpeg", "webp"=>"image/webp",
        _=>return Err("Referans yalnızca PNG, JPG/JPEG veya WEBP olabilir.".into()) };
    let meta=fs::metadata(file).map_err(|e|e.to_string())?;
    if meta.len()==0 || meta.len()>30_000_000 { return Err("Referans 1 byte–30 MB arasında olmalı.".into()); }
    let bytes=fs::read(file).map_err(|e|e.to_string())?;
    let original_name=file.file_name().and_then(|s|s.to_str()).unwrap_or("reference.png");
    // Avoid filename collisions on the ComfyUI input cache; always create a new
    // unique name and NEVER overwrite an existing user-uploaded image.
    let safe_name=format!("ai3d_{}_{}",Uuid::new_v4().simple(),original_name
        .chars().map(|c|if c.is_ascii_alphanumeric()||matches!(c,'.'|'_'|'-'){c}else{'_'}).collect::<String>());
    let part=reqwest::multipart::Part::bytes(bytes).file_name(safe_name)
        .mime_str(mime).map_err(|e|e.to_string())?;
    let form=reqwest::multipart::Form::new().part("image",part)
        .text("type","input").text("overwrite","false");
    let response=state.client.post(endpoint(&state,"/upload/image")?).multipart(form)
        .send().await.map_err(|e|format!("Görsel ComfyUI'ye aktarılamadı: {e}"))?
        .error_for_status().map_err(|e|e.to_string())?;
    let result:Value=response.json().await.map_err(|e|e.to_string())?;
    let name=result.get("name").and_then(Value::as_str)
        .ok_or("ComfyUI upload yanıtında dosya adı yok.")?;
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("ComfyUI geçersiz upload dosya adı döndürdü.".into());
    }
    Ok(name.to_owned())
}

#[tauri::command]
pub async fn comfy_submit_job(request:QueueRequest,state:State<'_,StudioState>) -> StudioResult<JobStatus> {
    workflow_json(&request.workflow_id)?; // Only allow registered production pipelines.
    if !request.api_prompt.is_object() || request.api_prompt.as_object().is_none_or(|v|v.is_empty()) {
        return Err("ComfyUI API prompt JSON geçersiz.".into());
    }
    let client_id=Uuid::new_v4().to_string();
    let body=json!({"prompt":request.api_prompt,"client_id":client_id,
        "extra_data":{"extra_pnginfo":{"workflow":request.ui_workflow}}});
    let response=state.client.post(endpoint(&state,"/prompt")?).json(&body)
        .send().await.map_err(|e|format!("ComfyUI kuyruğuna bağlanılamadı: {e}"))?;
    let status=response.status();
    let result:Value=response.json().await.map_err(|e|e.to_string())?;
    if !status.is_success() {
        return Err(format!("ComfyUI üretimi reddetti (HTTP {status}): {}",result));
    }
    let prompt_id=result.get("prompt_id").and_then(Value::as_str)
        .ok_or_else(||format!("ComfyUI prompt_id döndürmedi: {result}"))?;
    let uuid=Uuid::parse_str(prompt_id).map_err(|_|"ComfyUI prompt_id UUID değil.".to_owned())?;
    let pid=uuid.to_string();
    state.job_insert(&pid,&request.workflow_id)?;
    // Preserve a production audit locally. ComfyUI output/cache is untouched.
    let dir=state.library_dir()?.join("runs").join(&pid);
    fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let summary=json!({"promptId":pid,"workflowId":request.workflow_id,
        "station":state.current_settings()?.station,
        "startedAt":chrono::Utc::now().to_rfc3339(),
        "config":request.config, "imageSourcePath":request.image_path,
        "prompt":body["prompt"],"workflow":body["extra_data"]["extra_pnginfo"]["workflow"]});
    fs::write(dir.join("generation.json"),serde_json::to_vec_pretty(&summary).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    let image_path=Path::new(&request.image_path);
    if image_path.is_file() {
        if let Some(ext)=image_path.extension().and_then(|x|x.to_str()) {
            // No overwrite; already-allocated unique job directory.
            if ["png","jpg","jpeg","webp"].contains(&ext.to_ascii_lowercase().as_str()) {
                let _=fs::copy(image_path,dir.join(format!("reference.{}",ext.to_ascii_lowercase())));
            }
        }
    }
    Ok(JobStatus {prompt_id:pid,status:"queued".into(),message:"ComfyUI kuyruğuna eklendi".into(),asset:None})
}

#[derive(Debug,Clone)]
struct Output3d { filename:String, subfolder:String, kind:String }

fn gather_glb(value:&Value, found:&mut Vec<Output3d>) {
    match value {
        Value::Object(map)=>{
            if let Some(name)=map.get("filename").and_then(Value::as_str) {
                if name.to_ascii_lowercase().ends_with(".glb") {
                    let kind=map.get("type").and_then(Value::as_str).unwrap_or("output").to_owned();
                    let subfolder=map.get("subfolder").and_then(Value::as_str).unwrap_or("").to_owned();
                    if !name.contains('/') && !name.contains('\\') && !name.contains("..") && kind=="output" {
                        found.push(Output3d{filename:name.to_owned(),subfolder,kind});
                    }
                }
            }
            for v in map.values() { gather_glb(v,found); }
        },
        Value::Array(a)=>for v in a {gather_glb(v,found);},
        _=>{},
    }
}

fn queue_contains(queue:&Value,key:&str,prompt_id:&str)->bool {
    queue.get(key).and_then(Value::as_array).map(|q|q.iter().any(|entry| {
        entry.get(1).and_then(Value::as_str)==Some(prompt_id)
    })).unwrap_or(false)
}

#[tauri::command]
pub async fn comfy_poll_job(prompt_id:String,state:State<'_,StudioState>) -> StudioResult<JobStatus> {
    // A terminal record is idempotent; a second poll does not download a second copy.
    if let Some(asset)=state.job_asset(&prompt_id)? {
        return Ok(JobStatus{prompt_id,status:"done".into(),message:"GLB arşive alındı".into(),asset:Some(asset)});
    }
    let history_response=state.client.get(endpoint(&state,&format!("/history/{prompt_id}"))?)
        .send().await.map_err(|e|e.to_string())?.error_for_status().map_err(|e|e.to_string())?;
    let history:Value=history_response.json().await.map_err(|e|e.to_string())?;
    if let Some(entry)=history.get(&prompt_id) {
        let job_status=entry.pointer("/status/status_str").and_then(Value::as_str).unwrap_or("");
        if job_status=="error" || job_status=="failed" {
            let detail=entry.pointer("/status/messages").map(|v|v.to_string()).unwrap_or_default();
            state.job_update(&prompt_id,"failed",None,&detail)?;
            return Ok(JobStatus{prompt_id,status:"failed".into(),message:detail,asset:None});
        }
        let mut files=vec![];
        if let Some(outputs)=entry.get("outputs") { gather_glb(outputs,&mut files); }
        if let Some(output)=files.first() {
            let url=endpoint(&state,"/view")?;
            let response=state.client.get(url)
                .query(&[("filename",output.filename.as_str()),("subfolder",output.subfolder.as_str()),("type",output.kind.as_str())])
                .send().await.map_err(|e|e.to_string())?.error_for_status().map_err(|e|e.to_string())?;
            if response.content_length().unwrap_or(0)>250_000_000 {
                return Err("GLB 250 MB sınırını aşıyor; dosya arşive alınmadı.".into());
            }
            let bytes=response.bytes().await.map_err(|e|e.to_string())?;
            let provenance=json!({"promptId":prompt_id,"comfyOutput":output.filename,
                "comfySubfolder":output.subfolder,"captureAt":chrono::Utc::now().to_rfc3339()});
            let asset=state.store_asset(&bytes,"PLAYBOX Fox Plush", "ComfyUI",provenance)?;
            state.job_update(&prompt_id,"done",Some(&asset.id),"")?;
            return Ok(JobStatus{prompt_id,status:"done".into(),message:"GLB indirildi ve arşivlendi".into(),asset:Some(asset)});
        }
        if job_status=="success" || entry.pointer("/status/completed").and_then(Value::as_bool)==Some(true) {
            let detail="ComfyUI görevi tamamladı ama history içinde GLB bulunamadı. Save3DAdvanced çıktısını kontrol et.";
            state.job_update(&prompt_id,"no_output",None,detail)?;
            return Ok(JobStatus{prompt_id,status:"no_output".into(),message:detail.into(),asset:None});
        }
    }
    let response=state.client.get(endpoint(&state,"/queue")?).send().await
        .map_err(|e|e.to_string())?.error_for_status().map_err(|e|e.to_string())?;
    let queue:Value=response.json().await.map_err(|e|e.to_string())?;
    let (status,message)=if queue_contains(&queue,"queue_running",&prompt_id) {
        ("running","ComfyUI üretiyor…")
    } else if queue_contains(&queue,"queue_pending",&prompt_id) {
        ("queued","Üretim sırasında bekliyor")
    } else { ("pending_result","ComfyUI geçmiş kaydı bekleniyor") };
    state.job_update(&prompt_id,status,None,message)?;
    Ok(JobStatus{prompt_id,status:status.into(),message:message.into(),asset:None})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn finds_output_glb_nested() {
        let fixture=json!({"322":{"3d":[{"filename":"fox.glb","subfolder":"3d/Mallfunction","type":"output"}]}});
        let mut hits=vec![];gather_glb(&fixture,&mut hits);
        assert_eq!(hits.len(),1);
        assert_eq!(hits[0].filename,"fox.glb");
    }
    #[test] fn ignores_path_traversal() {
        let mut hits=vec![];gather_glb(&json!({"filename":"../bad.glb","type":"output"}),&mut hits);
        assert!(hits.is_empty());
    }
}
