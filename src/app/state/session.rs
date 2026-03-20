struct CachedSessionEntry {
    session: ChatSession,
    estimated_bytes: usize,
}
pub struct SessionCache {
    entries: HashMap<String, CachedSessionEntry>,
    order: std::collections::VecDeque<String>,
    total_bytes: usize,
}
impl SessionCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: std::collections::VecDeque::new(),
            total_bytes: 0,
        }
    }}
pub struct SessionState {
    pub current_session: Option<ChatSession>,
    pub sessions: Vec<SessionInfo>,
    pub data_manager: Option<DataManager>,
    pub provider: ModelProvider,
    pub show_new_session_dialog: bool,
    pub new_session_name: String,
    pub show_rename_session_dialog: bool,
    pub rename_session_id: String,
    pub rename_session_name: String,
    pub search_filter: String,
    pub search_matched_sessions: HashSet<String>,
    pub search_results_rx: Option<mpsc::Receiver<SessionSearchResult>>,
    pub search_results_query: Option<String>,
    pub session_cache: SessionCache,
    pub session_preload_rx: Option<mpsc::Receiver<ChatSession>>,
    pub height_cache_load_rx: Option<mpsc::Receiver<HeightCacheLoadResult>>,
    pub height_cache_load_started_at: Option<Instant>,
    pub latest_height_cache_load_request_id: u64,
    pub mcp_config_status_rx: Option<mpsc::Receiver<McpConfigStatusResult>>,
    pub mcp_config_status_started_at: Option<Instant>,
    pub latest_mcp_config_request_id: u64,
    pub mcp_config_cache_key: Option<String>,
    pub fff_mcp_server_explicitly_configured: Option<bool>,
    pub pending_post_load_session_id: Option<String>,
    /// The one authoritative selected chat — always matches current_session.id after load completes.
    pub selected_session_id: Option<String>,
    /// Transient: explicit load in flight, cleared when set_current_session() fires.
    pub loading_session_id: Option<String>,
    /// Ephemeral optimistic assistant draft for immediate loading indicator.
    pub pending_draft: Option<PendingDraft>,
    pub input_history_index: Option<usize>,
    pub input_history_draft: String,
    pub slash_command_index: Option<usize>,
    pub slash_commands: Vec<String>,
    /// Receiver for background slash command discovery.
    pub slash_commands_rx: Option<mpsc::Receiver<Vec<String>>>,
    pub system_prompt_text: String,
    pub system_prompt_dirty: bool,
    /// If present, the current `system_prompt_dirty` state belongs to this session.
    pub system_prompt_dirty_session_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub workspace_input_text: String,
    pub workspace_recent_dirs: Vec<String>,
    pub workspace_completions: Vec<String>,
    pub workspace_completion_query: Option<String>,
    pub workspace_completion_rx: Option<mpsc::Receiver<WorkspaceCompletionResult>>,
    pub workspace_completion_started_at: Option<Instant>,
    pub path_resolution_rx: Option<mpsc::Receiver<PathResolutionResult>>,
    pub path_resolution_started_at: Option<Instant>,
    pub latest_path_resolution_request_id: u64,
    pub runsc_dirs: Vec<String>,
    pub runsc_input_text: String,
    pub runsc_recent_dirs: Vec<String>,
    #[allow(dead_code)]
    pub runsc_image_input_text: String,
    pub show_system_prompt: bool,
    pub history: HistoryUiState,
    pub ui_io: UiIoState,
    pub event_queue: VecDeque<AppEvent>,
}
/// Ephemeral optimistic assistant draft — set on send, cleared on first stream chunk or terminal.
#[derive(Clone, Debug)]
pub struct PendingDraft {
    pub chat_id: String,
    /// Timestamp of the user message that triggered this draft. Kept for future
    /// diagnostics / race detection even though the current lifecycle only needs `chat_id`.
    #[allow(dead_code)]
    pub user_message_timestamp_ms: i64,
}
