const SESSION_CACHE_MAX_ENTRIES: usize = 4;
const SESSION_CACHE_MAX_BYTES: usize = 96 * 1024 * 1024;
const SESSION_CACHE_MAX_ENTRY_BYTES: usize = 24 * 1024 * 1024;

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
    }

    pub fn insert(&mut self, session_id: String, mut session: ChatSession) -> Option<ChatSession> {
        strip_summary_for_cache(&mut session);
        let estimated_bytes = estimate_session_bytes(&session);
        let previous = self.remove(&session_id);
        if estimated_bytes > SESSION_CACHE_MAX_ENTRY_BYTES {
            return previous;
        }

        self.order.push_back(session_id.clone());
        self.total_bytes = self.total_bytes.saturating_add(estimated_bytes);
        self.entries.insert(
            session_id,
            CachedSessionEntry {
                session,
                estimated_bytes,
            },
        );
        self.enforce_limits();
        previous
    }

    pub fn sync_current(&mut self, session: &ChatSession) {
        let mut cached = session.clone();
        strip_summary_for_cache(&mut cached);
        let estimated_bytes = estimate_session_bytes(&cached);
        let session_id = cached.id.clone();
        self.remove(&session_id);
        if estimated_bytes > SESSION_CACHE_MAX_ENTRY_BYTES {
            return;
        }

        self.order.push_back(session_id.clone());
        self.total_bytes = self.total_bytes.saturating_add(estimated_bytes);
        self.entries.insert(
            session_id,
            CachedSessionEntry {
                session: cached,
                estimated_bytes,
            },
        );
        self.enforce_limits();
    }

    pub fn get(&self, session_id: &str) -> Option<&ChatSession> {
        self.entries.get(session_id).map(|entry| &entry.session)
    }

    pub fn get_mut(&mut self, session_id: &str) -> Option<&mut ChatSession> {
        self.touch(session_id);
        self.entries
            .get_mut(session_id)
            .map(|entry| &mut entry.session)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn contains_key(&self, session_id: &str) -> bool {
        self.entries.contains_key(session_id)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total_bytes = 0;
    }

    pub fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(&String, &mut ChatSession) -> bool,
    {
        self.entries
            .retain(|session_id, entry| keep(session_id, &mut entry.session));
        self.rebuild_index();
    }

    pub fn remove(&mut self, session_id: &str) -> Option<ChatSession> {
        let entry = self.entries.remove(session_id)?;
        self.total_bytes = self.total_bytes.saturating_sub(entry.estimated_bytes);
        self.order.retain(|candidate| candidate != session_id);
        Some(entry.session)
    }

    fn touch(&mut self, session_id: &str) {
        if self.entries.contains_key(session_id) {
            self.order.retain(|candidate| candidate != session_id);
            self.order.push_back(session_id.to_string());
        }
    }

    fn rebuild_index(&mut self) {
        self.total_bytes = 0;
        let mut seen = HashSet::new();
        self.order.retain(|session_id| {
            self.entries.contains_key(session_id) && seen.insert(session_id.clone())
        });
        for session_id in self.entries.keys() {
            if !self.order.iter().any(|candidate| candidate == session_id) {
                self.order.push_back(session_id.clone());
            }
        }
        for entry in self.entries.values() {
            self.total_bytes = self.total_bytes.saturating_add(entry.estimated_bytes);
        }
        self.enforce_limits();
    }

    fn enforce_limits(&mut self) {
        while self.entries.len() > SESSION_CACHE_MAX_ENTRIES
            || self.total_bytes > SESSION_CACHE_MAX_BYTES
        {
            let Some(evicted_id) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&evicted_id) {
                self.total_bytes = self.total_bytes.saturating_sub(evicted.estimated_bytes);
            }
        }
    }
}

fn strip_summary_for_cache(session: &mut ChatSession) {
    if let Some(ref mut summary) = session.json_summary {
        summary.turn_history.clear();
        summary.pending_tool_calls.clear();
    }
}

fn estimate_session_bytes(session: &ChatSession) -> usize {
    let mut total = 256usize;
    total = total.saturating_add(session.id.len());
    total = total.saturating_add(session.name.len());
    total = total.saturating_add(session.system_prompt.as_deref().map(str::len).unwrap_or(0));
    total = total.saturating_add(session.launched_from.as_deref().map(str::len).unwrap_or(0));
    total = total.saturating_add(
        session
            .claude_session_id
            .as_deref()
            .map(str::len)
            .unwrap_or(0),
    );
    total = total.saturating_add(
        session
            .minimax_session_id
            .as_deref()
            .map(str::len)
            .unwrap_or(0),
    );
    total = total.saturating_add(
        session
            .qwen_session_id
            .as_deref()
            .map(str::len)
            .unwrap_or(0),
    );
    total = total.saturating_add(
        session
            .codex_session_id
            .as_deref()
            .map(str::len)
            .unwrap_or(0),
    );
    total = total.saturating_add(
        session
            .nvidia_session_id
            .as_deref()
            .map(str::len)
            .unwrap_or(0),
    );
    total = total.saturating_add(
        session
            .gemini_session_id
            .as_deref()
            .map(str::len)
            .unwrap_or(0),
    );
    total = total.saturating_add(
        session
            .openrouter_session_id
            .as_deref()
            .map(str::len)
            .unwrap_or(0),
    );

    for message in &session.messages {
        total = total.saturating_add(96);
        total = total.saturating_add(message.content.len());
        total = total.saturating_add(message.command.as_deref().map(str::len).unwrap_or(0));
        total = total.saturating_add(
            message
                .retrieved_memory
                .as_deref()
                .map(str::len)
                .unwrap_or(0),
        );
    }

    total
}

fn session_info_from_session(session: &ChatSession) -> SessionInfo {
    SessionInfo {
        id: session.id.clone(),
        name: session.name.clone(),
        created_at: session.created_at,
        updated_at: session.updated_at,
        message_count: session
            .messages
            .iter()
            .filter(|message| message.is_user)
            .count(),
        hidden: false,
    }
}

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

impl SessionState {
    /// Return the current session's ID, if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn current_session_id(&self) -> Option<&str> {
        self.current_session.as_ref().map(|s| s.id.as_str())
    }

    /// Check whether the given chat ID matches the current session.
    pub fn is_current_chat(&self, chat_id: &str) -> bool {
        self.current_session
            .as_ref()
            .map(|s| s.id == chat_id)
            .unwrap_or(false)
    }

    /// Update or insert a session info entry without re-sorting.
    /// Order is managed by `refresh_session_index` which does stable in-place updates.
    pub fn upsert_session_info(&mut self, session: &ChatSession) {
        self.upsert_session_info_entry(session_info_from_session(session));
    }

    pub fn upsert_session_info_entry(&mut self, session: SessionInfo) {
        if let Some(info) = self.sessions.iter_mut().find(|info| info.id == session.id) {
            info.name = session.name;
            info.updated_at = session.updated_at;
            info.message_count = session.message_count;
        } else {
            self.sessions.insert(0, session);
        }
    }

    /// Look up a cached session that is still fresh per the session info list.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cached_session(&self, session_id: &str) -> Option<ChatSession> {
        let session = self.session_cache.get(session_id)?;
        let is_fresh = self
            .sessions
            .iter()
            .find(|info| info.id == session_id)
            .map(|info| info.updated_at == session.updated_at)
            .unwrap_or(false);
        is_fresh.then(|| session.clone())
    }

    /// Clear the ephemeral `pending_draft` if it belongs to the given chat.
    /// Returns true if a draft was cleared.
    /// Called atomically when the first real stream chunk arrives or when the
    /// turn terminates (done/stopped/error/send-failed), so the synthetic
    /// "Thinking..." row transitions cleanly into the real assistant row.
    pub fn clear_pending_draft_if_matches(&mut self, chat_id: &str) -> bool {
        if self
            .pending_draft
            .as_ref()
            .map(|draft| draft.chat_id == chat_id)
            .unwrap_or(false)
        {
            self.pending_draft = None;
            return true;
        }
        false
    }

    /// Whether an optimistic assistant draft is in flight for the given chat.
    pub fn has_pending_draft_for(&self, chat_id: &str) -> bool {
        self.pending_draft
            .as_ref()
            .map(|draft| draft.chat_id == chat_id)
            .unwrap_or(false)
    }

    /// Move the given session to the top of the sidebar list (index 0).
    /// Used only on terminal worker completion to surface finished chats.
    pub fn promote_session_to_top(&mut self, chat_id: &str) {
        if let Some(idx) = self.sessions.iter().position(|s| s.id == chat_id) {
            if idx > 0 {
                let entry = self.sessions.remove(idx);
                self.sessions.insert(0, entry);
            }
        }
    }
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

#[cfg(test)]
mod session_state_tests {
    use super::*;

    fn make_session(id: &str) -> ChatSession {
        ChatSession {
            id: id.to_string(),
            name: id.to_string(),
            system_prompt: None,
            json_summary: None,
            claude_session_id: None,
            minimax_session_id: None,
            qwen_session_id: None,
            codex_session_id: None,
            nvidia_session_id: None,
            gemini_session_id: None,
            openrouter_session_id: None,
            omniforge_session_id: None,
            omniforge_model_id: None,
            kimi_session_id: None,
            opencode_session_id: None,
            hermes_session_id: None,
            forge_session_id: None,
            claw_session_id: None,
            launched_from: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            messages: Vec::new(),
            runsc_dirs: Vec::new(),
            runsc_image: None,
        }
    }

    fn test_session_state() -> SessionState {
        SessionState {
            current_session: None,
            sessions: Vec::new(),
            data_manager: None,
            provider: ModelProvider::Claude,
            show_new_session_dialog: false,
            new_session_name: String::new(),
            show_rename_session_dialog: false,
            rename_session_id: String::new(),
            rename_session_name: String::new(),
            search_filter: String::new(),
            search_matched_sessions: HashSet::new(),
            search_results_rx: None,
            search_results_query: None,
            session_cache: SessionCache::new(),
            session_preload_rx: None,
            height_cache_load_rx: None,
            height_cache_load_started_at: None,
            latest_height_cache_load_request_id: 0,
            mcp_config_status_rx: None,
            mcp_config_status_started_at: None,
            latest_mcp_config_request_id: 0,
            mcp_config_cache_key: None,
            fff_mcp_server_explicitly_configured: None,
            pending_post_load_session_id: None,
            selected_session_id: None,
            loading_session_id: None,
            pending_draft: None,
            input_history_index: None,
            input_history_draft: String::new(),
            slash_command_index: None,
            slash_commands: Vec::new(),
            slash_commands_rx: None,
            system_prompt_text: String::new(),
            system_prompt_dirty: false,
            system_prompt_dirty_session_id: None,
            workspace_dir: None,
            workspace_input_text: String::new(),
            workspace_recent_dirs: Vec::new(),
            workspace_completions: Vec::new(),
            workspace_completion_query: None,
            workspace_completion_rx: None,
            workspace_completion_started_at: None,
            path_resolution_rx: None,
            path_resolution_started_at: None,
            latest_path_resolution_request_id: 0,
            runsc_dirs: Vec::new(),
            runsc_input_text: String::new(),
            runsc_recent_dirs: Vec::new(),
            runsc_image_input_text: String::new(),
            show_system_prompt: false,
            history: HistoryUiState::default(),
            ui_io: UiIoState::new(),
            event_queue: VecDeque::new(),
        }
    }

    #[test]
    fn current_session_id_returns_none_when_empty() {
        let state = test_session_state();
        assert!(state.current_session_id().is_none());
    }

    #[test]
    fn current_session_id_returns_id() {
        let mut state = test_session_state();
        state.current_session = Some(make_session("abc"));
        assert_eq!(state.current_session_id(), Some("abc"));
    }

    #[test]
    fn is_current_chat_matches() {
        let mut state = test_session_state();
        state.current_session = Some(make_session("abc"));
        assert!(state.is_current_chat("abc"));
        assert!(!state.is_current_chat("xyz"));
    }

    #[test]
    fn upsert_session_info_inserts_new() {
        let mut state = test_session_state();
        let session = make_session("s1");
        state.upsert_session_info(&session);
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].id, "s1");
    }

    #[test]
    fn upsert_session_info_updates_existing() {
        let mut state = test_session_state();
        let mut session = make_session("s1");
        state.upsert_session_info(&session);
        session.name = "renamed".to_string();
        state.upsert_session_info(&session);
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].name, "renamed");
    }

    #[test]
    fn cached_session_returns_fresh_only() {
        let mut state = test_session_state();
        let session = make_session("s1");
        let info = SessionInfo {
            id: "s1".into(),
            name: "s1".into(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            message_count: 0,
            hidden: false,
        };
        state.sessions.push(info);
        state.session_cache.insert("s1".into(), session.clone());

        // Fresh cache hit
        assert!(state.cached_session("s1").is_some());

        // Stale: info has different timestamp
        state.sessions[0].updated_at = chrono::Utc::now();
        assert!(state.cached_session("s1").is_none());
    }

    #[test]
    fn session_cache_rejects_oversized_session() {
        let mut state = test_session_state();
        let mut session = make_session("big");
        session.messages.push(crate::data::ChatMessage {
            content: "x".repeat(SESSION_CACHE_MAX_ENTRY_BYTES + 1),
            is_user: true,
            timestamp: chrono::Utc::now(),
            command: None,
            retrieved_memory: None,
        });

        state.session_cache.insert(session.id.clone(), session);

        assert!(!state.session_cache.contains_key("big"));
    }

    #[test]
    fn session_cache_sync_current_removes_stale_entry_for_oversized_session() {
        let mut cache = SessionCache::new();
        let mut small = make_session("big");
        small.messages.push(crate::data::ChatMessage {
            content: "ok".to_string(),
            is_user: true,
            timestamp: chrono::Utc::now(),
            command: None,
            retrieved_memory: None,
        });
        cache.insert(small.id.clone(), small);
        assert!(cache.contains_key("big"));

        let mut oversized = make_session("big");
        oversized.messages.push(crate::data::ChatMessage {
            content: "x".repeat(SESSION_CACHE_MAX_ENTRY_BYTES + 1),
            is_user: true,
            timestamp: chrono::Utc::now(),
            command: None,
            retrieved_memory: None,
        });
        cache.sync_current(&oversized);

        assert!(
            !cache.contains_key("big"),
            "oversized current sessions must evict any stale cached copy"
        );
    }

    #[test]
    fn session_cache_evicts_oldest_entries_when_over_count_cap() {
        let mut cache = SessionCache::new();
        for id in ["a", "b", "c", "d", "e"] {
            cache.insert(id.to_string(), make_session(id));
        }

        assert!(!cache.contains_key("a"));
        assert!(cache.contains_key("b"));
        assert!(cache.contains_key("c"));
        assert!(cache.contains_key("d"));
        assert!(cache.contains_key("e"));
    }

    #[test]
    fn promote_session_to_top_moves_entry() {
        let mut state = test_session_state();
        let now = chrono::Utc::now();
        for id in &["a", "b", "c"] {
            state.sessions.push(SessionInfo {
                id: id.to_string(),
                name: id.to_string(),
                created_at: now,
                updated_at: now,
                message_count: 0,
                hidden: false,
            });
        }

        state.promote_session_to_top("c");
        assert_eq!(state.sessions[0].id, "c");
        assert_eq!(state.sessions[1].id, "a");
        assert_eq!(state.sessions[2].id, "b");
    }

    #[test]
    fn promote_session_to_top_noop_when_already_first() {
        let mut state = test_session_state();
        let now = chrono::Utc::now();
        for id in &["a", "b"] {
            state.sessions.push(SessionInfo {
                id: id.to_string(),
                name: id.to_string(),
                created_at: now,
                updated_at: now,
                message_count: 0,
                hidden: false,
            });
        }

        state.promote_session_to_top("a");
        assert_eq!(state.sessions[0].id, "a");
        assert_eq!(state.sessions[1].id, "b");
    }

    #[test]
    fn promote_session_to_top_noop_when_not_found() {
        let mut state = test_session_state();
        let now = chrono::Utc::now();
        state.sessions.push(SessionInfo {
            id: "a".into(),
            name: "a".into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            hidden: false,
        });

        state.promote_session_to_top("nonexistent");
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].id, "a");
    }

    #[test]
    fn new_selection_fields_initialized_to_none() {
        let state = test_session_state();
        assert!(state.selected_session_id.is_none());
        assert!(state.loading_session_id.is_none());
        assert!(state.pending_draft.is_none());
    }
}
