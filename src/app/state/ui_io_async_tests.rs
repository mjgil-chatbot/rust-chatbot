use super::*;
use std::fs;
use std::sync::mpsc;
use std::time::Duration as StdDuration;
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
fn spawn_create_session_async_emits_selected_load_without_targeted_session_id() {
    with_temp_home(|| {
        let mut state = test_app_state(ModelProvider::Claude);

        state.spawn_create_session_async("Created".to_string(), None);
        assert!(state.session.ui_io.is_create_session_in_flight());
        assert_eq!(
            state.session.ui_io.pending_create_session_name(),
            Some("Created")
        );

        let result = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("create result");

        match result {
            UiIoResult::SelectedSessionLoad {
                request_id: Some(_),
                requested_session_id,
                load,
            } => {
                assert!(requested_session_id.is_none());
                assert!(load.requested_session_id.is_none());
                let created = load.current_session.expect("created session payload");
                assert_eq!(created.name, "Created");
                assert!(load.sessions.as_ref().is_some_and(|sessions| sessions
                    .iter()
                    .any(|session| session.id == created.id)));
            }
            other => panic!(
                "unexpected create result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}
