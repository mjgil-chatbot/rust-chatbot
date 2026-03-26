use super::*;
fn make_session(id: &str) -> ChatSession {
    ChatSession {
        id: id.to_string(),
        name: id.to_string(),
        system_prompt: None,
        json_summary: None,
        claude_session_id: None,
        minimax_session_id: None,
        qwen_session_id: None,
        codex_session_id: None,
        nvidia_session_id: None,
        gemini_session_id: None,
        openrouter_session_id: None,
        omniforge_session_id: None,
        omniforge_model_id: None,
        kimi_session_id: None,
        opencode_session_id: None,
        hermes_session_id: None,
        forge_session_id: None,
        claw_session_id: None,
        launched_from: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: Vec::new(),
        runsc_dirs: Vec::new(),
        runsc_image: None,
    }
}
fn make_session_info(id: &str) -> SessionInfo {
    let session = make_session(id);
    SessionInfo {
        id: session.id,
        name: session.name,
        created_at: session.created_at,
        updated_at: session.updated_at,
        message_count: 0,
        hidden: false,
    }
}
fn test_app_state(provider: ModelProvider) -> AppState {
    let (_tx, rx) = mpsc::channel();
    AppState {
        session: SessionState::new(provider),
        worker: AppWorkerState::new(),
        layout: LayoutState::new(),
        sync_shm: None,
        sync_socket: SocketHandle::noop(),
        sync_events: rx,
        instance_number: None,
        dialog_parent: None,
    }
}
#[test]
fn reset_worker_dir_for_send_clears_stale_terminal_spool_files() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!("rust-chatbot-home-{}", uuid::Uuid::new_v4()));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let chat_id = format!("stale-worker-{}", uuid::Uuid::new_v4());
    let worker_dir = worker::worker_dir(&chat_id);
    std::fs::create_dir_all(&worker_dir).expect("worker dir");
    std::fs::write(worker::state_path(&chat_id), "{\"state\":\"Error\"}").expect("state file");
    std::fs::write(worker::result_path(&chat_id), "{\"error\":\"stale\"}").expect("result file");
    std::fs::write(worker::output_path(&chat_id), "old output").expect("output file");

    let reset_dir = reset_worker_dir_for_send(&chat_id).expect("reset worker dir");

    assert_eq!(reset_dir, worker_dir);
    assert!(worker_dir.exists(), "worker dir should be recreated");
    assert!(
        !worker::state_path(&chat_id).exists(),
        "stale state.json must be cleared before a new send"
    );
    assert!(
        !worker::result_path(&chat_id).exists(),
        "stale result.json must be cleared before a new send"
    );
    assert!(
        !worker::output_path(&chat_id).exists(),
        "stale output must be cleared before a new send"
    );

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = std::fs::remove_dir_all(home_dir);
}
