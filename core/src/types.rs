use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRequest {
    pub file_path: String,
    pub repo_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResponse {
    pub file_path: String,
    pub repo_root: String,
    pub coupled_files: Vec<CoupledFile>,
    pub commit_count: u32,
    pub analysis_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestIntent {
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoupledFile {
    pub path: String,
    pub coupling_score: f64,
    pub co_change_count: u32,
    pub risk_score: f64,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub memories: Vec<Memory>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub test_intents: Vec<TestIntent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: i64,
    pub file_path: String,
    pub symbol_name: Option<String>,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddNoteResponse {
    pub id: i64,
    pub file_path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchNotesResponse {
    pub query: String,
    pub memories: Vec<Memory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListNotesResponse {
    pub file_path: Option<String>,
    pub memories: Vec<Memory>,
}
