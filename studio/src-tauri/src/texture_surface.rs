//! M3 surface-space PBR seam diagnostics and non-destructive narrow-band repair.
//! Reuses original glTF UV0 and geometry; NO remesh/UV unwrap/normal averaging.
//! Generates a REVIEW-ONLY catalog asset until explicit user visual approval.
use crate::{
    models::Asset,
    storage::{inspect_glb,StudioResult,StudioState},
    texture_export::{owned_asset_dir,verify_sha256},
    texture_seam_geometry::{analyze_seams,SeamGeometryReport},
    texture_seam_pixels::{blend_seams,SeamMapReport},
    unity_export::{existing_texture_dir,geometry_only_glb,png_bytes,png_map,sum_sha},
    uv_repair::{build_repaired_glb,gltf_factors,make_orm},
};
use chrono::Utc;
use image::RgbaImage;
use serde::Serialize;
use serde_json::{json,Value};
use std::{collections::HashSet,fs,path::{Path,PathBuf}};
use tauri::State;
use uuid::Uuid;

const PBR_ROLES:[&str;5]=["baseColor","metallic","roughness","occlusion","normal"];
const EDITABLE_ROLES:[&str;4]=["baseColor","metallic","roughness","occlusion"];

#[derive(Serialize,Debug,Clone)]
#[serde(rename_all="camelCase")]
pub struct SurfaceInspection{
    pub source_untouched:bool,
    pub uv_unchanged:bool,
    pub source_normal_unchanged:bool,
    pub geometry:SeamGeometryReport,
    pub maps:Vec<SeamMapReport>,
    pub editable_pixels:usize,
    pub eligible_seam_pairs:usize,
    pub workflow:String,
    pub warning:String,
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct SurfaceRepairResult{
    pub asset:Asset,
    pub version_dir:String,
    pub report_path:String,
    pub inspection:SurfaceInspection,
    pub source_untouched:bool,
    pub source_normal_unchanged:bool,
    pub uv_unchanged:bool,
    pub review_only:bool,
}
struct Prepared {
    glb:Vec<u8>,
    dir:PathBuf,
    inspection:SurfaceInspection,
    changed_roles:Vec<String>,
    source_sha:String,
}
fn validate_settings(mode:&str,radius:u32)->StudioResult<()>{
    if !matches!(mode,"gentle"|"balanced") || !(1..=3).contains(&radius) {
        Err("Dikiş yumuşatma ayarı geçersiz: gentle/balanced ve 1..3 px desteklenir".into())
    }else{Ok(())}
}
fn roles_to_edit(roles:&[String])->StudioResult<HashSet<String>> {
    if roles.is_empty() || roles.len()>4 {
        return Err("En az bir, en fazla dört PBR haritası seç".into());
    }
    let mut out=HashSet::new();
    for role in roles {
        if !EDITABLE_ROLES.contains(&role.as_str()) || !out.insert(role.clone()) {
            return Err("Normal veya aynı PBR haritası birden fazla kez düzenlenemez".into());
        }
    }
    Ok(out)
}
fn owned_maps(asset_dir:&Path,asset_id:&str,sha:&str,requested:&str)
    ->StudioResult<(PathBuf,Value)> {
    let (map_dir,manifest)=existing_texture_dir(
        asset_dir,asset_id,sha,requested)?;
    for role in PBR_ROLES {
        let file=format!("m0_{role}.png");
        png_map(&map_dir,&manifest,&file)?;
    }
    Ok((map_dir,manifest))
}
fn process_maps(glb:&[u8],maps_dir:&Path,manifest:&Value,
    mode:&str,radius:u32,edited:&HashSet<String>)
    ->StudioResult<(SurfaceInspection,Vec<(String,RgbaImage,Option<RgbaImage>)>)> {
    let surface=analyze_seams(glb)?;
    let mut metrics=Vec::new();
    let mut outputs=Vec::new();
    let mut changed_total=0usize;
    for role in PBR_ROLES {
        let filename=format!("m0_{role}.png");
        let original=png_map(maps_dir,manifest,&filename)?;
        if edited.contains(role) {
            let (image,quality,overlay)=blend_seams(
                &original,&surface.seams,role,radius,mode)?;
            changed_total+=quality.changed_pixels;
            metrics.push(quality);
            outputs.push((role.to_owned(),image,Some(overlay)));
        }else{
            outputs.push((role.to_owned(),original,None));
        }
    }
    Ok((SurfaceInspection{
        source_untouched:true,uv_unchanged:true,source_normal_unchanged:true,
        geometry:surface.report,
        maps:metrics,editable_pixels:changed_total,
        eligible_seam_pairs:surface.seams.len(),
        workflow:"source-UV-3D-seam-bands-v1".into(),
        warning:"UV/mesh/normal untouched. Seams may be intentional; edits are review-only. \
            This does NOT fix broad texture noise away from UV seams.".into(),
    },outputs))
}
fn stage_repair(asset_dir:&Path,asset_id:&str,source:&[u8],source_sha:&str,
    texture_export_dir:&str, mode:&str,radius:u32,roles:&[String])
    ->StudioResult<Prepared> {
    validate_settings(mode,radius)?;
    let selected=roles_to_edit(roles)?;
    let (map_dir,manifest)=owned_maps(
        asset_dir,asset_id,source_sha,texture_export_dir)?;
    let (factor,metal_factor,rough_factor,ao_factor,normal_scale,double_sided)=
        gltf_factors(source)?;
    let (inspection,processed)=process_maps(
        source,&map_dir,&manifest,mode,radius,&selected)?;
    if inspection.eligible_seam_pairs==0 {
        return Err("Bu modelde geometrik olarak eşleşen, güvenli UV dikişi bulunamadı; kaynak korunuyor".into());
    }
    if inspection.editable_pixels==0 {
        return Err("Seçilen haritalarda güvenle düzeltilebilecek dikiş farkı yok. \
                    Daha fazla değişiklik zorlamak yerine kaynak doku korunuyor.".into());
    }
    let map_for=|key:&str| {
        processed.iter().position(|(role,_,_)|role==key)
            .ok_or_else(||format!("PBR role unavailable: {key}"))
    };
    let metallic=map_for("metallic")?;
    let roughness=map_for("roughness")?;
    let occlusion=map_for("occlusion")?;
    let base_color=map_for("baseColor")?;
    let (orm,m,r)=make_orm(
        &processed[metallic].1,&processed[roughness].1,&processed[occlusion].1,
        metal_factor,rough_factor,"preserve")?;
    let orm_png=png_bytes(&orm)?;
    let base_png=png_bytes(&processed[base_color].1)?;
    // EXACT original Texture Export normal PNG bytes; never recompute,
    // normalize or blur a tangent-space normal across two UV islands.
    let normal_png=fs::read(map_dir.join("m0_normal.png")).map_err(|e|e.to_string())?;
    let result=build_repaired_glb(
        source,&base_png,&orm_png,&normal_png,factor,m,r,
        ao_factor,normal_scale,double_sided)?;
    let (source_geometry,source_triangles)=geometry_only_glb(source)?;
    let (result_geometry,result_triangles)=geometry_only_glb(&result)?;
    if source_triangles!=result_triangles || source_geometry!=result_geometry {
        return Err("Texture-only GLB changed UV/mesh geometry. Safety gate rejected result".into());
    }
    let (triangles,_,materials,textures)=inspect_glb(&result)?;
    if triangles!=source_triangles || materials!=1 || textures!=3 {
        return Err("Texture-repaired GLB structure does not match input PBR model".into());
    }
    // All heavy validation done before allocating an immutable run folder.
    let root=asset_dir.join("texture_repairs");
    fs::create_dir_all(&root).map_err(|e|e.to_string())?;
    let dir=root.join(Uuid::new_v4().to_string());
    fs::create_dir(&dir).map_err(|e|e.to_string())?;
    let saved=(||->StudioResult<()>{
        let out_dir=dir.join("Textures");
        fs::create_dir(&out_dir).map_err(|e|e.to_string())?;
        for (role,image,overlay) in &processed{
            let img=png_bytes(image)?;
            fs::write(out_dir.join(format!("{role}.png")),&img).map_err(|e|e.to_string())?;
            if let Some(mask)=overlay{
                fs::write(dir.join(format!("{role}_ChangeMask.png")),png_bytes(mask)?)
                    .map_err(|e|e.to_string())?;
            }
        }
        fs::write(out_dir.join("ORM.png"),&orm_png).map_err(|e|e.to_string())?;
        fs::write(dir.join("Reviewed_Geometry_And_PBR.glb"),&result)
            .map_err(|e|e.to_string())?;
        let doc=json!({
            "schemaVersion":1,"type":"surface-seam-repair-review",
            "sourceAssetId":asset_id,"sourceGlbSha256":source_sha,
            "sourceTextureExport":map_dir,
            "outputSha256":sum_sha(&result),
            "normalMapSourceSha256":sum_sha(&normal_png),
            "normalMapUnmodified":true,"originalGeometryUvAndNormalsUnmodified":true,
            "reviewOnly":true,"exportBlockedUntilVisualApproval":true,
            "mode":mode,"radiusPixels":radius,
            "editedRoles":roles,
            "inspection":inspection,
            "createdAt":Utc::now().to_rfc3339(),
        });
        fs::write(dir.join("Texture_Repair_Report.json"),
            serde_json::to_vec_pretty(&doc).map_err(|e|e.to_string())?)
                .map_err(|e|e.to_string())?;
        Ok(())
    })();
    if saved.is_err(){let _=fs::remove_dir_all(&dir);}
    saved?;
    Ok(Prepared{
        glb:result,dir,inspection,
        changed_roles:roles.to_vec(),source_sha:source_sha.to_owned(),
    })
}
#[tauri::command]
pub async fn analyze_texture_seams(
    asset_id:String,texture_export_dir:String,mode:String,radius:u32,
    state:State<'_,StudioState>,
)->StudioResult<SurfaceInspection>{
    validate_settings(&mode,radius)?;
    let asset=state.asset_by_id(&asset_id)?;
    let owned=owned_asset_dir(&asset.file_path,&asset_id)?;
    verify_sha256(Path::new(&asset.file_path),&asset.sha256)?;
    let original=fs::read(&asset.file_path).map_err(|e|e.to_string())?;
    gltf_factors(&original)?;
    let (maps,manifest)=owned_maps(&owned,&asset_id,&asset.sha256,&texture_export_dir)?;
    tauri::async_runtime::spawn_blocking(move||{
        let all=EDITABLE_ROLES.iter().map(|x|x.to_string()).collect::<HashSet<_>>();
        process_maps(&original,&maps,&manifest,&mode,radius,&all).map(|x|x.0)
    }).await.map_err(|e|format!("Texture seam analysis aborted: {e}"))?
}

#[tauri::command]
pub async fn repair_texture_seams(
    asset_id:String,texture_export_dir:String,mode:String,radius:u32,
    roles:Vec<String>,state:State<'_,StudioState>,
)->StudioResult<SurfaceRepairResult>{
    validate_settings(&mode,radius)?;
    roles_to_edit(&roles)?;
    let asset=state.asset_by_id(&asset_id)?;
    let owned=owned_asset_dir(&asset.file_path,&asset_id)?;
    verify_sha256(Path::new(&asset.file_path),&asset.sha256)?;
    let original=fs::read(&asset.file_path).map_err(|e|e.to_string())?;
    let original_sha=asset.sha256.clone();
    let original_id=asset.id.clone();
    let prepared=tauri::async_runtime::spawn_blocking(move||{
        stage_repair(&owned,&original_id,&original,&original_sha,
            &texture_export_dir,&mode,radius,&roles)
    }).await.map_err(|e|format!("Texture Repair background failure: {e}"))??;
    verify_sha256(Path::new(&asset.file_path),&asset.sha256)?;
    let approved=prepared.inspection.clone();
    let version=prepared.dir.to_string_lossy().into_owned();
    let new_asset=state.store_asset(
        &prepared.glb,
        &format!("{} · Texture Seam REVIEW",asset.title),
        &asset.engine,
        json!({
            "origin":"texture-seam-repair",
            "parentAssetId":asset.id,
            "parentSha256":prepared.source_sha,
            "repairVersion":prepared.dir,
            "sourceUVUnchanged":true,
            "sourceNormalsUnchanged":true,
            "normalTextureUnmodified":true,
            "editedRoles":prepared.changed_roles,
            "inspection":prepared.inspection,
            "reviewOnly":true,
            "productionExportBlocked":true,
            "visualApprovalRequired":true,
        }),
    )?;
    Ok(SurfaceRepairResult{
        asset:new_asset,version_dir:version.clone(),
        report_path:prepared.dir.join("Texture_Repair_Report.json")
            .to_string_lossy().into_owned(),
        inspection:approved,source_untouched:true,
        source_normal_unchanged:true,uv_unchanged:true,
        review_only:true,
    })
}

/// Explicit final approval clones a REVIEW ONLY asset into a new game-export
/// eligible asset. The decision belongs to the user AFTER inspecting both
/// models in Studio; no hidden QA status mutation of an existing artifact.
#[tauri::command]
pub fn approve_texture_seam_review(
    review_asset_id:String,confirmed_visual_review:bool,
    state:State<'_,StudioState>
)->StudioResult<Asset>{
    if !confirmed_visual_review{
        return Err("Texture onarımının orijinal ile görsel karşılaştırmasını onaylamalısın".into());
    }
    let review=state.asset_by_id(&review_asset_id)?;
    let owner=owned_asset_dir(&review.file_path,&review_asset_id)?;
    let manifest=crate::unity_export::read_json(&owner.join("manifest.json"))?;
    let provenance=manifest.get("provenance").ok_or("Texture Repair provenance missing")?;
    if provenance.get("origin").and_then(Value::as_str)!=Some("texture-seam-repair") ||
       provenance.get("reviewOnly").and_then(Value::as_bool)!=Some(true) ||
       provenance.get("productionExportBlocked").and_then(Value::as_bool)!=Some(true) ||
       provenance.get("sourceUVUnchanged").and_then(Value::as_bool)!=Some(true) ||
       provenance.get("sourceNormalsUnchanged").and_then(Value::as_bool)!=Some(true) ||
       provenance.get("normalTextureUnmodified").and_then(Value::as_bool)!=Some(true) {
        return Err("Only verified texture seam REVIEW assets can be visually approved".into());
    }
    verify_sha256(Path::new(&review.file_path),&review.sha256)?;
    let content=fs::read(&review.file_path).map_err(|e|e.to_string())?;
    let parent_sha=provenance.get("parentSha256").and_then(Value::as_str)
        .ok_or("Texture Repair parent SHA missing")?;
    let parent_id=provenance.get("parentAssetId").and_then(Value::as_str)
        .ok_or("Texture Repair original asset reference missing")?;
    let parent=state.asset_by_id(parent_id)?;
    verify_sha256(Path::new(&parent.file_path),parent_sha)?;
    let (a,at)=geometry_only_glb(&content)?;
    let source=fs::read(&parent.file_path).map_err(|e|e.to_string())?;
    let (b,bt)=geometry_only_glb(&source)?;
    if a!=b || at!=bt {
        return Err("Visually reviewed asset differs in underlying UV/mesh geometry".into());
    }
    state.store_asset(&content,
        &format!("{} · Texture Repair Approved",parent.title),
        &review.engine,
        json!({
            "origin":"texture-seam-approved","reviewAssetId":review.id,
            "parentAssetId":parent.id,"parentSha256":parent.sha256,
            "sourceUVUnchanged":true,"sourceNormalsUnchanged":true,
            "normalTextureUnmodified":true,
            "reviewOnly":false,"productionExportBlocked":false,
            "approvedByUserAt":Utc::now().to_rfc3339(),
            "acceptancePolicy":"explicit-post-preview-visual-confirmation"
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn repair_roles_never_allow_normal_or_double_selection(){
        assert!(roles_to_edit(&["normal".to_owned()]).is_err());
        assert!(roles_to_edit(&["baseColor".to_owned(),"baseColor".to_owned()]).is_err());
        assert!(roles_to_edit(&[]).is_err());
        assert!(roles_to_edit(&["baseColor".to_owned(),"roughness".to_owned()]).is_ok());
    }
    #[test]fn repair_radius_and_mode_are_bounded(){
        assert!(validate_settings("gentle",1).is_ok());
        assert!(validate_settings("balanced",3).is_ok());
        assert!(validate_settings("aggressive",2).is_err());
        assert!(validate_settings("gentle",8).is_err());
    }
}
