use super::{
    render_workspace_picker, should_show_runsc_dir_picker, workspace_picker_test_snapshot,
};
use crate::app::AppState;
use chatbot_lib::data::{ChatMessage, ChatSession};
use chatbot_lib::model::ModelProvider;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct HomeGuard {
    previous: Option<OsString>,
}

impl HomeGuard {
    fn set(path: &Path) -> Self {
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
    previous: PathBuf,
}

impl CwdGuard {
    fn set(path: &Path) -> Self {
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

fn test_raw_input() -> egui::RawInput {
    let mut raw_input = egui::RawInput::default();
    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(1280.0, 900.0),
    ));
    raw_input
}

fn user_message_session() -> ChatSession {
    ChatSession {
        id: "session-1".to_string(),
        name: "session-1".to_string(),
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
        messages: vec![ChatMessage {
            content: "hello".to_string(),
            is_user: true,
            timestamp: chrono::Utc::now(),
            command: None,
            retrieved_memory: None,
        }],
    }
}

#[test]
fn hides_runsc_picker_after_first_message_without_runsc_dirs() {
    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.set_current_session(user_message_session());

    assert!(!should_show_runsc_dir_picker(&state));
}

#[test]
fn keeps_runsc_picker_visible_after_first_message_when_runsc_dirs_exist() {
    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    let mut session = user_message_session();
    session.runsc_dirs.push("/tmp/runsc".to_string());
    state.set_current_session(session);

    assert!(should_show_runsc_dir_picker(&state));
}

#[test]
fn keeps_runsc_picker_visible_after_first_message_when_runsc_image_exists() {
    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    let mut session = user_message_session();
    session.runsc_image = Some("custom/runsc:latest".to_string());
    state.set_current_session(session);

    assert!(should_show_runsc_dir_picker(&state));
}

#[test]
fn keeps_runsc_picker_visible_before_first_message() {
    let state = AppState::new_for_tests(ModelProvider::Claude);

    assert!(should_show_runsc_dir_picker(&state));
}

#[test]
fn selecting_workspace_completion_updates_recent_dropdown() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _home_guard = HomeGuard::set(temp_home.path());

    let workspace_root = TempDir::new().expect("workspace root");
    let matching_dir = workspace_root.path().join("alpha");
    std::fs::create_dir_all(&matching_dir).expect("create matching dir");
    let _cwd_guard = CwdGuard::set(workspace_root.path());
    let normalized = matching_dir
        .canonicalize()
        .expect("canonical path")
        .to_string_lossy()
        .into_owned();

    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.session.workspace_input_text = "alp".to_string();

    let ctx = egui::Context::default();
    let _ = ctx.run(test_raw_input(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_workspace_picker(ui, &mut state);
        });
    });

    let completion_snapshot = workspace_picker_test_snapshot().expect("completion snapshot");
    let completion_rect = completion_snapshot
        .completion_rects
        .iter()
        .find(|(path, _)| path == "alpha/")
        .map(|(_, rect)| *rect)
        .expect("alpha completion");
    let completion_center = completion_rect.center();

    let mut press_completion = test_raw_input();
    press_completion
        .events
        .push(egui::Event::PointerMoved(completion_center));
    press_completion.events.push(egui::Event::PointerButton {
        pos: completion_center,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    let _ = ctx.run(press_completion, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_workspace_picker(ui, &mut state);
        });
    });

    let mut release_completion = test_raw_input();
    release_completion
        .events
        .push(egui::Event::PointerMoved(completion_center));
    release_completion.events.push(egui::Event::PointerButton {
        pos: completion_center,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    let _ = ctx.run(release_completion, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_workspace_picker(ui, &mut state);
        });
    });

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
}

#[test]
fn pressing_enter_in_workspace_input_applies_unique_completion_match() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _home_guard = HomeGuard::set(temp_home.path());

    let workspace_root = TempDir::new().expect("workspace root");
    let matching_dir = workspace_root.path().join("alpha");
    std::fs::create_dir_all(&matching_dir).expect("create matching dir");
    let _cwd_guard = CwdGuard::set(workspace_root.path());
    let normalized = matching_dir
        .canonicalize()
        .expect("canonical path")
        .to_string_lossy()
        .into_owned();

    let mut state = AppState::new_for_tests(ModelProvider::Claude);
    state.session.workspace_input_text = "alp".to_string();

    let ctx = egui::Context::default();
    ctx.memory_mut(|mem| mem.request_focus(super::workspace_input_id()));
    let _ = ctx.run(test_raw_input(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_workspace_picker(ui, &mut state);
        });
    });

    let mut raw_input = test_raw_input();
    raw_input.events.push(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let _ = ctx.run(raw_input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_workspace_picker(ui, &mut state);
        });
    });

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
}
