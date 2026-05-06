use super::*;

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
fn system_prompt_dirty_tracks_active_session() {
    let mut state = test_app_state(ModelProvider::Codex);
    state.set_current_session(make_session("session-1"));
    state.session.system_prompt_dirty = true;
    state.session.system_prompt_dirty_session_id = Some("session-1".to_string());

    state.set_current_session(make_session("session-2"));
    assert!(!state.session.system_prompt_dirty);

    state.set_current_session(make_session("session-1"));
    assert!(state.session.system_prompt_dirty);
    assert_eq!(
        state.session.system_prompt_dirty_session_id.as_deref(),
        Some("session-1")
    );
}

#[test]
fn update_session_in_memory_refreshes_current_layout_view_without_auto_save() {
    let mut state = test_app_state(ModelProvider::Claude);
    state.set_current_session(make_session("session-1"));

    let session_to_save = state
        .update_session_in_memory("session-1", |session| {
            session.system_prompt = Some("updated prompt".to_string());
            session.json_summary = Some(JsonSummary {
                response_count: 2,
                ..Default::default()
            });
        })
        .expect("session should be returned for explicit save");

    assert_eq!(session_to_save.id, "session-1");
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
