impl AppState {
    fn try_stop_worker_pid(&self, chat_id: &str, worker_pid: u32) -> bool {
        if !crate::sync::is_pid_alive(worker_pid) {
            return false;
        }

        info!(pid = worker_pid, chat_id = %chat_id, "sending SIGTERM to worker");
        unsafe {
            libc::kill(worker_pid as libc::pid_t, libc::SIGTERM);
        }
        true
    }

    fn send_message_text(&mut self, raw_query: String) -> Result<()> {
        if raw_query.trim().is_empty() || self.is_processing() {
            debug!(
                is_processing = self.is_processing(),
                input_len = raw_query.len(),
                "send_message_text skipped"
            );
            return Ok(());
        }

        let selected_workspace_dir = self.session.workspace_dir.clone();
        let Some(session) = self.session.current_session.as_mut() else {
            return Ok(());
        };
        if session.launched_from.is_none() {
            session.launched_from = selected_workspace_dir;
        }
        session.add_message_with_command(raw_query.clone(), true, None);
        let Some(message_timestamp_ms) = session
            .messages
            .last()
            .map(|message| message.timestamp.timestamp_millis())
        else {
            return Ok(());
        };
        let session_to_prepare = session.clone();
        let chat_id = session_to_prepare.id.clone();
        info!(session_id=%chat_id, "user_message_queued_for_background_send");

        self.session.pending_draft = Some(PendingDraft {
            chat_id: chat_id.clone(),
            user_message_timestamp_ms: message_timestamp_ms,
        });

        self.session.promote_session_to_top(&chat_id);
        self.refresh_current_session_tracking();
        self.bump_sessions_version();
        self.spawn_send_message_async(
            session_to_prepare,
            raw_query,
            PendingUiSend {
                chat_id,
                message_timestamp_ms,
            },
        );
        self.layout.should_focus_input = true;
        Ok(())
    }

    fn bump_sessions_version(&mut self) {
        if let Some(ref mut shm) = self.sync_shm {
            shm.bump_sessions_version();
        }
    }

    pub fn send_message(&mut self) -> Result<()> {
        if self.layout.input_text.trim().is_empty() || self.is_processing() {
            debug!(
                is_processing = self.is_processing(),
                input_len = self.layout.input_text.len(),
                "send_message skipped"
            );
            return Ok(());
        }

        let raw_query = self.layout.input_text.clone();
        self.layout.input_text.clear();
        self.session.input_history_index = None;
        self.send_message_text(raw_query)
    }

    // This helper is called from the background send worker, not from the
    // steady-state UI update/check loop.
    // ui-thread-guard: allow-blocking-start
    fn resolve_worker_binary_for_current_profile() -> Result<std::path::PathBuf> {
        let current_exe = std::env::current_exe()?;
        let current_exe = current_exe.canonicalize().ok().unwrap_or(current_exe);
        let current_dir = current_exe.parent().unwrap_or(Path::new("."));
        let worker_in_profile = current_dir.join("chatbot-worker");
        if worker_in_profile.exists() {
            return Ok(worker_in_profile);
        }

        let current_profile = current_dir.file_name().and_then(|name| name.to_str());
        let target_dir = current_dir.parent();
        if let (Some(profile), Some(target_dir)) = (current_profile, target_dir) {
            let other_profile = match profile {
                "debug" => Some("release"),
                "release" => Some("debug"),
                _ => None,
            };
            if let Some(other_profile) = other_profile {
                if target_dir.file_name().and_then(|name| name.to_str()) == Some("target") {
                    let mismatch = target_dir.join(other_profile).join("chatbot-worker");
                    if mismatch.exists() {
                        anyhow::bail!(
                            "chatbot-worker was not found in the current {} build directory {} for {}. Found only {} instead; mixed debug/release launches are not allowed.",
                            profile,
                            current_dir.display(),
                            current_exe.display(),
                            mismatch.display(),
                        );
                    }
                }
            }
        }

        anyhow::bail!(
            "chatbot-worker was not found alongside the current executable {} in {}. Build chatbot-worker in the same profile first.",
            current_exe.display(),
            current_dir.display(),
        );
    }
    // ui-thread-guard: allow-blocking-end

    pub fn stop_current_process(&mut self) {
        let Some(chat_id) = self
            .session
            .current_session
            .as_ref()
            .map(|session| session.id.clone())
        else {
            self.layout.should_focus_input = true;
            return;
        };

        if let Some(worker_pid) = self
            .worker
            .local_worker_pids
            .get(&chat_id)
            .copied()
            .or_else(|| self.worker.pending_worker_pids.get(&chat_id).copied())
        {
            let _ = self.try_stop_worker_pid(&chat_id, worker_pid);
        }

        self.layout.should_focus_input = true;
    }

    pub(crate) fn enqueue_event(&mut self, event: AppEvent) {
        self.session.event_queue.push_back(event);
    }

    fn maybe_continue_forge_loop(&mut self, chat_id: &str) {
        if !self.worker.should_auto_continue_forge_loop(chat_id) {
            return;
        }
        if !self.session.is_current_chat(chat_id) {
            return;
        }

        self.worker.mark_forge_loop_acknowledged(chat_id);
        if let Err(err) = self.send_message_text("Y".to_string()) {
            warn!(error = %err, chat_id, "forge loop auto-continue failed");
        }
    }

    pub fn check_for_response(&mut self) {
        // Phase 1: ingest async ingress into the local event queue.
        self.drain_ui_io_results();
        self.drain_history_results();
        self.drain_sync_events_async();
        self.drain_worker_discovery_events();
        self.drain_workspace_completion_results();
        self.drain_height_cache_load_results();
        self.drain_mcp_config_status_results();
        self.drain_path_resolution_results();

        // Phase 2: deterministic replay of queued events before local polling effects.
        self.process_event_queue();

        // Phase 3: local polling/reconciliation side effects.
        self.drain_session_search_results();
        self.drain_slash_command_discovery();
        self.drain_session_preload();
        self.drain_pending_session_post_load();
        self.poll_history_sync();
        self.poll_shared_session_changes_async();
        self.poll_worker_slots();
    }

    fn process_event_queue(&mut self) {
        while let Some(event) = self.session.event_queue.pop_front() {
            match event {
                AppEvent::UiAction(action) => self.handle_ui_action(action),
                AppEvent::UiIo(result) => self.handle_ui_io_result(result),
                AppEvent::Sync(event) => self.handle_sync_event(event),
                AppEvent::WorkerDiscovery(event) => self.handle_worker_discovery_event(event),
            }
        }
    }

    fn should_drop_stale_result(
        &self,
        kind: &str,
        request_id: Option<u64>,
        latest_request_id: u64,
        targeted_session_id: Option<&str>,
        keep_active_target: bool,
    ) -> bool {
        let Some(request_id) = request_id else {
            return false;
        };

        let is_targeted_session_still_active = targeted_session_id
            .map(|session_id| {
                self.session.loading_session_id.as_deref() == Some(session_id)
                    || self.session.selected_session_id.as_deref() == Some(session_id)
                    || self
                        .session
                        .current_session
                        .as_ref()
                        .is_some_and(|session| session.id.as_str() == session_id)
            })
            .unwrap_or(false);

        if request_id >= latest_request_id {
            return false;
        }
        if keep_active_target && is_targeted_session_still_active {
            return false;
        }
        debug!(
            request_id,
            latest_request_id, kind, "dropping stale UI result event"
        );
        true
    }

    fn begin_session_load(&mut self, session_id: String) -> bool {
        self.session.ui_io.cancel_pending_create_session();
        let is_current_chat = self
            .session
            .current_session
            .as_ref()
            .map(|session| session.id == session_id)
            .unwrap_or(false);
        self.session.selected_session_id = Some(session_id.clone());
        self.session.loading_session_id = if is_current_chat {
            None
        } else {
            if let Some(cached_session) = self.session.cached_session(&session_id) {
                debug!(
                    session_id=%session_id,
                    "activating cached session before async reload"
                );
                self.set_current_session(cached_session);
            }
            Some(session_id)
        };
        is_current_chat
    }

    fn mark_sidebar_session_load_complete(&mut self, completed_session_id: Option<&str>) {
        let Some(completed_session_id) = completed_session_id else {
            return;
        };
        if self.session.loading_session_id.as_deref() == Some(completed_session_id) {
            self.session.loading_session_id = None;
        }
        if self.session.selected_session_id.is_none()
            || self.session.selected_session_id.as_deref() == Some(completed_session_id)
        {
            self.session.selected_session_id = Some(completed_session_id.to_string());
        }
    }

    fn mark_sidebar_session_load_failed(&mut self, failed_session_id: &str) {
        if self.session.loading_session_id.as_deref() == Some(failed_session_id) {
            self.session.loading_session_id = None;
        }
        if self
            .session
            .selected_session_id
            .as_deref()
            .is_some_and(|session_id| session_id == failed_session_id)
        {
            self.session.selected_session_id = self
                .session
                .current_session
                .as_ref()
                .map(|session| session.id.clone());
        }
    }

    fn handle_ui_action(&mut self, action: AppUiAction) {
        match action {
            AppUiAction::SubmitMessage => {
                if let Err(err) = self.send_message() {
                    warn!(error = %err, "submit message action failed");
                }
            }
            AppUiAction::StopCurrentProcess => {
                self.stop_current_process();
            }
            AppUiAction::LoadSession(session_id) => {
                let is_current_chat = self.begin_session_load(session_id.clone());
                if !is_current_chat {
                    self.spawn_load_session_async(session_id);
                }
            }
            AppUiAction::CreateSession {
                name,
                system_prompt,
            } => {
                self.spawn_create_session_async(name, system_prompt);
            }
            AppUiAction::RenameSession {
                session_id,
                new_name,
            } => {
                self.spawn_rename_session_async(session_id, new_name);
            }
            AppUiAction::DeleteSession(session_id) => {
                self.spawn_delete_session_async(session_id);
            }
            AppUiAction::SaveCurrentSystemPrompt {
                session_id,
                system_prompt,
            } => {
                let Some(session_to_save) = self.update_session_in_memory(&session_id, |session| {
                    session.set_system_prompt(system_prompt.clone());
                }) else {
                    return;
                };
                let request_id = Some(self.session.ui_io.next_save_request_id());
                self.spawn_save_session_async(session_to_save, false, request_id, true);
            }
            AppUiAction::SaveCurrentOmniForgeModel {
                session_id,
                model_id,
            } => {
                let Some(session_to_save) = self.update_session_in_memory(&session_id, |session| {
                    session.omniforge_model_id = Some(model_id.clone());
                }) else {
                    return;
                };
                let request_id = Some(self.session.ui_io.next_save_request_id());
                self.spawn_save_session_async(session_to_save, false, request_id, false);
            }
        }
    }

    fn handle_ui_io_result(&mut self, result: UiIoResult) {
        match result {
            UiIoResult::SessionListPatch { request_id, patch } => {
                if self.should_drop_stale_result(
                    "SessionListPatch",
                    request_id,
                    self.session.ui_io.latest_list_request_id,
                    None,
                    false,
                ) {
                    return;
                }
                if self.session.ui_io.session_refresh_in_flight {
                    self.session.ui_io.session_refresh_in_flight = false;
                }
                self.apply_sidebar_patch(patch);
            }
            UiIoResult::SelectedSessionLoad {
                request_id,
                requested_session_id,
                load,
            } => {
                if request_id.is_none() {
                    self.session.ui_io.session_refresh_in_flight = false;
                }
                let current_selection = load
                    .current_session
                    .as_ref()
                    .map(|session| session.id.clone());
                let is_create_session_load = request_id.is_some()
                    && requested_session_id.is_none()
                    && load.requested_session_id.is_none()
                    && current_selection.is_some();
                let requested_session_id = requested_session_id
                    .or_else(|| load.requested_session_id.clone())
                    .or(current_selection);
                let latest_load_request_id = self
                    .session
                    .ui_io
                    .latest_load_request_id_for_session(requested_session_id.as_deref());
                if self.should_drop_stale_result(
                    "SelectedSessionLoad",
                    request_id,
                    latest_load_request_id,
                    requested_session_id.as_deref(),
                    false,
                ) {
                    return;
                }
                let is_expected_session = is_create_session_load
                    || requested_session_id
                        .as_deref()
                        .map(|session_id| {
                            self.session.loading_session_id.as_deref() == Some(session_id)
                                || self.session.selected_session_id.as_deref() == Some(session_id)
                                || self
                                    .session
                                    .current_session
                                    .as_ref()
                                    .map(|session| session.id.as_str() == session_id)
                                    .unwrap_or(false)
                        })
                        .unwrap_or(true);
                if !is_expected_session {
                    return;
                }

                if is_create_session_load {
                    self.session.ui_io.clear_pending_create_session(request_id);
                }
                self.mark_sidebar_session_load_complete(requested_session_id.as_deref());
                self.apply_selected_session_reload(load);
                if is_create_session_load {
                    self.sync_socket.send(&WireMessage::sessions_changed());
                    self.bump_sessions_version();
                    self.spawn_ensure_preallocated_sessions_async();
                }
            }
            UiIoResult::SessionLoadFailed {
                request_id,
                session_id,
                requested_session_id,
                error,
            } => {
                let failed_session_id = requested_session_id
                    .as_deref()
                    .unwrap_or(session_id.as_str());
                let latest_load_request_id = self
                    .session
                    .ui_io
                    .latest_load_request_id_for_session(Some(failed_session_id));
                if self.should_drop_stale_result(
                    "SessionLoadFailed",
                    request_id,
                    latest_load_request_id,
                    Some(failed_session_id),
                    true,
                ) {
                    return;
                }
                self.mark_sidebar_session_load_failed(failed_session_id);
                if !self.session.ui_io.session_refresh_in_flight {
                    self.spawn_refresh_sessions_async();
                }
                warn!(
                    session_id=%session_id,
                    requested_session_id=failed_session_id,
                    error=%error,
                    "background session load failed"
                );
                self.layout.status_feedback = Some(UiStatusFeedback {
                    at: Instant::now(),
                    is_error: true,
                    message: format!("Failed to load session {failed_session_id}: {error}"),
                });
            }
            UiIoResult::SessionSaveFailed {
                request_id,
                session_id,
                operation,
                is_system_prompt_save,
                error,
            } => {
                let is_targeted_system_prompt_session = self
                    .session
                    .system_prompt_dirty_session_id
                    .as_deref()
                    .is_some_and(|dirty_session_id| dirty_session_id == session_id.as_str());
                let latest_save_request_id = if is_system_prompt_save {
                    self.session
                        .ui_io
                        .latest_save_request_id_for_session(Some(session_id.as_str()))
                } else {
                    match operation {
                        UiIoSaveOperation::RenameSession => self
                            .session
                            .ui_io
                            .latest_list_request_id_for_session(Some(session_id.as_str())),
                        UiIoSaveOperation::DeleteSession => self
                            .session
                            .ui_io
                            .latest_load_request_id_for_session(Some(session_id.as_str())),
                        UiIoSaveOperation::RefreshSessionList
                        | UiIoSaveOperation::SaveSession
                        | UiIoSaveOperation::SystemPromptSave => self
                            .session
                            .ui_io
                            .latest_save_request_id_for_session(Some(session_id.as_str())),
                        UiIoSaveOperation::CreateSession => self
                            .session
                            .ui_io
                            .latest_load_request_id_for_session(Some(session_id.as_str())),
                    }
                };
                if self.should_drop_stale_result(
                    "SessionSaveFailed",
                    request_id,
                    latest_save_request_id,
                    Some(session_id.as_str()),
                    false,
                ) {
                    return;
                }
                warn!(
                    request_id=request_id,
                    session_id=%session_id,
                    is_system_prompt_save=%is_system_prompt_save,
                    operation=%save_operation_label(operation),
                    error=%error,
                    "background session save failed"
                );
                if matches!(operation, UiIoSaveOperation::CreateSession) {
                    self.session.ui_io.clear_pending_create_session(request_id);
                }
                let session_id_for_feedback = session_id.clone();
                if is_system_prompt_save && is_targeted_system_prompt_session {
                    self.session.system_prompt_dirty = true;
                    self.session.system_prompt_dirty_session_id = Some(session_id);
                }
                self.layout.status_feedback = Some(UiStatusFeedback {
                    at: Instant::now(),
                    is_error: true,
                    message: match operation {
                        UiIoSaveOperation::CreateSession => {
                            "Failed to create session: ".to_string() + &error
                        }
                        UiIoSaveOperation::RenameSession => {
                            format!("Failed to rename session {session_id_for_feedback}: {error}")
                        }
                        UiIoSaveOperation::DeleteSession => {
                            format!("Failed to delete session {session_id_for_feedback}: {error}")
                        }
                        UiIoSaveOperation::RefreshSessionList => {
                            format!("Failed to refresh session list after save: {error}")
                        }
                        UiIoSaveOperation::SystemPromptSave | UiIoSaveOperation::SaveSession => {
                            format!("Failed to save session {session_id_for_feedback}: {error}")
                        }
                    },
                });
            }
            UiIoResult::SessionSaveSuccess {
                request_id,
                session_id,
                is_system_prompt_save,
            } => {
                if request_id.is_none() && !is_system_prompt_save {
                    return;
                }
                let latest_save_request_id = self
                    .session
                    .ui_io
                    .latest_save_request_id_for_session(Some(session_id.as_str()));
                if self.should_drop_stale_result(
                    "SessionSaveSuccess",
                    request_id,
                    latest_save_request_id,
                    Some(session_id.as_str()),
                    false,
                ) {
                    return;
                }
                if is_system_prompt_save {
                    if self
                        .session
                        .system_prompt_dirty_session_id
                        .as_deref()
                        .is_some_and(|dirty_session_id| dirty_session_id == session_id)
                    {
                        self.session.system_prompt_dirty = false;
                        self.session.system_prompt_dirty_session_id = None;
                    }
                    self.layout.status_feedback = Some(UiStatusFeedback {
                        at: Instant::now(),
                        is_error: false,
                        message: "System prompt saved".to_string(),
                    });
                } else {
                    self.layout.status_feedback = Some(UiStatusFeedback {
                        at: Instant::now(),
                        is_error: false,
                        message: "Session saved".to_string(),
                    });
                }
            }
            UiIoResult::SessionRefreshFailed { error } => {
                self.session.ui_io.session_refresh_in_flight = false;
                warn!(error=%error, "background session refresh failed");
                self.layout.status_feedback = Some(UiStatusFeedback {
                    at: Instant::now(),
                    is_error: true,
                    message: format!("Failed to refresh sessions: {error}"),
                });
            }
            UiIoResult::SendCompleted {
                chat_id,
                message_timestamp_ms,
                command_str,
                retrieved_memory,
                worker_pid,
            } => {
                self.session.ui_io.pending_send_chat_ids.remove(&chat_id);
                self.worker.processing_chat_ids.insert(chat_id.clone());
                self.worker
                    .local_processing_chat_ids
                    .insert(chat_id.clone());
                self.worker
                    .local_worker_pids
                    .insert(chat_id.clone(), worker_pid);
                self.worker
                    .pending_worker_pids
                    .insert(chat_id.clone(), worker_pid);
                self.worker.response_started_chat_ids.remove(&chat_id);
                self.sync_socket.send(&WireMessage::begin(
                    &chat_id,
                    self.session.provider.as_str(),
                ));
                if let Some(session) = self
                    .session
                    .current_session
                    .as_mut()
                    .filter(|session| session.id == chat_id)
                {
                    let _ = update_message_request_metadata(
                        session,
                        message_timestamp_ms,
                        &command_str,
                        retrieved_memory.as_deref(),
                    );
                }
                if let Some(session) = self.session.session_cache.get_mut(&chat_id) {
                    let _ = update_message_request_metadata(
                        session,
                        message_timestamp_ms,
                        &command_str,
                        retrieved_memory.as_deref(),
                    );
                }
                self.refresh_current_session_tracking();
                self.bump_sessions_version();
            }
            UiIoResult::SendFailed { chat_id, error } => {
                self.session.ui_io.pending_send_chat_ids.remove(&chat_id);
                // Send failed before the worker ever produced output — drop
                // the optimistic "Thinking..." draft so the error message
                // (appended below) is the only terminal visual.
                self.session.clear_pending_draft_if_matches(&chat_id);
                self.worker.clear_processing(&chat_id);
                self.sync_socket.send(&WireMessage::stopped(
                    &chat_id,
                    self.session.provider.as_str(),
                    "",
                ));

                let mut session_to_save = None;
                if let Some(session) = self
                    .session
                    .current_session
                    .as_mut()
                    .filter(|session| session.id == chat_id)
                {
                    session.add_message(format!("Error: {error}"), false);
                    session_to_save = Some(session.clone());
                } else if let Some(session) = self.session.session_cache.get_mut(&chat_id) {
                    session.add_message(format!("Error: {error}"), false);
                    session_to_save = Some(session.clone());
                }
                if let Some(session) = session_to_save {
                    self.spawn_save_session_async(session, false, None, false);
                }
                self.refresh_current_session_tracking();
                self.bump_sessions_version();
                if self.is_current_chat(&chat_id) {
                    self.layout.should_focus_input = true;
                }
            }
        }
    }

    fn drain_slash_command_discovery(&mut self) {
        let commands = self
            .session
            .slash_commands_rx
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(commands) => Some(commands),
                Err(_) => None,
            });
        if let Some(commands) = commands {
            self.session.slash_commands = commands;
            self.session.slash_commands_rx = None;
        }
    }
}

#[cfg(test)]
#[path = "messaging_create_session_tests.rs"]
mod create_session_tests;

#[cfg(test)]
#[path = "messaging_stop_tests.rs"]
mod stop_tests;

#[cfg(test)]
#[path = "messaging_flow_tests.rs"]
mod flow_tests;
