//! Main chat panel UI
//!
//! This module renders the main chat interface including the message list,
//! system prompt area, JSON summary, and input controls.

use eframe::egui;
use rust_debug::tracing::{error, info};
use std::sync::{mpsc::TryRecvError, OnceLock};
use std::time::Instant;
use crate::app::state::{AppEvent, AppState, AppUiAction, HeightCacheScan, LayoutLogFeedback};
use crate::config;
#[cfg(test)]
mod flow_tests;
mod header;
mod input;
mod summary;
#[derive(Clone)]
pub(crate) struct LayoutSnapshot {
    pub(crate) viewport_rect: egui::Rect,
    pub(crate) content_rect: egui::Rect,
    pub(crate) globally_used_rect: egui::Rect,
    pub(crate) pixels_per_point: f32,
    pub(crate) content_width: f32,
    pub(crate) main_available_width_start: f32,
    pub(crate) main_available_width_before_input: f32,
    pub(crate) main_available_rect_before_input: egui::Rect,
    pub(crate) header_rect: Option<egui::Rect>,
    pub(crate) system_prompt_rect: Option<egui::Rect>,
    pub(crate) json_summary_rect: Option<egui::Rect>,
    pub(crate) message_area: Option<MessageAreaSnapshot>,
    pub(crate) input_rect: Option<egui::Rect>,
    pub(crate) input_area: Option<InputAreaSnapshot>,
    pub(crate) status_rect: Option<egui::Rect>,
}
struct HeaderHeightCacheInfo {
    width_key: u32,
    scan_active: bool,
    can_compute: bool,
}
impl LayoutSnapshot {
    fn new(ctx: &egui::Context, content_width: f32) -> Self {
        Self {
            viewport_rect: ctx.screen_rect(),
            content_rect: ctx.available_rect(),
            globally_used_rect: ctx.used_rect(),
            pixels_per_point: ctx.pixels_per_point(),
            content_width,
            main_available_width_start: 0.0,
            main_available_width_before_input: 0.0,
            main_available_rect_before_input: egui::Rect::NOTHING,
            header_rect: None,
            system_prompt_rect: None,
            json_summary_rect: None,
            message_area: None,
            input_rect: None,
            input_area: None,
            status_rect: None,
        }
    }}
#[derive(Clone)]
pub(crate) struct MessageAreaSnapshot {
    pub(crate) available_width: f32,
    pub(crate) available_height: f32,
    pub(crate) scroll_inner_rect: egui::Rect,
    pub(crate) scroll_content_size: egui::Vec2,
    pub(crate) viewport_rect: egui::Rect,
    pub(crate) message_count: usize,
    pub(crate) row_heights_width: f32,
    pub(crate) row_total_height: f32,
    pub(crate) visible_range: (usize, usize),
    pub(crate) message_group_min_width: f32,
    pub(crate) claude_content_width_max: f32,
    pub(crate) message_rows: Vec<MessageRowSnapshot>,
    pub(crate) max_rendered_right: f32,
    pub(crate) max_rendered_overflow: f32,
}
#[derive(Clone)]
pub(crate) struct MessageRowSnapshot {
    pub(crate) index: usize,
    pub(crate) is_user: bool,
    pub(crate) timestamp_ms: i64,
    pub(crate) content_len: usize,
    pub(crate) rendered: bool,
    pub(crate) message_rect: Option<egui::Rect>,
    pub(crate) command_rect: Option<egui::Rect>,
    pub(crate) has_command: bool,
    pub(crate) content_preview: Option<String>,
    pub(crate) command_preview: Option<String>,
    pub(crate) raw_height: f32,
    pub(crate) height: f32,
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}
#[derive(Clone)]
pub(crate) struct InputAreaSnapshot {
    pub(crate) rect: egui::Rect,
    pub(crate) available_width: f32,
    pub(crate) available_rect_before_wrap: egui::Rect,
    pub(crate) min_rect: egui::Rect,
    pub(crate) max_rect: egui::Rect,
    pub(crate) row_width: f32,
    pub(crate) spacing: f32,
    pub(crate) button_width: f32,
    pub(crate) text_width: f32,
    pub(crate) text_rect: egui::Rect,
    pub(crate) button_rect: egui::Rect,
    pub(crate) input_focus_active: bool,
}
fn optimistic_create_session_name(state: &AppState) -> Option<&str> {
    state.session.ui_io.pending_create_session_name()
}
fn connected_mcp_servers(state: &AppState) -> Vec<String> {
    let overrides = session_effective_mcp_overrides(state);
    let mut names = crate::provider_bridge::effective_mcp_server_names(
        state.session.provider,
        overrides.as_ref(),
    );
    if !state
        .session
        .fff_mcp_server_explicitly_configured
        .unwrap_or(false)
    {
        names.retain(|name| name != "fff");
    }
    names.sort_unstable();
    names.dedup();
    names
}
fn session_effective_mcp_overrides(
    state: &AppState,
) -> Option<crate::provider_bridge::ProviderRequestOverrides> {
    let mut overrides =
        crate::provider_bridge::background_provider_overrides(state.session.provider);
    overrides.runsc_dirs = state.session.runsc_dirs.clone();
    overrides.runsc_image = state
        .session
        .current_session
        .as_ref()
        .and_then(|session| session.runsc_image.clone())
        .or_else(|| {
            let runsc_image = state.session.runsc_image_input_text.trim();
            (!runsc_image.is_empty()).then(|| runsc_image.to_string())
        })
        .or(overrides.runsc_image);

    if overrides.runsc_image.is_none() && overrides.runsc_dirs.is_empty() {
        return None;
    }

    Some(overrides)
}
fn mcp_status_text(names: &[String]) -> String {
    format!("MCP: {}", names.len())
}
fn mcp_hover_text(names: &[String]) -> String {
    if names.is_empty() {
        return "No MCP servers configured".to_string();
    }

    let mut lines = Vec::with_capacity(names.len() + 1);
    lines.push("Connected MCP servers:".to_string());
    lines.extend(names.iter().map(|name| format!("- {name}")));
    if names.iter().any(|name| name == "mcp-coordinator") {
        lines.push(String::new());
        lines.push(
            "Routed upstream MCP tools remain callable through the `mcp-coordinator` namespace."
                .to_string(),
        );
    }
    lines.join("\n")
}
fn omniforge_selected_model_label(session: Option<&crate::data::ChatSession>) -> &'static str {
    crate::omniforge::selected_model(session).label
}
fn render_model_status(ui: &mut egui::Ui, state: &mut AppState) {
    if state.session.provider != crate::model::ModelProvider::OmniForge {
        let model_label =
            config::interactive_model_for(state.session.provider).unwrap_or("default");
        ui.label(format!("Model: {}", model_label));
        return;
    }

    ui.label("Model:");
    let session_id = state
        .session
        .current_session
        .as_ref()
        .map(|session| session.id.clone());
    let selected_entry = crate::omniforge::selected_model(state.session.current_session.as_ref());
    let mut selected_model_id = selected_entry.id.to_string();

    ui.add_enabled_ui(session_id.is_some(), |ui| {
        egui::ComboBox::from_id_salt("omniforge_model_selector")
            .selected_text(omniforge_selected_model_label(
                state.session.current_session.as_ref(),
            ))
            .show_ui(ui, |ui| {
                for entry in crate::omniforge::MODELS {
                    ui.selectable_value(&mut selected_model_id, entry.id.to_string(), entry.label);
                }
            });
    });

    if selected_model_id != selected_entry.id {
        if let Some(session_id) = session_id {
            state.enqueue_event(AppEvent::UiAction(AppUiAction::SaveCurrentOmniForgeModel {
                session_id,
                model_id: selected_model_id,
            }));
        }
    }
}
fn drain_layout_log_completion(state: &mut AppState) {
    let Some(rx) = state.layout.layout_log_rx.take() else {
        return;
    };

    match rx.try_recv() {
        Ok(completion) => match completion.result {
            Ok(path) => {
                info!(path=%path.display(), "layout log written");
                state.layout.layout_log_feedback = Some(LayoutLogFeedback {
                    at: Instant::now(),
                    success: true,
                    path: Some(path),
                });
            }
            Err(err) => {
                error!(error=%err, "failed to write layout log");
                state.layout.layout_log_feedback = Some(LayoutLogFeedback {
                    at: Instant::now(),
                    success: false,
                    path: None,
                });
            }
        },
        Err(TryRecvError::Empty) => {
            state.layout.layout_log_rx = Some(rx);
        }
        Err(TryRecvError::Disconnected) => {
            state.layout.layout_log_feedback = Some(LayoutLogFeedback {
                at: Instant::now(),
                success: false,
                path: None,
            });
        }
    }
}
fn clamp_content_width(ui: &mut egui::Ui, width: f32) {
    ui.set_width(width);
    ui.set_min_width(width);
    ui.set_max_width(width);
}
fn render_panel_separator(ui: &mut egui::Ui, width: f32) {
    let separator_height = ui.spacing().item_spacing.y.max(6.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, separator_height), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
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
fn emit_auto_ui_debug_trace(state: &mut AppState, layout_snapshot: &LayoutSnapshot) {
    if !auto_ui_debug_enabled() {
        return;
    }

    let Some(session) = state.session.current_session.as_ref() else {
        return;
    };
    let Some(message_area) = layout_snapshot.message_area.as_ref() else {
        return;
    };

    let first_rendered_row = message_area
        .message_rows
        .iter()
        .find(|row| row.rendered && row.message_rect.is_some());
    let first_preview = first_rendered_row
        .and_then(|row| row.content_preview.as_deref())
        .unwrap_or("");
    let first_rect = first_rendered_row.and_then(|row| row.message_rect);
    let rendered_rows = message_area
        .message_rows
        .iter()
        .filter(|row| row.rendered)
        .count();
    let height_cache_load_pending = state.session.height_cache_load_rx.is_some();
    let height_cache_present = state.layout.session_height_cache.contains_key(&session.id);
    let sidebar_top_session = state.session.sessions.first();
    let sidebar_top_session_id = sidebar_top_session
        .map(|session_info| session_info.id.as_str())
        .unwrap_or("");
    let sidebar_top_session_name = sidebar_top_session
        .map(|session_info| session_info.name.as_str())
        .unwrap_or("");
    let sidebar_selected_session_id = state.session.selected_session_id.as_deref().unwrap_or("");
    let sidebar_selected_session_name = state
        .session
        .sessions
        .iter()
        .find(|session_info| session_info.id == sidebar_selected_session_id)
        .map(|session_info| session_info.name.as_str())
        .unwrap_or("");

    let signature = format!(
        "{}|{:.1}|{}|{}-{}|{:.1}|{}|{}|{}|{}|{}|{}|{}",
        session.id,
        message_area.available_width,
        message_area.message_count,
        message_area.visible_range.0,
        message_area.visible_range.1,
        message_area.max_rendered_overflow,
        rendered_rows,
        height_cache_load_pending,
        height_cache_present,
        state.session.sessions.len(),
        sidebar_top_session_id,
        sidebar_selected_session_id,
        first_rendered_row
            .map(|row| row.timestamp_ms)
            .unwrap_or_default(),
    );

    if state.layout.last_auto_ui_debug_signature.as_deref() == Some(signature.as_str()) {
        return;
    }
    state.layout.last_auto_ui_debug_signature = Some(signature);

    info!(
        session_id = %session.id,
        session_name = %session.name,
        provider = %state.session.provider.as_str(),
        content_width = layout_snapshot.content_width,
        pixels_per_point = layout_snapshot.pixels_per_point,
        message_area_available_width = message_area.available_width,
        message_count = message_area.message_count,
        visible_range_start = message_area.visible_range.0,
        visible_range_end = message_area.visible_range.1,
        max_rendered_right = message_area.max_rendered_right,
        max_rendered_overflow = message_area.max_rendered_overflow,
        rendered_rows,
        height_cache_load_pending,
        height_cache_present,
        sidebar_session_count = state.session.sessions.len(),
        sidebar_top_session_id,
        sidebar_top_session_name,
        sidebar_selected_session_id,
        sidebar_selected_session_name,
        first_preview = %first_preview,
        first_rect_min_x = first_rect.map(|rect| rect.min.x).unwrap_or_default(),
        first_rect_min_y = first_rect.map(|rect| rect.min.y).unwrap_or_default(),
        first_rect_width = first_rect.map(|rect| rect.width()).unwrap_or_default(),
        first_rect_height = first_rect.map(|rect| rect.height()).unwrap_or_default(),
        "ui_auto_debug"
    );
}
/// Render the system prompt input area
fn render_system_prompt_area(ui: &mut egui::Ui, state: &mut AppState) -> egui::Rect {
    let response = ui.group(|ui| {
        ui.label("System Prompt:");
        let response = ui.add_sized(
            [ui.available_width(), 60.0],
            egui::TextEdit::multiline(&mut state.session.system_prompt_text)
                .hint_text("Enter system prompt here (optional)..."),
        );

        if response.changed() {
            if state.session.current_session.is_some() {
                state.session.system_prompt_dirty = true;
                state.session.system_prompt_dirty_session_id = state
                    .session
                    .current_session
                    .as_ref()
                    .map(|session| session.id.clone());
            }
        }
        if state.session.system_prompt_dirty && response.lost_focus() {
            if let Some(current_session) = state.session.current_session.as_ref() {
                let new_prompt = if state.session.system_prompt_text.trim().is_empty() {
                    None
                } else {
                    Some(state.session.system_prompt_text.clone())
                };
                state.enqueue_event(AppEvent::UiAction(AppUiAction::SaveCurrentSystemPrompt {
                    session_id: current_session.id.clone(),
                    system_prompt: new_prompt,
                }));
            }
        }
    });
    response.response.rect
}
/// Render the status bar
fn render_status_bar(
    ui: &mut egui::Ui,
    state: &mut AppState,
    optimistic_empty_session: bool,
) -> egui::Rect {
    let mcp_servers = connected_mcp_servers(state);
    let response = ui.horizontal(|ui| {
        render_model_status(ui, state);

        if let Some(ref effort) =
            config::effective_reasoning_effort_for_provider(state.session.provider)
        {
            ui.label(format!("Effort: {}", effort));
        }

        let mcp_label = ui.label(mcp_status_text(&mcp_servers));
        mcp_label.on_hover_text(mcp_hover_text(&mcp_servers));

        if optimistic_empty_session {
            ui.label("Messages: 0");
        } else if let Some(ref session) = state.session.current_session {
            let user_msg_count = session.messages.iter().filter(|m| m.is_user).count();
            ui.label(format!("Messages: {}", user_msg_count));

            if let Some(ref preview) = state.layout.current_json_summary_preview {
                if preview.context_window_tokens > 0 && preview.context_window_size > 0 {
                    let pct = (preview.context_window_tokens as f64
                        / preview.context_window_size as f64
                        * 100.0) as u64;
                    ui.label(format!(
                        "Context: {}k/{}k ({}%) | Compacted: {}",
                        preview.context_window_tokens / 1000,
                        preview.context_window_size / 1000,
                        pct,
                        preview.num_compactions
                    ));
                } else if preview.context_window_size > 0 {
                    ui.label(format!(
                        "Context max: {}k | Compacted: {}",
                        preview.context_window_size / 1000,
                        preview.num_compactions
                    ));
                } else {
                    ui.label("Context: unavailable");
                }
            }
        } else {
            ui.label("No active session");
        }

        if let Some(ref feedback) = state.layout.status_feedback {
            if feedback.at.elapsed().as_secs_f32() < 3.0 {
                if feedback.is_error {
                    ui.colored_label(egui::Color32::from_rgb(255, 80, 80), &feedback.message);
                } else {
                    ui.colored_label(egui::Color32::from_rgb(120, 200, 120), &feedback.message);
                }
            }
        }

        if state.is_processing() {
            ui.colored_label(egui::Color32::from_rgb(255, 165, 0), "| Processing...");
        }

        if state
            .layout
            .status_feedback
            .as_ref()
            .is_some_and(|feedback| feedback.at.elapsed().as_secs_f32() >= 3.0)
        {
            state.layout.status_feedback = None;
        }
    });
    response.response.rect
}
