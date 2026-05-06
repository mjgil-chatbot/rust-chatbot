use super::*;

use std::fs;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration as StdDuration, Instant};

use chrono::Duration;

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

/// Variant of `test_app_state` that keeps the `SyncEvent` sender alive,
/// so tests can push events into `drain_sync_events_async`.
fn test_app_state_with_sync(provider: ModelProvider) -> (AppState, mpsc::Sender<SyncEvent>) {
    let (tx, rx) = mpsc::channel();
    let state = AppState {
        session: SessionState::new(provider),
        worker: AppWorkerState::new(),
        layout: LayoutState::new(),
        sync_shm: None,
        sync_socket: SocketHandle::noop(),
        sync_events: rx,
        instance_number: None,
        dialog_parent: None,
    };
    (state, tx)
}

fn wait_for_height_cache_load(state: &mut AppState, session_id: &str) {
    let deadline = Instant::now() + StdDuration::from_secs(1);
    while Instant::now() < deadline {
        state.drain_height_cache_load_results();
        if state.layout.session_height_cache.contains_key(session_id) {
            return;
        }
        thread::sleep(StdDuration::from_millis(10));
    }
    panic!("timed out waiting for height cache load for {session_id}");
}

#[path = "session_runtime_tests_tail.rs"]
mod tail;

#[test]
fn set_current_session_syncs_session_and_layout_state() {
    let mut state = test_app_state(ModelProvider::Codex);
    let mut session = make_session("session-1");
    session.system_prompt = Some("use tools".to_string());
    session.json_summary = Some(JsonSummary {
        total_input_tokens: 42,
        ..Default::default()
    });

    state.set_current_session(session);

    assert_eq!(state.session.current_session_id(), Some("session-1"));
    assert_eq!(state.session.system_prompt_text, "use tools");
    assert_eq!(
        state
            .layout
            .current_json_summary_preview
            .as_ref()
            .map(|preview| preview.total_input_tokens),
        Some(42)
    );
    assert!(state.session.session_cache.contains_key("session-1"));
}

#[test]
fn set_current_session_keeps_oversized_current_session_out_of_cache() {
    let mut state = test_app_state(ModelProvider::Codex);
    let mut session = make_session("oversized");
    session.messages.push(crate::data::ChatMessage {
        content: "x".repeat(SESSION_CACHE_MAX_ENTRY_BYTES + 1),
        is_user: false,
        timestamp: chrono::Utc::now(),
        command: None,
        retrieved_memory: None,
    });

    state.set_current_session(session);

    assert_eq!(state.session.current_session_id(), Some("oversized"));
    assert!(
        !state.session.session_cache.contains_key("oversized"),
        "oversized current session must not be cloned into the recent-session cache"
    );
}

#[test]
fn with_session_mut_refreshes_current_layout_view() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));

    state.with_session_mut("session-1", |session| {
        session.system_prompt = Some("updated prompt".to_string());
        session.json_summary = Some(JsonSummary {
            response_count: 2,
            ..Default::default()
        });
    });

    assert_eq!(state.session.system_prompt_text, "updated prompt");
    assert_eq!(
        state
            .layout
            .current_json_summary_preview
            .as_ref()
            .map(|preview| preview.response_count),
        Some(2)
    );
    assert_eq!(
        state
            .session
            .current_session
            .as_ref()
            .and_then(|session| session.system_prompt.as_deref()),
        Some("updated prompt")
    );
}

#[test]
fn remote_terminal_update_updates_current_session_owned_state() {
    let mut state = test_app_state(ModelProvider::Codex);
    state.set_current_session(make_session("session-1"));
    state.layout.should_focus_input = false;

    state.handle_remote_stream_terminal_update(
        "session-1",
        "assistant reply",
        Some("resume-1".to_string()),
        true,
    );

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(current.messages.len(), 1);
    assert_eq!(current.messages[0].content, "assistant reply");
    assert_eq!(current.codex_session_id.as_deref(), Some("resume-1"));
    assert_eq!(
        state.layout.pending_height_cache_save.as_deref(),
        Some("session-1")
    );
    assert!(state.layout.should_focus_input);
}

#[test]
fn remote_terminal_update_replaces_stale_provider_session_id() {
    let mut state = test_app_state(ModelProvider::MiniMax);
    let mut session = make_session("session-1");
    session.minimax_session_id = Some("mm-stale".to_string());
    state.set_current_session(session);

    state.handle_remote_stream_terminal_update(
        "session-1",
        "assistant reply",
        Some("mm-fresh".to_string()),
        false,
    );

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(current.minimax_session_id.as_deref(), Some("mm-fresh"));
}

#[test]
fn hydrating_active_worker_output_does_not_promote_clicked_auto_session() {
    // worker_dir() resolves through HOME via config::shared_infra_dir(); other
    // tests in this file mutate HOME under TEST_ENV_LOCK, so this test must
    // hold the same lock to avoid reading/writing under a foreign HOME.
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chat_id = format!("auto-hydrate-{}", uuid::Uuid::new_v4());
    let worker_dir = worker::worker_dir(&chat_id);
    fs::create_dir_all(&worker_dir).unwrap();

    let original_updated_at = chrono::Utc::now() - Duration::minutes(10);
    let newer_updated_at = chrono::Utc::now() - Duration::minutes(1);

    let mut state = test_app_state(ModelProvider::Claude);
    let mut current = make_session(&chat_id);
    current.name = "[auto]".to_string();
    current.updated_at = original_updated_at;
    state.session.current_session = Some(current.clone());
    state.session.session_cache.insert(chat_id.clone(), current);
    state.session.sessions = vec![
        SessionInfo {
            id: "newer".to_string(),
            name: "Chat 2".to_string(),
            created_at: newer_updated_at,
            updated_at: newer_updated_at,
            message_count: 0,
            hidden: false,
        },
        SessionInfo {
            id: chat_id.clone(),
            name: "[auto]".to_string(),
            created_at: original_updated_at,
            updated_at: original_updated_at,
            message_count: 0,
            hidden: false,
        },
    ];

    let job = worker::WorkerJobSnapshot::new(&chat_id, "claude", 0, 0);
    fs::write(
        worker::state_path(&chat_id),
        serde_json::to_string(&job).unwrap(),
    )
    .unwrap();
    fs::write(worker::output_path(&chat_id), "live autonomous output").unwrap();

    state.hydrate_current_session_from_active_worker();

    let hydrated = state
        .session
        .current_session
        .as_ref()
        .expect("current session should remain loaded");
    assert_eq!(hydrated.messages.len(), 1);
    assert_eq!(hydrated.messages[0].content, "live autonomous output");
    assert_eq!(hydrated.updated_at, original_updated_at);
    assert_eq!(state.session.sessions[0].id, "newer");
    assert_eq!(state.session.sessions[1].id, chat_id);

    let _ = fs::remove_dir_all(worker_dir);
}

#[test]
fn set_current_session_syncs_selection_fields() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.session.loading_session_id = Some("session-1".to_string());
    state.session.selected_session_id = Some("old-session".to_string());

    state.set_current_session(make_session("session-1"));

    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("session-1")
    );
    assert!(
        state.session.loading_session_id.is_none(),
        "loading_session_id must be cleared after set_current_session"
    );
    assert_eq!(state.session.current_session_id(), Some("session-1"));
}

#[test]
fn set_current_session_clears_loading_for_different_id() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.session.loading_session_id = Some("loading-x".to_string());

    state.set_current_session(make_session("session-2"));

    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("session-2")
    );
    assert!(state.session.loading_session_id.is_none());
}

#[test]
fn apply_stream_to_current_clears_pending_draft_on_first_chunk() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "session-1".to_string(),
        user_message_timestamp_ms: 123,
    });

    let response_changed =
        state
            .session
            .apply_stream_to_current("session-1", "hello", None, &mut state.layout, false);

    assert!(response_changed, "first chunk must register as changed");
    assert!(
        state.session.pending_draft.is_none(),
        "pending_draft must clear atomically on first real chunk"
    );
}

#[test]
fn apply_stream_to_current_empty_response_keeps_pending_draft() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "session-1".to_string(),
        user_message_timestamp_ms: 123,
    });

    state
        .session
        .apply_stream_to_current("session-1", "", None, &mut state.layout, false);

    assert!(
        state.session.pending_draft.is_some(),
        "empty response must not clear pending_draft"
    );
    let session = state
        .session
        .current_session
        .as_ref()
        .expect("current session should remain loaded");
    assert_eq!(
        session.messages.len(),
        0,
        "empty response must not insert a blank assistant row"
    );
    assert!(session.messages.last().is_none());
}

#[test]
fn apply_stream_to_current_preserves_draft_for_other_chat() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "session-other".to_string(),
        user_message_timestamp_ms: 123,
    });

    state
        .session
        .apply_stream_to_current("session-1", "hello", None, &mut state.layout, false);

    let draft = state
        .session
        .pending_draft
        .as_ref()
        .expect("draft for unrelated chat must survive");
    assert_eq!(draft.chat_id, "session-other");
}

#[test]
fn local_terminal_worker_clears_pending_draft() {
    let mut state = test_app_state(ModelProvider::Claude);
    let now = chrono::Utc::now();
    state.session.sessions = vec![SessionInfo {
        id: "a".into(),
        name: "A".into(),
        created_at: now,
        updated_at: now,
        message_count: 0,
        hidden: false,
    }];
    state.set_current_session(make_session("a"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "a".to_string(),
        user_message_timestamp_ms: 42,
    });

    state.handle_local_terminal_worker(
        "a",
        "final response",
        WireMessage::sessions_changed(),
        "done",
        "completed",
        false,
        |session, response| upsert_assistant_response(session, response),
    );

    assert!(
        state.session.pending_draft.is_none(),
        "terminal worker must clear pending draft"
    );
}

#[test]
fn local_terminal_worker_clears_pending_draft_even_on_empty_response() {
    let mut state = test_app_state(ModelProvider::Claude);
    let now = chrono::Utc::now();
    state.session.sessions = vec![SessionInfo {
        id: "a".into(),
        name: "A".into(),
        created_at: now,
        updated_at: now,
        message_count: 0,
        hidden: false,
    }];
    state.set_current_session(make_session("a"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "a".to_string(),
        user_message_timestamp_ms: 42,
    });

    state.handle_local_terminal_worker(
        "a",
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
        "terminal worker must clear pending draft even on empty response"
    );
}

#[test]
fn remote_terminal_update_clears_pending_draft() {
    let mut state = test_app_state(ModelProvider::Codex);
    state.set_current_session(make_session("session-1"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "session-1".to_string(),
        user_message_timestamp_ms: 42,
    });

    state.handle_remote_stream_terminal_update(
        "session-1",
        "assistant reply",
        Some("resume-1".to_string()),
        true,
    );

    assert!(
        state.session.pending_draft.is_none(),
        "remote terminal update must clear pending draft"
    );
}

#[test]
fn remote_stream_end_does_not_promote_non_current_chat() {
    let (mut state, tx) = test_app_state_with_sync(ModelProvider::Claude);
    let now = chrono::Utc::now();
    state.session.sessions = vec![
        SessionInfo {
            id: "current".into(),
            name: "Current".into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            hidden: false,
        },
        SessionInfo {
            id: "other".into(),
            name: "Other".into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            hidden: false,
        },
    ];
    state.set_current_session(make_session("current"));

    tx.send(SyncEvent::StreamEnd {
        chat_id: "other".to_string(),
        provider: ModelProvider::Claude.as_str().to_string(),
        response: "done".to_string(),
        session_id: None,
    })
    .expect("send stream end");

    state.drain_sync_events_async();

    assert_eq!(
        state.session.sessions[1].id, "other",
        "completed non-current chat must not be promoted — promotion happens on activation only"
    );
    assert_eq!(
        state.session.current_session_id(),
        Some("current"),
        "stream end must not change the active main-panel chat"
    );
}

#[test]
fn remote_stream_stopped_does_not_promote_non_current_chat() {
    let (mut state, tx) = test_app_state_with_sync(ModelProvider::Claude);
    let now = chrono::Utc::now();
    state.session.sessions = vec![
        SessionInfo {
            id: "current".into(),
            name: "Current".into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            hidden: false,
        },
        SessionInfo {
            id: "other".into(),
            name: "Other".into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            hidden: false,
        },
    ];
    state.set_current_session(make_session("current"));

    tx.send(SyncEvent::StreamStopped {
        chat_id: "other".to_string(),
        provider: ModelProvider::Claude.as_str().to_string(),
        response: "partial".to_string(),
    })
    .expect("send stream stopped");

    state.drain_sync_events_async();

    assert_eq!(
        state.session.sessions[1].id, "other",
        "stopped non-current chat must not be promoted — promotion happens on activation only"
    );
    assert_eq!(state.session.current_session_id(), Some("current"));
}

#[test]
fn observed_terminal_worker_does_not_promote_non_current_chat() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!("rust-chatbot-home-{}", uuid::Uuid::new_v4()));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let older = chrono::Utc::now() - Duration::minutes(5);
    let newer = chrono::Utc::now() - Duration::minutes(1);
    let mut current = make_session("current");
    current.updated_at = older;
    let mut other = make_session("other");
    other.updated_at = newer;

    let data_manager = DataManager::new(ModelProvider::Claude).expect("temp data manager");
    data_manager
        .save_session(&current)
        .expect("save current session");
    data_manager
        .save_session(&other)
        .expect("save other session");

    let mut state = test_app_state(ModelProvider::Claude);
    state.session.data_manager = Some(data_manager);
    state.session.sessions = vec![
        SessionInfo {
            id: current.id.clone(),
            name: current.name.clone(),
            created_at: current.created_at,
            updated_at: current.updated_at,
            message_count: 0,
            hidden: false,
        },
        SessionInfo {
            id: other.id.clone(),
            name: other.name.clone(),
            created_at: other.created_at,
            updated_at: other.updated_at,
            message_count: 0,
            hidden: false,
        },
    ];
    state.set_current_session(current.clone());
    state.worker.processing_chat_ids.insert(other.id.clone());

    state.finish_observed_terminal_worker(&other.id, "done");

    assert_eq!(
        state.session.current_session_id(),
        Some(current.id.as_str()),
        "observed terminal completion must not change the active main-panel chat"
    );
    assert!(
        !state.worker.processing_chat_ids.contains(&other.id),
        "observed terminal completion must clear the processing marker"
    );

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn local_terminal_worker_does_not_promote_session() {
    let mut state = test_app_state(ModelProvider::Claude);
    let now = chrono::Utc::now();
    state.session.sessions = vec![
        SessionInfo {
            id: "a".into(),
            name: "A".into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            hidden: false,
        },
        SessionInfo {
            id: "b".into(),
            name: "B".into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            hidden: false,
        },
    ];

    state.set_current_session(make_session("a"));

    state.handle_local_terminal_worker(
        "b",
        "done",
        WireMessage::sessions_changed(),
        "done",
        "completed",
        false,
        |session, response| upsert_assistant_response(session, response),
    );

    assert_eq!(
        state.session.sessions[0].id, "a",
        "completion must not promote — session order stays unchanged"
    );
    assert_eq!(state.session.sessions[1].id, "b");
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("a"),
        "terminal completion must not change the active main-panel chat"
    );
    assert_eq!(
        state.session.current_session_id(),
        Some("a"),
        "current session must remain unchanged after completion"
    );
}

#[test]
fn remote_stream_data_for_different_provider_is_ignored() {
    let (mut state, tx) = test_app_state_with_sync(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "session-1".to_string(),
        user_message_timestamp_ms: 1,
    });

    tx.send(SyncEvent::StreamData {
        chat_id: "session-1".to_string(),
        provider: ModelProvider::Codex.as_str().to_string(),
        response: "should be ignored".to_string(),
    })
    .expect("send stream data");

    state.drain_sync_events_async();

    assert!(
        state.session.pending_draft.is_some(),
        "events from another provider must not clear our draft"
    );
}

#[test]
fn switching_chats_preserves_pending_draft_for_original_chat() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("draft-owner"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "draft-owner".to_string(),
        user_message_timestamp_ms: 123,
    });

    state.set_current_session(make_session("other-chat"));
    assert!(state.session.has_pending_draft_for("draft-owner"));
    assert!(!state.session.has_pending_draft_for("other-chat"));
    assert_eq!(state.session.current_session_id(), Some("other-chat"));

    state.set_current_session(make_session("draft-owner"));
    assert!(state.session.has_pending_draft_for("draft-owner"));
    assert_eq!(state.session.current_session_id(), Some("draft-owner"));
}
