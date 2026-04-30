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
    }

    pub fn load_sessions(&mut self) -> Result<()> {
        debug!("loading sessions");
        self.refresh_sessions_list()?;

        if self.session.current_session.is_none() || self.session.sessions.is_empty() {
            self.load_first_available_session()?;
        }
        Ok(())
    }

    /// Create a new chat session
    pub fn create_new_session(
        &mut self,
        name: String,
        system_prompt: Option<String>,
    ) -> Result<()> {
        if let Some(ref data_manager) = self.session.data_manager {
            info!(session_name=%name, "creating new session");
            let session = data_manager.create_session(name, system_prompt.clone())?;
            // Insert directly into the session list instead of re-reading all files.
            let info = SessionInfo {
                id: session.id.clone(),
                name: session.name.clone(),
                created_at: session.created_at,
                updated_at: session.updated_at,
                message_count: 0,
                hidden: false,
            };
            let new_session_id = info.id.clone();
            self.session.sessions.insert(0, info);
            self.session.selected_session_id = Some(new_session_id);
            self.set_current_session(session);
        }
        // Notify other instances to reload their session list.
        self.sync_socket.send(&WireMessage::sessions_changed());
        if let Some(ref mut shm) = self.sync_shm {
            shm.bump_sessions_version();
        }
        self.spawn_ensure_preallocated_sessions_async();
        Ok(())
    }

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
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        info!("app state dropped");
    }
}
