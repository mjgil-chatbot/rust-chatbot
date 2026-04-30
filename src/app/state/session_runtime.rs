impl SessionState {
    fn refresh_current_tracking(&mut self) {
        let Some(session_info) = self.current_session.as_ref().map(session_info_from_session)
        else {
            return;
        };
        if let Some(session) = self.current_session.as_ref() {
            self.session_cache.sync_current(session);
        }
        self.upsert_session_info_entry(session_info);
    }

    fn sync_current_view(&mut self, layout: &mut LayoutState) {
        self.system_prompt_text = self
            .current_session
            .as_ref()
            .and_then(|session| session.system_prompt.clone())
            .unwrap_or_default();
        self.runsc_dirs = self
            .current_session
            .as_ref()
            .map(|session| session.runsc_dirs.clone())
            .unwrap_or_default();
        for dir in self.runsc_dirs.clone() {
            crate::app::workspace_picker::push_recent_dir(&mut self.runsc_recent_dirs, &dir);
        }
        self.runsc_input_text = self.runsc_dirs.first().cloned().unwrap_or_default();
        self.runsc_image_input_text = self
            .current_session
            .as_ref()
            .and_then(|session| session.runsc_image.clone())
            .unwrap_or_default();
        layout.current_json_summary_preview = self
            .current_session
            .as_ref()
            .and_then(|session| session.json_summary.as_ref().map(|s| s.to_preview()));
        layout.current_json_summary_full = None;
        // Discover slash commands in a background thread to avoid blocking
        // the UI during session switches (filesystem scanning).
        let provider = self.provider;
        let working_dir = self
            .current_session
            .as_ref()
            .and_then(|session| session.launched_from.clone())
            .or_else(|| self.workspace_dir.clone());
        let (tx, rx) = mpsc::channel();
        self.slash_commands_rx = Some(rx);
        thread::spawn(move || {
            let commands = discover_slash_commands(provider, working_dir.as_deref());
            let _ = tx.send(commands);
        });
    }

    fn store_current_session(&mut self, session: ChatSession, layout: &mut LayoutState) -> String {
        let session_id = session.id.clone();
        self.current_session = Some(session);
        if let Some(current_session) = self.current_session.as_ref() {
            self.session_cache.sync_current(current_session);
        }
        self.sync_current_view(layout);
        session_id
    }

    fn clear_current_view(&mut self, layout: &mut LayoutState) {
        self.current_session = None;
        self.system_prompt_text.clear();
        self.runsc_dirs.clear();
        self.runsc_input_text.clear();
        self.runsc_image_input_text.clear();
        layout.current_json_summary_preview = None;
        layout.current_json_summary_full = None;
    }

    fn apply_stream_to_current(
        &mut self,
        chat_id: &str,
        response: &str,
        session_id: Option<String>,
        layout: &mut LayoutState,
        mark_height_cache_save: bool,
    ) -> bool {
        let Some(session) = self
            .current_session
            .as_mut()
            .filter(|session| session.id == chat_id)
        else {
            return false;
        };

        let response_changed = upsert_assistant_response(session, response);
        replace_provider_session_id(session, self.provider, session_id.as_deref());
        if mark_height_cache_save {
            layout.pending_height_cache_save = Some(session.id.clone());
        }
        // Atomically clear the optimistic "Thinking..." draft now that the real
        // assistant row is present. No-op if the draft is for a different chat
        // or already cleared by a terminal path.
        if response_changed && !response.is_empty() {
            self.clear_pending_draft_if_matches(chat_id);
        }
        response_changed
    }

    fn take_session_for_update(&mut self, session_id: &str) -> Result<Option<(ChatSession, bool)>> {
        if self.is_current_chat(session_id) {
            return Ok(self.current_session.take().map(|session| (session, true)));
        }

        let Some(ref data_manager) = self.data_manager else {
            return Ok(None);
        };
        data_manager
            .load_session(session_id)
            .map(|session| Some((session, false)))
    }

    fn commit_session_update(
        &mut self,
        session: ChatSession,
        is_current: bool,
        layout: &mut LayoutState,
    ) {
        if is_current {
            self.current_session = Some(session);
            let session_info = self.current_session.as_ref().map(session_info_from_session);
            if let Some(current_session) = self.current_session.as_ref() {
                self.session_cache.sync_current(current_session);
            }
            if let Some(session_info) = session_info {
                self.upsert_session_info_entry(session_info);
            }
            self.sync_current_view(layout);
            return;
        }

        self.upsert_session_info(&session);
        self.session_cache.insert(session.id.clone(), session);
    }

    fn refresh_session_index(&mut self, pending_delete_chat_ids: &HashSet<String>) -> Result<()> {
        if let Some(ref data_manager) = self.data_manager {
            let fresh = data_manager.list_sessions()?;
            // Stable in-place update: preserve the order of existing sessions so the
            // scroll offset stays valid, then prepend genuinely new ones.
            let mut fresh_map: HashMap<String, SessionInfo> = fresh
                .into_iter()
                .filter(|s| !pending_delete_chat_ids.contains(&s.id))
                .map(|s| (s.id.clone(), s))
                .collect();
            // Drop sessions that were deleted or are pending delete.
            let before_len = self.sessions.len();
            self.sessions.retain(|s| fresh_map.contains_key(&s.id));
            let removed = before_len - self.sessions.len();
            if removed > 0 {
                debug!(
                    removed,
                    remaining = self.sessions.len(),
                    "sessions pruned from list"
                );
            }
            // Update mutable fields without moving items in the list.
            for session in &mut self.sessions {
                if let Some(updated) = fresh_map.remove(&session.id) {
                    session.name = updated.name;
                    session.updated_at = updated.updated_at;
                    session.message_count = updated.message_count;
                    session.hidden = updated.hidden;
                }
            }
            // Prepend genuinely new sessions (sorted newest-first).
            let mut new_sessions: Vec<SessionInfo> = fresh_map.into_values().collect();
            if !new_sessions.is_empty() {
                debug!(count = new_sessions.len(), "new sessions prepended to list");
            }
            new_sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            new_sessions.extend(self.sessions.drain(..));
            self.sessions = new_sessions;

            self.session_cache.retain(|session_id, session| {
                self.sessions
                    .iter()
                    .find(|info| info.id == *session_id)
                    .map(|info| info.updated_at == session.updated_at)
                    .unwrap_or(false)
            });
        } else {
            self.sessions.clear();
            self.session_cache.clear();
        }
        Ok(())
    }
}

impl AppWorkerState {
    fn start_response_logging(&mut self, chat_id: &str, response: &str) -> bool {
        if response.is_empty() || self.response_started_chat_ids.contains(chat_id) {
            return false;
        }

        self.response_started_chat_ids.insert(chat_id.to_string());
        true
    }

    fn finish_response_logging(&mut self, chat_id: &str) -> bool {
        self.response_started_chat_ids.remove(chat_id)
    }
}

impl LayoutState {
    fn clear_session_artifacts(&mut self, session_id: &str) {
        self.session_height_cache.remove(session_id);
        if self.pending_height_cache_save.as_deref() == Some(session_id) {
            self.pending_height_cache_save = None;
        }
    }
}

impl AppState {
    pub(crate) fn refresh_current_session_tracking(&mut self) {
        self.session.refresh_current_tracking();
    }

    pub(crate) fn set_current_session(&mut self, session: ChatSession) {
        debug!(
            session_id=%session.id,
            session_name=%session.name,
            total_messages=session.messages.len(),
            has_system_prompt=session
                .system_prompt
                .as_deref()
                .map(|prompt| !prompt.trim().is_empty())
                .unwrap_or(false),
            last_message_at=?session.messages.last().map(|message| message.timestamp),
            "setting current session"
        );
        let session_id = self
            .session
            .store_current_session(session, &mut self.layout);
        self.session.system_prompt_dirty = self
            .session
            .system_prompt_dirty_session_id
            .as_deref()
            .is_some_and(|dirty_session_id| dirty_session_id == session_id);
        self.session.selected_session_id = Some(session_id.clone());
        self.session.loading_session_id = None;
        self.sync_workspace_state_from_current_session();
        self.refresh_mcp_config_status_in_background();
        self.spawn_height_cache_load_for_session(session_id.clone());
        self.session.pending_post_load_session_id = Some(session_id);
    }

    fn clear_current_session(&mut self) {
        self.session.clear_current_view(&mut self.layout);
        self.session.system_prompt_dirty = false;
        self.session.system_prompt_dirty_session_id = None;
        self.session.workspace_input_text = self.session.workspace_dir.clone().unwrap_or_default();
    }

    fn reload_current_session_from_disk(&mut self, session_id: &str) {
        self.spawn_reload_current_session_async(session_id.to_string());
    }

    fn sync_current_session_from_disk_if_safe(&mut self, chat_id: &str) -> bool {
        let current_snapshot = self.session.current_session.as_ref().map(|session| {
            (
                session.id.clone(),
                session.updated_at,
                session.messages.len(),
                session
                    .messages
                    .last()
                    .map(|message| message.is_user)
                    .unwrap_or(false),
            )
        });
        if !self.session.is_current_chat(chat_id) {
            debug!(
                chat_id=%chat_id,
                current_session_id=?self.session.current_session.as_ref().map(|session| session.id.as_str()),
                "skip current-session disk sync: target chat is not the active chat"
            );
            return false;
        }

        let loaded =
            self.session.data_manager.as_ref().and_then(|data_manager| {
                match data_manager.load_session(chat_id) {
                    Ok(session) => Some(session),
                    Err(err) => {
                        warn!(
                            chat_id=%chat_id,
                            error=%err,
                            "failed to load current session from disk during sync"
                        );
                        None
                    }
                }
            });
        let Some(loaded) = loaded else {
            debug!(
                chat_id=%chat_id,
                "skip current-session disk sync: no loadable session snapshot was available"
            );
            return false;
        };
        let local_is_dirty =
            self.worker.is_chat_processing(chat_id) || self.worker.is_locally_finalized(chat_id);
        let allow_dirty_reload = self
            .session
            .current_session
            .as_ref()
            .map(|current| local_is_dirty && session_transcript_is_strict_prefix(current, &loaded))
            .unwrap_or(false);
        let allow_appended_user_turn_reload = self
            .session
            .current_session
            .as_ref()
            .map(|current| local_is_dirty && session_reload_appends_user_turn(current, &loaded))
            .unwrap_or(false);
        if local_is_dirty && !(allow_dirty_reload || allow_appended_user_turn_reload) {
            debug!(
                chat_id=%chat_id,
                is_processing=self.worker.is_chat_processing(chat_id),
                is_locally_finalized=self.worker.is_locally_finalized(chat_id),
                current_updated_at=?current_snapshot.as_ref().map(|(_, updated_at, _, _)| *updated_at),
                current_message_count=?current_snapshot.as_ref().map(|(_, _, message_count, _)| *message_count),
                "skip current-session disk sync: local in-memory state is authoritative"
            );
            return false;
        }

        let should_replace = self
            .session
            .current_session
            .as_ref()
            .map(|current| {
                allow_dirty_reload
                    || allow_appended_user_turn_reload
                    || session_reload_is_safe(current, &loaded)
            })
            .unwrap_or(true);

        debug!(
            chat_id=%chat_id,
            current_updated_at=?current_snapshot.as_ref().map(|(_, updated_at, _, _)| *updated_at),
            current_message_count=?current_snapshot.as_ref().map(|(_, _, message_count, _)| *message_count),
            current_last_is_user=?current_snapshot.as_ref().map(|(_, _, _, last_is_user)| *last_is_user),
            loaded_updated_at=?loaded.updated_at,
            loaded_message_count=loaded.messages.len(),
            loaded_last_is_user=loaded.messages.last().map(|message| message.is_user).unwrap_or(false),
            allow_dirty_reload,
            allow_appended_user_turn_reload,
            should_replace,
            "evaluated current-session disk sync candidate"
        );

        if should_replace {
            debug!(
                chat_id=%chat_id,
                loaded_updated_at=?loaded.updated_at,
                loaded_message_count=loaded.messages.len(),
                "replacing current session from disk before applying worker output"
            );
            self.set_current_session(loaded);
            return true;
        }

        debug!(
            chat_id=%chat_id,
            "skip current-session disk sync: current in-memory session is newer than disk"
        );
        false
    }

    fn sync_current_session_from_disk_for_stream_begin(&mut self, chat_id: &str) -> bool {
        let current_snapshot = self.session.current_session.as_ref().map(|session| {
            (
                session.id.clone(),
                session.updated_at,
                session.messages.len(),
                session
                    .messages
                    .last()
                    .map(|message| message.is_user)
                    .unwrap_or(false),
            )
        });
        if !self.session.is_current_chat(chat_id) {
            debug!(
                chat_id=%chat_id,
                current_session_id=?self.session.current_session.as_ref().map(|session| session.id.as_str()),
                "skip stream-begin disk sync: target chat is not the active chat"
            );
            return false;
        }

        let loaded =
            self.session.data_manager.as_ref().and_then(|data_manager| {
                match data_manager.load_session(chat_id) {
                    Ok(session) => Some(session),
                    Err(err) => {
                        warn!(
                            chat_id=%chat_id,
                            error=%err,
                            "failed to load current session from disk during stream-begin sync"
                        );
                        None
                    }
                }
            });
        let Some(loaded) = loaded else {
            debug!(
                chat_id=%chat_id,
                "skip stream-begin disk sync: no loadable session snapshot was available"
            );
            return false;
        };

        let should_replace = self
            .session
            .current_session
            .as_ref()
            .map(|current| {
                session_reload_is_safe(current, &loaded)
                    || (loaded.id == current.id
                        && loaded.updated_at >= current.updated_at
                        && loaded.messages.len() > current.messages.len()
                        && loaded
                            .messages
                            .last()
                            .is_some_and(|message| message.is_user))
            })
            .unwrap_or(true);

        debug!(
            chat_id=%chat_id,
            current_updated_at=?current_snapshot.as_ref().map(|(_, updated_at, _, _)| *updated_at),
            current_message_count=?current_snapshot.as_ref().map(|(_, _, message_count, _)| *message_count),
            current_last_is_user=?current_snapshot.as_ref().map(|(_, _, _, last_is_user)| *last_is_user),
            loaded_updated_at=?loaded.updated_at,
            loaded_message_count=loaded.messages.len(),
            loaded_last_is_user=loaded.messages.last().map(|message| message.is_user).unwrap_or(false),
            should_replace,
            "evaluated stream-begin disk sync candidate"
        );

        if should_replace {
            debug!(
                chat_id=%chat_id,
                loaded_updated_at=?loaded.updated_at,
                loaded_message_count=loaded.messages.len(),
                "replacing current session from disk at stream begin"
            );
            self.set_current_session(loaded);
            return true;
        }

        false
    }

    fn handle_remote_stream_terminal_update(
        &mut self,
        chat_id: &str,
        response: &str,
        session_id: Option<String>,
        mark_height_cache_save: bool,
    ) {
        debug!(
            chat_id=%chat_id,
            response_len=response.len(),
            has_session_id=session_id.is_some(),
            mark_height_cache_save,
            current_session_id=?self.session.current_session.as_ref().map(|session| session.id.as_str()),
            current_message_count=?self.session.current_session.as_ref().map(|session| session.messages.len()),
            "handling remote terminal worker update"
        );
        // Skip disk reload when the local session has unsaved in-memory mutations.
        // This covers two cases:
        // 1. The worker is still running (streaming response in progress).
        // 2. The local worker just finalized: the in-memory session is already
        //    correct but the async disk save may not have completed yet.
        //    Reloading from disk in this window would overwrite the final message.
        let local_is_dirty = self.session.is_current_chat(chat_id)
            && (self.worker.is_chat_processing(chat_id)
                || self.worker.is_locally_finalized(chat_id));
        // Consume the locally-finalized marker now that we've seen the echo.
        self.worker.locally_finalized_chat_ids.remove(chat_id);
        if !local_is_dirty {
            let reloaded = self.sync_current_session_from_disk_if_safe(chat_id);
            debug!(
                chat_id=%chat_id,
                reloaded,
                "remote terminal update pre-sync decision completed"
            );
        } else {
            debug!(
                chat_id=%chat_id,
                is_processing=self.worker.is_chat_processing(chat_id),
                was_locally_finalized=local_is_dirty,
                "remote terminal update skipped current-session disk sync because local state was dirty"
            );
        }

        let response_changed = self.session.apply_stream_to_current(
            chat_id,
            response,
            session_id,
            &mut self.layout,
            mark_height_cache_save,
        );
        debug!(
            chat_id=%chat_id,
            response_changed,
            current_message_count=?self.session.current_session.as_ref().map(|session| session.messages.len()),
            current_last_is_user=?self.session.current_session.as_ref().and_then(|session| session.messages.last().map(|message| message.is_user)),
            "applied remote terminal worker update to current session"
        );

        // Terminal path: always clear any optimistic draft for this chat so
        // the "Thinking..." row disappears even if the response was empty.
        self.session.clear_pending_draft_if_matches(chat_id);

        self.refresh_current_session_tracking();
        self.layout.should_focus_input = true;
    }

    fn hydrate_current_session_from_active_worker(&mut self) {
        let Some(chat_id) = self
            .session
            .current_session
            .as_ref()
            .map(|session| session.id.clone())
        else {
            return;
        };
        let original_updated_at = self
            .session
            .current_session
            .as_ref()
            .map(|session| session.updated_at);
        let Some(response) = self.current_worker_response(&chat_id) else {
            return;
        };

        let response_changed = self.session.apply_stream_to_current(
            &chat_id,
            &response,
            None,
            &mut self.layout,
            false,
        );
        if response_changed {
            if let (Some(session), Some(updated_at)) = (
                self.session
                    .current_session
                    .as_mut()
                    .filter(|session| session.id == chat_id),
                original_updated_at,
            ) {
                session.updated_at = updated_at;
            }
        }
        self.refresh_current_session_tracking();
    }

    fn current_worker_response(&mut self, chat_id: &str) -> Option<String> {
        if let Some(response) = self
            .worker
            .cached_response(chat_id)
            .map(str::to_string)
            .filter(|response| !response.is_empty())
        {
            return Some(response);
        }

        let response = read_worker_response(chat_id);
        if response.is_empty() {
            return None;
        }

        self.worker
            .cache_response_snapshot(chat_id, response.clone());
        Some(response)
    }

    fn update_session_in_memory<F>(&mut self, session_id: &str, f: F) -> Option<ChatSession>
    where
        F: FnOnce(&mut ChatSession),
    {
        let (mut session, is_current) = match self.session.take_session_for_update(session_id) {
            Ok(Some(session)) => session,
            Ok(None) => return None,
            Err(err) => {
                error!(error=%err, session_id=%session_id, "failed to load session for update");
                return None;
            }
        };

        f(&mut session);
        let session_to_save = session.clone();
        self.session
            .commit_session_update(session, is_current, &mut self.layout);
        Some(session_to_save)
    }

    #[allow(dead_code)]
    fn with_session_mut<F>(&mut self, session_id: &str, f: F)
    where
        F: FnOnce(&mut ChatSession),
    {
        let Some(session_to_save) = self.update_session_in_memory(session_id, f) else {
            return;
        };
        self.spawn_save_session_async(session_to_save, false, None, false);
    }

    fn save_session_now(&self, session: ChatSession) -> Result<()> {
        if self.session.data_manager.is_none() {
            debug!(
                session_id=%session.id,
                "skip synchronous session save: no data manager is configured"
            );
            return Ok(());
        }
        debug!(
            session_id=%session.id,
            updated_at=?session.updated_at,
            message_count=session.messages.len(),
            last_is_user=session.messages.last().map(|message| message.is_user).unwrap_or(false),
            "persisting session synchronously"
        );
        DataManager::new(self.session.provider)?.save_session(&session)
    }

    fn with_session_mut_and_save_now<F>(
        &mut self,
        session_id: &str,
        f: F,
    ) -> Result<Option<ChatSession>>
    where
        F: FnOnce(&mut ChatSession),
    {
        let Some(session_to_save) = self.update_session_in_memory(session_id, f) else {
            return Ok(None);
        };
        self.save_session_now(session_to_save.clone())?;
        Ok(Some(session_to_save))
    }

    fn with_latest_session_mut_and_save_now<F>(
        &mut self,
        session_id: &str,
        persist_source: &'static str,
        f: F,
    ) -> Result<Option<ChatSession>>
    where
        F: FnOnce(&mut ChatSession),
    {
        let Some(ref data_manager) = self.session.data_manager else {
            debug!(
                session_id=%session_id,
                persist_source,
                "falling back to in-memory terminal save: no data manager is configured"
            );
            return self.with_session_mut_and_save_now(session_id, f);
        };

        let is_current = self.session.is_current_chat(session_id);
        let mut persisted_session = None;
        data_manager.mutate_session_with_source(session_id, true, persist_source, |session| {
            f(session);
            persisted_session = Some(session.clone());
        })?;
        let Some(persisted_session) = persisted_session else {
            return Ok(None);
        };

        debug!(
            session_id=%session_id,
            persist_source,
            is_current,
            updated_at=?persisted_session.updated_at,
            message_count=persisted_session.messages.len(),
            last_is_user=persisted_session
                .messages
                .last()
                .map(|message| message.is_user)
                .unwrap_or(false),
            history_fingerprint=%format!("{:016x}", persisted_session.history_fingerprint()),
            "persisted session from latest disk snapshot"
        );

        self.session
            .commit_session_update(persisted_session.clone(), is_current, &mut self.layout);
        Ok(Some(persisted_session))
    }

    fn handle_local_terminal_worker<F>(
        &mut self,
        chat_id: &str,
        response: &str,
        socket_message: WireMessage,
        status: &'static str,
        memory_final_state: &'static str,
        save_height_cache: bool,
        mutate_session: F,
    ) where
        F: FnOnce(&mut ChatSession, &str) -> bool,
    {
        debug!(
            chat_id=%chat_id,
            response_len=response.len(),
            status,
            save_height_cache,
            current_session_id=?self.session.current_session.as_ref().map(|session| session.id.as_str()),
            current_message_count=?self.session.current_session.as_ref().map(|session| session.messages.len()),
            "handling local terminal worker update"
        );
        let reloaded = self.sync_current_session_from_disk_if_safe(chat_id);
        debug!(
            chat_id=%chat_id,
            reloaded,
            "local terminal worker pre-sync decision completed"
        );
        let mut response_changed = false;
        if let Err(err) = self.with_latest_session_mut_and_save_now(
            chat_id,
            "rust_chatbot.local_terminal_worker",
            |session| {
                response_changed = mutate_session(session, response);
            },
        ) {
            error!(
                error=%err,
                chat_id=%chat_id,
                "failed to persist terminal session update before worker finalization"
            );
            return;
        }

        let is_current_chat = self.session.is_current_chat(chat_id);
        debug!(
            chat_id=%chat_id,
            response_changed,
            is_current_chat,
            current_message_count=?self.session.current_session.as_ref().map(|session| session.messages.len()),
            current_last_is_user=?self.session.current_session.as_ref().and_then(|session| session.messages.last().map(|message| message.is_user)),
            "local terminal worker session mutation persisted"
        );
        if is_current_chat && response_changed && save_height_cache {
            self.layout.pending_height_cache_save = Some(chat_id.to_string());
        }

        // Terminal path: clear any optimistic "Thinking..." draft.
        self.session.clear_pending_draft_if_matches(chat_id);

        // Prevent our echoed StreamEnd from reloading past the final local save.
        self.worker.mark_locally_finalized(chat_id);

        self.bump_sessions_version();
        self.sync_socket.send(&socket_message);
        self.log_response_end(chat_id, status);
        self.finalize_worker_slot(chat_id, None, Some(memory_final_state));
    }
}

#[path = "session_runtime_cleanup.rs"]
mod session_runtime_cleanup;

#[cfg(test)]
#[path = "session_runtime_tests.rs"]
mod session_runtime_tests;

#[cfg(test)]
#[path = "session_runtime_terminal_persistence_tests.rs"]
mod session_runtime_terminal_persistence_tests;

#[cfg(test)]
#[path = "session_runtime_prompt_tests.rs"]
mod session_runtime_prompt_tests;

#[cfg(test)]
#[path = "session_promotion_tests.rs"]
mod session_promotion_tests;
