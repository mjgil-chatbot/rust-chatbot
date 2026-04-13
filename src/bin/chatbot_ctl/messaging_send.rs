fn cmd_send(
    provider: ModelProvider,
    chat_id: &str,
    raw_query: &str,
    owner_pid_override: Option<u32>,
) {
    if chat_id.trim().is_empty() {
        eprintln!("Missing value for --chat-id");
        process::exit(1);
    }
    if raw_query.trim().is_empty() {
        eprintln!("Missing value for --text");
        process::exit(1);
    }

    ensure_broker_best_effort();

    if let Err(err) = run_send(provider, chat_id.trim(), raw_query, owner_pid_override) {
        eprintln!("{err:#}");
        process::exit(1);
    }
}

fn run_send(
    provider: ModelProvider,
    chat_id: &str,
    raw_query: &str,
    owner_pid_override: Option<u32>,
) -> Result<()> {
    ensure_chat_idle_for_send(chat_id)?;

    let worker_req = prepare_send_request(provider, chat_id, raw_query, owner_pid_override)
        .with_context(|| format!("failed to prepare send request for session {chat_id}"))?;

    std::fs::create_dir_all(worker::worker_dir(chat_id))?;
    worker::write_json_atomic(&worker::request_path(chat_id), &worker_req)?;

    let worker_bin = worker_binary_path();
    let mut child = unsafe {
        Command::new(&worker_bin)
            .arg(chat_id)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .pre_exec(|| {
                libc::setsid();
                Ok(())
            })
            .spawn()
            .with_context(|| format!("failed to spawn {}", worker_bin.display()))?
    };

    // Only notify broker after worker has actually spawned successfully
    if let Err(err) = send_begin(chat_id, provider.as_str()) {
        eprintln!("warning: failed to notify broker about stream begin: {err:#}");
    }
    if let Err(err) = send_sessions_changed() {
        eprintln!("warning: failed to notify broker about sessions change: {err:#}");
    }

    println!(
        "Sent prompt to session {} via {}",
        chat_id,
        provider.as_str()
    );
    println!("Spawned chatbot-worker PID {}", child.id());
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn prepare_send_request(
    provider: ModelProvider,
    chat_id: &str,
    raw_query: &str,
    owner_pid_override: Option<u32>,
) -> Result<WorkerRequest> {
    let data_manager = DataManager::new(provider)?;
    let query = raw_query.to_string();
    let (
        system_prompt,
        session_id_to_resume,
        working_dir,
        memory_context,
        command_str,
        model_override,
        audit_session_id,
        audit_session_name,
        audit_message_timestamp_ms,
        retrieved_memory_preview,
    ) =
        data_manager
            .mutate_session_with_source(chat_id, true, "chatbot_ctl.send", |session| {
                let system_prompt = session.system_prompt.clone();
                let session_id_to_resume = provider.session_id_from_chat(session).cloned();
                let working_dir = session.launched_from.clone().or_else(|| {
                    std::env::current_dir()
                        .ok()
                        .map(|path| path.to_string_lossy().into_owned())
                });
                let memory_context = match working_dir.as_deref() {
                    Some(workspace_path) => {
                        chatbot_lib::memory::retrieve_memory_context(workspace_path, &query)
                            .ok()
                            .flatten()
                    }
                    None => None,
                };
                let command_str = chatbot_lib::provider_bridge::build_provider_command_string(
                    provider,
                    system_prompt.as_deref(),
                    Some(session),
                    memory_context.as_ref(),
                    &query,
                );
                let retrieved_memory_preview =
                    chatbot_lib::memory::format_retrieved_memory_preview(memory_context.as_ref());

                session.add_message_with_command(
                    raw_query.to_string(),
                    true,
                    Some(command_str.clone()),
                );
                if let Some(message) = session.messages.last_mut() {
                    message.retrieved_memory = retrieved_memory_preview;
                }
                let model_override = if provider == ModelProvider::OmniForge {
                    Some(chatbot_lib::omniforge::selected_model(Some(session)).model.to_string())
                } else {
                    None
                };
                let last_message = session.messages.last().expect("new user message should exist");

                (
                    system_prompt,
                    session_id_to_resume,
                    working_dir,
                    memory_context,
                    command_str,
                    model_override,
                    session.id.clone(),
                    session.name.clone(),
                    last_message.timestamp.timestamp_millis(),
                    last_message.retrieved_memory.clone(),
                )
            })
            .with_context(|| format!("failed to update session {chat_id}"))?;

    if let Some(workspace_path) = working_dir.as_deref() {
        if let Err(err) = chatbot_lib::memory_audit::append_memory_retrieval_audit(
            workspace_path,
            chatbot_lib::memory_audit::AppendMemoryRetrievalAudit {
                provider: provider.as_str(),
                provider_display_name: provider.display_name(),
                session_id: &audit_session_id,
                session_name: &audit_session_name,
                requested_at_ms: audit_message_timestamp_ms,
                query: raw_query,
                user_message: raw_query,
                memory_context: memory_context.as_ref(),
                injected_preview: retrieved_memory_preview.as_deref(),
            },
        ) {
            warn!(
                error=%err,
                workspace_path,
                session_id=%audit_session_id,
                "failed to append memory retrieval audit; continuing without blocking send"
            );
        }
    }

    Ok(WorkerRequest {
        provider: provider.as_str().to_string(),
        query,
        working_dir,
        system_prompt,
        resume_session_id: session_id_to_resume,
        memory_context,
        parent_pid_override: Some(
            owner_pid_override.unwrap_or_else(|| provider_reconcile_owner_pid(provider)),
        ),
        runsc_dirs: Vec::new(),
        runsc_image: None,
        model_override,
        command: command_str,
        chat_id: chat_id.to_string(),
    })
}

fn ensure_chat_idle_for_send(chat_id: &str) -> Result<()> {
    let worker_dir = worker::worker_dir(chat_id);
    if !worker_dir.exists() {
        return Ok(());
    }

    let state_detail = worker::read_job_state(chat_id)
        .ok()
        .flatten()
        .map(|job| job.state.name().to_string())
        .unwrap_or_else(|| "PENDING".to_string());

    anyhow::bail!(
        "chat {} still has worker spool state ({}) at {}; wait until reconciliation finishes before sending another message",
        chat_id,
        state_detail,
        worker_dir.display()
    );
}

