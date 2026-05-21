use super::*;
use crate::app::state::{AppEvent, AppUiAction};
use crate::app::ui::{dialogs, side_panel};
use crate::data::DataManager;
use crate::model::ModelProvider;
use crate::TEST_ENV_LOCK;
use std::ffi::OsString;
use std::time::Duration as StdDuration;
use tempfile::TempDir;

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

fn test_raw_input() -> egui::RawInput {
    let mut raw_input = egui::RawInput::default();
    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(1280.0, 900.0),
    ));
    raw_input
}

fn render_root_frame(
    ctx: &egui::Context,
    raw_input: egui::RawInput,
    state: &mut AppState,
) -> egui::FullOutput {
    ctx.run(raw_input, |ctx| {
        side_panel::panel_config().show(ctx, |ui| {
            side_panel::render(ui, state);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            super::render(ui, state);
        });

        dialogs::render_new_session_dialog(ctx, state);
        dialogs::render_rename_session_dialog(ctx, state);
    })
}

fn apply_pending_create_result(state: &mut AppState) {
    let create_result = state
        .session
        .ui_io
        .rx
        .recv_timeout(StdDuration::from_secs(1))
        .expect("create result");
    state.enqueue_event(AppEvent::UiIo(create_result));
    state.check_for_response();
}

#[test]
fn root_ui_enqueues_first_message_submit_for_newly_created_chat() {
    let _lock = TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _guard = HomeGuard::set(temp_home.path());

    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).expect("data manager");
    data_manager
        .ensure_preallocated_sessions(1)
        .expect("preallocated session pool");

    let mut state = AppState::new_for_tests(provider);
    state.session.data_manager = Some(data_manager);
    state.enqueue_event(AppEvent::UiAction(AppUiAction::CreateSession {
        name: "Created".to_string(),
        system_prompt: None,
    }));
    state.check_for_response();
    apply_pending_create_result(&mut state);

    let created_session = state
        .session
        .current_session
        .as_ref()
        .expect("created session");
    assert_eq!(created_session.name, "Created");

    let ctx = egui::Context::default();
    let _ = render_root_frame(&ctx, test_raw_input(), &mut state);
    assert!(
        ctx.memory(|mem| mem.has_focus(egui::Id::new("main_chat_input"))),
        "newly created chat should focus the composer before the first message"
    );

    state.layout.input_text = "hello".to_string();
    let mut raw_input = test_raw_input();
    raw_input.events.push(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let _ = render_root_frame(&ctx, raw_input, &mut state);

    assert!(matches!(
        state.session.event_queue.pop_front(),
        Some(AppEvent::UiAction(AppUiAction::SubmitMessage))
    ));
}

#[test]
fn root_ui_enqueues_first_message_submit_on_initial_focus_frame() {
    let _lock = TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _guard = HomeGuard::set(temp_home.path());

    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).expect("data manager");
    data_manager
        .ensure_preallocated_sessions(1)
        .expect("preallocated session pool");

    let mut state = AppState::new_for_tests(provider);
    state.session.data_manager = Some(data_manager);
    state.enqueue_event(AppEvent::UiAction(AppUiAction::CreateSession {
        name: "Created".to_string(),
        system_prompt: None,
    }));
    state.check_for_response();
    apply_pending_create_result(&mut state);
    state.layout.input_text = "hello".to_string();

    let ctx = egui::Context::default();
    let mut raw_input = test_raw_input();
    raw_input.events.push(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let _ = render_root_frame(&ctx, raw_input, &mut state);

    assert!(matches!(
        state.session.event_queue.pop_front(),
        Some(AppEvent::UiAction(AppUiAction::SubmitMessage))
    ));
}
