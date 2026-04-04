//! Virtualized message list rendering and height cache management.
//!
//! Extracted from main_panel.rs — handles the scroll area with virtual
//! row rendering, height-cache synchronization, and scroll-offset adjustment.

use std::time::{Duration, Instant};
use chrono::Utc;
use eframe::egui;
use rust_debug::tracing::error;
use crate::app::state::AppState;
use crate::app::ui::main_panel::{MessageAreaSnapshot, MessageRowSnapshot};
use crate::app::ui::message;
use crate::config;
use crate::data::{
    height_cache_width_key, resolve_height_cache_entry, DataManager, HeightCacheEntry,
    SessionHeightCache, SESSION_HEIGHT_CACHE_VERSION,
};
#[cfg(test)]
static FORCE_SYNC_HEIGHT_CACHE_SAVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
mod height_seed;
mod timing;
use self::height_seed::seed_message_height_cache_if_needed;
const MESSAGE_OVERSCAN_PX: f32 = 200.0;
const DEFAULT_MESSAGE_ROW_HEIGHT: f32 = 140.0;
const MIN_MESSAGE_ROW_HEIGHT: f32 = 28.0;
const HEIGHT_UPDATE_EPSILON: f32 = 1.0;
const WIDTH_RESET_EPSILON: f32 = 1.0;
const HEIGHT_SCAN_STEP_RATIO: f32 = 0.8;
const HEIGHT_SCAN_MIN_STEP: f32 = 120.0;
const HEIGHT_SCAN_END_EPSILON: f32 = 1.0;
const DEFAULT_SLOW_RENDER_WARN_THRESHOLD: Duration = Duration::from_millis(50);
pub(super) fn inter_message_spacing(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body) * 2.0
}
#[cfg(test)]
pub(crate) struct SyncHeightCacheSaveGuard {
    previous: bool,
}
#[cfg(test)]
impl SyncHeightCacheSaveGuard {
    pub(crate) fn enable() -> Self {
        let previous = FORCE_SYNC_HEIGHT_CACHE_SAVE.swap(true, std::sync::atomic::Ordering::SeqCst);
        Self { previous }
    }}
#[cfg(test)]
impl Drop for SyncHeightCacheSaveGuard {
    fn drop(&mut self) {
        FORCE_SYNC_HEIGHT_CACHE_SAVE.store(self.previous, std::sync::atomic::Ordering::SeqCst);
    }
}
/// Render the message list area
pub(crate) fn render_message_area(
    ui: &mut egui::Ui,
    state: &mut AppState,
    capture_snapshot: bool,
) -> Option<MessageAreaSnapshot> {
    let render_start = Instant::now();
    let available_height = ui.available_height() - 80.0; // Reserve space for input
    let available_width = ui.available_width();
    let width_key = height_cache_width_key(available_width);
    sync_message_height_cache(state, available_width);
    seed_message_height_cache_if_needed(ui, state, available_width, width_key);

    let current_session = state.session.current_session.as_ref();
    let session_id = current_session.as_ref().map(|session| session.id.as_str());
    let message_count = current_session
        .map(|session| session.messages.len())
        .unwrap_or(0);
    let user_scrolling_this_frame = ui.ctx().input(|input| input.smooth_scroll_delta.y != 0.0);
    let scan_mismatch = state
        .layout
        .height_cache_scan
        .as_ref()
        .map(|scan| {
            session_id.map_or(true, |id| scan.session_id != id) || scan.width_key != width_key
        })
        .unwrap_or(false);
    if scan_mismatch {
        state.layout.height_cache_scan = None;
    }
    let mut rendered_rows: Vec<Option<(egui::Rect, Option<egui::Rect>)>> =
        vec![None; message_count];
    let row_heights_width = state.layout.message_row_heights_width;
    let is_processing = state.is_processing();
    let show_pending_draft = should_show_pending_draft(
        is_processing,
        state
            .session
            .pending_draft
            .as_ref()
            .map(|d| d.chat_id.as_str()),
        session_id,
    );

    let mut viewport_rect = egui::Rect::NOTHING;
    let mut rendered_range = (0usize, 0usize);
    let mut rendered_row_count = 0usize;
    // Compensate scroll offset when measured row heights above the viewport change.
    let mut scroll_adjustment = 0.0f32;
    let mut row_heights_dirty = false;
    let mut scroll_area = egui::ScrollArea::vertical()
        .max_height(available_height)
        .max_width(available_width)
        .auto_shrink(egui::Vec2b::new(false, true))
        .stick_to_bottom(true)
        .id_salt(session_id.unwrap_or("message_scroll"))
        // Force horizontal offset to zero.  A stale offset can persist
        // from previous frames (or from when hscroll was enabled) and
        // shifts content_max_rect left, causing a cascading width blow-up.
        .horizontal_scroll_offset(0.0);
    if state.layout.message_scroll_offset_session_id.as_deref() == session_id {
        if let Some(offset) = state.layout.message_scroll_offset_override.take() {
            scroll_area = scroll_area.vertical_scroll_offset(offset);
        }
    } else {
        state.layout.message_scroll_offset_override = None;
        state.layout.message_scroll_offset_session_id = session_id.map(|id| id.to_string());
    }
    if state
        .layout
        .deferred_message_scroll_adjustment_session_id
        .as_deref()
        != session_id
    {
        state.layout.deferred_message_scroll_adjustment = 0.0;
        state.layout.deferred_message_scroll_adjustment_session_id =
            session_id.map(|id| id.to_string());
    }

    let scroll_output = {
        let row_heights = &mut state.layout.message_row_heights;
        let markdown_cache = &mut state.layout.markdown_cache;
        let copy_feedback = &mut state.layout.copy_feedback;
        scroll_area.show_viewport(ui, |ui, viewport| {
            // Hard-lock the scroll content to the available width.
            // set_width sets BOTH min and max so the scroll area will
            // never report a wider available_width() to children.
            ui.set_width(available_width);
            viewport_rect = viewport;
            let Some(session) = current_session else {
                if is_processing {
                    message::render_processing_indicator(ui);
                }
                return;
            };

            let messages = &session.messages;
            if messages.is_empty() {
                if is_processing {
                    message::render_processing_indicator(ui);
                }
                return;
            }

            let row_prefix = build_row_prefix_sums(row_heights);
            let total_height = *row_prefix.last().unwrap_or(&0.0);
            ui.set_height(total_height);

            let (start_idx, end_idx) =
                viewport_message_range(&row_prefix, viewport, messages.len());
            rendered_range = (start_idx, end_idx);
            rendered_row_count = end_idx.saturating_sub(start_idx);

            let top_spacer = row_prefix[start_idx];
            if top_spacer > 0.0 {
                ui.add_space(top_spacer);
            }

            for idx in start_idx..end_idx {
                let row_start = ui.cursor().top();
                let msg = &messages[idx];

                // Render each row inside its own clipped child UI so a wide
                // markdown subtree cannot widen the scroll content rect.
                let row_min = ui.cursor().left_top();
                let row_rect = egui::Rect::from_min_max(
                    row_min,
                    egui::pos2(row_min.x + available_width, ui.clip_rect().max.y),
                );
                let mut row_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row_rect)
                        .layout(*ui.layout()),
                );
                row_ui.set_clip_rect(ui.clip_rect().intersect(row_rect));
                row_ui.set_width(available_width);
                row_ui.set_min_width(available_width);
                row_ui.set_max_width(available_width);

                let row_render_start = Instant::now();
                let previous_message = idx
                    .checked_sub(1)
                    .and_then(|previous| messages.get(previous));
                let message_rect = message::render_message(
                    &mut row_ui,
                    msg,
                    previous_message,
                    markdown_cache,
                    copy_feedback,
                );
                timing::log_message_row_render_timing(
                    session_id,
                    state.session.provider,
                    idx,
                    messages.len(),
                    msg,
                    !msg.is_user,
                    is_processing,
                    available_width,
                    row_render_start.elapsed(),
                );
                let command_rect = None;

                let row_used = row_ui.min_rect();
                if row_used.is_positive() {
                    let mut size = row_used.size();
                    size.x = size.x.min(available_width);
                    ui.advance_cursor_after_rect(egui::Rect::from_min_size(row_rect.min, size));
                }

                rendered_rows[idx] = Some((message_rect, command_rect));
                ui.add_space(inter_message_spacing(ui));

                let measured_height = (ui.cursor().top() - row_start).max(MIN_MESSAGE_ROW_HEIGHT);
                let prev_height = row_heights[idx];
                if (prev_height - measured_height).abs() > HEIGHT_UPDATE_EPSILON {
                    row_heights[idx] = measured_height;
                    if row_prefix[idx] < viewport.min.y {
                        scroll_adjustment += measured_height - prev_height;
                    }
                    row_heights_dirty = true;
                }
            }

            let bottom_spacer = (total_height - row_prefix[end_idx]).max(0.0);
            if bottom_spacer > 0.0 {
                ui.add_space(bottom_spacer);
            }

            if is_processing {
                message::render_processing_indicator(ui);
            }

            if show_pending_draft {
                message::render_processing_indicator(ui);
            }
        })
    };

    let scan_active = state
        .layout
        .height_cache_scan
        .as_ref()
        .map(|scan| session_id.map_or(false, |id| scan.session_id == id))
        .unwrap_or(false);
    let previous_deferred_adjustment = state.layout.deferred_message_scroll_adjustment;
    let (apply_scroll_adjustment, next_deferred_adjustment) = resolve_scroll_adjustment(
        scroll_adjustment,
        previous_deferred_adjustment,
        user_scrolling_this_frame,
        scan_active,
    );
    state.layout.deferred_message_scroll_adjustment = next_deferred_adjustment;
    state.layout.deferred_message_scroll_adjustment_session_id =
        session_id.map(|id| id.to_string());
    if let Some(scroll_adjustment) = apply_scroll_adjustment {
        let mut new_offset = scroll_output.state.offset.y + scroll_adjustment;
        if new_offset < 0.0 {
            new_offset = 0.0;
        }
        let max_offset =
            (scroll_output.content_size.y - scroll_output.inner_rect.height()).max(0.0);
        if new_offset > max_offset {
            new_offset = max_offset;
        }
        state.layout.message_scroll_offset_override = Some(new_offset);
        state.layout.message_scroll_offset_session_id = session_id.map(|id| id.to_string());
    }

    let mut scan_complete: Option<(String, f32, f32)> = None;
    if scan_active {
        if let Some(session_id) = session_id {
            let max_offset =
                (scroll_output.content_size.y - scroll_output.inner_rect.height()).max(0.0);
            let current_offset = scroll_output.state.offset.y;
            if let Some(scan) = state.layout.height_cache_scan.as_mut() {
                if scan.start_offset.is_none() {
                    scan.start_offset = Some(current_offset);
                }
                if scan.seeking_top {
                    if current_offset <= HEIGHT_SCAN_END_EPSILON {
                        scan.seeking_top = false;
                    } else {
                        state.layout.message_scroll_offset_override = Some(0.0);
                        state.layout.message_scroll_offset_session_id =
                            Some(session_id.to_string());
                    }
                }
                if !scan.seeking_top {
                    if message_count == 0 || max_offset <= HEIGHT_SCAN_END_EPSILON {
                        let restore = scan.start_offset.unwrap_or(0.0);
                        scan_complete = Some((session_id.to_string(), restore, max_offset));
                    } else if current_offset + HEIGHT_SCAN_END_EPSILON >= max_offset {
                        if scan.at_end {
                            let restore = scan.start_offset.unwrap_or(0.0);
                            scan_complete = Some((session_id.to_string(), restore, max_offset));
                        } else {
                            scan.at_end = true;
                            state.layout.message_scroll_offset_override = Some(max_offset);
                            state.layout.message_scroll_offset_session_id =
                                Some(session_id.to_string());
                        }
                    } else {
                        scan.at_end = false;
                        let step = (scroll_output.inner_rect.height() * HEIGHT_SCAN_STEP_RATIO)
                            .max(HEIGHT_SCAN_MIN_STEP);
                        let next_offset = (current_offset + step).min(max_offset);
                        state.layout.message_scroll_offset_override = Some(next_offset);
                        state.layout.message_scroll_offset_session_id =
                            Some(session_id.to_string());
                    }
                }
            }
        }
    }

    if let Some((session_id, restore_offset, max_offset)) = scan_complete {
        let restore = restore_offset.clamp(0.0, max_offset);
        state.layout.message_scroll_offset_override = Some(restore);
        state.layout.message_scroll_offset_session_id = Some(session_id.clone());
        state.layout.pending_height_cache_save = Some(session_id.clone());
        state.layout.height_cache_scan = None;
    }

    if let Some(session_id) = session_id {
        let should_repersist_stale_cache = should_schedule_height_cache_save(
            state
                .layout
                .session_height_cache
                .get(session_id)
                .and_then(|cache| resolve_height_cache_entry(cache, width_key)),
            message_count,
            row_heights_dirty,
            is_processing,
        );
        if should_repersist_stale_cache {
            state.layout.pending_height_cache_save = Some(session_id.to_string());
        }

        let should_update_cache = row_heights_dirty
            || state.layout.pending_height_cache_save.as_deref() == Some(session_id);
        if should_update_cache {
            let clamped: Vec<f32> = state
                .layout
                .message_row_heights
                .iter()
                .map(|h| h.max(MIN_MESSAGE_ROW_HEIGHT))
                .collect();
            let total_height = clamped.iter().sum::<f32>();
            let entry = HeightCacheEntry {
                row_heights: clamped,
                total_height,
                message_count,
                updated_at: Utc::now(),
            };
            let cache = state
                .layout
                .session_height_cache
                .entry(session_id.to_string())
                .or_insert_with(|| SessionHeightCache::new(session_id.to_string()));
            cache.version = SESSION_HEIGHT_CACHE_VERSION;
            cache.widths.insert(width_key, entry);
        }

        if state.layout.pending_height_cache_save.as_deref() == Some(session_id) {
            if let Some(cache) = state.layout.session_height_cache.get(session_id).cloned() {
                persist_height_cache(state.session.provider, cache);
            }
            state.layout.pending_height_cache_save = None;
        }
    }

    timing::log_message_area_render_timing(
        session_id,
        available_width,
        message_count,
        rendered_range,
        rendered_row_count,
        is_processing,
        render_start.elapsed(),
    );

    if !capture_snapshot {
        return None;
    }

    let row_heights = &state.layout.message_row_heights;
    let row_prefix = build_row_prefix_sums(row_heights);
    let visible_range = viewport_message_range(&row_prefix, viewport_rect, message_count);
    let row_total_height = *row_prefix.last().unwrap_or(&0.0);
    let claude_content_width_max = config::MESSAGE_MAX_WIDTH.min(available_width);
    let message_group_min_width = (available_width - 10.0).max(0.0);

    let mut message_rows = Vec::with_capacity(message_count);
    let mut max_rendered_right = scroll_output.inner_rect.min.x;
    if let Some(session) = current_session {
        for (idx, msg) in session.messages.iter().enumerate() {
            let raw_height = row_heights
                .get(idx)
                .copied()
                .unwrap_or(DEFAULT_MESSAGE_ROW_HEIGHT);
            let height = raw_height.max(MIN_MESSAGE_ROW_HEIGHT);
            let top = row_prefix.get(idx).copied().unwrap_or(0.0);
            let bottom = top + height;
            let mut rendered = false;
            let mut message_rect = None;
            let mut command_rect = None;
            let content_preview = Some(preview_text(&msg.content, 96));
            let command_preview = msg.command.as_ref().map(|cmd| preview_text(cmd, 96));
            if let Some((msg_rect, cmd_rect)) = rendered_rows.get(idx).and_then(|v| *v) {
                rendered = true;
                message_rect = Some(msg_rect);
                command_rect = cmd_rect;
                max_rendered_right = max_rendered_right.max(msg_rect.max.x);
                if let Some(cmd_rect) = cmd_rect {
                    max_rendered_right = max_rendered_right.max(cmd_rect.max.x);
                }
            }
            message_rows.push(MessageRowSnapshot {
                index: idx,
                is_user: msg.is_user,
                timestamp_ms: msg.timestamp.timestamp_millis(),
                content_len: msg.content.len(),
                rendered,
                message_rect,
                command_rect,
                has_command: msg.command.is_some(),
                content_preview,
                command_preview,
                raw_height,
                height,
                top,
                bottom,
            });
        }
    }
    let max_rendered_overflow = (max_rendered_right - scroll_output.inner_rect.max.x).max(0.0);

    Some(MessageAreaSnapshot {
        available_width,
        available_height,
        scroll_inner_rect: scroll_output.inner_rect,
        scroll_content_size: scroll_output.content_size,
        viewport_rect,
        message_count,
        row_heights_width,
        row_total_height,
        visible_range,
        message_group_min_width,
        claude_content_width_max,
        message_rows,
        max_rendered_right,
        max_rendered_overflow,
    })
}
pub(crate) fn render_empty_message_area(ui: &mut egui::Ui, session_id: &str) {
    let available_height = ui.available_height() - 80.0;
    let available_width = ui.available_width();
    egui::ScrollArea::vertical()
        .max_height(available_height)
        .max_width(available_width)
        .auto_shrink(egui::Vec2b::new(false, true))
        .stick_to_bottom(true)
        .id_salt(session_id)
        .horizontal_scroll_offset(0.0)
        .show_viewport(ui, |ui, _viewport| {
            ui.set_width(available_width);
        });
}
fn sync_message_height_cache(state: &mut AppState, available_width: f32) {
    let Some((session_id, message_count)) = state
        .session
        .current_session
        .as_ref()
        .map(|session| (session.id.as_str(), session.messages.len()))
    else {
        state.layout.message_row_heights.clear();
        state.layout.message_row_heights_session_id = None;
        state.layout.message_row_heights_width = available_width;
        state.layout.message_scroll_offset_override = None;
        state.layout.message_scroll_offset_session_id = None;
        state.layout.deferred_message_scroll_adjustment = 0.0;
        state.layout.deferred_message_scroll_adjustment_session_id = None;
        return;
    };

    let session_changed =
        state.layout.message_row_heights_session_id.as_deref() != Some(session_id);
    let width_changed =
        (state.layout.message_row_heights_width - available_width).abs() > WIDTH_RESET_EPSILON;
    let width_key = height_cache_width_key(available_width);
    let cached_entry = state
        .layout
        .session_height_cache
        .get(session_id)
        .and_then(|cache| resolve_height_cache_entry(cache, width_key));
    if session_changed || width_changed {
        state.layout.message_row_heights.clear();
        state.layout.message_row_heights_session_id = Some(session_id.to_string());
        state.layout.message_row_heights_width = available_width;
        state.layout.message_scroll_offset_override = None;
        state.layout.message_scroll_offset_session_id = Some(session_id.to_string());
        state.layout.deferred_message_scroll_adjustment = 0.0;
        state.layout.deferred_message_scroll_adjustment_session_id = Some(session_id.to_string());

        if let Some(entry) = cached_entry {
            state.layout.message_row_heights = entry
                .row_heights
                .iter()
                .map(|h| h.max(MIN_MESSAGE_ROW_HEIGHT))
                .collect();
        }
    }

    let default_height = cached_entry
        .and_then(cached_entry_default_height)
        .unwrap_or_else(|| estimated_row_height(&state.layout.message_row_heights));
    let old_len = state.layout.message_row_heights.len();
    state
        .layout
        .message_row_heights
        .resize(message_count, default_height);

    if let Some(entry) = cached_entry {
        let cached_len = entry.row_heights.len();
        let fill_start = old_len.min(message_count);
        if cached_len > 0 {
            for idx in fill_start..message_count {
                if idx < cached_len {
                    state.layout.message_row_heights[idx] =
                        entry.row_heights[idx].max(MIN_MESSAGE_ROW_HEIGHT);
                }
            }
        }
    }
}
fn estimated_row_height(row_heights: &[f32]) -> f32 {
    if row_heights.is_empty() {
        return DEFAULT_MESSAGE_ROW_HEIGHT;
    }

    let avg = row_heights.iter().copied().sum::<f32>() / row_heights.len() as f32;
    avg.clamp(80.0, 500.0)
}
fn resolve_scroll_adjustment(
    immediate_adjustment: f32,
    deferred_adjustment: f32,
    user_scrolling_this_frame: bool,
    scan_active: bool,
) -> (Option<f32>, f32) {
    if scan_active {
        return (None, 0.0);
    }

    if user_scrolling_this_frame {
        return (None, deferred_adjustment + immediate_adjustment);
    }

    let total_adjustment = deferred_adjustment + immediate_adjustment;
    if total_adjustment.abs() > 0.5 {
        (Some(total_adjustment), 0.0)
    } else {
        (None, 0.0)
    }
}
pub(crate) fn build_row_prefix_sums(row_heights: &[f32]) -> Vec<f32> {
    let mut prefix = Vec::with_capacity(row_heights.len() + 1);
    prefix.push(0.0);

    for height in row_heights {
        let clamped = height.max(MIN_MESSAGE_ROW_HEIGHT);
        let next = prefix.last().copied().unwrap_or(0.0) + clamped;
        prefix.push(next);
    }

    prefix
}
fn cached_entry_default_height(entry: &HeightCacheEntry) -> Option<f32> {
    if entry.message_count > 0 && entry.total_height.is_finite() && entry.total_height > 0.0 {
        return Some((entry.total_height / entry.message_count as f32).clamp(80.0, 500.0));
    }

    if entry.row_heights.is_empty() {
        None
    } else {
        let avg = entry.row_heights.iter().sum::<f32>() / entry.row_heights.len() as f32;
        Some(avg.clamp(80.0, 500.0))
    }
}
pub(super) fn has_complete_height_cache_entry(
    cached_entry: Option<&HeightCacheEntry>,
    message_count: usize,
) -> bool {
    matches!(
        cached_entry,
        Some(entry) if entry.message_count == message_count && entry.row_heights.len() == message_count
    )
}
fn should_schedule_height_cache_save(
    cached_entry: Option<&HeightCacheEntry>,
    message_count: usize,
    row_heights_dirty: bool,
    is_processing: bool,
) -> bool {
    if is_processing || !row_heights_dirty {
        return false;
    }

    cached_entry.is_some() && !has_complete_height_cache_entry(cached_entry, message_count)
}
fn persist_height_cache(provider: crate::model::ModelProvider, cache: SessionHeightCache) {
    #[cfg(test)]
    if FORCE_SYNC_HEIGHT_CACHE_SAVE.load(std::sync::atomic::Ordering::SeqCst) {
        if let Err(err) = save_height_cache(provider, &cache) {
            error!(
                error=%err,
                session_id=%cache.session_id,
                "failed to save height cache"
            );
        }
        return;
    }

    std::thread::spawn(move || {
        if let Err(err) = save_height_cache(provider, &cache) {
            error!(
                error=%err,
                session_id=%cache.session_id,
                "failed to save height cache"
            );
        }
    });
}
fn save_height_cache(
    provider: crate::model::ModelProvider,
    cache: &SessionHeightCache,
) -> anyhow::Result<()> {
    let data_manager = DataManager::new(provider)?;
    data_manager.save_height_cache(cache)?;
    Ok(())
}
pub(crate) fn viewport_message_range(
    row_prefix: &[f32],
    viewport: egui::Rect,
    message_count: usize,
) -> (usize, usize) {
    if message_count == 0 {
        return (0, 0);
    }

    let start_y = (viewport.min.y - MESSAGE_OVERSCAN_PX).max(0.0);
    let end_y = viewport.max.y + MESSAGE_OVERSCAN_PX;

    let mut start_idx = lower_bound(row_prefix, start_y).saturating_sub(1);
    if start_idx >= message_count {
        start_idx = message_count - 1;
    }

    let mut end_idx = lower_bound(row_prefix, end_y).min(message_count);
    if end_idx <= start_idx {
        end_idx = (start_idx + 1).min(message_count);
    }

    (start_idx, end_idx)
}
fn lower_bound(values: &[f32], target: f32) -> usize {
    let mut left = 0;
    let mut right = values.len();

    while left < right {
        let mid = (left + right) / 2;
        if values[mid] < target {
            left = mid + 1;
        } else {
            right = mid;
        }
    }

    left
}
pub(crate) fn should_show_pending_draft(
    is_processing: bool,
    pending_draft_chat_id: Option<&str>,
    current_session_id: Option<&str>,
) -> bool {
    if is_processing {
        return false;
    }
    match (pending_draft_chat_id, current_session_id) {
        (Some(draft_id), Some(session_id)) => draft_id == session_id,
        _ => false,
    }
}
fn preview_text(text: &str, max_len: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    for ch in text.chars() {
        if count >= max_len {
            out.push_str("...");
            break;
        }
        if ch == '\n' || ch == '\r' || ch == '\t' {
            out.push(' ');
        } else {
            out.push(ch);
        }
        count += 1;
    }
    out.trim().to_string()
}
