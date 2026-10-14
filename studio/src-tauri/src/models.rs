use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct StudioSettings {
    pub comfy_url: String,
    pub blender_path: String,
    pub station: String,
    pub library_root: String,
}
impl Default for StudioSettings {
    fn default() -> Self {
        Self {
            comfy_url:"http://127.0.0.1:8188".to_owned(),
            blender_path:String::new(),
            station:"local".to_owned(),
            library_root:String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Environment {
    pub settings: StudioSettings,
    pub app_data_path: String,
    pub detected_blender: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all="camelCase")]
pub struct WorkflowInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    pub family: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Asset {
    pub id: String,
    pub title: String,
    pub engine: String,
    pub station: String,
    pub created_at: String,
    pub file_size: i64,
    pub triangle_count: u64,
    pub vertex_count: u64,
    pub material_count: u64,
    pub texture_count: u64,
    pub sha256: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all="camelCase")]
pub struct JobStatus {
    pub prompt_id: String,
    pub status: String,
    pub message: String,
    pub asset: Option<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct QueueRequest {
    pub workflow_id: String,
    pub image_path: String,
    pub api_prompt: serde_json::Value,
    pub ui_workflow: serde_json::Value,
    pub config: serde_json::Value,
}
