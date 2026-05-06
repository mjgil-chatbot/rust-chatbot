use super::*;
use crate::app::state::ui_io::background::resolve_snapshot_current_session_id;
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
#[test]
fn passive_refresh_does_not_pick_first_session_as_current() {
    let sessions = vec![make_session_info("top"), make_session_info("next")];
    let selected = resolve_snapshot_current_session_id(&sessions, None, false);
    assert!(selected.is_none());
}
#[test]
fn fallback_refresh_can_pick_first_session_when_needed() {
    let sessions = vec![make_session_info("top"), make_session_info("next")];
    let selected = resolve_snapshot_current_session_id(&sessions, None, true);
    assert_eq!(selected, Some("top"));
}
#[test]
fn sidebar_patch_without_current_session_keeps_existing_active_chat() {
    let mut state = test_app_state(ModelProvider::Claude);
    let current = make_session("current");
    state.set_current_session(current);
    state.apply_sidebar_patch(SessionListPatch {
        sessions: vec![make_session_info("other"), make_session_info("current")],
    });
    assert_eq!(state.session.current_session_id(), Some("current"));
}
#[test]
fn selected_session_reload_without_session_clears_selection() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));
    state.session.selected_session_id = Some("current".to_string());
    state.apply_selected_session_reload(SelectedSessionLoad {
        sessions: Some(Vec::new()),
        current_session: None,
        requested_session_id: None,
    });
    assert!(state.session.current_session.is_none());
    assert!(state.session.selected_session_id.is_none());
}
#[test]
fn selected_session_reload_skips_replacement_when_pending_draft_active() {
    let mut state = test_app_state(ModelProvider::Claude);
    let original = make_session("session-1");
    let original_updated_at = original.updated_at;
    state.set_current_session(original);
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "session-1".to_string(),
        user_message_timestamp_ms: 42,
    });

    let mut replacement = make_session("session-1");
    replacement.updated_at = original_updated_at + chrono::Duration::seconds(30);
    replacement.messages.push(crate::data::ChatMessage {
        content: "stale payload".to_string(),
        is_user: false,
        timestamp: chrono::Utc::now(),
        command: None,
        retrieved_memory: None,
    });

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
    assert_eq!(
        current.updated_at, original_updated_at,
        "pending_draft guard must reject passive reload replacement"
    );
    assert!(
        current.messages.is_empty(),
        "stale passive reload must not inject messages into active send"
    );
}
#[test]
fn selected_session_reload_proceeds_when_no_pending_draft() {
    let mut state = test_app_state(ModelProvider::Claude);
    let original = make_session("session-1");
    let original_updated_at = original.updated_at;
    state.set_current_session(original);

    let mut replacement = make_session("session-1");
    replacement.updated_at = original_updated_at + chrono::Duration::seconds(30);
    let replacement_updated_at = replacement.updated_at;

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
    assert_eq!(
        current.updated_at, replacement_updated_at,
        "without pending_draft, newer disk version must be applied"
    );
}
#[test]
fn selected_session_reload_proceeds_when_pending_draft_is_for_other_chat() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "session-1".to_string(),
        user_message_timestamp_ms: 42,
    });

    let replacement = make_session("session-2");
    state.apply_selected_session_reload(SelectedSessionLoad {
        sessions: Some(vec![make_session_info("session-2")]),
        current_session: Some(replacement),
        requested_session_id: Some("session-2".to_string()),
    });

    assert_eq!(state.session.current_session_id(), Some("session-2"));
    assert_eq!(
        state
            .session
            .pending_draft
            .as_ref()
            .map(|draft| draft.chat_id.as_str()),
        Some("session-1")
    );
}
#[test]
fn load_failure_clears_loading_indicator_and_restores_selected_session() {
    let mut state = test_app_state(ModelProvider::Claude);
    let current = make_session("current");
    state.set_current_session(current);
    state.session.selected_session_id = Some("failed".to_string());
    state.session.loading_session_id = Some("failed".to_string());

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionLoadFailed {
        request_id: Some(1),
        session_id: "failed".to_string(),
        requested_session_id: Some("failed".to_string()),
        error: "boom".to_string(),
    }));
    state.check_for_response();

    assert_eq!(state.session.loading_session_id, None);
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("current")
    );
    let feedback = state
        .layout
        .status_feedback
        .as_ref()
        .expect("status feedback should be set on load failure");
    assert!(feedback.is_error);
    assert_eq!(
        feedback.message,
        "Failed to load session failed: boom".to_string()
    );
}
#[test]
fn begin_session_load_activates_cached_session_immediately() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));

    let cached = make_session("session-2");
    let mut cached_info = make_session_info("session-2");
    cached_info.updated_at = cached.updated_at;
    state
        .session
        .session_cache
        .insert("session-2".to_string(), cached.clone());
    state.session.sessions = vec![cached_info];

    assert!(!state.begin_session_load("session-2".to_string()));

    assert_eq!(state.session.current_session_id(), Some("session-2"));
    assert_eq!(
        state.session.loading_session_id.as_deref(),
        Some("session-2")
    );
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("session-2")
    );
}
#[test]
fn stale_load_failure_for_targeted_session_still_clears_sidebar_state() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));
    state.session.selected_session_id = Some("failed".to_string());
    state.session.loading_session_id = Some("failed".to_string());

    let _ = state.session.ui_io.next_load_request_id();
    state.session.ui_io.next_load_request_id();

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionLoadFailed {
        request_id: Some(1),
        session_id: "failed".to_string(),
        requested_session_id: Some("failed".to_string()),
        error: "boom".to_string(),
    }));
    state.check_for_response();

    assert_eq!(state.session.loading_session_id, None);
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("current")
    );
    let feedback = state
        .layout
        .status_feedback
        .as_ref()
        .expect("status feedback should be set on stale load failure");
    assert!(feedback.is_error);
    assert_eq!(
        feedback.message,
        "Failed to load session failed: boom".to_string()
    );
}
#[test]
fn stale_session_load_for_non_targeted_row_is_dropped() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));

    let _ = state.session.ui_io.next_load_request_id();
    state.session.ui_io.next_load_request_id();

    let mut replacement = make_session("other");
    replacement.updated_at = chrono::Utc::now() + chrono::Duration::minutes(5);

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SelectedSessionLoad {
        request_id: Some(1),
        requested_session_id: Some("other".to_string()),
        load: SelectedSessionLoad {
            sessions: Some(vec![make_session_info("other")]),
            current_session: Some(replacement),
            requested_session_id: Some("other".to_string()),
        },
    }));
    state.check_for_response();

    assert_eq!(
        state
            .session
            .current_session
            .as_ref()
            .expect("current session should stay")
            .id,
        "current"
    );
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("current")
    );
}
#[test]
fn delete_non_current_session_reload_keeps_current_session() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));
    state.session.sessions = vec![make_session_info("current"), make_session_info("other")];

    let request_id = state.session.ui_io.next_load_request_id();
    let mut current = make_session("current");
    current.updated_at = chrono::Utc::now() + chrono::Duration::minutes(5);

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SelectedSessionLoad {
        request_id: Some(request_id),
        requested_session_id: Some("current".to_string()),
        load: SelectedSessionLoad {
            sessions: Some(vec![make_session_info("current")]),
            current_session: Some(current),
            requested_session_id: Some("current".to_string()),
        },
    }));
    state.check_for_response();

    assert_eq!(state.session.current_session_id(), Some("current"));
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("current")
    );
    assert_eq!(state.session.sessions.len(), 1);
    assert_eq!(state.session.sessions[0].id, "current");
}
#[test]
fn selected_session_reload_without_sidebar_patch_preserves_existing_sidebar_state() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.session.sessions = vec![make_session_info("current"), make_session_info("other")];

    state.apply_selected_session_reload(SelectedSessionLoad {
        sessions: None,
        current_session: Some(make_session("current")),
        requested_session_id: Some("current".to_string()),
    });

    assert_eq!(state.session.sessions.len(), 2);
    assert_eq!(state.session.sessions[0].id, "current");
    assert_eq!(state.session.sessions[1].id, "other");
}
#[test]
fn stale_rename_failure_for_current_session_is_dropped() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));
    state
        .session
        .ui_io
        .record_list_request_id_for_session("current", 1);
    state
        .session
        .ui_io
        .record_list_request_id_for_session("current", 2);

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveFailed {
        request_id: Some(1),
        session_id: "current".to_string(),
        operation: UiIoSaveOperation::RenameSession,
        is_system_prompt_save: false,
        error: "boom".to_string(),
    }));
    state.check_for_response();

    assert!(
        state.layout.status_feedback.is_none(),
        "stale rename failure should be dropped for the active row"
    );
}
#[test]
fn delete_failure_for_non_current_session_uses_scoped_load_request_ids() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));
    let request_for_other = state.session.ui_io.next_load_request_id();
    state
        .session
        .ui_io
        .record_load_request_id_for_session("other", request_for_other);
    let request_for_current = state.session.ui_io.next_load_request_id();
    state
        .session
        .ui_io
        .record_load_request_id_for_session("current", request_for_current);

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveFailed {
        request_id: Some(request_for_other),
        session_id: "other".to_string(),
        operation: UiIoSaveOperation::DeleteSession,
        is_system_prompt_save: false,
        error: "boom".to_string(),
    }));
    state.check_for_response();

    let feedback = state
        .layout
        .status_feedback
        .as_ref()
        .expect("delete failure should stay visible for another row");
    assert!(feedback.is_error);
    assert_eq!(feedback.message, "Failed to delete session other: boom");
}
#[test]
fn stale_save_result_for_another_session_remains_visible_when_scoped_ids_differ() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));
    state.session.selected_session_id = Some("current".to_string());
    let request_for_other = state.session.ui_io.next_save_request_id();
    state
        .session
        .ui_io
        .record_save_request_id_for_session("other", request_for_other);
    let request_for_current = state.session.ui_io.next_save_request_id();
    state
        .session
        .ui_io
        .record_save_request_id_for_session("current", request_for_current);
    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveFailed {
        request_id: Some(request_for_other),
        session_id: "other".to_string(),
        operation: UiIoSaveOperation::SaveSession,
        is_system_prompt_save: false,
        error: "boom".to_string(),
    }));
    state.check_for_response();

    let feedback = state
        .layout
        .status_feedback
        .as_ref()
        .expect("status feedback should be set on stale save failure");
    assert!(feedback.is_error);
    assert_eq!(feedback.message, "Failed to save session other: boom");
}
#[test]
fn save_result_for_older_session_is_not_dropped_by_newer_request_in_same_session() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("current"));
    state.session.system_prompt_dirty = false;
    state.session.system_prompt_dirty_session_id = Some("other".to_string());

    state
        .session
        .ui_io
        .record_save_request_id_for_session("other", 1);
    state
        .session
        .ui_io
        .record_save_request_id_for_session("current", 2);

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveSuccess {
        request_id: Some(1),
        session_id: "other".to_string(),
        is_system_prompt_save: false,
    }));
    state.check_for_response();

    let feedback = state
        .layout
        .status_feedback
        .as_ref()
        .expect("status feedback should be set on save success");
    assert!(!feedback.is_error);
    assert_eq!(feedback.message, "Session saved");
}
#[test]
fn system_prompt_save_failed_keeps_prompt_dirty_for_current_session() {
    let mut state = test_app_state(ModelProvider::Claude);
    let mut session = make_session("session-1");
    session.system_prompt = Some("before".to_string());
    state.set_current_session(session);
    state.session.system_prompt_dirty = false;
    state.session.system_prompt_dirty_session_id = Some("session-1".to_string());
    state.session.system_prompt_text = "after".to_string();

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveFailed {
        request_id: Some(1),
        session_id: "session-1".to_string(),
        operation: UiIoSaveOperation::SystemPromptSave,
        is_system_prompt_save: true,
        error: "boom".to_string(),
    }));
    state.check_for_response();

    assert!(state.session.system_prompt_dirty);
    assert_eq!(
        state.session.system_prompt_dirty_session_id.as_deref(),
        Some("session-1")
    );
    let feedback = state
        .layout
        .status_feedback
        .as_ref()
        .expect("status feedback should be set on save failure");
    assert!(feedback.is_error);
    assert_eq!(
        feedback.message,
        "Failed to save session session-1: boom".to_string()
    );
}
#[test]
fn stale_system_prompt_save_failure_for_current_session_is_dropped() {
    let mut state = test_app_state(ModelProvider::Claude);
    let mut session = make_session("session-1");
    session.system_prompt = Some("before".to_string());
    state.set_current_session(session);
    state.session.system_prompt_dirty = true;
    state.session.system_prompt_dirty_session_id = Some("session-1".to_string());
    state
        .session
        .ui_io
        .record_save_request_id_for_session("session-1", 1);
    state
        .session
        .ui_io
        .record_save_request_id_for_session("session-1", 2);

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveFailed {
        request_id: Some(1),
        session_id: "session-1".to_string(),
        operation: UiIoSaveOperation::SystemPromptSave,
        is_system_prompt_save: true,
        error: "boom".to_string(),
    }));
    state.check_for_response();

    assert!(state.session.system_prompt_dirty);
    assert_eq!(
        state.session.system_prompt_dirty_session_id.as_deref(),
        Some("session-1")
    );
    assert!(
        state.layout.status_feedback.is_none(),
        "stale system prompt save failure should be dropped"
    );
}
#[test]
fn system_prompt_save_success_clears_prompt_dirty_for_target_session() {
    let mut state = test_app_state(ModelProvider::Claude);
    let mut session = make_session("session-1");
    session.system_prompt = Some("before".to_string());
    state.set_current_session(session);
    state.session.system_prompt_dirty = true;
    state.session.system_prompt_dirty_session_id = Some("session-1".to_string());
    state.session.system_prompt_text = "after".to_string();

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveSuccess {
        request_id: Some(1),
        session_id: "session-1".to_string(),
        is_system_prompt_save: true,
    }));
    state.check_for_response();

    assert!(!state.session.system_prompt_dirty);
    assert!(state.session.system_prompt_dirty_session_id.is_none());
    let feedback = state
        .layout
        .status_feedback
        .as_ref()
        .expect("status feedback should be set on save success");
    assert!(!feedback.is_error);
    assert_eq!(feedback.message, "System prompt saved");
}
#[test]
fn stale_system_prompt_save_success_for_current_session_is_dropped() {
    let mut state = test_app_state(ModelProvider::Claude);
    let mut session = make_session("session-1");
    session.system_prompt = Some("before".to_string());
    state.set_current_session(session);
    state.session.system_prompt_dirty = true;
    state.session.system_prompt_dirty_session_id = Some("session-1".to_string());
    state
        .session
        .ui_io
        .record_save_request_id_for_session("session-1", 1);
    state
        .session
        .ui_io
        .record_save_request_id_for_session("session-1", 2);

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveSuccess {
        request_id: Some(1),
        session_id: "session-1".to_string(),
        is_system_prompt_save: true,
    }));
    state.check_for_response();

    assert!(state.session.system_prompt_dirty);
    assert_eq!(
        state.session.system_prompt_dirty_session_id.as_deref(),
        Some("session-1")
    );
    assert!(
        state.layout.status_feedback.is_none(),
        "stale system prompt save success should be dropped"
    );
}
#[test]
fn implicit_session_save_success_does_not_set_status_feedback() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));

    state.enqueue_event(AppEvent::UiIo(UiIoResult::SessionSaveSuccess {
        request_id: None,
        session_id: "session-1".to_string(),
        is_system_prompt_save: false,
    }));
    state.check_for_response();

    assert!(
        state.layout.status_feedback.is_none(),
        "implicit autosave success should stay silent"
    );
}
#[test]
fn system_prompt_save_missing_session_does_not_set_dirty_marker() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.session.current_session = None;
    state.session.system_prompt_dirty = false;
    state.session.system_prompt_dirty_session_id = None;

    state.enqueue_event(AppEvent::UiAction(AppUiAction::SaveCurrentSystemPrompt {
        session_id: "missing-session".to_string(),
        system_prompt: Some("new prompt".to_string()),
    }));
    state.check_for_response();

    assert!(!state.session.system_prompt_dirty);
    assert!(state.session.system_prompt_dirty_session_id.is_none());
}
