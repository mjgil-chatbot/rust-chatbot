use super::*;

use std::sync::mpsc;

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
fn chat_completion_does_not_promote_to_top() {
    let mut state = test_app_state(ModelProvider::Codex);
    let now = chrono::Utc::now();
    state.session.sessions = vec![
        SessionInfo {
            id: "top".into(),
            name: "Top".into(),
            created_at: now,
            updated_at: now,
            message_count: 3,
            hidden: false,
        },
        SessionInfo {
            id: "middle".into(),
            name: "Middle".into(),
            created_at: now,
            updated_at: now,
            message_count: 1,
            hidden: false,
        },
        SessionInfo {
            id: "bottom".into(),
            name: "Bottom".into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            hidden: false,
        },
    ];

    state.set_current_session(make_session("bottom"));
    assert_eq!(state.session.current_session_id(), Some("bottom"));
    assert_eq!(state.session.sessions[2].id, "bottom");

    state.handle_local_terminal_worker(
        "bottom",
        "model response text",
        WireMessage::sessions_changed(),
        "done",
        "completed",
        true,
        |session, response| upsert_assistant_response(session, response),
    );

    assert_eq!(
        state.session.sessions[2].id, "bottom",
        "completed chat must stay in its original position — promotion happens on activation, not completion"
    );
    assert_eq!(
        state.session.current_session_id(),
        Some("bottom"),
        "completed chat must remain the active session"
    );
    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session must exist");
    assert!(
        current
            .messages
            .iter()
            .any(|m| m.content == "model response text"),
        "assistant response must be appended to the session"
    );
    assert_eq!(
        state.layout.pending_height_cache_save.as_deref(),
        Some("bottom"),
        "height cache save must be scheduled for the active chat"
    );
}

#[test]
fn finished_conversation_does_not_move_when_selected() {
    let mut state = test_app_state(ModelProvider::Codex);
    let now = chrono::Utc::now();
    state.session.sessions = vec![
        SessionInfo {
            id: "first".into(),
            name: "First".into(),
            created_at: now,
            updated_at: now,
            message_count: 5,
            hidden: false,
        },
        SessionInfo {
            id: "second".into(),
            name: "Second".into(),
            created_at: now,
            updated_at: now,
            message_count: 3,
            hidden: false,
        },
        SessionInfo {
            id: "finished".into(),
            name: "Finished".into(),
            created_at: now,
            updated_at: now,
            message_count: 2,
            hidden: false,
        },
    ];

    state.set_current_session(make_session("finished"));

    state.handle_local_terminal_worker(
        "finished",
        "done generating",
        WireMessage::sessions_changed(),
        "done",
        "completed",
        true,
        |session, response| upsert_assistant_response(session, response),
    );

    assert_eq!(
        state.session.sessions[2].id, "finished",
        "completed chat must NOT be promoted — promotion should only happen when a chat first becomes active"
    );

    state.set_current_session(make_session("first"));
    state.set_current_session(make_session("finished"));

    assert_eq!(
        state.session.sessions[2].id, "finished",
        "selecting a finished chat must not move it in the sidebar"
    );
}
