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

#[test]
fn spawn_create_session_async_for_omniforge_sets_default_model_id() {
    with_temp_home(|| {
        let mut state = test_app_state(ModelProvider::OmniForge);

        state.spawn_create_session_async("Omni".to_string(), None);
        let result = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("create result");

        match result {
            UiIoResult::SelectedSessionLoad { load, .. } => {
                let created = load.current_session.expect("created session payload");
                assert_eq!(
                    created.omniforge_model_id.as_deref(),
                    Some(crate::omniforge::DEFAULT_MODEL_ID)
                );
            }
            other => panic!(
                "unexpected create result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}

#[test]
fn spawn_rename_session_async_emits_sidebar_patch_with_updated_name() {
    with_temp_home(|| {
        let provider = ModelProvider::Claude;
        let session = make_session("session-1");
        let data_manager = DataManager::new(provider).expect("data manager");
        data_manager.save_session(&session).expect("save session");

        let mut state = test_app_state(provider);
        state.spawn_rename_session_async("session-1".to_string(), "Renamed".to_string());

        let result = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("rename result");

        match result {
            UiIoResult::SessionListPatch {
                request_id: Some(_),
                patch,
            } => {
                let renamed = patch
                    .sessions
                    .iter()
                    .find(|session| session.id == "session-1")
                    .expect("renamed session in patch");
                assert_eq!(renamed.name, "Renamed");
            }
            other => panic!(
                "unexpected rename result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}

#[test]
fn spawn_delete_session_async_emits_reload_for_surviving_current_session() {
    with_temp_home(|| {
        let provider = ModelProvider::Claude;
        let current = make_session("current");
        let other = make_session("other");
        let data_manager = DataManager::new(provider).expect("data manager");
        data_manager.save_session(&current).expect("save current");
        data_manager.save_session(&other).expect("save other");

        let mut state = test_app_state(provider);
        state.set_current_session(current);

        state.spawn_delete_session_async("other".to_string());

        let result = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("delete result");

        match result {
            UiIoResult::SelectedSessionLoad {
                request_id: Some(_),
                requested_session_id,
                load,
            } => {
                assert_eq!(requested_session_id.as_deref(), Some("current"));
                assert_eq!(load.requested_session_id.as_deref(), Some("current"));
                assert_eq!(
                    load.current_session
                        .as_ref()
                        .map(|session| session.id.as_str()),
                    Some("current")
                );
                let sessions = load.sessions.expect("delete reload should refresh sidebar");
                assert_eq!(sessions.len(), 1);
                assert_eq!(sessions[0].id, "current");
            }
            other => panic!(
                "unexpected delete result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}

#[test]
fn spawn_delete_session_async_selects_first_surviving_session_when_current_is_deleted() {
    with_temp_home(|| {
        let provider = ModelProvider::Claude;
        let current = make_session("current");
        let other = make_session("other");
        let data_manager = DataManager::new(provider).expect("data manager");
        data_manager.save_session(&current).expect("save current");
        data_manager.save_session(&other).expect("save other");

        let mut state = test_app_state(provider);
        state.set_current_session(current);

        state.spawn_delete_session_async("current".to_string());

        let result = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("delete result");

        match result {
            UiIoResult::SelectedSessionLoad {
                request_id: Some(_),
                requested_session_id,
                load,
            } => {
                assert_eq!(requested_session_id.as_deref(), Some("other"));
                assert_eq!(load.requested_session_id.as_deref(), Some("other"));
                assert_eq!(
                    load.current_session
                        .as_ref()
                        .map(|session| session.id.as_str()),
                    Some("other")
                );
                let sessions = load.sessions.expect("delete reload should refresh sidebar");
                assert_eq!(sessions.len(), 1);
                assert_eq!(sessions[0].id, "other");
            }
            other => panic!(
                "unexpected delete result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}

#[test]
fn spawn_reload_current_session_async_emits_selected_load_for_target_session() {
    with_temp_home(|| {
        let provider = ModelProvider::Claude;
        let mut session = make_session("session-1");
        session.system_prompt = Some("prompt".to_string());
        let data_manager = DataManager::new(provider).expect("data manager");
        data_manager.save_session(&session).expect("save session");

        let mut state = test_app_state(provider);
        state.spawn_reload_current_session_async("session-1".to_string());

        let result = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("reload result");

        match result {
            UiIoResult::SelectedSessionLoad {
                request_id: None,
                requested_session_id,
                load,
            } => {
                assert_eq!(requested_session_id.as_deref(), Some("session-1"));
                assert_eq!(load.requested_session_id.as_deref(), Some("session-1"));
                let loaded = load.current_session.expect("reloaded session payload");
                assert_eq!(loaded.id, "session-1");
                assert_eq!(loaded.system_prompt.as_deref(), Some("prompt"));
            }
            other => panic!(
                "unexpected reload result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}

#[test]
fn spawn_load_session_async_skips_sidebar_snapshot_reload_on_critical_path() {
    with_temp_home(|| {
        let provider = ModelProvider::Claude;
        let mut session = make_session("session-1");
        session.system_prompt = Some("prompt".to_string());
        let data_manager = DataManager::new(provider).expect("data manager");
        data_manager.save_session(&session).expect("save session");

        let mut state = test_app_state(provider);
        state.spawn_load_session_async("session-1".to_string());

        let result = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("load result");

        match result {
            UiIoResult::SelectedSessionLoad {
                request_id: Some(_),
                requested_session_id,
                load,
            } => {
                assert_eq!(requested_session_id.as_deref(), Some("session-1"));
                assert_eq!(load.requested_session_id.as_deref(), Some("session-1"));
                assert!(
                    load.sessions.is_none(),
                    "uncached session switches should not wait on a sidebar relist"
                );
                let loaded = load.current_session.expect("loaded session payload");
                assert_eq!(loaded.id, "session-1");
                assert_eq!(loaded.system_prompt.as_deref(), Some("prompt"));
            }
            other => panic!(
                "unexpected load result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}

#[test]
fn spawn_reload_current_session_async_preserves_minimax_resume_id_for_follow_up_commands() {
    with_temp_home(|| {
        let provider = ModelProvider::MiniMax;
        let mut session = make_session("session-1");
        session.launched_from = Some("/workspace".to_string());
        session.minimax_session_id = Some("mm-session-123".to_string());
        let data_manager = DataManager::new(provider).expect("data manager");
        data_manager.save_session(&session).expect("save session");

        let mut state = test_app_state(provider);
        state.spawn_reload_current_session_async("session-1".to_string());

        let result = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("reload result");

        match result {
            UiIoResult::SelectedSessionLoad {
                request_id: None,
                requested_session_id,
                load,
            } => {
                assert_eq!(requested_session_id.as_deref(), Some("session-1"));
                let loaded = load.current_session.expect("reloaded session payload");
                assert_eq!(loaded.minimax_session_id.as_deref(), Some("mm-session-123"));

                let command = crate::provider_bridge::build_provider_command_string(
                    provider,
                    loaded.system_prompt.as_deref(),
                    Some(&loaded),
                    None,
                    "follow-up question",
                );
                assert!(
                    command.contains("--session-id mm-session-123"),
                    "expected resumed adapter command, got: {command}"
                );
                assert!(
                    command.contains("--resume"),
                    "expected resume flag in adapter command, got: {command}"
                );
            }
            other => panic!(
                "unexpected reload result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}

#[test]
fn spawn_reload_current_session_async_emits_load_failed_for_missing_session() {
    with_temp_home(|| {
        let mut state = test_app_state(ModelProvider::Claude);
        state.spawn_reload_current_session_async("missing".to_string());

        let result = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("reload failure result");

        match result {
            UiIoResult::SessionLoadFailed {
                request_id: None,
                session_id,
                requested_session_id,
                error,
            } => {
                assert_eq!(session_id, "missing");
                assert_eq!(requested_session_id.as_deref(), Some("missing"));
                assert!(error.contains("reload current session failed"));
            }
            other => panic!(
                "unexpected reload failure result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}

#[test]
fn spawn_save_session_async_with_refresh_emits_success_then_selected_load() {
    with_temp_home(|| {
        let provider = ModelProvider::Claude;
        let mut session = make_session("session-1");
        session.system_prompt = Some("prompt".to_string());

        let data_manager = DataManager::new(provider).expect("data manager");
        let mut state = test_app_state(provider);
        state.session.data_manager = Some(data_manager);

        state.spawn_save_session_async(session, true, Some(7), false);

        let success = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("save success result");
        let refresh = state
            .session
            .ui_io
            .rx
            .recv_timeout(StdDuration::from_secs(1))
            .expect("save refresh result");

        match success {
            UiIoResult::SessionSaveSuccess {
                request_id: Some(7),
                session_id,
                is_system_prompt_save: false,
            } => assert_eq!(session_id, "session-1"),
            other => panic!(
                "unexpected save success result: {:?}",
                std::mem::discriminant(&other)
            ),
        }

        match refresh {
            UiIoResult::SelectedSessionLoad {
                request_id: None,
                requested_session_id,
                load,
            } => {
                assert_eq!(requested_session_id.as_deref(), Some("session-1"));
                assert_eq!(load.requested_session_id.as_deref(), Some("session-1"));
                let saved = load.current_session.expect("saved session payload");
                assert_eq!(saved.id, "session-1");
                assert_eq!(saved.system_prompt.as_deref(), Some("prompt"));
            }
            other => panic!(
                "unexpected save refresh result: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    });
}
