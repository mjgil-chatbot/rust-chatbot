pub struct LayoutState {
    pub input_text: String,
    pub should_focus_input: bool,
    pub show_json_summary: bool,
    pub current_json_summary_preview: Option<JsonSummaryPreview>,
    pub current_json_summary_full: Option<JsonSummary>,
    pub pending_layout_log: bool,
    pub layout_log_rx: Option<mpsc::Receiver<crate::app::ui::layout_log::LayoutLogCompletion>>,
    pub markdown_cache: MarkdownCache,
    pub copy_feedback: HashMap<i64, std::time::Instant>,
    pub message_row_heights: Vec<f32>,
    pub message_row_heights_session_id: Option<String>,
    pub message_row_heights_width: f32,
    pub session_height_cache: HashMap<String, SessionHeightCache>,
    pub layout_log_feedback: Option<LayoutLogFeedback>,
    pub last_auto_ui_debug_signature: Option<String>,
    pub last_auto_ui_sidebar_debug_signature: Option<String>,
    pub message_scroll_offset_override: Option<f32>,
    pub message_scroll_offset_session_id: Option<String>,
    pub deferred_message_scroll_adjustment: f32,
    pub deferred_message_scroll_adjustment_session_id: Option<String>,
    pub pending_height_cache_save: Option<String>,
    pub height_cache_scan: Option<HeightCacheScan>,
    pub status_feedback: Option<UiStatusFeedback>,
}

#[derive(Clone, Debug)]
pub(crate) struct UiStatusFeedback {
    pub(crate) at: Instant,
    pub(crate) is_error: bool,
    pub(crate) message: String,
}

impl LayoutState {
    pub(crate) fn layout_log_in_flight(&self) -> bool {
        self.layout_log_rx.is_some()
    }

    /// Reset height tracking when switching sessions.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn reset_heights_for_session(&mut self, session_id: &str) {
        self.message_row_heights.clear();
        self.message_row_heights_session_id = Some(session_id.to_string());
        self.message_row_heights_width = 0.0;
        self.message_scroll_offset_override = None;
        self.message_scroll_offset_session_id = None;
        self.deferred_message_scroll_adjustment = 0.0;
        self.deferred_message_scroll_adjustment_session_id = None;
        self.height_cache_scan = None;
        self.status_feedback = None;
    }

    /// Whether the height cache should be invalidated because the session
    /// or viewport width changed.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn heights_stale(&self, session_id: &str, width: f32) -> bool {
        self.message_row_heights_session_id.as_deref() != Some(session_id)
            || (self.message_row_heights_width - width).abs() > 1.0
    }

    /// Remove expired copy feedback entries.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn prune_copy_feedback(&mut self) {
        self.copy_feedback
            .retain(|_, instant| instant.elapsed() < Duration::from_secs(2));
    }
}

#[cfg(test)]
mod layout_state_tests {
    use super::*;

    fn test_layout_state() -> LayoutState {
        LayoutState {
            input_text: String::new(),
            should_focus_input: false,
            show_json_summary: false,
            current_json_summary_preview: None,
            current_json_summary_full: None,
            pending_layout_log: false,
            layout_log_rx: None,
            markdown_cache: MarkdownCache::default(),
            copy_feedback: HashMap::new(),
            message_row_heights: vec![10.0, 20.0],
            message_row_heights_session_id: Some("s1".into()),
            message_row_heights_width: 800.0,
            session_height_cache: HashMap::new(),
            layout_log_feedback: None,
            last_auto_ui_debug_signature: None,
            last_auto_ui_sidebar_debug_signature: None,
            message_scroll_offset_override: Some(42.0),
            message_scroll_offset_session_id: Some("s1".into()),
            deferred_message_scroll_adjustment: 5.0,
            deferred_message_scroll_adjustment_session_id: Some("s1".into()),
            pending_height_cache_save: None,
            height_cache_scan: None,
            status_feedback: None,
        }
    }

    #[test]
    fn reset_heights_clears_state() {
        let mut state = test_layout_state();
        assert!(!state.message_row_heights.is_empty());
        assert!(state.message_scroll_offset_override.is_some());

        state.reset_heights_for_session("s2");
        assert!(state.message_row_heights.is_empty());
        assert_eq!(state.message_row_heights_session_id.as_deref(), Some("s2"));
        assert!(state.message_scroll_offset_override.is_none());
        assert_eq!(state.deferred_message_scroll_adjustment, 0.0);
    }

    #[test]
    fn heights_stale_detects_session_change() {
        let state = test_layout_state();
        assert!(!state.heights_stale("s1", 800.0));
        assert!(state.heights_stale("s2", 800.0));
    }

    #[test]
    fn heights_stale_detects_width_change() {
        let state = test_layout_state();
        assert!(!state.heights_stale("s1", 800.5)); // within tolerance
        assert!(state.heights_stale("s1", 802.0)); // beyond tolerance
    }

    #[test]
    fn prune_copy_feedback_removes_expired() {
        let mut state = test_layout_state();
        state.copy_feedback.insert(1, Instant::now());
        state
            .copy_feedback
            .insert(2, Instant::now() - Duration::from_secs(5));
        state.prune_copy_feedback();
        assert!(state.copy_feedback.contains_key(&1));
        assert!(!state.copy_feedback.contains_key(&2));
    }
}
