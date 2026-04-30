use super::*;

impl AppState {
    pub(crate) fn poll_shared_session_changes_async(&mut self) {
        let sessions_changed = self
            .sync_shm
            .as_mut()
            .map(|shm| shm.sessions_changed())
            .unwrap_or(false);
        if sessions_changed {
            debug!("sync shm: sessions changed notification");
            // Passive refresh: sidebar-only. Explicit current-session reloads
            // come through StreamBegin / explicit user actions instead.
            self.spawn_refresh_sessions_async();
        }
    }

    pub(crate) fn drain_sync_events_async(&mut self) {
        let events: Vec<SyncEvent> = self.sync_events.try_iter().collect();

        for event in events {
            self.enqueue_event(AppEvent::Sync(event));
        }
    }

    pub(crate) fn handle_sync_event(&mut self, event: SyncEvent) {
        match event {
            SyncEvent::StreamBegin { chat_id, provider } => {
                if !socket_provider_matches(&provider, self.session.provider) {
                    return;
                }
                debug!(
                    chat_id=%chat_id,
                    provider=%provider,
                    current_session_id=?self.session.current_session.as_ref().map(|session| session.id.as_str()),
                    current_message_count=?self.session.current_session.as_ref().map(|session| session.messages.len()),
                    "socket: remote stream began"
                );
                self.worker.processing_chat_ids.insert(chat_id.clone());
                let is_our_chat = self
                    .session
                    .current_session
                    .as_ref()
                    .map(|session| session.id == chat_id)
                    .unwrap_or(false);
                if is_our_chat {
                    if !self.session.system_prompt_dirty {
                        let reloaded =
                            self.sync_current_session_from_disk_for_stream_begin(&chat_id);
                        debug!(
                            chat_id=%chat_id,
                            reloaded,
                            system_prompt_dirty=self.session.system_prompt_dirty,
                            "socket: remote stream begin current-chat sync completed"
                        );
                    } else {
                        debug!(
                            chat_id=%chat_id,
                            "socket: remote stream begin skipped current-chat sync because system prompt is dirty"
                        );
                    }
                } else {
                    debug!(
                        chat_id=%chat_id,
                        "socket: remote stream begin is for another chat; refreshing sidebar only"
                    );
                    self.spawn_refresh_sessions_async();
                }
            }
            SyncEvent::StreamData {
                chat_id,
                provider,
                response,
            } => {
                if !socket_provider_matches(&provider, self.session.provider) {
                    return;
                }
                debug!(
                    chat_id=%chat_id,
                    provider=%provider,
                    response_len=response.len(),
                    current_session_id=?self.session.current_session.as_ref().map(|session| session.id.as_str()),
                    current_message_count=?self.session.current_session.as_ref().map(|session| session.messages.len()),
                    "socket: remote stream data received"
                );
                let mut response_changed = false;
                if let Some(ref mut session) = self.session.current_session {
                    if session.id == chat_id {
                        response_changed = apply_stream_response(session, &response);
                    }
                }
                debug!(
                    chat_id=%chat_id,
                    response_changed,
                    current_message_count=?self.session.current_session.as_ref().map(|session| session.messages.len()),
                    current_last_is_user=?self.session.current_session.as_ref().and_then(|session| session.messages.last().map(|message| message.is_user)),
                    "socket: remote stream data applied"
                );
                if response_changed {
                    // First real chunk arrived for this chat — drop the
                    // optimistic "Thinking..." draft atomically.
                    self.session.clear_pending_draft_if_matches(&chat_id);
                    self.refresh_current_session_tracking();
                }
            }
            SyncEvent::StreamFeedback {
                chat_id,
                provider,
                response,
                is_error,
            } => {
                if !socket_provider_matches(&provider, self.session.provider) {
                    return;
                }
                if self
                    .session
                    .current_session
                    .as_ref()
                    .map(|session| session.id == chat_id)
                    .unwrap_or(false)
                {
                    debug!(
                        chat_id=%chat_id,
                        provider=%provider,
                        is_error,
                        feedback_len=response.len(),
                        current_session_id=?self.session.current_session.as_ref().map(|session| session.id.as_str()),
                        "socket: remote stream feedback"
                    );
                    self.layout.status_feedback = Some(UiStatusFeedback {
                        at: Instant::now(),
                        is_error,
                        message: response,
                    });
                }
            }
            SyncEvent::StreamEnd {
                chat_id,
                provider,
                response,
                session_id,
            } => {
                if !socket_provider_matches(&provider, self.session.provider) {
                    return;
                }
                debug!(
                    chat_id=%chat_id,
                    provider=%provider,
                    response_len=response.len(),
                    has_session_id=session_id.is_some(),
                    current_session_id=?self.session.current_session.as_ref().map(|session| session.id.as_str()),
                    "socket: remote stream ended"
                );
                let is_our_chat = self
                    .session
                    .current_session
                    .as_ref()
                    .map(|session| session.id == chat_id)
                    .unwrap_or(false);
                if is_our_chat {
                    self.handle_remote_stream_terminal_update(
                        &chat_id, &response, session_id, true,
                    );
                } else {
                    self.spawn_refresh_sessions_async();
                }
                self.worker.processing_chat_ids.remove(&chat_id);
                self.log_response_start_if_needed(&chat_id, &response);
                self.log_response_end(&chat_id, "done");
                self.finalize_pending_delete_if_requested(&chat_id);
            }
            SyncEvent::StreamStopped {
                chat_id,
                provider,
                response,
            } => {
                if !socket_provider_matches(&provider, self.session.provider) {
                    return;
                }
                debug!(
                    chat_id=%chat_id,
                    provider=%provider,
                    response_len=response.len(),
                    current_session_id=?self.session.current_session.as_ref().map(|session| session.id.as_str()),
                    "socket: remote stream stopped"
                );
                let is_our_chat = self
                    .session
                    .current_session
                    .as_ref()
                    .map(|session| session.id == chat_id)
                    .unwrap_or(false);
                if is_our_chat {
                    self.handle_remote_stream_terminal_update(&chat_id, &response, None, false);
                } else {
                    self.spawn_refresh_sessions_async();
                }
                self.worker.processing_chat_ids.remove(&chat_id);
                self.log_response_start_if_needed(&chat_id, &response);
                self.log_response_end(&chat_id, "stopped");
                self.finalize_pending_delete_if_requested(&chat_id);
            }
            SyncEvent::SessionsChanged => {
                debug!("socket: sessions changed notification");
                // Passive refresh: sidebar-only.
                self.spawn_refresh_sessions_async();
            }
            SyncEvent::Disconnected => {
                warn!("socket: disconnected from broker");
            }
            SyncEvent::Reconnected => {
                info!("socket: connected to broker");
            }
        }
    }
}
