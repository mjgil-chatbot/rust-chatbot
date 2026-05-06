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

#[test]
fn local_terminal_worker_persists_session_before_worker_cleanup() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-terminal-save-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Claude;
    let data_manager = DataManager::new(provider).expect("temp data manager");
    let session = make_session("terminal-save");
    data_manager
        .save_session(&session)
        .expect("seed terminal-save session");

    let worker_dir = worker::worker_dir(&session.id);
    fs::create_dir_all(&worker_dir).expect("create worker dir");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);
    state.session.sessions = vec![SessionInfo {
        id: session.id.clone(),
        name: session.name.clone(),
        created_at: session.created_at,
        updated_at: session.updated_at,
        message_count: 0,
        hidden: false,
    }];
    state.set_current_session(session.clone());
    state.worker.processing_chat_ids.insert(session.id.clone());

    state.handle_local_terminal_worker(
        &session.id,
        "final response",
        WireMessage::sessions_changed(),
        "done",
        "done",
        false,
        |session, response| upsert_assistant_response(session, response),
    );

    let persisted = DataManager::new(provider)
        .expect("reload data manager")
        .load_session(&session.id)
        .expect("persisted session");
    assert_eq!(persisted.messages.len(), 1);
    assert_eq!(persisted.messages[0].content, "final response");
    assert!(
        !worker_dir.exists(),
        "worker spool should only be removed after the terminal save lands on disk"
    );

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn local_terminal_worker_reloads_newer_disk_session_before_applying_response() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-terminal-reload-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Claude;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let mut disk_session = make_session("terminal-reload");
    disk_session.add_message("older assistant".to_string(), false);
    let stale_current = disk_session.clone();
    disk_session.add_message("continue".to_string(), true);
    data_manager
        .save_session(&disk_session)
        .expect("persist disk session with newer user turn");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);
    state.session.sessions = vec![SessionInfo {
        id: disk_session.id.clone(),
        name: disk_session.name.clone(),
        created_at: disk_session.created_at,
        updated_at: disk_session.updated_at,
        message_count: 1,
        hidden: false,
    }];
    state.set_current_session(stale_current);

    state.handle_local_terminal_worker(
        &disk_session.id,
        "new assistant reply",
        WireMessage::sessions_changed(),
        "done",
        "done",
        false,
        |session, response| upsert_assistant_response(session, response),
    );

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(current.messages.len(), 3);
    assert_eq!(current.messages[0].content, "older assistant");
    assert!(current.messages[1].is_user);
    assert_eq!(current.messages[1].content, "continue");
    assert_eq!(current.messages[2].content, "new assistant reply");

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn local_terminal_worker_keeps_existing_visible_assistant_when_terminal_response_is_think_only() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-terminal-think-only-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Claude;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let mut session = make_session("terminal-think-only");
    session.add_message("Visible final answer".to_string(), false);
    data_manager
        .save_session(&session)
        .expect("seed terminal-think-only session");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);
    state.session.sessions = vec![SessionInfo {
        id: session.id.clone(),
        name: session.name.clone(),
        created_at: session.created_at,
        updated_at: session.updated_at,
        message_count: 1,
        hidden: false,
    }];
    state.set_current_session(session.clone());
    state.worker.processing_chat_ids.insert(session.id.clone());

    state.handle_local_terminal_worker(
        &session.id,
        "<think>Need to inspect the code first.</think>\n\n",
        WireMessage::sessions_changed(),
        "done",
        "done",
        false,
        |session, response| upsert_assistant_response(session, response),
    );

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(current.messages.len(), 1);
    assert_eq!(current.messages[0].content, "Visible final answer");

    let persisted = DataManager::new(provider)
        .expect("reload data manager")
        .load_session(&session.id)
        .expect("persisted session");
    assert_eq!(persisted.messages.len(), 1);
    assert_eq!(persisted.messages[0].content, "Visible final answer");

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn current_session_sync_allows_processing_chat_to_reload_strict_disk_extension() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-processing-sync-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Claude;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let mut disk_session = make_session("processing-sync");
    disk_session.add_message("older assistant".to_string(), false);
    let stale_current = disk_session.clone();
    disk_session.add_message("continue".to_string(), true);
    data_manager
        .save_session(&disk_session)
        .expect("persist newer disk session");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);
    state.set_current_session(stale_current);
    state
        .worker
        .processing_chat_ids
        .insert(disk_session.id.clone());

    assert!(state.sync_current_session_from_disk_if_safe(&disk_session.id));
    assert_eq!(
        state
            .session
            .current_session
            .as_ref()
            .map(|session| session.messages.len()),
        Some(2)
    );
    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert!(current.messages[1].is_user);
    assert_eq!(current.messages[1].content, "continue");

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn current_session_sync_allows_processing_chat_to_reload_appended_user_turn_after_stale_assistant()
{
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-processing-sync-stale-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let mut current = make_session("processing-sync-stale");
    current.add_message("question".to_string(), true);
    current.add_message("stale transcript tail".to_string(), false);

    let mut disk_session = current.clone();
    disk_session.messages[1].content = "final persisted answer".to_string();
    disk_session.updated_at = chrono::Utc::now();
    disk_session.add_message("next question".to_string(), true);
    data_manager
        .save_session(&disk_session)
        .expect("persist newer disk session");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);
    state.set_current_session(current);
    state
        .worker
        .processing_chat_ids
        .insert(disk_session.id.clone());

    assert!(state.sync_current_session_from_disk_if_safe(&disk_session.id));
    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(current.messages.len(), 3);
    assert_eq!(current.messages[1].content, "final persisted answer");
    assert!(current.messages[2].is_user);
    assert_eq!(current.messages[2].content, "next question");

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn remote_terminal_update_reloads_newer_disk_session_before_applying_response() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-remote-terminal-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let mut disk_session = make_session("remote-terminal");
    disk_session.add_message("older assistant".to_string(), false);
    let stale_current = disk_session.clone();
    disk_session.add_message("continue".to_string(), true);
    data_manager
        .save_session(&disk_session)
        .expect("persist disk session with newer user turn");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);
    state.session.sessions = vec![SessionInfo {
        id: disk_session.id.clone(),
        name: disk_session.name.clone(),
        created_at: disk_session.created_at,
        updated_at: disk_session.updated_at,
        message_count: 1,
        hidden: false,
    }];
    state.set_current_session(stale_current);

    state.handle_remote_stream_terminal_update(
        &disk_session.id,
        "new assistant reply",
        Some("resume-2".to_string()),
        true,
    );

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(current.messages.len(), 3);
    assert_eq!(current.messages[0].content, "older assistant");
    assert!(current.messages[1].is_user);
    assert_eq!(current.messages[1].content, "continue");
    assert_eq!(current.messages[2].content, "new assistant reply");
    assert_eq!(current.codex_session_id.as_deref(), Some("resume-2"));

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn local_terminal_worker_preserves_newer_disk_turn_while_current_chat_is_processing() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-terminal-latest-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Claude;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let mut disk_session = make_session("terminal-processing");
    disk_session.add_message("older assistant".to_string(), false);
    let stale_current = disk_session.clone();
    disk_session.add_message("continue".to_string(), true);
    data_manager
        .save_session(&disk_session)
        .expect("persist disk session with newer user turn");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);
    state.session.sessions = vec![SessionInfo {
        id: disk_session.id.clone(),
        name: disk_session.name.clone(),
        created_at: disk_session.created_at,
        updated_at: disk_session.updated_at,
        message_count: 1,
        hidden: false,
    }];
    state.set_current_session(stale_current);
    state
        .worker
        .processing_chat_ids
        .insert(disk_session.id.clone());

    state.handle_local_terminal_worker(
        &disk_session.id,
        "",
        WireMessage::stopped(&disk_session.id, provider.as_str(), ""),
        "stopped",
        "stopped",
        false,
        |session, _response| {
            append_terminal_message_once(session, "Process was stopped by user.");
            false
        },
    );

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(current.messages.len(), 3);
    assert_eq!(current.messages[0].content, "older assistant");
    assert!(current.messages[1].is_user);
    assert_eq!(current.messages[1].content, "continue");
    assert_eq!(current.messages[2].content, "Process was stopped by user.");

    let persisted = DataManager::new(provider)
        .expect("reload data manager")
        .load_session(&disk_session.id)
        .expect("persisted session");
    assert_eq!(persisted.messages.len(), 3);
    assert_eq!(persisted.messages[1].content, "continue");
    assert_eq!(
        persisted.messages[2].content,
        "Process was stopped by user."
    );

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn current_session_sync_rejects_divergent_newer_disk_snapshot() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-sync-divergent-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Claude;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let mut current = make_session("divergent-sync");
    current.add_message("assistant reply".to_string(), false);
    current.updated_at -= chrono::Duration::seconds(30);

    let mut divergent_disk = make_session("divergent-sync");
    divergent_disk.updated_at = current.updated_at + chrono::Duration::seconds(60);
    divergent_disk.add_message("different assistant reply".to_string(), false);
    data_manager
        .save_session(&divergent_disk)
        .expect("persist divergent disk session");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);
    state.set_current_session(current);

    assert!(
        !state.sync_current_session_from_disk_if_safe("divergent-sync"),
        "divergent newer disk snapshot must not overwrite current session"
    );
    let session = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].content, "assistant reply");

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn current_session_sync_accepts_strict_disk_extension_with_equal_timestamp() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-sync-extension-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Claude;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let mut current = make_session("extension-sync");
    current.add_message("assistant reply".to_string(), false);
    let shared_updated_at = current.updated_at;

    let mut extended_disk = current.clone();
    extended_disk.updated_at = shared_updated_at;
    extended_disk.add_message("continue".to_string(), true);
    data_manager
        .save_session(&extended_disk)
        .expect("persist strict extension");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);
    state.set_current_session(current);

    assert!(
        state.sync_current_session_from_disk_if_safe("extension-sync"),
        "strict transcript extension should reload even without a newer timestamp"
    );
    let session = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(session.messages.len(), 2);
    assert_eq!(session.messages[0].content, "assistant reply");
    assert!(session.messages[1].is_user);
    assert_eq!(session.messages[1].content, "continue");

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}
