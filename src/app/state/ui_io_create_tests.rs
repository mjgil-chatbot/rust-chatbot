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
fn create_session_load_without_prior_target_is_applied() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));
    state.session.selected_session_id = Some("current".to_string());

    let request_id = state.session.ui_io.next_load_request_id();
    state
        .session
        .ui_io
        .mark_create_session_started(request_id, "Created".to_string());
    let created = make_session("created");

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SelectedSessionLoad {
        request_id: Some(request_id),
        requested_session_id: None,
        load: SelectedSessionLoad {
            sessions: Some(vec![
                make_session_info("created"),
                make_session_info("current"),
            ]),
            current_session: Some(created),
            requested_session_id: None,
        },
    }));
    state.check_for_response();

    assert_eq!(state.session.current_session_id(), Some("created"));
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("created")
    );
    assert!(!state.session.ui_io.is_create_session_in_flight());
}

#[test]
fn create_session_failure_clears_pending_transition_and_keeps_current_session() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));

    let request_id = state.session.ui_io.next_load_request_id();
    state
        .session
        .ui_io
        .mark_create_session_started(request_id, "Created".to_string());

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveFailed {
        request_id: Some(request_id),
        session_id: "<create>".to_string(),
        operation: UiIoSaveOperation::CreateSession,
        is_system_prompt_save: false,
        error: "boom".to_string(),
    }));
    state.check_for_response();

    assert_eq!(state.session.current_session_id(), Some("current"));
    assert!(!state.session.ui_io.is_create_session_in_flight());
    let feedback = state
        .layout
        .status_feedback
        .as_ref()
        .expect("status feedback should be set on create failure");
    assert!(feedback.is_error);
    assert_eq!(feedback.message, "Failed to create session: boom");
}
