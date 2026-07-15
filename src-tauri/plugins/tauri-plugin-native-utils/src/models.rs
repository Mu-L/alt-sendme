use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectDonwloadFolderResponse {
    pub uri: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct SelectItemArgs {
    pub channel: Channel,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsyncJob {
    pub channel_id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportToTreeArgs {
    pub tree_uri: String,
    pub source_dir: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportToTreeConflict {
    pub original: String,
    pub resolved: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportToTreeResult {
    pub exported_count: u32,
    pub conflicts: Vec<ExportToTreeConflict>,
}
