use super::*;
impl AppState {
    pub(crate) fn log_response_start_if_needed(&mut self, chat_id: &str, response: &str) {
        if !self.worker.start_response_logging(chat_id, response) {
            return;
        }
        debug!(session_id=%chat_id, "ai_response_start");
    }}
impl AppState {
    pub(crate) fn log_response_end(&mut self, chat_id: &str, status: &str) {
        self.worker.finish_response_logging(chat_id);
        debug!(session_id=%chat_id, status=%status, "ai_response_end");
    }}
impl AppState {
    pub(crate) fn refresh_sessions_list(&mut self) -> Result<()> {
        self.session
            .refresh_session_index(&self.worker.pending_delete_chat_ids)
    }}
impl AppState {
    pub(crate) fn load_first_available_session(&mut self) -> Result<()> {
        let Some(session_id) = self
            .session
            .sessions
            .first()
            .map(|session| session.id.clone())
        else {
            self.clear_current_session();
            return Ok(());
        };
        self.load_session(&session_id)
    }}
impl AppState {
    /// Whether the *currently viewed* chat session is being processed.
    /// Checks both the local tracking set and the mmap worker registry
    /// (authoritative source — catches workers spawned by other instances
    /// even if the socket event was missed).
    pub fn is_processing(&self) -> bool {
        let chat_id = match self.session.current_session.as_ref() {
            Some(s) => &s.id,
            None => return false,
        };

        if self.session.ui_io.pending_send_chat_ids.contains(chat_id) {
            return true;
        }

        if self.worker.processing_chat_ids.contains(chat_id) {
            return true;
        }

        self.is_session_locked(chat_id)
    }}
