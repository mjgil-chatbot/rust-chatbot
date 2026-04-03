use eframe::egui;
use rust_debug::tracing::debug;

use super::*;

const WORKSPACE_INPUT_ID: &str = "workspace_input";

#[cfg(test)]
#[derive(Clone)]
struct WorkspacePickerTestSnapshot {
    completion_rects: Vec<(String, egui::Rect)>,
}

#[cfg(test)]
thread_local! {
    static WORKSPACE_PICKER_TEST_SNAPSHOT: std::cell::RefCell<Option<WorkspacePickerTestSnapshot>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn render_header(
    ui: &mut egui::Ui,
    state: &mut AppState,
    cache_info: HeaderHeightCacheInfo,
    show_debug_controls: bool,
    optimistic_create_session_name: Option<&str>,
) -> egui::Rect {
    let header_width = ui.available_width();
    let header_details = optimistic_create_session_name
        .map(|name| (name.to_string(), None))
        .or_else(|| {
            state.session.current_session.as_ref().map(|session| {
                let id_hash = &session.id[..8.min(session.id.len())];
                (
                    format!("{} ({})", session.name, id_hash),
                    Some(session.id.clone()),
                )
            })
        });
    let create_session_transition = optimistic_create_session_name.is_some();

    let response = ui.allocate_ui(egui::vec2(header_width, 0.0), |ui| {
        clamp_content_width(ui, header_width);
        ui.vertical(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if !create_session_transition {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let system_prompt_text = if state.session.show_system_prompt {
                            "Hide System Prompt"
                        } else {
                            "Show System Prompt"
                        };
                        if ui.button(system_prompt_text).clicked() {
                            state.session.show_system_prompt = !state.session.show_system_prompt;
                            debug!(
                                enabled = state.session.show_system_prompt,
                                "toggled system prompt"
                            );
                        }

                        let json_summary_text = if state.layout.show_json_summary {
                            "Hide JSON Summary"
                        } else {
                            "Show JSON Summary"
                        };
                        if ui.button(json_summary_text).clicked() {
                            state.layout.show_json_summary = !state.layout.show_json_summary;
                            debug!(
                                enabled = state.layout.show_json_summary,
                                "toggled json summary"
                            );
                        }

                        if show_debug_controls {
                            if cache_info.scan_active {
                                ui.add_enabled(false, egui::Button::new("Computing Heights..."));
                            } else {
                                let mut compute_response = ui.add_enabled(
                                    cache_info.can_compute,
                                    egui::Button::new("Recompute Heights"),
                                );
                                if !cache_info.can_compute {
                                    compute_response =
                                        compute_response.on_hover_text("No messages to measure");
                                }
                                if compute_response.clicked() {
                                    if let Some(ref session) = state.session.current_session {
                                        state.layout.height_cache_scan = Some(HeightCacheScan {
                                            session_id: session.id.clone(),
                                            width_key: cache_info.width_key,
                                            start_offset: None,
                                            seeking_top: true,
                                            at_end: false,
                                        });
                                        state.layout.message_scroll_offset_override = None;
                                        state.layout.message_scroll_offset_session_id =
                                            Some(session.id.clone());
                                        state.layout.layout_log_feedback = None;
                                        debug!(
                                            session_id = %session.id,
                                            width_key = cache_info.width_key,
                                            "height cache scan requested"
                                        );
                                    }
                                }
                            }

                            let log_feedback = state
                                .layout
                                .layout_log_feedback
                                .as_ref()
                                .filter(|feedback| feedback.at.elapsed().as_secs_f32() < 2.0);
                            let log_button_text = if state.layout.layout_log_in_flight() {
                                "Logging Layout..."
                            } else {
                                match log_feedback {
                                    Some(feedback) if feedback.success => "✓ Layout Logged",
                                    Some(_) => "✗ Log Failed",
                                    None => "Log Layout",
                                }
                            };
                            let mut log_response = ui.add_enabled(
                                !state.layout.layout_log_in_flight(),
                                egui::Button::new(log_button_text),
                            );
                            if state.layout.layout_log_in_flight() {
                                log_response =
                                    log_response.on_hover_text("Background layout log pending");
                            } else if let Some(feedback) = log_feedback {
                                if feedback.success {
                                    if let Some(path) = feedback.path.as_ref() {
                                        log_response =
                                            log_response.on_hover_text(path.display().to_string());
                                    }
                                } else {
                                    log_response = log_response.on_hover_text("Layout log failed");
                                }
                            }
                            if log_response.clicked() {
                                state.layout.pending_layout_log = true;
                                state.layout.layout_log_feedback = None;
                                debug!("layout log requested");
                            }
                        }
                    });
                }

                let title_width = ui.available_width().max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(title_width, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.set_width(title_width);
                        ui.spacing_mut().item_spacing.y = 2.0;

                        show_header_row(ui, |ui| {
                            if let Some((session_title, session_id_hover)) = header_details.as_ref()
                            {
                                let title_response = ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(session_title).heading(),
                                            )
                                            .wrap_mode(egui::TextWrapMode::Truncate),
                                        )
                                    },
                                );
                                if let Some(session_id_hover) = session_id_hover.as_ref() {
                                    let _ = title_response
                                        .inner
                                        .on_hover_text(format!("Chat ID: {}", session_id_hover));
                                }
                            } else {
                                ui.add_sized(
                                    [title_width, 0.0],
                                    egui::Label::new(egui::RichText::new("No Session").heading())
                                        .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                            }
                        });
                    },
                );
            });

            ui.add_space(6.0);
            render_workspace_picker(ui, state);
            if should_show_runsc_dir_picker(state) {
                ui.add_space(4.0);
                render_runsc_dir_picker(ui, state);
            }
        });
    });
    response.response.rect
}

fn should_show_runsc_dir_picker(state: &AppState) -> bool {
    !state.session.runsc_selection_locked()
        || !state.session.runsc_dirs.is_empty()
        || !state.session.runsc_image_input_text.trim().is_empty()
}

fn show_header_row<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::none()
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, add_contents)
}

fn render_workspace_picker(ui: &mut egui::Ui, state: &mut AppState) {
    let effective_dir = state.session.effective_workspace_dir();
    let workspace_locked = state.session.workspace_selection_locked();

    if workspace_locked {
        show_header_row(ui, |ui| {
            ui.label("Workspace");
            let workspace_text = effective_dir
                .as_deref()
                .map(crate::app::workspace_picker::collapse_home_prefix)
                .unwrap_or_else(|| "No workspace".to_string());
            ui.label(workspace_text).on_hover_text(
                effective_dir.unwrap_or_else(|| "Workspace unavailable".to_string()),
            );
        });
        return;
    }

    let mut selected_recent_dir = None;
    let mut open_folder_dialog = false;
    let mut selected_completion = None;
    #[cfg(test)]
    let mut completion_rects = Vec::new();

    let input_response = show_header_row(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Workspace");
            egui::ComboBox::from_id_salt("workspace_recent_dirs")
                .width(220.0)
                .selected_text(
                    state
                        .session
                        .workspace_recent_dirs
                        .first()
                        .as_ref()
                        .map(|d| crate::app::workspace_picker::collapse_home_prefix(d))
                        .unwrap_or_else(|| "Recent folders".to_string()),
                )
                .show_ui(ui, |ui| {
                    for recent_dir in state.session.workspace_recent_dirs.clone() {
                        let selected = effective_dir.as_deref() == Some(recent_dir.as_str());
                        let response = ui
                            .selectable_label(
                                selected,
                                crate::app::workspace_picker::collapse_home_prefix(&recent_dir),
                            )
                            .on_hover_text(&recent_dir);
                        if response.clicked() {
                            selected_recent_dir = Some(recent_dir);
                        }
                    }
                    if !state.session.workspace_recent_dirs.is_empty() {
                        ui.separator();
                    }
                    if ui.button("Open Folder...").clicked() {
                        open_folder_dialog = true;
                    }
                });

            let response = ui.add_sized(
                [ui.available_width(), 25.0],
                egui::TextEdit::singleline(&mut state.session.workspace_input_text)
                    .id(workspace_input_id())
                    .hint_text("Directory path"),
            );
            if response.changed() {
                state.request_workspace_completion_refresh();
            }

            let tab = response.has_focus()
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
            let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));

            if response.has_focus() && tab {
                state.autocomplete_workspace_input();
                set_text_edit_cursor(
                    ui.ctx(),
                    response.id,
                    state.session.workspace_input_text.len(),
                );
            }
            if (response.has_focus() || response.lost_focus()) && enter {
                state.apply_workspace_input();
                set_text_edit_cursor(
                    ui.ctx(),
                    response.id,
                    state.session.workspace_input_text.len(),
                );
                ui.ctx().request_repaint();
            }

            response
        })
    })
    .inner
    .inner;

    let input_text = state.session.workspace_input_text.trim();
    let completion_interaction_active = effective_dir.as_deref() != Some(input_text);
    let should_show_completions =
        !input_text.is_empty() && (input_response.has_focus() || completion_interaction_active);
    if should_show_completions
        && state.session.workspace_completion_query.as_deref() != Some(input_text)
    {
        state.request_workspace_completion_refresh();
    }
    let completions = if should_show_completions {
        state.workspace_completion_matches()
    } else {
        Vec::new()
    };

    if !completions.is_empty() {
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            for completion in completions {
                let response = ui
                    .selectable_label(
                        false,
                        crate::app::workspace_picker::collapse_home_prefix(&completion),
                    )
                    .on_hover_text(&completion);
                #[cfg(test)]
                completion_rects.push((completion.clone(), response.rect));
                if response.clicked() {
                    selected_completion = Some(completion);
                }
            }
        });
    }

    #[cfg(test)]
    WORKSPACE_PICKER_TEST_SNAPSHOT.with(|snapshot| {
        *snapshot.borrow_mut() = Some(WorkspacePickerTestSnapshot { completion_rects });
    });

    if let Some(path) = selected_recent_dir {
        state.select_workspace_dir(path);
    }

    if open_folder_dialog {
        let mut dialog = if let Some(dir) = effective_dir.as_deref() {
            rfd::FileDialog::new()
                .set_title("Select workspace folder")
                .set_directory(dir)
        } else {
            rfd::FileDialog::new().set_title("Select workspace folder")
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(parent) = state.dialog_parent.as_ref() {
            dialog = dialog.set_parent(parent);
        }
        if let Some(path) = dialog.pick_folder() {
            state.select_workspace_dir(path.to_string_lossy().into_owned());
        }
    }

    if let Some(path) = selected_completion {
        apply_selected_workspace_completion(ui.ctx(), state, path);
    }
}

fn set_text_edit_cursor(ctx: &egui::Context, id: egui::Id, pos: usize) {
    if let Some(mut text_edit_state) = egui::TextEdit::load_state(ctx, id) {
        let cursor = egui::text::CCursor::new(pos);
        text_edit_state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
        text_edit_state.store(ctx, id);
    }
}

fn workspace_input_id() -> egui::Id {
    egui::Id::new(WORKSPACE_INPUT_ID)
}

fn apply_selected_workspace_completion(ctx: &egui::Context, state: &mut AppState, path: String) {
    state.select_workspace_dir(path);
    ctx.request_repaint();
}

#[cfg(test)]
fn workspace_picker_test_snapshot() -> Option<WorkspacePickerTestSnapshot> {
    WORKSPACE_PICKER_TEST_SNAPSHOT.with(|snapshot| snapshot.borrow().clone())
}

fn render_runsc_dir_picker(ui: &mut egui::Ui, state: &mut AppState) {
    let runsc_locked = state.session.runsc_selection_locked();

    show_header_row(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Runsc Dirs");
            if runsc_locked {
                // Display only: comma-separated dirs
                let dir_text = if state.session.runsc_dirs.is_empty() {
                    "No folders".to_string()
                } else {
                    state
                        .session
                        .runsc_dirs
                        .iter()
                        .map(|d| crate::app::workspace_picker::collapse_home_prefix(d))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                ui.label(dir_text)
                    .on_hover_text(state.session.runsc_dirs.join("\n"));
                let runsc_image = state.session.runsc_image_input_text.trim();
                if !runsc_image.is_empty() {
                    ui.add_space(12.0);
                    ui.label(format!("Image: {}", runsc_image))
                        .on_hover_text(runsc_image);
                }
            } else {
                let response = ui.add_sized(
                    [220.0_f32.min(ui.available_width().max(120.0)), 25.0],
                    egui::TextEdit::singleline(&mut state.session.runsc_image_input_text)
                        .hint_text("runsc image"),
                );
                let should_apply = response.lost_focus()
                    || (response.has_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                if should_apply {
                    state.apply_runsc_image_input();
                }
                // Add Folder button pinned to far right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized([80.0, 25.0], egui::Button::new("Add Folder"))
                        .clicked()
                    {
                        let mut dialog = rfd::FileDialog::new().set_title("Select runsc directory");
                        #[cfg(not(target_arch = "wasm32"))]
                        if let Some(parent) = state.dialog_parent.as_ref() {
                            dialog = dialog.set_parent(parent);
                        }
                        if let Some(path) = dialog.pick_folder() {
                            state.select_runsc_dir(path.to_string_lossy().into_owned());
                        }
                    }
                });
            }
        });

        if !runsc_locked && state.session.runsc_dirs.is_empty() {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let input_width = ui.available_width() - 160.0;
                let response = ui.add_sized(
                    [input_width.max(100.0), 25.0],
                    egui::TextEdit::singleline(&mut state.session.runsc_input_text)
                        .hint_text("Directory path"),
                );
                if response.lost_focus() && !state.session.runsc_input_text.is_empty() {
                    state.apply_runsc_input();
                }

                egui::ComboBox::from_id_salt("runsc_recent_empty")
                    .width(100.0)
                    .selected_text("Recent")
                    .show_ui(ui, |ui| {
                        for recent_dir in state.session.runsc_recent_dirs.clone() {
                            let response = ui
                                .selectable_label(
                                    false,
                                    crate::app::workspace_picker::collapse_home_prefix(&recent_dir),
                                )
                                .on_hover_text(&recent_dir);
                            if response.clicked() {
                                state.preview_runsc_input(recent_dir);
                            }
                        }
                    });
            });
        }

        if !runsc_locked {
            for dir in state.session.runsc_dirs.clone() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let input_width = ui.available_width() - 160.0;
                    let response = ui.add_sized(
                        [input_width.max(100.0), 25.0],
                        egui::TextEdit::singleline(&mut state.session.runsc_input_text.clone())
                            .hint_text("Directory path"),
                    );
                    if response.lost_focus() && !state.session.runsc_input_text.is_empty() {
                        state.apply_runsc_input();
                    }

                    egui::ComboBox::from_id_salt(format!("runsc_recent_{}", dir))
                        .width(100.0)
                        .selected_text(
                            state
                                .session
                                .runsc_recent_dirs
                                .first()
                                .map(|d| crate::app::workspace_picker::collapse_home_prefix(d))
                                .unwrap_or_else(|| "Recent".to_string()),
                        )
                        .show_ui(ui, |ui| {
                            for recent_dir in state.session.runsc_recent_dirs.clone() {
                                let selected = recent_dir == dir;
                                let response = ui
                                    .selectable_label(
                                        selected,
                                        crate::app::workspace_picker::collapse_home_prefix(
                                            &recent_dir,
                                        ),
                                    )
                                    .on_hover_text(&recent_dir);
                                if response.clicked() {
                                    state.replace_runsc_dir(&dir, recent_dir);
                                }
                            }
                            if !state.session.runsc_recent_dirs.is_empty() {
                                ui.separator();
                            }
                            if ui.button("Open Folder...").clicked() {
                                let mut dialog =
                                    rfd::FileDialog::new().set_title("Select runsc directory");
                                #[cfg(not(target_arch = "wasm32"))]
                                if let Some(parent) = state.dialog_parent.as_ref() {
                                    dialog = dialog.set_parent(parent);
                                }
                                if let Some(path) = dialog.pick_folder() {
                                    state.replace_runsc_dir(
                                        &dir,
                                        path.to_string_lossy().into_owned(),
                                    );
                                }
                            }
                        });

                    if ui.add_sized([30.0, 25.0], egui::Button::new("🗑")).clicked() {
                        state.remove_runsc_dir(&dir);
                    }
                });
            }
        }
    });
}

#[cfg(test)]
#[path = "header_tests.rs"]
mod tests;
