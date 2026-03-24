impl AppState {
    fn is_session_locked(&self, chat_id: &str) -> bool {
        self.session.ui_io.pending_send_chat_ids.contains(chat_id)
            || self.worker.processing_chat_ids.contains(chat_id)
            || self.worker.pending_worker_pids.contains_key(chat_id)
    }}
impl AppState {
    fn can_control_running_job(&self, parent_pid: u32) -> bool {
        self.should_reconcile_job(parent_pid)
    }}
impl AppState {
    fn is_reconcile_leader(&self) -> bool {
        let our_pid = std::process::id();
        self.sync_shm
            .as_ref()
            .and_then(|shm| {
                shm.live_instances()
                    .into_iter()
                    .filter(|entry| entry.provider.as_str() == self.session.provider.as_str())
                    .map(|entry| entry.pid)
                    .min()
            })
            .map(|leader_pid| leader_pid == our_pid)
            .unwrap_or(true)
    }}
impl AppState {
    fn should_reconcile_job(&self, parent_pid: u32) -> bool {
        let our_pid = std::process::id();
        parent_pid == our_pid
            || (parent_pid == 0 && self.is_reconcile_leader())
            || (!crate::sync::is_pid_alive(parent_pid) && self.is_reconcile_leader())
    }}
impl AppState {
    fn backfill_all_workspace_memory_async(&self) {
        if !memory::memory_enabled() {
            return;
        }
        if !self.is_reconcile_leader() {
            return;
        }

        if let Err(err) = crate::memory_job::spawn_backfill_process(self.session.provider) {
            warn!(
                error=%err,
                provider=%self.session.provider.as_str(),
                "failed to launch workspace-memory backfill helper"
            );
        }
    }}
impl AppState {
    fn preload_recent_sessions_in_background(&mut self, limit: usize) {
        let current_id = self
            .session
            .current_session
            .as_ref()
            .map(|session| session.id.as_str());
        let session_ids: Vec<String> = self
            .session
            .sessions
            .iter()
            .filter(|session| Some(session.id.as_str()) != current_id)
            .take(limit)
            .map(|session| session.id.clone())
            .collect();

        if session_ids.is_empty() {
            self.session.session_preload_rx = None;
            return;
        }

        let provider = self.session.provider;
        let (tx, rx) = mpsc::channel();
        self.session.session_preload_rx = Some(rx);

        thread::spawn(move || {
            let data_manager = match DataManager::new(provider) {
                Ok(data_manager) => data_manager,
                Err(err) => {
                    warn!(
                        error=%err,
                        provider=%provider.as_str(),
                        "failed to initialize data manager for session preload"
                    );
                    return;
                }
            };

            for session_id in session_ids {
                match data_manager.load_session(&session_id) {
                    Ok(session) => {
                        if tx.send(session).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        warn!(
                            error=%err,
                            session_id=%session_id,
                            "failed to preload session"
                        );
                    }
                }
            }
        });
    }}
impl AppState {
    pub(crate) fn refresh_session_search_in_background(&mut self) {
        let query = self.session.search_filter.trim().to_string();
        self.session.search_results_query = (!query.is_empty()).then_some(query.clone());
        self.session.search_matched_sessions.clear();

        if query.is_empty() {
            self.session.search_results_rx = None;
            return;
        }

        let provider = self.session.provider;
        let (tx, rx) = mpsc::channel();
        self.session.search_results_rx = Some(rx);

        thread::spawn(move || {
            let matched_session_ids = match DataManager::new(provider)
                .and_then(|data_manager| data_manager.search_session_content(&query))
            {
                Ok(matched_session_ids) => matched_session_ids,
                Err(err) => {
                    warn!(error=%err, query=%query, "background session search failed");
                    HashSet::new()
                }
            };

            let _ = tx.send(SessionSearchResult {
                query,
                matched_session_ids,
            });
        });
    }}
impl AppState {
    fn drain_session_search_results(&mut self) {
        let result = self
            .session
            .search_results_rx
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(_) => None,
            });

        if let Some(result) = result {
            if self.session.search_results_query.as_deref() == Some(result.query.as_str())
                && self.session.search_filter.trim() == result.query
            {
                self.session.search_matched_sessions = result.matched_session_ids;
            }
            self.session.search_results_rx = None;
        }
    }}
impl AppState {
    fn index_memory_for_chat(&self, chat_id: &str, final_state: &str) {
        if !memory::memory_enabled() {
            return;
        }

        if let Err(err) =
            crate::memory_job::spawn_index_chat_process(self.session.provider, chat_id, final_state)
        {
            warn!(
                error=%err,
                provider=%self.session.provider.as_str(),
                chat_id=%chat_id,
                "failed to launch workspace-memory index helper"
            );
        }
    }}
impl AppState {
    /// Whether *any* chat is being processed (for repaint scheduling).
    pub fn any_processing(&self) -> bool {
        !self.worker.processing_chat_ids.is_empty()
            || !self.session.ui_io.pending_send_chat_ids.is_empty()
    }}
impl AppState {
    /// Background repaint cadence while worker activity is ongoing.
    /// Local worker state is only polled every 100ms, so repainting faster
    /// than that mostly burns UI time without surfacing new information.
    pub fn background_repaint_interval(&self) -> Option<Duration> {
        if self.any_processing() {
            Some(WORKER_POLL_INTERVAL)
        } else if self.session.pending_post_load_session_id.is_some() {
            Some(Duration::from_millis(50))
        } else {
            None
        }
    }}
impl AppState {
    /// Whether a specific chat ID is the one being processed.
    pub fn is_chat_processing(&self, chat_id: &str) -> bool {
        self.worker.processing_chat_ids.contains(chat_id)
    }}
impl AppState {
    /// Whether the current chat has a live worker that this UI can stop.
    pub fn is_local_processing(&self) -> bool {
        let Some(session) = self.session.current_session.as_ref() else {
            return false;
        };

        self.worker.local_processing_chat_ids.contains(&session.id)
            || self.worker.local_worker_pids.contains_key(&session.id)
    }}
impl AppState {
    pub fn is_current_chat(&self, chat_id: &str) -> bool {
        self.session
            .current_session
            .as_ref()
            .map(|session| session.id == chat_id)
            .unwrap_or(false)
    }}
impl AppState {
    /// Read new streaming events for a chat since the given byte offset.
    ///
    /// Returns `(events, text_output, new_offset)`.  The caller should
    /// persist `new_offset` and pass it back on the next poll cycle.
    /// `text_output` is the backward-compatible flat-text view computed
    /// from the new events only.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn poll_worker_events(
        &self,
        chat_id: &str,
        offset: u64,
    ) -> (Vec<crate::stream::StreamEvent>, String, u64) {
        let (events, new_offset) = worker::read_events_since(chat_id, offset);
        let text = crate::stream::compute_output_from_events(&events);
        (events, text, new_offset)
    }}
impl AppState {
    /// Read all events for a chat from the beginning.
    ///
    /// Useful for one-shot reads or when the caller needs the full
    /// history (e.g. session replay).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn read_all_worker_events(&self, chat_id: &str) -> Vec<crate::stream::StreamEvent> {
        let path = worker::events_path(chat_id);
        crate::stream::read_events(&path)
    }}
impl AppState {
    /// Read worker output with backward-compatible fallback.
    ///
    /// If a flat `output` file exists it is returned directly.  Otherwise
    /// the output is reconstructed from the event log.  This keeps
    /// existing UI code working unchanged.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn read_worker_output_compat(&self, chat_id: &str) -> String {
        worker::read_output(chat_id)
    }}
#[cfg(test)]
mod process_state_tests {
    use super::*;

    fn test_app_state(provider: ModelProvider) -> AppState {
        let (_tx, rx) = mpsc::channel();
        AppState {
            session: SessionState::new(provider),
            worker: AppWorkerState::new(),
            layout: LayoutState::new(),
            sync_shm: None,
            sync_socket: SocketHandle::noop(),
            sync_events: rx,
            instance_number: None,
            dialog_parent: None,
        }
    }

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

    #[test]
    fn is_local_processing_uses_cached_local_worker_flag() {
        let mut state = test_app_state(ModelProvider::Qwen);
        state.session.current_session = Some(make_session("chat-1"));
        state
            .worker
            .local_processing_chat_ids
            .insert("chat-1".into());

        assert!(state.is_local_processing());
    }

    #[test]
    fn can_control_running_job_accepts_dead_parent_for_leader() {
        let state = test_app_state(ModelProvider::Qwen);
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .spawn()
            .expect("spawn helper child");
        let dead_pid = child.id();
        child.wait().expect("wait helper child");

        assert!(state.can_control_running_job(dead_pid));
    }

    #[test]
    fn background_repaint_interval_covers_pending_send_preparation() {
        let mut state = test_app_state(ModelProvider::Codex);
        state
            .session
            .ui_io
            .pending_send_chat_ids
            .insert("chat-1".into());

        assert_eq!(
            state.background_repaint_interval(),
            Some(WORKER_POLL_INTERVAL)
        );
    }
}
