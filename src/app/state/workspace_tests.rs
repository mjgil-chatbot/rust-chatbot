use super::*;
use crate::data::ChatSession;
use crate::model::ModelProvider;
use std::thread;
use std::time::Duration;

fn test_session(id: &str) -> ChatSession {
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
        runsc_dirs: Vec::new(),
        runsc_image: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: Vec::new(),
    }
}

fn drain_path_resolution_until_idle(state: &mut AppState) {
    for _ in 0..40 {
        state.drain_path_resolution_results();
        if state.session.path_resolution_rx.is_none() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn first_runsc_dir_sets_workspace_and_current_session_root() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = tempfile::tempdir().unwrap();
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", home.path());

    let workspace = tempfile::tempdir().unwrap();
    let runsc_dir = workspace.path().join("sandbox");
    std::fs::create_dir_all(&runsc_dir).unwrap();
    let normalized = runsc_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.set_current_session(test_session("session-1"));
    state.select_runsc_dir(normalized.clone());

    assert_eq!(
        state.session.workspace_dir.as_deref(),
        Some(normalized.as_str())
    );
    assert_eq!(state.session.workspace_input_text, normalized);
    assert_eq!(
        state.session.runsc_dirs,
        vec![state.session.workspace_input_text.clone()]
    );
    assert_eq!(
        state
            .session
            .current_session
            .as_ref()
            .and_then(|s| s.launched_from.as_deref()),
        Some(state.session.workspace_input_text.as_str())
    );

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
}

#[test]
fn additional_runsc_dirs_do_not_replace_existing_workspace() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = tempfile::tempdir().unwrap();
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", home.path());

    let workspace = tempfile::tempdir().unwrap();
    let first_dir = workspace.path().join("first");
    let second_dir = workspace.path().join("second");
    std::fs::create_dir_all(&first_dir).unwrap();
    std::fs::create_dir_all(&second_dir).unwrap();
    let first = first_dir.canonicalize().unwrap().to_string_lossy().into_owned();
    let second = second_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.select_runsc_dir(first.clone());
    state.select_runsc_dir(second.clone());

    assert_eq!(state.session.workspace_dir.as_deref(), Some(first.as_str()));
    assert_eq!(state.session.runsc_dirs, vec![first, second]);

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
}

#[test]
fn preview_runsc_input_does_not_add_a_row() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let workspace = tempfile::tempdir().unwrap();
    let recent_dir = workspace.path().join("recent");
    std::fs::create_dir_all(&recent_dir).unwrap();
    let normalized = recent_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.preview_runsc_input(normalized.clone());

    assert!(state.session.runsc_dirs.is_empty());
    assert_eq!(state.session.runsc_input_text, normalized);
}

#[test]
fn apply_workspace_input_uses_unique_completion_match() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = tempfile::tempdir().unwrap();
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", home.path());

    let workspace = tempfile::tempdir().unwrap();
    let matching_dir = workspace.path().join("alpha");
    std::fs::create_dir_all(&matching_dir).unwrap();
    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(workspace.path()).unwrap();

    let normalized = matching_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.session.workspace_input_text = "alp".to_string();
    state.apply_workspace_input();

    assert_eq!(
        state.session.workspace_dir.as_deref(),
        Some(normalized.as_str())
    );
    assert_eq!(state.session.workspace_input_text, normalized);
    assert_eq!(
        state
            .session
            .workspace_recent_dirs
            .first()
            .map(String::as_str),
        Some(state.session.workspace_input_text.as_str())
    );

    std::env::set_current_dir(old_cwd).unwrap();
    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
}

#[test]
fn replacing_runsc_row_does_not_append_and_keeps_workspace_aligned() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = tempfile::tempdir().unwrap();
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", home.path());

    let workspace = tempfile::tempdir().unwrap();
    let first_dir = workspace.path().join("first");
    let replacement_dir = workspace.path().join("replacement");
    std::fs::create_dir_all(&first_dir).unwrap();
    std::fs::create_dir_all(&replacement_dir).unwrap();
    let first = first_dir.canonicalize().unwrap().to_string_lossy().into_owned();
    let replacement = replacement_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.set_current_session(test_session("session-1"));
    state.select_runsc_dir(first.clone());
    state.replace_runsc_dir(&first, replacement.clone());

    assert_eq!(state.session.runsc_dirs, vec![replacement.clone()]);
    assert_eq!(
        state.session.workspace_dir.as_deref(),
        Some(replacement.as_str())
    );
    assert_eq!(
        state.session.current_session.as_ref().unwrap().runsc_dirs,
        vec![replacement.clone()]
    );
    assert_eq!(
        state
            .session
            .current_session
            .as_ref()
            .and_then(|s| s.launched_from.as_deref()),
        Some(replacement.as_str())
    );

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
}

#[test]
fn async_apply_workspace_input_updates_state() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = tempfile::tempdir().unwrap();
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", home.path());

    let workspace = tempfile::tempdir().unwrap();
    let matching_dir = workspace.path().join("alpha");
    std::fs::create_dir_all(&matching_dir).unwrap();
    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(workspace.path()).unwrap();

    let normalized = matching_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.session.ui_io.repaint_ctx = Some(eframe::egui::Context::default());
    state.session.workspace_input_text = "alp".to_string();
    state.apply_workspace_input();
    drain_path_resolution_until_idle(&mut state);

    assert_eq!(
        state.session.workspace_dir.as_deref(),
        Some(normalized.as_str())
    );
    assert_eq!(state.session.workspace_input_text, normalized);

    std::env::set_current_dir(old_cwd).unwrap();
    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
}

#[test]
fn async_preview_runsc_input_updates_field() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let workspace = tempfile::tempdir().unwrap();
    let recent_dir = workspace.path().join("recent");
    std::fs::create_dir_all(&recent_dir).unwrap();
    let normalized = recent_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.session.ui_io.repaint_ctx = Some(eframe::egui::Context::default());
    state.preview_runsc_input(normalized.clone());
    drain_path_resolution_until_idle(&mut state);

    assert_eq!(state.session.runsc_input_text, normalized);
    assert!(state.session.runsc_dirs.is_empty());
}
