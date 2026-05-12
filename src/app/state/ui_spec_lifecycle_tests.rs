use super::*;

use std::fs;
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

fn with_temp_home<T>(f: impl FnOnce() -> T) -> T {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!("rust-chatbot-home-{}", uuid::Uuid::new_v4()));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let result = f();

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
    result
}

#[test]
fn submit_valid_creates_pending_draft_and_requests_focus() {
    with_temp_home(|| {
        let mut state = test_app_state(ModelProvider::Claude);
        state.set_current_session(make_session("session-1"));
        state.layout.should_focus_input = false;

        state
            .send_message_text("hello".to_string())
            .expect("send message");

        let current = state
            .session
            .current_session
            .as_ref()
            .expect("current session");
        let last = current.messages.last().expect("user message appended");
        assert!(last.is_user, "submit must append a user message");
        assert_eq!(last.content, "hello");

        let pending = state
            .session
            .pending_draft
            .as_ref()
            .expect("submit must create pending draft authority");
        assert_eq!(pending.chat_id, "session-1");
        assert_eq!(
            pending.user_message_timestamp_ms,
            last.timestamp.timestamp_millis()
        );
        assert!(
            state
                .session
                .ui_io
                .pending_send_chat_ids
                .contains("session-1"),
            "submit must hand the chat off to async send preparation"
        );
        assert!(
            state.layout.should_focus_input,
            "submit must request focus restoration"
        );
    });
}

#[test]
fn local_terminal_empty_response_requests_focus_and_clears_pending_draft() {
    let mut state = test_app_state(ModelProvider::Claude);
    let now = chrono::Utc::now();
    state.session.sessions = vec![SessionInfo {
        id: "session-1".into(),
        name: "Session 1".into(),
        created_at: now,
        updated_at: now,
        message_count: 0,
        hidden: false,
    }];
    state.set_current_session(make_session("session-1"));
    state.layout.should_focus_input = false;
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "session-1".to_string(),
        user_message_timestamp_ms: 42,
    });

    state.handle_local_terminal_worker(
        "session-1",
        "",
        WireMessage::sessions_changed(),
        "stopped",
        "stopped",
        false,
        |session, _response| {
            append_terminal_message_once(session, "Process was stopped by user.");
            false
        },
    );

    assert!(
        state.session.pending_draft.is_none(),
        "terminal cleanup must clear the pending draft even on empty output"
    );
    assert!(
        state.layout.should_focus_input,
        "terminal cleanup must return the chat to a recoverable focused idle state"
    );
}

#[test]
fn send_failed_returns_chat_to_recoverable_idle_state() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));
    state.layout.should_focus_input = false;
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "session-1".to_string(),
        user_message_timestamp_ms: 123,
    });
    state
        .session
        .ui_io
        .pending_send_chat_ids
        .insert("session-1".to_string());

    state
        .session
        .ui_io
        .tx
        .send(UiIoResult::SendFailed {
            chat_id: "session-1".to_string(),
            error: "boom".to_string(),
        })
        .expect("send ui io result");

    state.check_for_response();

    assert!(
        state.session.pending_draft.is_none(),
        "send failure must clear optimistic draft ownership"
    );
    assert!(
        !state
            .session
            .ui_io
            .pending_send_chat_ids
            .contains("session-1"),
        "send failure must clear pending send bookkeeping"
    );
    assert!(
        state.layout.should_focus_input,
        "send failure must return focus to the composer"
    );

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    let last = current.messages.last().expect("error message appended");
    assert!(
        !last.is_user,
        "error result must be visible as assistant-side output"
    );
    assert!(
        last.content.contains("boom"),
        "error text must remain visible to keep the chat recoverable"
    );
}
