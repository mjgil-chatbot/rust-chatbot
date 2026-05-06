use super::*;

use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chatbot_lib::worker::WorkerRequest;

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

fn stale_worker_request(chat_id: &str) -> WorkerRequest {
    WorkerRequest {
        provider: ModelProvider::Claude.as_str().to_string(),
        query: "hello".to_string(),
        working_dir: None,
        system_prompt: None,
        resume_session_id: None,
        memory_context: None,
        parent_pid_override: None,
        runsc_dirs: Vec::new(),
        runsc_image: None,
        model_override: None,
        command: "claude -p hello".to_string(),
        chat_id: chat_id.to_string(),
    }
}

fn set_path_age(path: &Path, age: Duration) {
    let modified = SystemTime::now()
        .checked_sub(age)
        .expect("target modified time should be representable");
    let modified = modified
        .duration_since(UNIX_EPOCH)
        .expect("modified time should be after unix epoch");
    let times = [
        libc::timespec {
            tv_sec: modified.as_secs() as libc::time_t,
            tv_nsec: modified.subsec_nanos() as libc::c_long,
        },
        libc::timespec {
            tv_sec: modified.as_secs() as libc::time_t,
            tv_nsec: modified.subsec_nanos() as libc::c_long,
        },
    ];
    let c_path = CString::new(path.as_os_str().as_bytes()).expect("path bytes");
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), 0) };
    assert_eq!(rc, 0, "utimensat should update {}", path.display());
}

#[test]
fn poll_worker_slots_clears_processing_for_stale_request_only_spool_entry() {
    with_temp_home(|| {
        let chat_id = format!("stale-request-only-{}", uuid::Uuid::new_v4());
        let worker_dir = worker::worker_dir(&chat_id);
        fs::create_dir_all(&worker_dir).expect("create worker dir");
        worker::write_json_atomic(
            &worker::request_path(&chat_id),
            &stale_worker_request(&chat_id),
        )
        .expect("write request");
        set_path_age(&worker::request_path(&chat_id), Duration::from_secs(10));
        set_path_age(&worker_dir, Duration::from_secs(10));

        let mut state = test_app_state(ModelProvider::Claude);
        state.set_current_session(make_session(&chat_id));
        state.worker.processing_chat_ids.insert(chat_id.clone());
        state
            .worker
            .local_processing_chat_ids
            .insert(chat_id.clone());
        state.worker.last_worker_poll =
            std::time::Instant::now() - WORKER_POLL_INTERVAL - WORKER_POLL_INTERVAL;

        state.poll_worker_slots();

        assert!(
            !worker_dir.exists(),
            "stale request-only spool entry should be removed during polling"
        );
        assert!(
            !state.worker.processing_chat_ids.contains(&chat_id),
            "stale request-only spool entry should clear processing state"
        );
        assert!(
            !state.worker.local_processing_chat_ids.contains(&chat_id),
            "stale request-only spool entry should clear local processing state"
        );
        assert!(
            state.layout.should_focus_input,
            "current chat should regain input focus after stale spool cleanup"
        );
    });
}

#[test]
fn poll_worker_slots_reuses_cached_running_response_when_output_file_is_missing() {
    with_temp_home(|| {
        let chat_id = format!("cached-running-{}", uuid::Uuid::new_v4());
        let worker_dir = worker::worker_dir(&chat_id);
        fs::create_dir_all(&worker_dir).expect("create worker dir");

        let running_job = worker::WorkerJobSnapshot::new(
            &chat_id,
            ModelProvider::Claude.as_str(),
            std::process::id(),
            std::process::id(),
        );
        worker::write_json_atomic(&worker::state_path(&chat_id), &running_job)
            .expect("write running state");

        let timestamp_ms = chrono::Local::now().timestamp_millis();
        crate::stream::append_event(
            &worker::events_path(&chat_id),
            &crate::stream::StreamEvent::TextDelta {
                content: "alpha".to_string(),
                timestamp_ms,
            },
        )
        .expect("write first event");

        let mut state = test_app_state(ModelProvider::Claude);
        state.set_current_session(make_session(&chat_id));
        state.worker.last_worker_poll =
            std::time::Instant::now() - WORKER_POLL_INTERVAL - WORKER_POLL_INTERVAL;
        state.poll_worker_slots();

        let first_response = state
            .worker
            .cached_response(&chat_id)
            .expect("response should be cached")
            .to_string();
        assert!(
            first_response.contains("alpha"),
            "initial poll should reconstruct cached output from events"
        );
        assert!(
            !worker::output_path(&chat_id).exists(),
            "test should exercise the events.jsonl fallback path"
        );

        crate::stream::append_event(
            &worker::events_path(&chat_id),
            &crate::stream::StreamEvent::TextDelta {
                content: "beta".to_string(),
                timestamp_ms: timestamp_ms + 1,
            },
        )
        .expect("write second event");
        worker::write_text_atomic(&worker::output_path(&chat_id), "stale output snapshot")
            .expect("write stale flat output");

        state.worker.last_worker_poll =
            std::time::Instant::now() - WORKER_POLL_INTERVAL - WORKER_POLL_INTERVAL;
        state.poll_worker_slots();

        let second_response = state
            .worker
            .cached_response(&chat_id)
            .expect("response should remain cached")
            .to_string();
        assert!(
            second_response.contains("beta"),
            "subsequent polls should append new event output into the cache"
        );
        assert!(
            second_response.starts_with(&first_response),
            "cache should preserve previously reconstructed output"
        );
        assert!(
            !second_response.contains("stale output snapshot"),
            "incremental running polls should ignore stale flat output once events are tracked"
        );
    });
}

#[test]
fn poll_worker_slots_done_replaces_stale_provider_session_id() {
    with_temp_home(|| {
        let chat_id = format!("done-session-id-{}", uuid::Uuid::new_v4());
        let worker_dir = worker::worker_dir(&chat_id);
        fs::create_dir_all(&worker_dir).expect("create worker dir");

        let mut done_job = worker::WorkerJobSnapshot::new(
            &chat_id,
            ModelProvider::MiniMax.as_str(),
            std::process::id(),
            std::process::id(),
        );
        done_job.state = WorkerState::Done;
        done_job.sequence = 1;
        worker::write_json_atomic(&worker::state_path(&chat_id), &done_job)
            .expect("write done state");
        worker::write_text_atomic(&worker::output_path(&chat_id), "assistant reply")
            .expect("write output");
        worker::write_json_atomic(
            &worker::result_path(&chat_id),
            &WorkerResult {
                session_id: Some("mm-fresh".to_string()),
                summary: None,
                error: None,
            },
        )
        .expect("write result");

        let data_manager = DataManager::new(ModelProvider::MiniMax).expect("data manager");
        let mut session = make_session(&chat_id);
        session.minimax_session_id = Some("mm-stale".to_string());
        data_manager.save_session(&session).expect("save session");

        let mut state = test_app_state(ModelProvider::MiniMax);
        state.session.data_manager = Some(data_manager);
        state.session.sessions = vec![SessionInfo {
            id: session.id.clone(),
            name: session.name.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            message_count: 0,
            hidden: false,
        }];
        state.set_current_session(session);
        state.worker.processing_chat_ids.insert(chat_id.clone());
        state
            .worker
            .local_processing_chat_ids
            .insert(chat_id.clone());
        state.worker.last_worker_poll =
            std::time::Instant::now() - WORKER_POLL_INTERVAL - WORKER_POLL_INTERVAL;

        state.poll_worker_slots();

        let current = state
            .session
            .current_session
            .as_ref()
            .expect("current session");
        assert_eq!(current.minimax_session_id.as_deref(), Some("mm-fresh"));

        let persisted = DataManager::new(ModelProvider::MiniMax)
            .expect("reload data manager")
            .load_session(&chat_id)
            .expect("persisted session");
        assert_eq!(persisted.minimax_session_id.as_deref(), Some("mm-fresh"));
    });
}

#[test]
fn build_worker_poll_snapshot_omits_full_running_response_after_offsets_exist() {
    with_temp_home(|| {
        let chat_id = format!("running-offset-{}", uuid::Uuid::new_v4());
        let worker_dir = worker::worker_dir(&chat_id);
        fs::create_dir_all(&worker_dir).expect("create worker dir");

        let mut running_job = worker::WorkerJobSnapshot::new(
            &chat_id,
            ModelProvider::Claude.as_str(),
            std::process::id(),
            std::process::id(),
        );
        running_job.sequence = 4;
        worker::write_json_atomic(&worker::state_path(&chat_id), &running_job)
            .expect("write running state");
        worker::write_text_atomic(&worker::output_path(&chat_id), "full snapshot")
            .expect("write output");
        crate::stream::append_event(
            &worker::events_path(&chat_id),
            &crate::stream::StreamEvent::TextDelta {
                content: "alpha".to_string(),
                timestamp_ms: chrono::Local::now().timestamp_millis(),
            },
        )
        .expect("write first event");
        let (_, first_offset) = worker::read_events_since(&chat_id, 0);
        crate::stream::append_event(
            &worker::events_path(&chat_id),
            &crate::stream::StreamEvent::TextDelta {
                content: "beta".to_string(),
                timestamp_ms: chrono::Local::now().timestamp_millis() + 1,
            },
        )
        .expect("write second event");

        let mut last_seen_worker_events = std::collections::HashMap::new();
        last_seen_worker_events.insert(chat_id.clone(), (running_job.sequence, first_offset));
        let snapshot = build_worker_poll_snapshot(
            ModelProvider::Claude,
            last_seen_worker_events,
            std::collections::HashSet::new(),
        )
        .expect("snapshot");

        let running = snapshot
            .jobs
            .into_iter()
            .find(|item| item.job.chat_id == chat_id)
            .expect("running job present");
        assert_eq!(running.response, None);
        assert_eq!(running.worker_events.len(), 1);
        assert!(matches!(
            &running.worker_events[0],
            crate::stream::StreamEvent::TextDelta { content, .. } if content == "beta"
        ));
    });
}
