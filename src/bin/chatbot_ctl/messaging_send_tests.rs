use super::*;

struct SendTestHomeGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    home_dir: std::path::PathBuf,
    previous_home: Option<std::ffi::OsString>,
}

impl SendTestHomeGuard {
    fn new(label: &str) -> Self {
        let lock = chatbot_lib::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let home_dir = std::env::temp_dir().join(format!(
            "rust-chatbot-ctl-send-{label}-home-{}",
            uuid::Uuid::new_v4()
        ));
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home_dir);
        Self {
            _lock: lock,
            home_dir,
            previous_home,
        }
    }
}

impl Drop for SendTestHomeGuard {
    fn drop(&mut self) {
        match self.previous_home.as_ref() {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(&self.home_dir);
    }
}

#[test]
fn ensure_chat_idle_for_send_rejects_existing_worker_spool() {
    let _guard = SendTestHomeGuard::new("busy");

    let chat_id = format!("busy-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(worker::worker_dir(&chat_id)).expect("create worker dir");

    let err = ensure_chat_idle_for_send(&chat_id).expect_err("worker spool must block send");
    let message = format!("{err:#}");
    assert!(message.contains("still has worker spool state"));
    assert!(message.contains("PENDING"));
}

#[test]
fn ensure_chat_idle_for_send_accepts_missing_worker_spool() {
    let _guard = SendTestHomeGuard::new("idle");

    let chat_id = format!("idle-{}", uuid::Uuid::new_v4());
    ensure_chat_idle_for_send(&chat_id).expect("missing worker spool should allow send");
}

#[test]
fn prepare_send_request_reloads_latest_disk_session_before_appending_user_turn() {
    let _guard = SendTestHomeGuard::new("latest");

    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).expect("data manager");
    let session = data_manager
        .create_session("Atomic Send".to_string(), None)
        .expect("create session");
    let session_id = session.id.clone();
    let stale_snapshot = data_manager
        .load_session(&session_id)
        .expect("stale snapshot");

    let mut newer_snapshot = data_manager
        .load_session(&session_id)
        .expect("newer snapshot");
    newer_snapshot.add_message("assistant reply".to_string(), false);
    data_manager
        .save_session(&newer_snapshot)
        .expect("save newer snapshot");

    let request = prepare_send_request(provider, &session_id, "continue", Some(4242))
        .expect("prepare send request");

    let saved = data_manager
        .load_session(&session_id)
        .expect("saved session");
    assert_ne!(
        stale_snapshot.history_fingerprint(),
        saved.history_fingerprint(),
        "stale pre-update state must not overwrite newer disk messages"
    );
    assert_eq!(saved.messages.len(), 2);
    assert_eq!(saved.messages[0].content, "assistant reply");
    assert!(saved.messages[1].is_user);
    assert_eq!(saved.messages[1].content, "continue");
    assert_eq!(
        saved.messages[1].command.as_deref(),
        Some(request.command.as_str())
    );
    assert_eq!(request.chat_id, session_id);
    assert_eq!(request.query, "continue");
    assert_eq!(request.parent_pid_override, Some(4242));
}

#[test]
fn prepare_send_request_logs_candidate_retrievals_and_saved_injected_preview() {
    let _guard = SendTestHomeGuard::new("memory-audit");

    let workspace = std::env::temp_dir().join(format!(
        "rust-chatbot-ctl-send-memory-audit-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&workspace).expect("create workspace");

    chatbot_lib::memory::remember_manual_memory(
        &workspace,
        "Grep calls require the case_sensitive field.",
        chatbot_lib::memory::ManualMemoryPriority::High,
    )
    .expect("remember manual memory");

    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).expect("data manager");
    let mut session = data_manager
        .create_session("Memory Audit".to_string(), None)
        .expect("create session");
    session.launched_from = Some(workspace.to_string_lossy().into_owned());
    data_manager.save_session(&session).expect("save session");

    let request = prepare_send_request(
        provider,
        &session.id,
        "Why does Grep keep failing?",
        Some(4242),
    )
    .expect("prepare send request");

    let saved = data_manager
        .load_session(&session.id)
        .expect("load saved session");
    let saved_message = saved.messages.last().expect("saved user message");
    let saved_preview = saved_message
        .retrieved_memory
        .as_deref()
        .expect("saved retrieved memory preview");
    assert!(saved_preview.contains("case_sensitive field"));

    let context = request
        .memory_context
        .expect("worker request memory context");
    assert_eq!(context.hits.len(), 1);
    assert!(context.hits[0].doc_key.starts_with("manual:"));
    assert!(context.hits[0].snippet.contains("case_sensitive field"));

    let retrievals = chatbot_lib::memory_audit::list_memory_retrieval_audits(&workspace, 10)
        .expect("list retrieval audits");
    assert_eq!(retrievals.len(), 1);
    assert_eq!(retrievals[0].query, "Why does Grep keep failing?");
    assert_eq!(retrievals[0].candidate_hit_count, 1);
    assert!(retrievals[0]
        .candidate_preview
        .as_deref()
        .expect("candidate preview")
        .contains("case_sensitive field"));
    assert_eq!(
        retrievals[0].injected_preview.as_deref(),
        Some(saved_preview)
    );

    let _ = std::fs::remove_dir_all(workspace);
}
