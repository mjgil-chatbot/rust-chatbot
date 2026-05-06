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
fn selected_session_reload_drops_divergent_newer_snapshot_for_current_chat() {
    let mut state = test_app_state(ModelProvider::Claude);
    let mut original = make_session("session-1");
    original.add_message("assistant reply".to_string(), false);
    let original_fingerprint = original.history_fingerprint();
    state.set_current_session(original);

    let mut replacement = make_session("session-1");
    replacement.updated_at += chrono::Duration::seconds(30);
    replacement.add_message("different assistant reply".to_string(), false);

    state.apply_selected_session_reload(SelectedSessionLoad {
        sessions: Some(vec![make_session_info("session-1")]),
        current_session: Some(replacement),
        requested_session_id: None,
    });

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("session retained");
    assert_eq!(current.history_fingerprint(), original_fingerprint);
    assert_eq!(current.messages.len(), 1);
    assert_eq!(current.messages[0].content, "assistant reply");
}

#[test]
fn selected_session_reload_accepts_strict_extension_even_with_equal_timestamp() {
    let mut state = test_app_state(ModelProvider::Claude);
    let mut original = make_session("session-1");
    original.add_message("assistant reply".to_string(), false);
    let shared_updated_at = original.updated_at;
    state.set_current_session(original);

    let mut replacement = state
        .session
        .current_session
        .as_ref()
        .expect("current session")
        .clone();
    replacement.updated_at = shared_updated_at;
    replacement.add_message("continue".to_string(), true);

    state.apply_selected_session_reload(SelectedSessionLoad {
        sessions: Some(vec![make_session_info("session-1")]),
        current_session: Some(replacement),
        requested_session_id: None,
    });

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("session present");
    assert_eq!(current.messages.len(), 2);
    assert_eq!(current.messages[0].content, "assistant reply");
    assert!(current.messages[1].is_user);
    assert_eq!(current.messages[1].content, "continue");
}
