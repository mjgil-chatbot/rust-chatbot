use super::*;

#[test]
fn save_omniforge_model_updates_current_session_in_memory() {
    let mut state = test_app_state(ModelProvider::OmniForge);
    let mut session = make_session("session-1");
    session.omniforge_model_id = Some(crate::omniforge::DEFAULT_MODEL_ID.to_string());
    state.set_current_session(session);

    state.enqueue_event(AppEvent::UiAction(AppUiAction::SaveCurrentOmniForgeModel {
        session_id: "session-1".to_string(),
        model_id: "minimax-m2.7".to_string(),
    }));
    state.check_for_response();

    assert_eq!(
        state
            .session
            .current_session
            .as_ref()
            .and_then(|value| value.omniforge_model_id.as_deref()),
        Some("minimax-m2.7")
    );
}

#[test]
fn send_failed_clears_pending_draft_and_appends_error() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));
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
        "SendFailed must clear pending_draft"
    );
    let current = state
        .session
        .current_session
        .as_ref()
        .expect("session present");
    let last = current.messages.last().expect("error message appended");
    assert!(
        last.content.contains("boom"),
        "error text must be part of the appended message: {:?}",
        last.content
    );
    assert!(!last.is_user, "error message must be non-user");
    assert!(
        !state
            .session
            .ui_io
            .pending_send_chat_ids
            .contains("session-1"),
        "pending send marker must be cleared"
    );
}

#[test]
fn worker_discovery_events_are_replayed_by_check_for_response() {
    let mut state = test_app_state(ModelProvider::Claude);
    let chat_id = "discovered-session";

    let mut discovery_job =
        worker::WorkerJobSnapshot::new(chat_id, ModelProvider::Claude.as_str(), 1234, 0);
    discovery_job.state = worker::WorkerState::Done;
    discovery_job.sequence = 17;

    state.worker.processing_chat_ids.insert(chat_id.to_string());

    let (tx, rx) = mpsc::channel();
    state.worker.worker_discovery_rx = Some(rx);

    tx.send(DiscoveredWorkerJob {
        job: discovery_job,
        response: String::new(),
        result: None,
    })
    .expect("enqueue discovered worker job");

    state.worker.last_worker_poll =
        std::time::Instant::now() - WORKER_POLL_INTERVAL - WORKER_POLL_INTERVAL;
    state.check_for_response();

    assert_eq!(state.worker.last_seen_worker_seq.get(chat_id), Some(&17));
    assert!(state.worker.worker_discovery_rx.is_some());
    assert!(!state.worker.processing_chat_ids.contains(chat_id));
    assert!(state.worker.locally_finalized_chat_ids.contains(chat_id));
}

#[test]
fn poll_worker_slots_skips_job_matching_discovery_sequence() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!("rust-chatbot-home-{}", uuid::Uuid::new_v4()));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let chat_id = "stale-seq-chat";
    let mut stale_job =
        worker::WorkerJobSnapshot::new(chat_id, ModelProvider::Claude.as_str(), 0, 0);
    stale_job.state = worker::WorkerState::Done;
    stale_job.sequence = 19;

    let worker_dir = worker::worker_dir(chat_id);
    std::fs::create_dir_all(&worker_dir).expect("worker dir");
    std::fs::write(
        worker::state_path(chat_id),
        serde_json::to_string(&stale_job).expect("serialize discovered worker state"),
    )
    .expect("write worker state");

    let mut state = test_app_state(ModelProvider::Claude);
    state
        .worker
        .last_seen_worker_seq
        .insert(chat_id.to_string(), stale_job.sequence);
    state.worker.last_worker_poll =
        std::time::Instant::now() - WORKER_POLL_INTERVAL - WORKER_POLL_INTERVAL;

    state.poll_worker_slots();

    assert!(worker_dir.exists());
    assert_eq!(
        state.worker.last_seen_worker_seq.get(chat_id),
        Some(&stale_job.sequence)
    );
    assert!(!state.worker.processing_chat_ids.contains(chat_id));

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = std::fs::remove_dir_all(home_dir);
}

#[test]
fn check_for_response_replays_discovery_before_poll_and_preserves_poll_skips() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!("rust-chatbot-home-{}", uuid::Uuid::new_v4()));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let chat_id = "combined-order-chat";
    let mut discovery_job = worker::WorkerJobSnapshot::new(
        chat_id,
        ModelProvider::Claude.as_str(),
        std::process::id(),
        0,
    );
    discovery_job.state = worker::WorkerState::Running;
    discovery_job.sequence = 31;

    let worker_dir = worker::worker_dir(chat_id);
    std::fs::create_dir_all(&worker_dir).expect("worker dir");
    std::fs::write(
        worker::state_path(chat_id),
        serde_json::to_string(&discovery_job).expect("serialize worker state"),
    )
    .expect("write worker state file");

    let mut state = test_app_state(ModelProvider::Claude);
    let (tx, rx) = mpsc::channel();
    state.worker.worker_discovery_rx = Some(rx);
    tx.send(DiscoveredWorkerJob {
        job: discovery_job,
        response: String::new(),
        result: None,
    })
    .expect("enqueue discovery event");

    state.worker.last_worker_poll = std::time::Instant::now() - WORKER_POLL_INTERVAL;
    state.check_for_response();

    assert_eq!(state.worker.last_seen_worker_seq.get(chat_id), Some(&31));
    assert!(state.worker.processing_chat_ids.contains(chat_id));
    assert!(state.worker.local_processing_chat_ids.contains(chat_id));
    assert!(
        std::fs::metadata(&worker_dir).is_ok(),
        "worker dir should remain until terminal state"
    );

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = std::fs::remove_dir_all(home_dir);
}
