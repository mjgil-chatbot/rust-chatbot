use serde::Deserialize;

fn default_memory_priority() -> String {
    "high".to_string()
}

fn default_memory_injection_limit() -> usize {
    20
}

fn default_memory_retrieval_limit() -> usize {
    20
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceMemoryStatusInput {
    /// Absolute path to the workspace whose memory store should be inspected.
    pub workspace_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchMemoryInput {
    /// Absolute path to the workspace whose memory store should be searched.
    pub workspace_path: String,
    /// Search query passed to the workspace memory retrieval layer.
    pub query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListMemoryInjectionsInput {
    /// Absolute path to the workspace whose memory injections should be listed.
    pub workspace_path: String,
    /// Maximum number of recent injections to return.
    #[serde(default = "default_memory_injection_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListMemoryRetrievalsInput {
    /// Absolute path to the workspace whose memory retrieval audit should be listed.
    pub workspace_path: String,
    /// Maximum number of recent retrieval attempts to return.
    #[serde(default = "default_memory_retrieval_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InvalidateDocKeyInput {
    /// Absolute path to the workspace whose memory store should be updated.
    pub workspace_path: String,
    /// Deterministic document key, for example `<chat_id>:<message_index>:<role>`.
    pub doc_key: String,
    /// Optional reason recorded alongside the invalidation entry.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RestoreDocKeyInput {
    /// Absolute path to the workspace whose memory store should be updated.
    pub workspace_path: String,
    /// Deterministic document key, for example `<chat_id>:<message_index>:<role>`.
    pub doc_key: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RememberFactInput {
    /// Absolute path to the workspace whose memory store should be updated.
    pub workspace_path: String,
    /// Fact text to persist into explicit workspace memory.
    pub fact: String,
    /// Priority for retrieval ordering. Supported values: "high", "normal".
    #[serde(default = "default_memory_priority")]
    pub priority: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ForgetFactInput {
    /// Absolute path to the workspace whose memory store should be updated.
    pub workspace_path: String,
    /// Manual memory identifier returned by remember_fact or list_manual_memories.
    pub memory_id: String,
}
