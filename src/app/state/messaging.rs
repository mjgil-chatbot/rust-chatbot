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
    fn bump_sessions_version(&mut self) {
        if let Some(ref mut shm) = self.sync_shm {
            shm.bump_sessions_version();
        }
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
