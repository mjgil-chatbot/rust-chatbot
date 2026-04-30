use super::*;

impl AppState {
    /// Clean up after a terminal worker slot (Done/Stopped/Error):
    /// free the mmap slot, reset the sequence tracker, remove worker files,
    /// remove from processing sets, and finalize any pending delete.
    pub(crate) fn finalize_worker_slot(
        &mut self,
        chat_id: &str,
        slot_index: Option<usize>,
        memory_final_state: Option<&str>,
    ) {
        if let Some(ref mut shm) = self.sync_shm {
            let slot_index = slot_index.or_else(|| {
                shm.read_worker_slots()
                    .into_iter()
                    .find(|slot| slot.chat_id == chat_id)
                    .map(|slot| slot.slot_index)
            });
            if let Some(slot_index) = slot_index {
                shm.free_worker_slot(slot_index);
            }
        }
        if let Some(memory_final_state) = memory_final_state {
            self.index_memory_for_chat(chat_id, memory_final_state);
        }
        let _ = std::fs::remove_dir_all(worker::worker_dir(chat_id));
        self.worker.clear_processing(chat_id);
        self.finalize_pending_delete_if_requested(chat_id);
        if self.is_current_chat(chat_id) {
            self.layout.should_focus_input = true;
        }
    }

    pub(crate) fn log_response_start_if_needed(&mut self, chat_id: &str, response: &str) {
        if !self.worker.start_response_logging(chat_id, response) {
            return;
        }
        debug!(session_id=%chat_id, "ai_response_start");
    }

    pub(crate) fn log_response_end(&mut self, chat_id: &str, status: &str) {
        self.worker.finish_response_logging(chat_id);
        debug!(session_id=%chat_id, status=%status, "ai_response_end");
    }

    pub(crate) fn refresh_sessions_list(&mut self) -> Result<()> {
        self.session
            .refresh_session_index(&self.worker.pending_delete_chat_ids)
    }

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
    }

    pub(crate) fn finalize_pending_delete_if_requested(&mut self, session_id: &str) {
        if !self.worker.pending_delete_chat_ids.contains(session_id) {
            return;
        }
        if self.worker.processing_chat_ids.contains(session_id)
            || self.is_session_locked(session_id)
        {
            return;
        }

        self.worker.pending_delete_chat_ids.remove(session_id);
        if let Err(err) = self.delete_session_now(session_id) {
            error!(error=%err, session_id=%session_id, "failed to finalize pending delete");
        }
    }

    pub(crate) fn delete_session_now(&mut self, session_id: &str) -> Result<()> {
        if let Some(ref data_manager) = self.session.data_manager {
            data_manager.delete_session(session_id)?;
        }

        self.layout.clear_session_artifacts(session_id);
        self.worker.clear_processing(session_id);

        let deleted_current = self
            .session
            .current_session
            .as_ref()
            .map(|session| session.id == session_id)
            .unwrap_or(false);
        if deleted_current {
            self.clear_current_session();
        }

        self.refresh_sessions_list()?;

        if deleted_current {
            if self.session.sessions.is_empty() {
                self.create_new_session(config::NEW_SESSION_NAME.to_string(), None)?;
            } else {
                self.load_first_available_session()?;
            }
        }

        self.sync_socket.send(&WireMessage::sessions_changed());
        if let Some(ref mut shm) = self.sync_shm {
            shm.bump_sessions_version();
        }

        Ok(())
    }

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
    }
}
