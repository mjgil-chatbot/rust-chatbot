use super::*;
use crate::model::ModelProvider;
use std::ffi::OsString;
use std::time::Duration as StdDuration;
use tempfile::TempDir;

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

struct HomeGuard {
    previous: Option<OsString>,
}

impl HomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("HOME");
        std::env::set_var("HOME", path);
        Self { previous }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match self.previous.as_ref() {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
}

struct CwdGuard {
    previous: std::path::PathBuf,
}

impl CwdGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(path).expect("set current dir");
        Self { previous }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.previous).expect("restore current dir");
    }
}

#[test]
fn check_for_response_drains_worker_workspace_and_history_background_channels_together() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _home_guard = HomeGuard::set(temp_home.path());

    let workspace_root = TempDir::new().expect("workspace root");
    let _cwd_guard = CwdGuard::set(workspace_root.path());
    let alpha_dir = workspace_root.path().join("alpha");
    std::fs::create_dir_all(&alpha_dir).expect("create alpha dir");

    let workspace_root_normalized = workspace_root
        .path()
        .canonicalize()
        .expect("canonical workspace root")
        .to_string_lossy()
        .into_owned();
    let alpha_normalized = alpha_dir
        .canonicalize()
        .expect("canonical alpha dir")
        .to_string_lossy()
        .into_owned();
    let expected_history_notice = history_missing_notice();

    let chat_id = "channel-flow-chat";
    let worker_dir = worker::worker_dir(chat_id);
    std::fs::create_dir_all(&worker_dir).expect("worker dir");
    let running_job = worker::WorkerJobSnapshot::new(
        chat_id,
        ModelProvider::Claude.as_str(),
        std::process::id(),
        std::process::id(),
    );
    worker::write_json_atomic(&worker::state_path(chat_id), &running_job)
        .expect("write running worker state");

    crate::stream::append_event(
        &worker::events_path(chat_id),
        &crate::stream::StreamEvent::TextDelta {
            content: "worker delta".to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        },
    )
    .expect("append worker delta");

    let mut state = test_app_state(ModelProvider::Claude);
    state.session.ui_io.repaint_ctx = Some(eframe::egui::Context::default());
    state.set_current_session(make_session(chat_id));
    state.session.workspace_dir = Some(workspace_root_normalized);
    state.session.workspace_input_text = "al".to_string();
    state.request_workspace_completion_refresh();
    state.preview_runsc_input("alpha".to_string());
    state.worker.last_worker_poll =
        std::time::Instant::now() - WORKER_POLL_INTERVAL - WORKER_POLL_INTERVAL;

    let mut completed = false;
    for _ in 0..80 {
        state.check_for_response();
        let completion_ready = state.session.workspace_completions == vec!["alpha/".to_string()];
        let path_resolution_ready = state.session.runsc_input_text == alpha_normalized;
        let history_ready =
            state.session.history.snapshot.notice.as_deref() == Some(expected_history_notice.as_str());
        let worker_ready = state
            .worker
            .cached_response(chat_id)
            .map(|response| response.contains("worker delta"))
            .unwrap_or(false);
        if completion_ready && path_resolution_ready && history_ready && worker_ready {
            completed = true;
            break;
        }
        std::thread::sleep(StdDuration::from_millis(10));
    }

    assert!(
        completed,
        "background channels did not converge: completions={:?} runsc_input={} history_notice={:?} worker_cached={:?}",
        state.session.workspace_completions,
        state.session.runsc_input_text,
        state.session.history.snapshot.notice,
        state.worker.cached_response(chat_id),
    );
    assert_eq!(state.session.workspace_completions, vec!["alpha/".to_string()]);
    assert_eq!(state.session.runsc_input_text, alpha_normalized);
    assert_eq!(
        state.session.history.snapshot.notice.as_deref(),
        Some(expected_history_notice.as_str())
    );
    assert!(
        state.worker
            .cached_response(chat_id)
            .expect("cached worker response")
            .contains("worker delta")
    );
}
