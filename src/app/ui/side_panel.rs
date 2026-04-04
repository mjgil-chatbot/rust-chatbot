//! Side panel UI for session management
//!
//! This module renders the session list and provides controls for
//! creating, selecting, and deleting chat sessions.

use eframe::egui;
use rust_debug::tracing::{debug, info};
use std::sync::OnceLock;

use crate::app::state::{AppEvent, AppState, AppUiAction};
use crate::config;
use crate::data::SessionInfo;

/// Resolve the single highlighted session ID from loading (in-flight) or selected (authoritative).
fn highlighted_session_id(state: &AppState) -> Option<&str> {
    state
        .session
        .loading_session_id
        .as_deref()
        .or(state.session.selected_session_id.as_deref())
}

#[derive(Clone)]
enum SidebarRow {
    PendingCreate { name: String },
    Existing(SessionInfo),
}

fn auto_ui_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("RUST_CHATBOT_AUTO_UI_DEBUG")
            .map(|value| {
                let value = value.to_ascii_lowercase();
                value == "1" || value == "true" || value == "yes"
            })
            .unwrap_or(false)
    })
}

fn pending_create_matches_search(state: &AppState, pending_name: &str) -> bool {
    let query = state.session.search_filter.trim();
    query.is_empty() || pending_name.to_lowercase().contains(&query.to_lowercase())
}

fn visible_sidebar_rows(state: &AppState, sessions: Vec<SessionInfo>) -> Vec<SidebarRow> {
    let mut rows = Vec::with_capacity(sessions.len() + 1);
    if let Some(pending_name) = state.session.ui_io.pending_create_session_name() {
        if pending_create_matches_search(state, pending_name) {
            rows.push(SidebarRow::PendingCreate {
                name: pending_name.to_string(),
            });
        }
    }
    rows.extend(
        sessions
            .into_iter()
            .filter(|session| session_matches_search(state, session))
            .map(SidebarRow::Existing),
    );
    rows
}

fn emit_sidebar_auto_ui_debug_trace(state: &mut AppState, rows: &[SidebarRow]) {
    if !auto_ui_debug_enabled() {
        return;
    }

    let pending_create_name = state
        .session
        .ui_io
        .pending_create_session_name()
        .unwrap_or("");
    let pending_create_visible = matches!(rows.first(), Some(SidebarRow::PendingCreate { .. }));
    let (sidebar_top_session_id, sidebar_top_session_name, sidebar_top_pending) = match rows.first()
    {
        Some(SidebarRow::PendingCreate { name }) => ("", name.as_str(), true),
        Some(SidebarRow::Existing(session)) => (session.id.as_str(), session.name.as_str(), false),
        None => ("", "", false),
    };
    let sidebar_selected_session_id = state.session.selected_session_id.as_deref().unwrap_or("");
    let sidebar_selected_session_name = state
        .session
        .sessions
        .iter()
        .find(|session_info| session_info.id == sidebar_selected_session_id)
        .map(|session_info| session_info.name.as_str())
        .unwrap_or("");
    let sidebar_selected_pending = pending_create_visible;
    let current_session_id = state
        .session
        .current_session
        .as_ref()
        .map(|session| session.id.as_str())
        .unwrap_or("");
    let current_session_name = state
        .session
        .current_session
        .as_ref()
        .map(|session| session.name.as_str())
        .unwrap_or("");

    let signature = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        pending_create_name,
        pending_create_visible,
        sidebar_top_session_id,
        sidebar_top_session_name,
        sidebar_top_pending,
        sidebar_selected_session_id,
        sidebar_selected_pending,
        state.session.sessions.len(),
    );
    if state.layout.last_auto_ui_sidebar_debug_signature.as_deref() == Some(signature.as_str()) {
        return;
    }
    state.layout.last_auto_ui_sidebar_debug_signature = Some(signature);

    info!(
        pending_create_name = %pending_create_name,
        pending_create_visible,
        sidebar_top_session_id = %sidebar_top_session_id,
        sidebar_top_session_name = %sidebar_top_session_name,
        sidebar_top_pending,
        sidebar_selected_session_id = %sidebar_selected_session_id,
        sidebar_selected_session_name = %sidebar_selected_session_name,
        sidebar_selected_pending,
        current_session_id = %current_session_id,
        current_session_name = %current_session_name,
        sidebar_session_count = state.session.sessions.len(),
        "ui_auto_debug_sidebar"
    );
}

/// Render the side panel with session list
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    // Force content width to match the panel's available width exactly.
    // This constrains content_ui.min_rect() so Frame.show() returns
    // a rect matching the panel width (fixes resize snap-back).
    ui.set_width(ui.available_width());
    let search_id = ui.make_persistent_id("session_search");
    let focus_search = ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F));

    ui.horizontal(|ui| {
        ui.heading("Chat Sessions");
    });
    ui.separator();

    // New session button + search bar
    ui.horizontal(|ui| {
        if ui.button("+ New Chat").clicked() {
            state.session.show_new_session_dialog = true;
            state.session.new_session_name = format!("Chat {}", state.session.sessions.len() + 1);
            debug!("new session dialog opened");
        }

        let prev_filter = state.session.search_filter.clone();
        let search = egui::TextEdit::singleline(&mut state.session.search_filter)
            .id_source(search_id)
            .hint_text("Search…")
            .desired_width(ui.available_width());
        let search_response = ui.add(search);
        if focus_search {
            search_response.request_focus();
        }

        // Refresh content matches when the filter text changes
        if state.session.search_filter != prev_filter {
            if state.session.search_filter.is_empty() {
                state.session.search_matched_sessions.clear();
                state.session.search_results_query = None;
                state.session.search_results_rx = None;
            } else {
                state.refresh_session_search_in_background();
            }
        }

        // Keep search Enter handling field-local. This input does not lock focus
        // across frames, so egui's default single-line Enter-to-surrender-focus
        // behavior remains the right trigger for applying the search selection.
        if search_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some(session_id) = first_filtered_session_id(state) {
                debug!(session_id=%session_id, "session selected from search");
                state.enqueue_event(AppEvent::UiAction(AppUiAction::LoadSession(session_id)));
            }
        }
    });

    ui.separator();

    let sidebar_rows = visible_sidebar_rows(state, state.session.sessions.clone());
    emit_sidebar_auto_ui_debug_trace(state, &sidebar_rows);

    let highlight_id = highlighted_session_id(state).map(|s| s.to_owned());
    let mut clicked_session: Option<String> = None;

    // Sessions list — ScrollArea consumes available space (required for panel resize).
    // id_salt keeps egui's scroll-offset memory stable across refreshes.
    egui::ScrollArea::vertical()
        .id_salt("sessions")
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            // Long session names should not force the panel wider than the dragged width.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);

            for (index, row) in sidebar_rows.iter().enumerate() {
                match row {
                    SidebarRow::PendingCreate { name } => {
                        ui.push_id(("pending-create", index), |ui| {
                            ui.horizontal(|ui| {
                                let delete_button_width = ui.spacing().interact_size.x;
                                let label_max_width = (ui.available_width()
                                    - delete_button_width
                                    - ui.spacing().item_spacing.x)
                                    .max(0.0);
                                let session_response = ui
                                    .scope(|ui| {
                                        ui.set_max_width(label_max_width);
                                        ui.add(egui::SelectableLabel::new(
                                            true,
                                            format!("{name} (0)"),
                                        ))
                                    })
                                    .inner;
                                session_response.on_hover_text("Creating chat session...");
                                ui.add_space(delete_button_width);
                            });
                        });
                    }
                    SidebarRow::Existing(session_info) => {
                        let is_selected = highlight_id
                            .as_deref()
                            .map_or(false, |id| id == session_info.id);

                        ui.push_id(&session_info.id, |ui| {
                            ui.horizontal(|ui| {
                                let delete_button_width = ui.spacing().interact_size.x;
                                let row_height = ui.spacing().interact_size.y;
                                let label_max_width = (ui.available_width()
                                    - delete_button_width
                                    - ui.spacing().item_spacing.x)
                                    .max(0.0);

                                let is_busy = state.is_chat_processing(&session_info.id);
                                let label_text = if is_busy {
                                    format!(
                                        "\u{25B6} {} ({})",
                                        session_info.name, session_info.message_count
                                    )
                                } else {
                                    format!("{} ({})", session_info.name, session_info.message_count)
                                };

                                let session_response = ui
                                    .scope(|ui| {
                                        ui.set_max_width(label_max_width);
                                        let rich = if is_busy {
                                            egui::RichText::new(&label_text)
                                                .color(egui::Color32::from_rgb(255, 165, 0))
                                        } else {
                                            egui::RichText::new(&label_text)
                                        };
                                        ui.add(egui::SelectableLabel::new(is_selected, rich))
                                    })
                                    .inner;

                                if session_response.double_clicked() {
                                    state.session.show_rename_session_dialog = true;
                                    state.session.rename_session_id = session_info.id.clone();
                                    state.session.rename_session_name = session_info.name.clone();
                                    debug!(session_id=%session_info.id, "rename session dialog opened");
                                } else if session_response.clicked() {
                                    clicked_session = Some(session_info.id.clone());
                                }

                                // Stable row IDs prevent a refresh from reusing pointer/focus state
                                // for the adjacent session when the list changes under the cursor.
                                if ui
                                    .add_sized([delete_button_width, row_height], egui::Button::new("🗑"))
                                    .clicked()
                                {
                                    debug!(session_id=%session_info.id, "delete session clicked");
                                    state.enqueue_event(AppEvent::UiAction(AppUiAction::DeleteSession(
                                        session_info.id.clone(),
                                    )));
                                }
                            });
                        });
                    }
                }

                ui.add_space(2.0);
            }
        });

    if let Some(session_id) = clicked_session {
        debug!(session_id=%session_id, "session selected");
        state.enqueue_event(AppEvent::UiAction(AppUiAction::LoadSession(session_id)));
    }
}

fn first_filtered_session_id(state: &AppState) -> Option<String> {
    state.session.sessions.iter().find_map(|session_info| {
        if !session_matches_search(state, session_info) {
            return None;
        }

        let is_current = state
            .session
            .current_session
            .as_ref()
            .map(|s| s.id == session_info.id)
            .unwrap_or(false);
        if is_current {
            None
        } else {
            Some(session_info.id.clone())
        }
    })
}

fn session_matches_search(state: &AppState, session_info: &SessionInfo) -> bool {
    let query = state.session.search_filter.trim();
    if query.is_empty() {
        return true;
    }

    let query = query.to_lowercase();
    let name_match = session_info.name.to_lowercase().contains(&query);
    let content_match = state
        .session
        .search_matched_sessions
        .contains(&session_info.id);

    name_match || content_match
}

/// Get the side panel configuration
pub fn panel_config() -> egui::SidePanel {
    egui::SidePanel::left("session_panel")
        .resizable(true)
        .min_width(config::SIDE_PANEL_MIN_WIDTH)
        .default_width(config::SIDE_PANEL_WIDTH)
        .max_width(config::SIDE_PANEL_WIDTH * 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ChatSession;
    use crate::model::ModelProvider;
    use chrono::Utc;

    fn make_session_info(id: &str, name: &str) -> SessionInfo {
        let now = Utc::now();
        SessionInfo {
            id: id.to_string(),
            name: name.to_string(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            hidden: false,
        }
    }

    fn state_with_selection(loading: Option<&str>, selected: Option<&str>) -> AppState {
        let mut state = AppState::new_for_tests(ModelProvider::Codex);
        state.session.loading_session_id = loading.map(str::to_owned);
        state.session.selected_session_id = selected.map(str::to_owned);
        state
    }

    fn state_with_search(query: &str, sessions: Vec<SessionInfo>) -> AppState {
        let mut state = AppState::new_for_tests(ModelProvider::Codex);
        state.session.search_filter = query.to_string();
        state.session.sessions = sessions;
        state
    }

    fn row_labels(rows: &[SidebarRow]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                SidebarRow::PendingCreate { name } => name.clone(),
                SidebarRow::Existing(session) => session.name.clone(),
            })
            .collect()
    }

    #[test]
    fn highlight_prefers_loading_over_selected() {
        let state = state_with_selection(Some("loading-id"), Some("selected-id"));
        assert_eq!(highlighted_session_id(&state), Some("loading-id"));
    }

    #[test]
    fn highlight_falls_back_to_selected() {
        let state = state_with_selection(None, Some("selected-id"));
        assert_eq!(highlighted_session_id(&state), Some("selected-id"));
    }

    #[test]
    fn highlight_none_when_both_empty() {
        let state = state_with_selection(None, None);
        assert!(highlighted_session_id(&state).is_none());
    }

    #[test]
    fn highlight_prefers_loading_even_when_selected_matches_current() {
        // Race scenario: a load is in flight for "loading-id" while the previously
        // selected chat ("selected-id") is still the active session. The sidebar
        // must show the loading row, never the stale selected row.
        let state = state_with_selection(Some("loading-id"), Some("selected-id"));
        assert_eq!(highlighted_session_id(&state), Some("loading-id"));
    }

    #[test]
    fn session_matches_search_accepts_name_match_without_content_hit() {
        let state = state_with_search("alpha", vec![make_session_info("1", "Alpha Chat")]);
        assert!(session_matches_search(&state, &state.session.sessions[0]));
    }

    #[test]
    fn session_matches_search_accepts_content_hit_without_name_match() {
        let mut state = state_with_search("needle", vec![make_session_info("1", "General Chat")]);
        state
            .session
            .search_matched_sessions
            .insert("1".to_string());
        assert!(session_matches_search(&state, &state.session.sessions[0]));
    }

    #[test]
    fn session_matches_search_is_case_insensitive_for_names() {
        let state = state_with_search("ALPHA", vec![make_session_info("1", "alpha chat")]);
        assert!(session_matches_search(&state, &state.session.sessions[0]));
    }

    #[test]
    fn first_filtered_session_id_uses_name_matches() {
        let mut state = state_with_search(
            "beta",
            vec![
                make_session_info("current", "Alpha Chat"),
                make_session_info("target", "Beta Chat"),
            ],
        );
        state.session.current_session = Some(ChatSession {
            id: "current".to_string(),
            name: "Alpha Chat".to_string(),
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            messages: Vec::new(),
            runsc_dirs: Vec::new(),
            runsc_image: None,
        });

        assert_eq!(
            first_filtered_session_id(&state),
            Some("target".to_string())
        );
    }

    #[test]
    fn visible_sidebar_rows_place_pending_create_at_top() {
        let mut state = state_with_search("", vec![make_session_info("1", "Existing Chat")]);
        state
            .session
            .ui_io
            .mark_create_session_started(7, "New Chat".to_string());

        let rows = visible_sidebar_rows(&state, state.session.sessions.clone());
        assert_eq!(
            row_labels(&rows),
            vec!["New Chat".to_string(), "Existing Chat".to_string()]
        );
    }

    #[test]
    fn visible_sidebar_rows_hide_pending_create_when_search_misses() {
        let mut state =
            state_with_search("existing", vec![make_session_info("1", "Existing Chat")]);
        state
            .session
            .ui_io
            .mark_create_session_started(7, "New Chat".to_string());

        let rows = visible_sidebar_rows(&state, state.session.sessions.clone());
        assert_eq!(row_labels(&rows), vec!["Existing Chat".to_string()]);
    }
}
