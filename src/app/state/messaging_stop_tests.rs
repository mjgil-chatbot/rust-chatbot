use super::*;

use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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

#[test]
fn stop_current_process_uses_pending_worker_pid_before_spool_state_exists() {
    let mut state = test_app_state(ModelProvider::Codex);
    state.session.current_session = Some(make_session("chat-1"));

    let mut child = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn helper worker");
    let pid = child.id();
    state
        .worker
        .pending_worker_pids
        .insert("chat-1".to_string(), pid);

    state.stop_current_process();

    let mut exited = false;
    for _ in 0..40 {
        if child.try_wait().expect("poll helper worker").is_some() {
            exited = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    if !exited {
        let _ = child.kill();
    }

    assert!(
        exited,
        "stop_current_process should terminate the pending worker"
    );
    assert!(state.layout.should_focus_input);
}
