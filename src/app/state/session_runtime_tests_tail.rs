use super::*;

#[test]
fn remote_stream_begin_reloads_stale_current_session_from_disk_before_processing() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!(
        "rust-chatbot-stream-begin-home-{}",
        uuid::Uuid::new_v4()
    ));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let mut current = make_session("stream-begin-sync");
    current.add_message("question".to_string(), true);
    current.add_message("stale assistant".to_string(), false);
    let current_loaded = current.clone();

    let mut disk_session = current_loaded.clone();
    disk_session.messages[1].content = "final persisted answer".to_string();
    disk_session.updated_at = chrono::Utc::now();
    disk_session.add_message("next question".to_string(), true);
    data_manager
        .save_session(&disk_session)
        .expect("persist disk session");

    let (mut state, tx) = test_app_state_with_sync(provider);
    state.session.data_manager = Some(data_manager);
    state.set_current_session(current);

    tx.send(SyncEvent::StreamBegin {
        chat_id: disk_session.id.clone(),
        provider: provider.as_str().to_string(),
    })
    .expect("send stream begin");

    state.check_for_response();

    let current = state
        .session
        .current_session
        .as_ref()
        .expect("current session");
    assert_eq!(current.messages.len(), 3);
    assert_eq!(current.messages[1].content, "final persisted answer");
    assert!(current.messages[2].is_user);
    assert_eq!(current.messages[2].content, "next question");
    assert!(state.worker.is_chat_processing("stream-begin-sync"));

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn switching_chats_loads_each_session_height_cache_in_background() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!("rust-chatbot-home-{}", uuid::Uuid::new_v4()));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).expect("temp data manager");

    let session_a = make_session("alpha");
    let session_b = make_session("beta");
    data_manager
        .save_session(&session_a)
        .expect("save alpha session");
    data_manager
        .save_session(&session_b)
        .expect("save beta session");

    let now = chrono::Utc::now();
    let mut cache_a = crate::data::SessionHeightCache::new("alpha".to_string());
    cache_a.widths.insert(
        900,
        crate::data::HeightCacheEntry {
            row_heights: vec![100.0, 110.0],
            total_height: 210.0,
            message_count: 2,
            updated_at: now,
        },
    );
    data_manager
        .save_height_cache(&cache_a)
        .expect("save alpha height cache");

    let mut cache_b = crate::data::SessionHeightCache::new("beta".to_string());
    cache_b.widths.insert(
        900,
        crate::data::HeightCacheEntry {
            row_heights: vec![80.0, 90.0, 100.0],
            total_height: 270.0,
            message_count: 3,
            updated_at: now,
        },
    );
    data_manager
        .save_height_cache(&cache_b)
        .expect("save beta height cache");

    let mut state = test_app_state(provider);
    state.session.data_manager = Some(data_manager);

    state.set_current_session(session_a);
    assert!(
        state.layout.session_height_cache.get("alpha").is_none(),
        "switch should not synchronously hit height-cache storage"
    );
    assert!(
        state.session.height_cache_load_rx.is_some(),
        "switch should schedule a background height-cache load"
    );
    wait_for_height_cache_load(&mut state, "alpha");
    let loaded_alpha = state
        .layout
        .session_height_cache
        .get("alpha")
        .expect("alpha height cache must load when alpha becomes current");
    assert_eq!(
        loaded_alpha.widths.get(&900).map(|e| e.total_height),
        Some(210.0)
    );
    assert_eq!(state.session.selected_session_id.as_deref(), Some("alpha"));

    state.set_current_session(session_b);
    assert!(
        state.layout.session_height_cache.get("beta").is_none(),
        "switch should keep the next session activation immediate before the background load finishes"
    );
    wait_for_height_cache_load(&mut state, "beta");
    let loaded_beta = state
        .layout
        .session_height_cache
        .get("beta")
        .expect("beta height cache must load after switching from alpha");
    let beta_entry = loaded_beta
        .widths
        .get(&900)
        .expect("width 900 bucket must be present for beta");
    assert_eq!(beta_entry.total_height, 270.0);
    assert_eq!(beta_entry.row_heights.len(), 3);

    assert_eq!(state.session.selected_session_id.as_deref(), Some("beta"));
    assert!(
        state.session.loading_session_id.is_none(),
        "loading_session_id must clear after the switch settles"
    );
    assert!(
        state.session.height_cache_load_rx.is_none(),
        "background height-cache load should settle after draining its result"
    );

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
}

#[test]
fn switching_chats_keeps_previous_draft_hidden_from_new_selection() {
    // Switching the active chat while a draft is in flight for chat A must not
    // leave A's draft visible against chat B's render — the lifecycle clears
    // drafts only on terminal/stream events, but the sidebar selection swap
    // must keep selected_session_id authoritative.
    let mut state = test_app_state(ModelProvider::Codex);
    state.set_current_session(make_session("alpha"));
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "alpha".to_string(),
        user_message_timestamp_ms: 1,
    });

    state.set_current_session(make_session("beta"));

    assert_eq!(state.session.selected_session_id.as_deref(), Some("beta"));
    // The draft survives in state (cleared elsewhere on first chunk / terminal)
    // but should_show_pending_draft will hide it because the active chat differs.
    let draft = state
        .session
        .pending_draft
        .as_ref()
        .expect("draft for the previous chat must persist until its lifecycle completes");
    assert_eq!(draft.chat_id, "alpha");
}

#[test]
fn worker_response_logging_is_idempotent() {
    let mut worker = AppWorkerState::new();

    assert!(!worker.start_response_logging("session-1", ""));
    assert!(worker.start_response_logging("session-1", "chunk"));
    assert!(!worker.start_response_logging("session-1", "chunk"));
    assert!(worker.finish_response_logging("session-1"));
    assert!(!worker.finish_response_logging("session-1"));
}
