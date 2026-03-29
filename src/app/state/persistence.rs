impl AppState {
    pub(crate) fn spawn_ensure_preallocated_sessions_async(&self) {
        let provider = self.session.provider;
        thread::spawn(move || {
            // ui-thread-guard: allow-blocking-start
            let result = (|| -> Result<()> {
                let data_manager = DataManager::new(provider)?;
                data_manager.ensure_preallocated_sessions(PREALLOCATED_SESSION_POOL_SIZE)
            })();
            // ui-thread-guard: allow-blocking-end

            if let Err(err) = result {
                warn!(error=%err, "failed to replenish preallocated session pool");
            }
        });
    }}
impl AppState {
    pub fn load_sessions(&mut self) -> Result<()> {
        debug!("loading sessions");
        self.refresh_sessions_list()?;

        if self.session.current_session.is_none() || self.session.sessions.is_empty() {
            self.load_first_available_session()?;
        }
        Ok(())
    }}
impl AppState {
    /// Load a specific session by ID
    pub fn load_session(&mut self, session_id: &str) -> Result<()> {
        if let Some(session) = self.cached_session(session_id) {
            info!(session_id=%session_id, "loading cached session");
            self.set_current_session(session);
            return Ok(());
        }

        if let Some(ref data_manager) = self.session.data_manager {
            info!(session_id=%session_id, "loading session");
            let session = data_manager.load_session(session_id)?;
            self.set_current_session(session);
        }

        Ok(())
    }}
impl Drop for AppState {
    fn drop(&mut self) {
        info!("app state dropped");
    }
}
