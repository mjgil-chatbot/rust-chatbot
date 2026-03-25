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
