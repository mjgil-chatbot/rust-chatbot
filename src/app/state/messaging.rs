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
    }}
impl AppState {
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
    }}
impl AppState {
    fn bump_sessions_version(&mut self) {
        if let Some(ref mut shm) = self.sync_shm {
            shm.bump_sessions_version();
        }
    }}
impl AppState {
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
    }}
impl AppState {
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
    }}
impl AppState {
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
    }}
impl AppState {
    pub(crate) fn enqueue_event(&mut self, event: AppEvent) {
        self.session.event_queue.push_back(event);
    }}
impl AppState {
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
    }}
impl AppState {
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
    }}
impl AppState {
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
    }}
impl AppState {
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
    }}
impl AppState {
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
    }}
impl AppState {
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
    }}
#[cfg(test)]
#[path = "messaging_create_session_tests.rs"]
mod create_session_tests;
#[cfg(test)]
#[path = "messaging_stop_tests.rs"]
mod stop_tests;
#[cfg(test)]
#[path = "messaging_flow_tests.rs"]
mod flow_tests;
