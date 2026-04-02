//! Dialog windows for the application.

use eframe::egui;
use rust_debug::tracing::debug;

use crate::app::state::{AppEvent, AppState, AppUiAction};

pub fn render_new_session_dialog(ctx: &egui::Context, state: &mut AppState) {
    if !state.session.show_new_session_dialog {
        return;
    }

    egui::Window::new("New Chat Session")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Session Name:");
            let name_response = ui.text_edit_singleline(&mut state.session.new_session_name);
            // Keep dialog Enter handling field-local. Unlike the composer, this
            // single-line edit uses egui's default Enter-to-surrender-focus path
            // and does not preserve focus across frames.
            let enter_pressed =
                name_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            ui.horizontal(|ui| {
                if ui.button("Create").clicked() || enter_pressed {
                    let system_prompt = if state.session.system_prompt_text.trim().is_empty() {
                        None
                    } else {
                        Some(state.session.system_prompt_text.clone())
                    };

                    state.enqueue_event(AppEvent::UiAction(AppUiAction::CreateSession {
                        name: state.session.new_session_name.clone(),
                        system_prompt,
                    }));
                    state.session.show_new_session_dialog = false;
                    state.layout.should_focus_input = true;
                    debug!("new session dialog closed");
                }

                if ui.button("Cancel").clicked() {
                    state.session.show_new_session_dialog = false;
                    debug!("new session dialog canceled");
                }
            });
        });
}

pub fn render_rename_session_dialog(ctx: &egui::Context, state: &mut AppState) {
    if !state.session.show_rename_session_dialog {
        return;
    }

    egui::Window::new("Rename Chat Session")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Session Name:");
            let name_response = ui.text_edit_singleline(&mut state.session.rename_session_name);
            // Keep dialog Enter handling field-local. Unlike the composer, this
            // single-line edit uses egui's default Enter-to-surrender-focus path
            // and does not preserve focus across frames.
            let enter_pressed =
                name_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            ui.horizontal(|ui| {
                if ui.button("Rename").clicked() || enter_pressed {
                    state.enqueue_event(AppEvent::UiAction(AppUiAction::RenameSession {
                        session_id: state.session.rename_session_id.clone(),
                        new_name: state.session.rename_session_name.clone(),
                    }));
                    state.session.show_rename_session_dialog = false;
                    state.layout.should_focus_input = true;
                    debug!("rename session dialog closed");
                }

                if ui.button("Cancel").clicked() {
                    state.session.show_rename_session_dialog = false;
                    debug!("rename session dialog canceled");
                }
            });
        });
}
