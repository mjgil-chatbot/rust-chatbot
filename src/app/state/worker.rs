pub struct AppWorkerState {
    pub processing_chat_ids: HashSet<String>,
    pub local_processing_chat_ids: HashSet<String>,
    pub local_worker_pids: HashMap<String, u32>,
    pub response_cache_by_chat: HashMap<String, String>,
    pub pending_worker_pids: HashMap<String, u32>,
    pub response_started_chat_ids: HashSet<String>,
    pub pending_delete_chat_ids: HashSet<String>,
    pub forge_loop_active_chat_ids: HashSet<String>,
    pub forge_loop_continue_ids: HashMap<String, String>,
    pub forge_loop_acknowledged_ids: HashMap<String, String>,
    pub last_seen_worker_seq: HashMap<String, u64>,
    pub last_seen_worker_events: HashMap<String, (u64, u64)>,
    pub last_worker_poll: Instant,
    pub worker_poll_started_at: Option<Instant>,
    pub worker_discovery_rx: Option<mpsc::Receiver<DiscoveredWorkerJob>>,
    pub worker_poll_rx: Option<mpsc::Receiver<WorkerPollSnapshot>>,
    /// Chat IDs that were finalized locally (worker completed in this process)
    /// but whose StreamEnd socket echo has not yet been processed. Used to
    /// suppress stale disk reloads that would overwrite the in-memory state
    /// before the async save has completed.
    pub locally_finalized_chat_ids: HashSet<String>,
}

impl AppWorkerState {
    /// Whether *any* chat is being processed (for repaint scheduling).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn any_processing(&self) -> bool {
        !self.processing_chat_ids.is_empty()
    }

    /// Whether a specific chat ID is being processed.
    pub fn is_chat_processing(&self, chat_id: &str) -> bool {
        self.processing_chat_ids.contains(chat_id)
    }

    /// Whether a specific chat has a live local worker.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_local_processing(&self, chat_id: &str) -> bool {
        self.local_processing_chat_ids.contains(chat_id)
    }

    /// Mark a chat as processing (both global and local).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn mark_processing(&mut self, chat_id: &str) {
        self.processing_chat_ids.insert(chat_id.to_string());
        self.local_processing_chat_ids.insert(chat_id.to_string());
    }

    pub fn cache_response_snapshot(&mut self, chat_id: &str, response: String) -> bool {
        let changed = self
            .response_cache_by_chat
            .get(chat_id)
            .map(|cached| cached != &response)
            .unwrap_or(!response.is_empty());
        self.response_cache_by_chat
            .insert(chat_id.to_string(), response);
        changed
    }

    pub fn merge_running_response_snapshot(
        &mut self,
        chat_id: &str,
        response: Option<String>,
        events: &[crate::stream::StreamEvent],
    ) -> bool {
        if let Some(response) = response {
            let changed = self
                .response_cache_by_chat
                .get(chat_id)
                .map(|cached| cached != &response)
                .unwrap_or(!response.is_empty());
            if changed || !self.response_cache_by_chat.contains_key(chat_id) {
                self.response_cache_by_chat
                    .insert(chat_id.to_string(), response);
            }
            return changed;
        }

        if events.is_empty() {
            return false;
        }

        self.append_cached_response_events(chat_id, events)
    }

    pub fn append_cached_response_events(
        &mut self,
        chat_id: &str,
        events: &[crate::stream::StreamEvent],
    ) -> bool {
        let entry = self
            .response_cache_by_chat
            .entry(chat_id.to_string())
            .or_default();
        let original_len = entry.len();
        crate::stream::append_output_from_events(entry, events);
        entry.len() != original_len
    }

    pub fn cached_response(&self, chat_id: &str) -> Option<&str> {
        self.response_cache_by_chat.get(chat_id).map(String::as_str)
    }

    pub fn clear_cached_response(&mut self, chat_id: &str) {
        self.response_cache_by_chat.remove(chat_id);
    }

    /// Whether this process recently finalized the worker for a chat and the
    /// async save may still be in progress.
    pub fn is_locally_finalized(&self, chat_id: &str) -> bool {
        self.locally_finalized_chat_ids.contains(chat_id)
    }

    /// Record that this process just finalized a local worker for a chat.
    pub fn mark_locally_finalized(&mut self, chat_id: &str) {
        self.locally_finalized_chat_ids.insert(chat_id.to_string());
    }

    /// Clear all processing state for a chat.
    pub fn clear_processing(&mut self, chat_id: &str) {
        self.processing_chat_ids.remove(chat_id);
        self.local_processing_chat_ids.remove(chat_id);
        self.local_worker_pids.remove(chat_id);
        self.response_cache_by_chat.remove(chat_id);
        self.pending_worker_pids.remove(chat_id);
        self.response_started_chat_ids.remove(chat_id);
        self.last_seen_worker_seq.remove(chat_id);
        self.last_seen_worker_events.remove(chat_id);
    }

    pub fn update_forge_loop_state(
        &mut self,
        provider: crate::model::ModelProvider,
        chat_id: &str,
        response: &str,
    ) {
        if !matches!(
            provider,
            crate::model::ModelProvider::OmniForge
                | crate::model::ModelProvider::GeminiForge
                | crate::model::ModelProvider::GeminiCliForge
                | crate::model::ModelProvider::MiniMaxForge
        ) {
            self.clear_forge_loop_state(chat_id);
            return;
        }

        if let Some(continue_id) = crate::forge_loop::extract_continue_id(response) {
            self.forge_loop_active_chat_ids.insert(chat_id.to_string());
            self.forge_loop_continue_ids
                .insert(chat_id.to_string(), continue_id);
        } else {
            self.clear_forge_loop_state(chat_id);
        }
    }

    pub fn update_forge_loop_state_from_cache(
        &mut self,
        provider: crate::model::ModelProvider,
        chat_id: &str,
    ) {
        if !matches!(
            provider,
            crate::model::ModelProvider::OmniForge
                | crate::model::ModelProvider::GeminiForge
                | crate::model::ModelProvider::GeminiCliForge
                | crate::model::ModelProvider::MiniMaxForge
        ) {
            self.clear_forge_loop_state(chat_id);
            return;
        }

        let continue_id = self
            .response_cache_by_chat
            .get(chat_id)
            .and_then(|response| crate::forge_loop::extract_continue_id(response));
        if let Some(continue_id) = continue_id {
            self.forge_loop_active_chat_ids.insert(chat_id.to_string());
            self.forge_loop_continue_ids
                .insert(chat_id.to_string(), continue_id);
        } else {
            self.clear_forge_loop_state(chat_id);
        }
    }

    pub fn should_auto_continue_forge_loop(&self, chat_id: &str) -> bool {
        let Some(continue_id) = self.forge_loop_continue_ids.get(chat_id) else {
            return false;
        };
        self.forge_loop_active_chat_ids.contains(chat_id)
            && self.forge_loop_acknowledged_ids.get(chat_id) != Some(continue_id)
    }

    pub fn mark_forge_loop_acknowledged(&mut self, chat_id: &str) {
        let Some(continue_id) = self.forge_loop_continue_ids.get(chat_id).cloned() else {
            return;
        };
        self.forge_loop_acknowledged_ids
            .insert(chat_id.to_string(), continue_id);
    }

    fn clear_forge_loop_state(&mut self, chat_id: &str) {
        self.forge_loop_active_chat_ids.remove(chat_id);
        self.forge_loop_continue_ids.remove(chat_id);
        self.forge_loop_acknowledged_ids.remove(chat_id);
    }

    /// Whether the poll interval has elapsed since the last worker poll.
    pub fn should_poll(&self) -> bool {
        self.last_worker_poll.elapsed() >= WORKER_POLL_INTERVAL
    }
}

#[cfg(test)]
mod worker_state_tests {
    use super::*;

    fn test_worker_state() -> AppWorkerState {
        AppWorkerState {
            processing_chat_ids: HashSet::new(),
            local_processing_chat_ids: HashSet::new(),
            local_worker_pids: HashMap::new(),
            response_cache_by_chat: HashMap::new(),
            pending_worker_pids: HashMap::new(),
            response_started_chat_ids: HashSet::new(),
            pending_delete_chat_ids: HashSet::new(),
            forge_loop_active_chat_ids: HashSet::new(),
            forge_loop_continue_ids: HashMap::new(),
            forge_loop_acknowledged_ids: HashMap::new(),
            last_seen_worker_seq: HashMap::new(),
            last_seen_worker_events: HashMap::new(),
            last_worker_poll: Instant::now(),
            worker_poll_started_at: None,
            worker_discovery_rx: None,
            worker_poll_rx: None,
            locally_finalized_chat_ids: HashSet::new(),
        }
    }

    #[test]
    fn any_processing_is_false_when_empty() {
        let state = test_worker_state();
        assert!(!state.any_processing());
    }

    #[test]
    fn any_processing_is_true_with_entries() {
        let mut state = test_worker_state();
        state.processing_chat_ids.insert("c1".into());
        assert!(state.any_processing());
    }

    #[test]
    fn mark_and_clear_processing() {
        let mut state = test_worker_state();
        state.mark_processing("c1");
        assert!(state.is_chat_processing("c1"));
        assert!(state.is_local_processing("c1"));

        state.clear_processing("c1");
        assert!(!state.is_chat_processing("c1"));
        assert!(!state.is_local_processing("c1"));
    }

    #[test]
    fn clear_processing_removes_seq_tracker() {
        let mut state = test_worker_state();
        state.last_seen_worker_seq.insert("c1".into(), 5);
        state.last_seen_worker_events.insert("c1".into(), (1, 3));
        state.response_started_chat_ids.insert("c1".into());
        state.mark_processing("c1");

        state.clear_processing("c1");
        assert!(!state.last_seen_worker_seq.contains_key("c1"));
        assert!(!state.last_seen_worker_events.contains_key("c1"));
        assert!(!state.response_started_chat_ids.contains("c1"));
    }

    #[test]
    fn clear_processing_removes_cached_response() {
        let mut state = test_worker_state();
        state.mark_processing("c1");
        state.local_worker_pids.insert("c1".into(), 42);
        state
            .response_cache_by_chat
            .insert("c1".into(), "cached".into());

        state.clear_processing("c1");

        assert!(!state.response_cache_by_chat.contains_key("c1"));
        assert!(!state.local_worker_pids.contains_key("c1"));
    }

    #[test]
    fn forge_loop_state_tracks_pending_continue_once() {
        let mut state = test_worker_state();
        state.update_forge_loop_state(
            crate::model::ModelProvider::OmniForge,
            "c1",
            "● [00:52:32] Continue 9c58fe00-d47b-4b45-8faa-d2d908100bce",
        );

        assert!(state.should_auto_continue_forge_loop("c1"));

        state.mark_forge_loop_acknowledged("c1");
        assert!(!state.should_auto_continue_forge_loop("c1"));
    }

    #[test]
    fn merge_running_response_snapshot_appends_events_without_full_response() {
        let mut state = test_worker_state();
        let first_event = crate::stream::StreamEvent::TextDelta {
            content: "alpha".to_string(),
            timestamp_ms: 1,
        };
        let events = [crate::stream::StreamEvent::TextDelta {
            content: "beta".to_string(),
            timestamp_ms: 1,
        }];
        state.cache_response_snapshot(
            "c1",
            crate::stream::compute_output_from_events(std::slice::from_ref(&first_event)),
        );

        let changed = state.merge_running_response_snapshot("c1", None, &events);

        assert!(changed);
        assert_eq!(
            state.cached_response("c1"),
            Some(&crate::stream::compute_output_from_events(&[first_event, events[0].clone()])[..])
        );
    }

    #[test]
    fn update_forge_loop_state_from_cache_uses_cached_response() {
        let mut state = test_worker_state();
        state.cache_response_snapshot(
            "c1",
            "● [00:52:32] Continue 9c58fe00-d47b-4b45-8faa-d2d908100bce".to_string(),
        );

        state.update_forge_loop_state_from_cache(crate::model::ModelProvider::OmniForge, "c1");

        assert!(state.should_auto_continue_forge_loop("c1"));
    }
}
