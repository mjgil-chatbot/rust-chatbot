use super::{
    has_complete_height_cache_entry, AppState, HeightCacheEntry, SessionHeightCache,
    MIN_MESSAGE_ROW_HEIGHT,
};
use crate::app::ui::message;
use crate::data::{resolve_height_cache_entry, SESSION_HEIGHT_CACHE_VERSION};
use chrono::Utc;
use eframe::egui;

const FULL_HEIGHT_SEED_MAX_MESSAGES: usize = 200;
pub(super) fn seed_message_height_cache_if_needed(
    ui: &mut egui::Ui,
    state: &mut AppState,
    available_width: f32,
    width_key: u32,
) {
    let Some(session) = state.session.current_session.as_ref() else {
        return;
    };
    let session_id = session.id.clone();
    let message_count = session.messages.len();
    let cached_entry = state
        .layout
        .session_height_cache
        .get(&session_id)
        .and_then(|cache| resolve_height_cache_entry(cache, width_key));
    if !should_seed_full_height_cache(cached_entry, message_count, state.is_processing()) {
        return;
    }

    let measured_heights = measure_all_message_row_heights(
        ui,
        session,
        available_width,
        &mut state.layout.markdown_cache,
        &mut state.layout.copy_feedback,
    );
    if measured_heights.len() != message_count {
        return;
    }

    state.layout.message_row_heights = measured_heights.clone();
    state.layout.message_row_heights_session_id = Some(session_id.clone());
    state.layout.message_row_heights_width = available_width;
    let total_height = measured_heights.iter().sum::<f32>();
    let entry = HeightCacheEntry {
        row_heights: measured_heights,
        total_height,
        message_count,
        updated_at: Utc::now(),
    };
    let cache = state
        .layout
        .session_height_cache
        .entry(session_id.clone())
        .or_insert_with(|| SessionHeightCache::new(session_id.clone()));
    cache.version = SESSION_HEIGHT_CACHE_VERSION;
    cache.widths.insert(width_key, entry);
    state.layout.pending_height_cache_save = Some(session_id);
}

pub(super) fn should_seed_full_height_cache(
    cached_entry: Option<&HeightCacheEntry>,
    message_count: usize,
    is_processing: bool,
) -> bool {
    !has_complete_height_cache_entry(cached_entry, message_count)
        && !is_processing
        && message_count > 0
        && message_count <= FULL_HEIGHT_SEED_MAX_MESSAGES
}

fn measure_all_message_row_heights(
    ui: &mut egui::Ui,
    session: &crate::data::ChatSession,
    available_width: f32,
    markdown_cache: &mut crate::markdown::MarkdownCache,
    copy_feedback: &mut std::collections::HashMap<i64, std::time::Instant>,
) -> Vec<f32> {
    let measurement_rect = egui::Rect::from_min_size(
        egui::pos2(-10_000.0, -10_000.0),
        egui::vec2(available_width.max(0.0), 1.0),
    );
    let mut measure_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(measurement_rect)
            .layout(egui::Layout::top_down(egui::Align::LEFT))
            .invisible(),
    );
    measure_ui.set_width(available_width);
    measure_ui.set_min_width(available_width);
    measure_ui.set_max_width(available_width);

    let mut heights = Vec::with_capacity(session.messages.len());
    for (index, msg) in session.messages.iter().enumerate() {
        let row_start = measure_ui.cursor().top();
        let previous_message = index
            .checked_sub(1)
            .and_then(|previous| session.messages.get(previous));
        let _ = message::render_message(
            &mut measure_ui,
            msg,
            previous_message,
            markdown_cache,
            copy_feedback,
        );
        measure_ui.add_space(super::inter_message_spacing(&measure_ui));
        let measured_height = (measure_ui.cursor().top() - row_start).max(MIN_MESSAGE_ROW_HEIGHT);
        heights.push(measured_height);
    }

    heights
}
