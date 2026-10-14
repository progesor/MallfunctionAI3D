use crate::{models::{JobStatus, QueueRequest, WorkflowInfo}, storage::{StudioResult, StudioState}};
use serde_json::{json, Value};
use std::{fs, path::Path};
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

const PIXAL_API: &str=include_str!("../../../workflows/PLAYBOX_FoxPlush_pixal3d_api_v01.json");

fn validate_pilot_api(prompt:&Value) -> StudioResult<()> {
    let nodes=prompt.as_object().ok_or("ComfyUI API JSON nesnesi bekleniyor")?;
    if nodes.len()<25 || nodes.len()>600 {return Err("API 3D workflow boyutu geçersiz".into());}
    for (id,kind) in [("122","LoadImage"),("186","DecimateMesh"),("285","MeshToFile3D"),
        ("316","PrimitiveBoolean"),("322","Save3DAdvanced")] {
        if nodes.get(id).and_then(|n|n.get("class_type")).and_then(Value::as_str)!=Some(kind) {
            return Err(format!("API workflow #{id} ({kind}) içermiyor. Tam Pixal3D/TRELLIS.2 API Export gerekli."));
        }
    }
    let inputs=prompt.pointer("/322/inputs").ok_or("Save3DAdvanced girdileri eksik")?;
    let link=inputs.get("model_3d").and_then(Value::as_array)
        .ok_or("Save3DAdvanced 3D mesh bağlantısı yok; mask önizleme workflow'u desteklenmiyor")?;
    if link.first().and_then(Value::as_str)!=Some("285") {
        return Err("Save3DAdvanced #322, MeshToFile3D #285 düğümüne bağlı olmalı".into());
    }
    if !inputs.get("viewport_state").is_some_and(Value::is_object) {
        return Err("Save3DAdvanced viewport_state nesnesi yok. ComfyUI'den 'API Format' olarak dışa aktar.".into());
    }
    if !prompt.pointer("/316/inputs/value").is_some_and(Value::is_boolean) {
        return Err("Pixal3D/TRELLIS.2 motor seçimi #316 geçersiz".into());
    }
    Ok(())
}

fn builtin_api(id: &str) -> StudioResult<Value> {
    let mut prompt:Value=serde_json::from_str(PIXAL_API).map_err(|e|e.to_string())?;
    validate_pilot_api(&prompt)?;
    if id=="trellis2-fox" {prompt["316"]["inputs"]["value"]=json!(true);}
    Ok(prompt)
}

fn imported_dir(state:&StudioState) -> StudioResult<std::path::PathBuf> {
    let directory=state.library_dir()?.join("workflows");
    fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
    Ok(directory)
}

fn workflow_document(id:&str,state:&StudioState) -> StudioResult<Value> {
    if matches!(id,"pixal3d-fox" | "trellis2-fox") {
        return Ok(json!({"format":"api","prompt":builtin_api(id)?}));
    }
    if !id.starts_with("api-") || id.len()!=20 || !id[4..].chars().all(|c|c.is_ascii_hexdigit()) {
        return Err("Bilinmeyen veya geçersiz workflow kimliği".into());
    }
    let path=imported_dir(state)?.join(format!("{id}.json"));
    let contents=fs::read(&path).map_err(|e|format!("İçe aktarılan workflow bulunamadı: {e}"))?;
    let stored:Value=serde_json::from_slice(&contents).map_err(|e|e.to_string())?;
    let prompt=stored.get("prompt").ok_or("Arşiv workflow'u API grafı içermiyor")?;
    validate_pilot_api(prompt)?;
    Ok(json!({"format":"api","prompt":prompt}))
}

#[tauri::command]
pub fn get_workflows(state:State<'_,StudioState>) -> StudioResult<Vec<WorkflowInfo>> {
    let mut items=vec![
        WorkflowInfo { id:"pixal3d-fox".into(),title:"Pixal3D • Image → PBR Model".into(),
            description:"ComfyUI API formatından doğrulanmış PLAYBOX pilot profili".into(),
            family:"Pixal3D".into(),version:"api-v01".into() },
        WorkflowInfo { id:"trellis2-fox".into(),title:"TRELLIS.2 • Image → PBR Model".into(),
            description:"Aynı API grafı, TRELLIS.2 alternatif motor".into(),
            family:"TRELLIS.2".into(),version:"api-v01".into() },
    ];
    for entry in fs::read_dir(imported_dir(&state)?).map_err(|e|e.to_string())? {
        let entry=entry.map_err(|e|e.to_string())?;
        if !entry.file_type().map_err(|e|e.to_string())?.is_file() || entry.path().extension().and_then(|e|e.to_str())!=Some("json") {continue;}
        let Ok(raw)=fs::read(entry.path()) else {continue};
        let Ok(manifest)=serde_json::from_slice::<Value>(&raw) else {continue};
        let Some(id)=manifest.get("id").and_then(Value::as_str) else {continue};
        if !id.starts_with("api-") || id.len()!=20 || !id[4..].chars().all(|c|c.is_ascii_hexdigit()) {continue;}
        let Some(prompt)=manifest.get("prompt") else {continue};
        if validate_pilot_api(prompt).is_err() {continue;}
        let title=manifest.get("title").and_then(Value::as_str).unwrap_or("Imported API Workflow").to_owned();
        let family=if prompt.pointer("/316/inputs/value").and_then(Value::as_bool)==Some(true) {"TRELLIS.2"} else {"Pixal3D"};
        items.push(WorkflowInfo {id:id.into(),title,description:"Yerel API export · salt okunur kaynak kopyası".into(),family:family.into(),version:"user-api".into()});
    }
    Ok(items)
}

#[tauri::command]
pub fn get_workflow_json(id:String,state:State<'_,StudioState>) -> StudioResult<Value> {
    workflow_document(&id,&state)
}

#[tauri::command]
pub fn comfy_import_api_workflow(path:String,state:State<'_,StudioState>) -> StudioResult<WorkflowInfo> {
    let source=Path::new(&path);
    if !source.is_file() || source.extension().and_then(|v|v.to_str()).map(|v|v.to_ascii_lowercase())!=Some("json".into()) {
        return Err(".json API export dosyası seç.".into());
    }
    if fs::metadata(source).map_err(|e|e.to_string())?.len()>8_000_000 {return Err("Workflow 8 MB sınırını aşıyor".into());}
    let data=fs::read(source).map_err(|e|e.to_string())?;
    let parsed:Value=serde_json::from_slice(&data).map_err(|e|format!("API JSON okunamadı: {e}"))?;
    let prompt=parsed.get("prompt").filter(|v|v.is_object()).unwrap_or(&parsed);
    validate_pilot_api(prompt)?;
    let canonical=serde_json::to_vec(prompt).map_err(|e|e.to_string())?;
    let digest=format!("{:x}",Sha256::digest(&canonical));
    let id=format!("api-{}",&digest[..16]);
    let title=source.file_stem().and_then(|v|v.to_str()).unwrap_or("Imported API Workflow").chars().take(85).collect::<String>();
    let target=imported_dir(&state)?.join(format!("{id}.json"));
    if !target.exists() {
        let manifest=json!({"id":id,"title":title,"sha256":digest,"sourceName":source.file_name().and_then(|v|v.to_str()),
            "createdAt":chrono::Utc::now().to_rfc3339(),"prompt":prompt});
        let mut writer=fs::OpenOptions::new().write(true).create_new(true).open(&target).map_err(|e|e.to_string())?;
        use std::io::Write;
        writer.write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    }
    let family=if prompt.pointer("/316/inputs/value").and_then(Value::as_bool)==Some(true) {"TRELLIS.2"} else {"Pixal3D"};
    Ok(WorkflowInfo{id,title,description:"ComfyUI API export · arşivde korunan kopya".into(),family:family.into(),version:"user-api".into()})
}

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

/// Fail closed: the "source" mode must submit exactly the user's imported
/// workflow except the uploaded image name; no silent seeds/preset rewrites.
/// Custom mode permits only literal, documented controls (never any links).
fn validate_submission_fidelity(source:&Value, submitted:&Value, custom:bool, override_seeds:bool) -> StudioResult<Vec<String>> {
    let original=source.as_object().ok_or("Kaynak API workflow geçersiz")?;
    let prepared=submitted.as_object().ok_or("Çalıştırılacak API workflow geçersiz")?;
    if original.len()!=prepared.len() || original.keys().any(|k|!prepared.contains_key(k)) {
        return Err("Workflow node ekleme/silme tespit edildi. Kaynak grafı aynen çalıştırın.".into());
    }
    let mut edits=Vec::new();
    for (id,node) in original {
        let actual=prepared.get(id).ok_or("Workflow node eksik")?;
        if node.get("class_type")!=actual.get("class_type") || node.get("_meta")!=actual.get("_meta") {
            return Err(format!("Workflow #{id} düğüm türü/metaverisi değiştirilmiş"));
        }
        let before=node.get("inputs").and_then(Value::as_object).ok_or("Kaynak node inputs hatalı")?;
        let after=actual.get("inputs").and_then(Value::as_object).ok_or("Çalışma node inputs hatalı")?;
        if before.len()!=after.len() || before.keys().any(|name|!after.contains_key(name)) {
            return Err(format!("Workflow #{id} girişleri değiştirilmiş"));
        }
        for (key,old) in before {
            let new=&after[key];
            if old==new {continue;}
            let field=format!("{id}.{key}");
            let allowed=field=="122.image" || (custom && matches!(field.as_str(),
              "186.target_face_count" | "288.value" | "224.resolution" | "233.resolution" |
              "233.samples" | "261.resolution" | "241.resolution" | "94.target_resolution" | "316.value"))
              || (custom && override_seeds && matches!(field.as_str(),
              "3.seed" | "12.seed" | "18.seed" | "23.seed"));
            // A connected socket is not a literal setting; never permit rerouting.
            let connected=old.as_array().is_some_and(|v| v.len()==2 && v[0].is_string() && v[1].is_number());
            if !allowed || connected {return Err(format!("İzin verilmeyen workflow değişikliği: #{field}"));}
            edits.push(field);
        }
    }
    Ok(edits)
}

#[tauri::command]
pub async fn comfy_submit_job(request:QueueRequest,state:State<'_,StudioState>) -> StudioResult<JobStatus> {
    let document=workflow_document(&request.workflow_id,&state)?;
    let registered=document.get("prompt").ok_or("Kayıtlı kaynak workflow bulunamadı")?;
    if registered!=&request.ui_workflow {
        return Err("Kaynak workflow ile seçilen arşiv sürümü uyuşmuyor. Workflow'u yeniden seçin.".into());
    }
    let custom=request.config.get("productionMode").and_then(Value::as_str)==Some("custom");
    let override_seeds=request.config.get("overrideSeeds").and_then(Value::as_bool)==Some(true);
    let changes=validate_submission_fidelity(registered,&request.api_prompt,custom,override_seeds)?;
    validate_pilot_api(&request.api_prompt)?;
    if !request.api_prompt.is_object() || request.api_prompt.as_object().is_none_or(|v|v.is_empty()) {
        return Err("ComfyUI API prompt JSON geçersiz.".into());
    }
    let client_id=Uuid::new_v4().to_string();
    // API prompt is already ComfyUI executable format. Do not pass it as a
    // LiteGraph "workflow" PNG metadata field: UI and API formats differ.
    let body=json!({"prompt":request.api_prompt,"client_id":client_id,
        "extra_data":{"extra_pnginfo":{"ai3d_studio":{"workflowId":request.workflow_id,"format":"api"}}}});
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
        "config":request.config, "changedInputs":changes,
        "imageSourcePath":request.image_path,
        "prompt":body["prompt"],"sourceApiWorkflow":request.ui_workflow});
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

fn history_failure_detail(entry:&Value) -> String {
    if let Some(messages)=entry.pointer("/status/messages").and_then(Value::as_array) {
        for item in messages.iter().rev() {
            if let Some(values)=item.as_array() {
                if values.first().and_then(Value::as_str)==Some("execution_error") {
                    let detail=values.get(1).unwrap_or(&Value::Null);
                    let node=detail.get("node_id").and_then(Value::as_str).unwrap_or("?");
                    let kind=detail.get("node_type").and_then(Value::as_str).unwrap_or("node");
                    let err=detail.get("exception_message").and_then(Value::as_str)
                        .or_else(||detail.get("message").and_then(Value::as_str)).unwrap_or("Bilinmeyen ComfyUI hatası");
                    return format!("ComfyUI #{node} ({kind}) hata: {err}");
                }
            }
        }
    }
    let status=entry.pointer("/status/status_str").and_then(Value::as_str).unwrap_or("unknown");
    format!("ComfyUI işlem durumu: {status}; ayrıntılar için 'Tanı Kaydı' seç.")
}

#[tauri::command]
pub async fn comfy_job_diagnostics(prompt_id:String,state:State<'_,StudioState>) -> StudioResult<Value> {
    if Uuid::parse_str(&prompt_id).is_err(){return Err("Geçersiz prompt kimliği".into());}
    let result=state.client.get(endpoint(&state,&format!("/history/{prompt_id}"))?)
        .send().await.map_err(|e|e.to_string())?.error_for_status().map_err(|e|e.to_string())?;
    let history:Value=result.json().await.map_err(|e|e.to_string())?;
    let entry=history.get(&prompt_id).unwrap_or(&Value::Null);
    let outputs=entry.get("outputs").and_then(Value::as_object)
        .map(|v|v.iter().map(|(key,val)|json!({"node":key,"outputKeys":val.as_object()
          .map(|fields|fields.keys().cloned().collect::<Vec<_>>()).unwrap_or_default()})).collect::<Vec<_>>())
        .unwrap_or_default();
    let messages=entry.pointer("/status/messages").cloned().unwrap_or(json!([]));
    let stored_prompt=entry.get("prompt").and_then(Value::as_array).and_then(|items|items.get(2));
    let run_manifest=state.library_dir()?.join("runs").join(&prompt_id).join("generation.json");
    let summary=fs::read(&run_manifest).ok().and_then(|v|serde_json::from_slice::<Value>(&v).ok()).unwrap_or(Value::Null);
    Ok(json!({"promptId":prompt_id,"comfyUrl":state.current_settings()?.comfy_url,
        "gpuStation":state.current_settings()?.station,
        "status":entry.pointer("/status/status_str"),"completed":entry.pointer("/status/completed"),
        "saveNodeInExecutedPrompt":stored_prompt.and_then(|v|v.get("322")).is_some(),
        "saveOutput":entry.pointer("/outputs/322"), "changedInputs":summary.get("changedInputs"),
        "productionMode":summary.pointer("/config/productionMode"),
        "samplerSeeds":summary.get("prompt").and_then(|p|p.as_object()).map(|m|m.iter()
            .filter(|(_,node)|node.get("class_type").and_then(Value::as_str)==Some("KSampler"))
            .map(|(id,node)|json!({"id":id,"seed":node.pointer("/inputs/seed")})).collect::<Vec<_>>()),
        "outputNodes":outputs,"messages":messages,"historyAvailable":entry.is_object()}))
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
            let detail=history_failure_detail(entry);
            state.job_update(&prompt_id,"failed",None,&detail)?;
            return Ok(JobStatus{prompt_id,status:"failed".into(),message:detail,asset:None});
        }
        let mut files=vec![];
        if let Some(save_output)=entry.pointer("/outputs/322") { gather_glb(save_output,&mut files); }
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
            let detail="ComfyUI işi bitirdi ancak GLB history çıkışında yok. Save3DAdvanced #322 hatası veya farklı çıkış formatı olabilir. 'Tanı Kaydı' düğmesine basıp raporu paylaş.";
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
    #[test] fn supplied_api_pilot_has_full_3d_save_chain() {
        let template=builtin_api("pixal3d-fox").unwrap();
        validate_pilot_api(&template).unwrap();
        assert_eq!(template["322"]["class_type"],"Save3DAdvanced");
        assert!(template["322"]["inputs"]["viewport_state"].is_object());
        let switched=builtin_api("trellis2-fox").unwrap();
        assert_eq!(switched["316"]["inputs"]["value"],true);
    }
    #[test] fn rejects_mask_only_3d_output() {
        let mut payload=builtin_api("pixal3d-fox").unwrap();
        payload["322"]["inputs"]["model_3d"]=json!(["303",0]);
        assert!(validate_pilot_api(&payload).is_err());
        payload["322"]["inputs"]["model_3d"]=json!(["285",0]);
        payload["322"]["inputs"]["viewport_state"]=json!("");
        assert!(validate_pilot_api(&payload).is_err());
    }
    #[test] fn unchanged_imported_api_is_allowed_and_seed_rewrite_is_blocked() {
        let source=builtin_api("pixal3d-fox").unwrap();
        let mut submitted=source.clone();
        submitted["122"]["inputs"]["image"]=json!("uploaded.png");
        assert_eq!(validate_submission_fidelity(&source,&submitted,false,false).unwrap(),vec!["122.image"]);
        submitted["3"]["inputs"]["seed"]=json!(42);
        assert!(validate_submission_fidelity(&source,&submitted,false,false).is_err());
        assert!(validate_submission_fidelity(&source,&submitted,true,false).is_err());
        assert!(validate_submission_fidelity(&source,&submitted,true,true).is_ok());
        submitted["285"]["inputs"]["mesh"]=json!(["303",0]);
        assert!(validate_submission_fidelity(&source,&submitted,true,true).is_err());
    }
    #[test] fn prevent_pruned_output_workflow() {
        let source=builtin_api("pixal3d-fox").unwrap();
        let mut sub=source.clone();
        sub.as_object_mut().unwrap().remove("323");
        assert!(validate_submission_fidelity(&source,&sub,false,false).is_err());
    }
    #[test] fn extraction_of_comfy_execution_error() {
        let entry=json!({"status":{"messages":[["execution_error",{
            "node_id":"322","node_type":"Save3DAdvanced","exception_message":"viewport invalid"
        }]]}});
        let message=history_failure_detail(&entry);
        assert!(message.contains("#322"));
        assert!(message.contains("viewport invalid"));
    }
}
