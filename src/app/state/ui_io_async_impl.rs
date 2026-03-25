fn send_ui_io_result(
    tx: &mpsc::Sender<UiIoResult>,
    repaint_ctx: Option<&eframe::egui::Context>,
    result: UiIoResult,
) {
    if tx.send(result).is_ok() {
        if let Some(ctx) = repaint_ctx {
            ctx.request_repaint();
        }
    }
}

fn reset_worker_dir_for_send(chat_id: &str) -> Result<std::path::PathBuf> {
    let worker_dir = worker::worker_dir(chat_id);
    match std::fs::remove_dir_all(&worker_dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to clear stale {}", worker_dir.display()));
        }
    }
    std::fs::create_dir_all(&worker_dir)
        .with_context(|| format!("failed to create {}", worker_dir.display()))?;
    Ok(worker_dir)
}

impl AppState {
    /// Spawn a sidebar-only passive refresh. Produces a `SessionListPatch`
    /// and never includes the current session payload.
    ///
    /// Per the broker-sync design: passive refreshes update sidebar metadata
    /// only. Explicit current-session reloads go through `spawn_load_session_async`
    /// or `spawn_reload_current_session_async`, which produce `SelectedSessionLoad`.
    pub(crate) fn spawn_refresh_sessions_async(&mut self) {
        if self.session.ui_io.session_refresh_in_flight {
            return;
        }
        self.session.ui_io.session_refresh_in_flight = true;
        let request_id = self.session.ui_io.next_list_request_id();

        let provider = self.session.provider;
        let tx = self.session.ui_io.tx.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();

        thread::spawn(move || {
            let result =
                load_session_list_patch(provider).map(|patch| UiIoResult::SessionListPatch {
                    request_id: Some(request_id),
                    patch,
                });

            match result {
                Ok(result) => send_ui_io_result(&tx, repaint_ctx.as_ref(), result),
                Err(err) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionRefreshFailed {
                        error: format!("{err:#}"),
                    },
                ),
            }
        });
    }

    pub(crate) fn spawn_load_session_async(&mut self, session_id: String) {
        let request_id = self.session.ui_io.next_load_request_id();
        self.session
            .ui_io
            .record_load_request_id_for_session(&session_id, request_id);
        let provider = self.session.provider;
        let data_manager = self.session.data_manager.clone();
        let tx = self.session.ui_io.tx.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();

        thread::spawn(move || {
            let result = (|| -> Result<SelectedSessionLoad> {
                let data_manager = match data_manager {
                    Some(data_manager) => data_manager,
                    None => DataManager::new(provider)?,
                };
                let session = data_manager.load_session(&session_id)?;
                Ok(SelectedSessionLoad {
                    sessions: None,
                    current_session: Some(session),
                    requested_session_id: Some(session_id.clone()),
                })
            })();

            match result {
                Ok(load) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SelectedSessionLoad {
                        request_id: Some(request_id),
                        requested_session_id: Some(session_id.clone()),
                        load,
                    },
                ),
                Err(err) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionLoadFailed {
                        request_id: Some(request_id),
                        session_id: session_id.clone(),
                        requested_session_id: Some(session_id),
                        error: format!("load session failed: {err:#}"),
                    },
                ),
            }
        });
    }

    pub(crate) fn spawn_create_session_async(
        &mut self,
        name: String,
        system_prompt: Option<String>,
    ) {
        let request_id = self.session.ui_io.next_load_request_id();
        self.session
            .ui_io
            .mark_create_session_started(request_id, name.clone());
        let provider = self.session.provider;
        let tx = self.session.ui_io.tx.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();

        thread::spawn(move || {
            let result = (|| -> Result<SelectedSessionLoad> {
                let data_manager = DataManager::new(provider)?;
                let session = match data_manager
                    .checkout_preallocated_session(name.clone(), system_prompt.clone())?
                {
                    Some(session) => session,
                    None => data_manager.create_session(name, system_prompt)?,
                };
                load_selected_session(
                    provider,
                    Some(session.id.clone()),
                    Some(session),
                    true,
                    None,
                )
            })();

            match result {
                Ok(load) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SelectedSessionLoad {
                        request_id: Some(request_id),
                        requested_session_id: None,
                        load,
                    },
                ),
                Err(err) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionSaveFailed {
                        request_id: Some(request_id),
                        session_id: "<create>".to_string(),
                        operation: UiIoSaveOperation::CreateSession,
                        is_system_prompt_save: false,
                        error: format!("create session failed: {err:#}"),
                    },
                ),
            }
        });
    }

    pub(crate) fn spawn_rename_session_async(&mut self, session_id: String, new_name: String) {
        let request_id = self.session.ui_io.next_list_request_id();
        self.session
            .ui_io
            .record_list_request_id_for_session(&session_id, request_id);
        let provider = self.session.provider;
        let tx = self.session.ui_io.tx.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();

        thread::spawn(move || {
            let result = (|| -> Result<SessionListPatch> {
                let data_manager = DataManager::new(provider)?;
                data_manager.rename_session(&session_id, new_name)?;
                load_session_list_patch(provider)
            })();

            match result {
                Ok(patch) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionListPatch {
                        request_id: Some(request_id),
                        patch,
                    },
                ),
                Err(err) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionSaveFailed {
                        request_id: Some(request_id),
                        session_id,
                        operation: UiIoSaveOperation::RenameSession,
                        is_system_prompt_save: false,
                        error: format!("rename session failed: {err:#}"),
                    },
                ),
            }
        });
    }

    pub(crate) fn spawn_delete_session_async(&mut self, session_id: String) {
        let request_id = self.session.ui_io.next_load_request_id();
        self.session
            .ui_io
            .record_load_request_id_for_session(&session_id, request_id);
        let provider = self.session.provider;
        let current_session_id = self
            .session
            .current_session
            .as_ref()
            .filter(|session| session.id != session_id)
            .map(|session| session.id.clone());
        let requested_session_id = current_session_id.clone();
        let tx = self.session.ui_io.tx.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();

        thread::spawn(move || {
            let result = (|| -> Result<SelectedSessionLoad> {
                let data_manager = DataManager::new(provider)?;
                data_manager.delete_session(&session_id)?;
                load_selected_session_after_delete(
                    provider,
                    &session_id,
                    current_session_id,
                    requested_session_id.clone(),
                )
            })();

            match result {
                Ok(load) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SelectedSessionLoad {
                        request_id: Some(request_id),
                        requested_session_id: load.requested_session_id.clone(),
                        load,
                    },
                ),
                Err(err) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionSaveFailed {
                        request_id: Some(request_id),
                        session_id,
                        operation: UiIoSaveOperation::DeleteSession,
                        is_system_prompt_save: false,
                        error: format!("delete session failed: {err:#}"),
                    },
                ),
            }
        });
    }

    pub(crate) fn spawn_save_session_async(
        &mut self,
        session: ChatSession,
        refresh_session_list: bool,
        request_id: Option<u64>,
        is_system_prompt_save: bool,
    ) {
        if self.session.data_manager.is_none() {
            return;
        }
        let provider = self.session.provider;
        let tx = self.session.ui_io.tx.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();
        let session_id = session.id.clone();
        let current_session_id = refresh_session_list
            .then(|| Some(session.id.clone()))
            .flatten();

        if let Some(request_id) = request_id {
            self.session
                .ui_io
                .record_save_request_id_for_session(&session_id, request_id);
        }

        thread::spawn(move || {
            if let Err(err) = save_session_in_background(provider, session) {
                send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionSaveFailed {
                        request_id,
                        session_id,
                        operation: if is_system_prompt_save {
                            UiIoSaveOperation::SystemPromptSave
                        } else {
                            UiIoSaveOperation::SaveSession
                        },
                        is_system_prompt_save,
                        error: format!("{err:#}"),
                    },
                );
                return;
            }
            if request_id.is_some() || is_system_prompt_save {
                send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionSaveSuccess {
                        request_id,
                        session_id: session_id.clone(),
                        is_system_prompt_save,
                    },
                );
            }

            if refresh_session_list {
                let requested_session_id_for_refresh = current_session_id.clone();
                match load_selected_session(
                    provider,
                    current_session_id,
                    None,
                    true,
                    requested_session_id_for_refresh.clone(),
                ) {
                    Ok(load) => send_ui_io_result(
                        &tx,
                        repaint_ctx.as_ref(),
                        UiIoResult::SelectedSessionLoad {
                            request_id: None,
                            requested_session_id: requested_session_id_for_refresh,
                            load,
                        },
                    ),
                    Err(err) => send_ui_io_result(
                        &tx,
                        repaint_ctx.as_ref(),
                        UiIoResult::SessionSaveFailed {
                            request_id,
                            session_id,
                            operation: UiIoSaveOperation::RefreshSessionList,
                            is_system_prompt_save: false,
                            error: format!("save refresh failed: {err:#}"),
                        },
                    ),
                }
            }
        });
    }

    pub(crate) fn spawn_reload_current_session_async(&mut self, session_id: String) {
        let provider = self.session.provider;
        let tx = self.session.ui_io.tx.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();

        thread::spawn(move || {
            let result = (|| -> Result<SelectedSessionLoad> {
                let data_manager = DataManager::new(provider)?;
                let session = data_manager.load_session(&session_id)?;
                load_selected_session(
                    provider,
                    Some(session_id.clone()),
                    Some(session),
                    true,
                    Some(session_id.clone()),
                )
            })();

            match result {
                Ok(load) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SelectedSessionLoad {
                        request_id: None,
                        requested_session_id: Some(session_id.clone()),
                        load,
                    },
                ),
                Err(err) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionLoadFailed {
                        request_id: None,
                        session_id: session_id.clone(),
                        requested_session_id: Some(session_id),
                        error: format!("reload current session failed: {err:#}"),
                    },
                ),
            }
        });
    }

    pub(crate) fn spawn_send_message_async(
        &mut self,
        session: ChatSession,
        raw_query: String,
        pending: PendingUiSend,
    ) {
        let provider = self.session.provider;
        let tx = self.session.ui_io.tx.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();
        self.session
            .ui_io
            .pending_send_chat_ids
            .insert(pending.chat_id.clone());

        thread::spawn(move || {
            let result = (|| -> Result<(String, Option<String>, u32)> {
                let query = raw_query.clone();
                let mut session_to_save = session;
                let system_prompt = session_to_save.system_prompt.clone();
                let session_id_to_resume = provider.session_id_from_chat(&session_to_save).cloned();
                let working_dir = session_to_save.launched_from.clone().or_else(|| {
                    std::env::current_dir()
                        .ok()
                        .map(|path| path.to_string_lossy().into_owned())
                });
                let memory_context = match working_dir.as_deref() {
                    Some(workspace_path) => crate::memory_job::retrieve_memory_context_with_helper(
                        workspace_path,
                        &query,
                    )
                    .unwrap_or_else(|err| {
                        warn!(
                            error=%err,
                            workspace_path,
                            "memory retrieval failed in helper; continuing without memory"
                        );
                        None
                    }),
                    None => None,
                };
                let retrieved_memory_preview =
                    crate::memory::format_retrieved_memory_preview(memory_context.as_ref());

                let command_str = crate::provider_bridge::build_provider_command_string(
                    provider,
                    system_prompt.as_deref(),
                    Some(&session_to_save),
                    memory_context.as_ref(),
                    &query,
                );

                update_message_request_metadata(
                    &mut session_to_save,
                    pending.message_timestamp_ms,
                    &command_str,
                    retrieved_memory_preview.as_deref(),
                )?;
                let runsc_dirs = session_to_save.runsc_dirs.clone();
                let runsc_image = session_to_save.runsc_image.clone();
                let model_override =
                    crate::common::worker_request_model_override(provider, Some(&session_to_save));
                if let Some(workspace_path) = working_dir.as_deref() {
                    if let Err(err) = chatbot_lib::memory_audit::append_memory_retrieval_audit(
                        workspace_path,
                        chatbot_lib::memory_audit::AppendMemoryRetrievalAudit {
                            provider: provider.as_str(),
                            provider_display_name: provider.display_name(),
                            session_id: &session_to_save.id,
                            session_name: &session_to_save.name,
                            requested_at_ms: pending.message_timestamp_ms,
                            query: &raw_query,
                            user_message: &raw_query,
                            memory_context: memory_context.as_ref(),
                            injected_preview: retrieved_memory_preview.as_deref(),
                        },
                    )
                    {
                        warn!(
                            error=%err,
                            workspace_path,
                            session_id=%session_to_save.id,
                            "failed to append memory retrieval audit; continuing without blocking send"
                        );
                    }
                }
                save_session_in_background(provider, session_to_save)?;

                let worker_req = worker::WorkerRequest {
                    provider: provider.as_str().to_string(),
                    query,
                    working_dir,
                    system_prompt,
                    resume_session_id: session_id_to_resume,
                    memory_context,
                    parent_pid_override: None,
                    runsc_dirs,
                    runsc_image,
                    model_override,
                    command: command_str.clone(),
                    chat_id: pending.chat_id.clone(),
                };

                reset_worker_dir_for_send(&pending.chat_id)?;
                let req_json = serde_json::to_string_pretty(&worker_req)
                    .context("serialize worker request")?;
                std::fs::write(worker::request_path(&pending.chat_id), &req_json).with_context(
                    || {
                        format!(
                            "failed to write {}",
                            worker::request_path(&pending.chat_id).display()
                        )
                    },
                )?;

                let worker_bin = AppState::resolve_worker_binary_for_current_profile()?;
                let mut child = unsafe {
                    std::process::Command::new(&worker_bin)
                        .arg(&pending.chat_id)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .pre_exec(|| {
                            libc::setsid();
                            Ok(())
                        })
                        .spawn()
                }
                .context("spawn chatbot-worker")?;
                let worker_pid = child.id();
                thread::spawn(move || {
                    let _ = child.wait();
                });

                Ok((command_str, retrieved_memory_preview, worker_pid))
            })();

            match result {
                Ok((command_str, retrieved_memory, worker_pid)) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SendCompleted {
                        chat_id: pending.chat_id,
                        message_timestamp_ms: pending.message_timestamp_ms,
                        command_str,
                        retrieved_memory,
                        worker_pid,
                    },
                ),
                Err(err) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SendFailed {
                        chat_id: pending.chat_id,
                        error: format!("{err:#}"),
                    },
                ),
            }
        });
    }
}
