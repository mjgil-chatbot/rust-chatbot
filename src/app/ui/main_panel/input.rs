use eframe::egui;
use eframe::egui::TextBuffer as _;
use rust_debug::tracing::{debug, error};

use super::*;
use crate::app::state::{AppEvent, AppUiAction};

const MAIN_CHAT_INPUT_ID: &str = "main_chat_input";

pub(super) fn render_input_area(
    ui: &mut egui::Ui,
    state: &mut AppState,
    submit_enabled: bool,
) -> InputAreaSnapshot {
    let available_width = ui.available_width();
    let available_rect_before_wrap = ui.available_rect_before_wrap();
    let min_rect = ui.min_rect();
    let max_rect = ui.max_rect();
    let row_width = available_width;
    let mut spacing = 0.0;
    let mut button_width = 0.0;
    let mut text_width = 0.0;
    let mut text_rect = egui::Rect::NOTHING;
    let mut button_rect = egui::Rect::NOTHING;
    let mut input_focus_active = false;

    let chat_busy = state.is_processing();
    let input_disabled = chat_busy;
    let can_stop = state.is_local_processing();
    let inline_button_width = if can_stop { 60.0 } else { 70.0 };
    let min_inline_text_width = 160.0;
    let inline_button_gap = ui.spacing().item_spacing.x * 0.5;
    let stacked_button_gap = ui.spacing().item_spacing.y * 0.5;

    let response = ui.allocate_ui_with_layout(
        egui::vec2(row_width, 0.0),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            ui.group(|ui| {
                spacing = inline_button_gap;
                let can_render_inline =
                    row_width >= inline_button_width + inline_button_gap + min_inline_text_width;

                let hint_text = if chat_busy {
                    "This chat is processing...".to_string()
                } else {
                    format!(
                        "Type your message to {}...",
                        state.session.provider.display_name()
                    )
                };

                if can_render_inline {
                    button_width = inline_button_width;
                    text_width = (row_width - button_width - inline_button_gap).max(0.0);
                    ui.horizontal(|ui| {
                        let text_edit_response =
                            render_input_text_edit(ui, state, text_width, &hint_text);
                        text_rect = text_edit_response.response.rect;
                        input_focus_active = text_edit_response.input_focus_active;
                        button_rect = render_input_button(
                            ui,
                            state,
                            can_stop,
                            input_disabled,
                            submit_enabled,
                            button_width,
                        );
                    });
                } else {
                    text_width = row_width.max(0.0);
                    let text_edit_response =
                        render_input_text_edit(ui, state, text_width, &hint_text);
                    text_rect = text_edit_response.response.rect;
                    input_focus_active = text_edit_response.input_focus_active;

                    ui.add_space(stacked_button_gap);
                    button_width = inline_button_width.min(row_width.max(0.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        button_rect = render_input_button(
                            ui,
                            state,
                            can_stop,
                            input_disabled,
                            submit_enabled,
                            button_width,
                        );
                    });
                }
            });
        },
    );

    InputAreaSnapshot {
        rect: response.response.rect,
        available_width,
        available_rect_before_wrap,
        min_rect,
        max_rect,
        row_width,
        spacing,
        button_width,
        text_width,
        text_rect,
        button_rect,
        input_focus_active,
    }
}

pub(super) fn handle_global_input(
    ui: &egui::Ui,
    state: &mut AppState,
    submit_enabled: bool,
    input_focus_active: bool,
) {
    let input_id = main_chat_input_id();
    if !(input_focus_active || ui.ctx().memory(|mem| mem.has_focus(input_id))) {
        return;
    }

    if let Some(selected_text) = selected_input_text(ui.ctx(), &state.layout.input_text) {
        let copy_selection =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::C));
        if copy_selection {
            ui.ctx().copy_text(selected_text);
        }
    }

    let submit_via_enter = !state.is_processing()
        && submit_enabled
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));

    if submit_via_enter {
        if let Err(e) = submit_message(ui, state) {
            error!(error=%e, "error handling global input submit");
        }
    }
}

fn render_input_text_edit(
    ui: &mut egui::Ui,
    state: &mut AppState,
    width: f32,
    hint_text: &str,
) -> InputTextEditState {
    let tab = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    let up = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
    let down = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));

    let text_edit_response = ui.add_sized(
        [width, 25.0],
        egui::TextEdit::singleline(&mut state.layout.input_text)
            .id(main_chat_input_id())
            .hint_text(hint_text)
            .lock_focus(true)
            .return_key(None),
    );

    let mut requested_focus = false;
    if state.layout.should_focus_input
        && !state.session.show_new_session_dialog
        && !state.session.show_rename_session_dialog
    {
        text_edit_response.request_focus();
        requested_focus = true;
        state.layout.should_focus_input = false;
    }

    if text_edit_response.has_focus() {
        let in_slash_mode =
            state.layout.input_text.starts_with('/') || state.session.slash_command_index.is_some();
        let mut cursor_pos: Option<usize> = None;

        if tab && in_slash_mode {
            let prefix = &state.layout.input_text;
            let matches: Vec<&str> = state
                .session
                .slash_commands
                .iter()
                .filter(|cmd| cmd.starts_with(prefix.as_str()))
                .map(|s| s.as_str())
                .collect();

            if matches.len() == 1 {
                state.layout.input_text = format!("{} ", matches[0]);
                cursor_pos = Some(state.layout.input_text.len());
                state.session.slash_command_index = None;
            } else if matches.len() > 1 {
                let mut common = matches[0].to_string();
                for m in &matches[1..] {
                    while !m.starts_with(common.as_str()) {
                        common.pop();
                    }
                }

                if common.len() <= prefix.len() || common == *prefix {
                    state.layout.input_text = format!("{} ", matches[0]);
                    cursor_pos = Some(state.layout.input_text.len());
                    state.session.slash_command_index = None;
                } else {
                    state.layout.input_text = common;
                    cursor_pos = Some(state.layout.input_text.len());
                }
            }
        }

        if in_slash_mode && (up || down) {
            let prefix = if state.session.slash_command_index.is_some() {
                &state.session.input_history_draft
            } else {
                &state.layout.input_text
            };
            let matches: Vec<&str> = state
                .session
                .slash_commands
                .iter()
                .filter(|cmd| cmd.starts_with(prefix.as_str()))
                .map(|s| s.as_str())
                .collect();

            if !matches.is_empty() {
                if state.session.slash_command_index.is_none() {
                    state.session.input_history_draft = state.layout.input_text.clone();
                }
                let idx = match state.session.slash_command_index {
                    Some(idx) => {
                        if up {
                            if idx == 0 {
                                matches.len() - 1
                            } else {
                                idx - 1
                            }
                        } else if idx + 1 >= matches.len() {
                            0
                        } else {
                            idx + 1
                        }
                    }
                    None => {
                        if up {
                            matches.len() - 1
                        } else {
                            0
                        }
                    }
                };
                state.session.slash_command_index = Some(idx);
                state.layout.input_text = matches[idx].to_string();
                cursor_pos = Some(state.layout.input_text.len());
            }
        } else if !in_slash_mode && (up || down) {
            let history: Vec<&str> = state
                .session
                .current_session
                .as_ref()
                .map(|s| {
                    s.messages
                        .iter()
                        .filter(|m| m.is_user)
                        .map(|m| m.content.as_str())
                        .collect()
                })
                .unwrap_or_default();

            if !history.is_empty() {
                if up {
                    match state.session.input_history_index {
                        None => {
                            state.session.input_history_draft = state.layout.input_text.clone();
                            let idx = history.len() - 1;
                            state.session.input_history_index = Some(idx);
                            state.layout.input_text = history[idx].to_string();
                        }
                        Some(idx) if idx > 0 => {
                            let idx = idx - 1;
                            state.session.input_history_index = Some(idx);
                            state.layout.input_text = history[idx].to_string();
                        }
                        _ => {}
                    }
                    cursor_pos = Some(0);
                } else if down {
                    if let Some(idx) = state.session.input_history_index {
                        if idx + 1 < history.len() {
                            let idx = idx + 1;
                            state.session.input_history_index = Some(idx);
                            state.layout.input_text = history[idx].to_string();
                            cursor_pos = Some(0);
                        } else {
                            state.session.input_history_index = None;
                            state.layout.input_text = state.session.input_history_draft.clone();
                            cursor_pos = Some(0);
                        }
                    }
                }
            }
        }

        if let Some(pos) = cursor_pos {
            if let Some(mut te_state) = egui::TextEdit::load_state(ui.ctx(), text_edit_response.id)
            {
                let ccursor = egui::text::CCursor::new(pos);
                te_state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                te_state.store(ui.ctx(), text_edit_response.id);
            }
        }

        if !state.layout.input_text.starts_with('/') {
            state.session.slash_command_index = None;
        }
    }

    InputTextEditState {
        input_focus_active: requested_focus || text_edit_response.has_focus(),
        response: text_edit_response,
    }
}

struct InputTextEditState {
    response: egui::Response,
    input_focus_active: bool,
}

fn render_input_button(
    ui: &mut egui::Ui,
    state: &mut AppState,
    can_stop: bool,
    input_disabled: bool,
    submit_enabled: bool,
    width: f32,
) -> egui::Rect {
    ui.add_space(3.0);
    if can_stop {
        let stop_button = ui.add_sized([width, 25.0], egui::Button::new("Stop"));
        if stop_button.clicked() {
            debug!("stop button clicked");
            state.enqueue_event(AppEvent::UiAction(AppUiAction::StopCurrentProcess));
        }
        stop_button.rect
    } else {
        ui.add_enabled_ui(!input_disabled && submit_enabled, |ui| {
            let send_button = ui.add_sized([width, 25.0], egui::Button::new("Send"));
            if send_button.clicked() {
                debug!("send button clicked");
                if let Err(e) = submit_message(ui, state) {
                    error!(error=%e, "error sending message");
                }
            }
            send_button.rect
        })
        .inner
    }
}

fn submit_message(ui: &egui::Ui, state: &mut AppState) -> anyhow::Result<()> {
    state.enqueue_event(AppEvent::UiAction(AppUiAction::SubmitMessage));
    // The composer runs after the message list in the same frame, so a submit
    // needs one explicit repaint to show the optimistic user row immediately.
    ui.ctx().request_repaint();
    Ok(())
}

fn main_chat_input_id() -> egui::Id {
    egui::Id::new(MAIN_CHAT_INPUT_ID)
}

fn selected_input_text(ctx: &egui::Context, input_text: &String) -> Option<String> {
    let text_state = egui::TextEdit::load_state(ctx, main_chat_input_id())?;
    let [start, end] = text_state.cursor.char_range()?.sorted();
    if start.index == end.index {
        return None;
    }

    Some(input_text.char_range(start.index..end.index).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{ChatMessage, ChatSession};
    use crate::model::ModelProvider;
    use egui::text::{CCursor, CCursorRange};

    fn empty_session(id: &str) -> ChatSession {
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

    fn session_with_user_message(id: &str) -> ChatSession {
        let mut session = empty_session(id);
        session.messages.push(ChatMessage {
            content: "earlier".to_string(),
            is_user: true,
            timestamp: chrono::Utc::now(),
            command: None,
            retrieved_memory: None,
        });
        session
    }

    fn test_raw_input() -> egui::RawInput {
        let mut raw_input = egui::RawInput::default();
        raw_input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(640.0, 480.0),
        ));
        raw_input
    }

    #[test]
    fn enter_submits_first_message_while_input_stays_focused() {
        let mut state = AppState::new_for_tests(ModelProvider::Codex);
        state.set_current_session(empty_session("session-1"));
        state.layout.input_text = "hello".to_string();
        state.layout.should_focus_input = true;

        let ctx = egui::Context::default();
        let _ = ctx.run(test_raw_input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_input_area(ui, &mut state, true);
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
                render_input_area(ui, &mut state, true);
                handle_global_input(ui, &mut state, true, true);
            });
        });

        assert!(matches!(
            state.session.event_queue.pop_front(),
            Some(AppEvent::UiAction(AppUiAction::SubmitMessage))
        ));
    }

    #[test]
    fn enter_submits_focused_input_for_existing_chat() {
        let mut state = AppState::new_for_tests(ModelProvider::Codex);
        state.set_current_session(session_with_user_message("session-1"));
        state.layout.input_text = "follow up".to_string();
        state.layout.should_focus_input = true;

        let ctx = egui::Context::default();
        let _ = ctx.run(test_raw_input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_input_area(ui, &mut state, true);
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
                render_input_area(ui, &mut state, true);
                handle_global_input(ui, &mut state, true, true);
            });
        });

        assert!(matches!(
            state.session.event_queue.pop_front(),
            Some(AppEvent::UiAction(AppUiAction::SubmitMessage))
        ));
    }

    #[test]
    fn ctrl_c_copies_selected_input_text_when_focused() {
        let mut state = AppState::new_for_tests(ModelProvider::Codex);
        state.set_current_session(session_with_user_message("session-1"));
        state.layout.input_text = "copy this".to_string();
        state.layout.should_focus_input = true;

        let ctx = egui::Context::default();
        let _ = ctx.run(test_raw_input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_input_area(ui, &mut state, true);
            });
        });

        let mut text_state =
            egui::TextEdit::load_state(&ctx, main_chat_input_id()).expect("text edit state");
        text_state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(4))));
        text_state.store(&ctx, main_chat_input_id());

        let mut raw_input = test_raw_input();
        raw_input.events.push(egui::Event::Key {
            key: egui::Key::C,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        });
        let output = ctx.run(raw_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_input_area(ui, &mut state, true);
                handle_global_input(ui, &mut state, true, true);
            });
        });
        assert_eq!(output.platform_output.copied_text, "copy");
    }

    #[test]
    fn enter_submits_first_message_on_initial_focus_frame() {
        let mut state = AppState::new_for_tests(ModelProvider::Codex);
        state.set_current_session(empty_session("session-1"));
        state.layout.input_text = "hello".to_string();
        state.layout.should_focus_input = true;

        let ctx = egui::Context::default();
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
                let snapshot = render_input_area(ui, &mut state, true);
                handle_global_input(ui, &mut state, true, snapshot.input_focus_active);
            });
        });

        assert!(matches!(
            state.session.event_queue.pop_front(),
            Some(AppEvent::UiAction(AppUiAction::SubmitMessage))
        ));
    }
}
