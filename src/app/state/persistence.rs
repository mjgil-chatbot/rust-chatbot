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
impl Drop for AppState {
    fn drop(&mut self) {
        info!("app state dropped");
    }
}
