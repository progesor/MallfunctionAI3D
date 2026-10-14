mod blender;
mod commands;
mod comfy;
mod models;
mod storage;
mod texture_export;
mod unity_export;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state=storage::StudioState::setup(app.handle())
                .map_err(|e|->Box<dyn std::error::Error>{e.into()})?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_environment,
            commands::save_settings,
            commands::list_assets,
            commands::import_model_file,
            commands::read_asset_glb,
            texture_export::export_asset_textures,
            texture_export::list_asset_texture_exports,
            unity_export::export_unity_urp,
            unity_export::list_unity_exports,
            commands::read_reference_preview,
            commands::move_asset_to_trash,
            commands::get_asset_manifest,
            commands::gpu_snapshot,
            comfy::get_workflows,
            comfy::get_workflow_json,
            comfy::comfy_import_api_workflow,
            comfy::comfy_job_diagnostics,
            comfy::comfy_health,
            comfy::comfy_node_info,
            comfy::comfy_upload_image,
            comfy::comfy_submit_job,
            comfy::comfy_poll_job,
            blender::blender_detect,
            blender::blender_generate_lods,
            blender::blender_lod_report,
            blender::blender_read_lod,
        ])
        .run(tauri::generate_context!())
        .expect("Mallfunction AI3D Studio failed to start");
}
