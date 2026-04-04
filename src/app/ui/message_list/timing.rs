use std::sync::{Mutex, OnceLock};

use rust_debug::tracing::{debug, warn};

use super::*;
use crate::model::ModelProvider;

pub(super) fn log_message_area_render_timing(
    session_id: Option<&str>,
    available_width: f32,
    message_count: usize,
    rendered_range: (usize, usize),
    rendered_row_count: usize,
    is_processing: bool,
    elapsed: Duration,
) {
    let always_log = render_timing_log_enabled();
    let slow_threshold = slow_render_warn_threshold();
    let is_slow = elapsed >= slow_threshold;
    if !always_log && !is_slow {
        return;
    }

    if !is_slow {
        let signature = format!(
            "{}|{available_width:.1}|{message_count}|{}-{}|{rendered_row_count}|{is_processing}",
            session_id.unwrap_or("none"),
            rendered_range.0,
            rendered_range.1,
        );
        let last_signature = LAST_MESSAGE_AREA_RENDER_TIMING_SIGNATURE
            .get_or_init(|| Mutex::new(None))
            .lock();
        let Ok(mut last_signature) = last_signature else {
            return;
        };
        if last_signature.as_deref() == Some(signature.as_str()) {
            return;
        }
        *last_signature = Some(signature);
    }

    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    if is_slow {
        warn!(
            session_id = session_id.unwrap_or("none"),
            available_width,
            message_count,
            rendered_range_start = rendered_range.0,
            rendered_range_end = rendered_range.1,
            rendered_row_count,
            is_processing,
            elapsed_ms,
            slow_threshold_ms = slow_threshold.as_secs_f64() * 1000.0,
            "message_area_render_time_slow"
        );
    } else {
        debug!(
            session_id = session_id.unwrap_or("none"),
            available_width,
            message_count,
            rendered_range_start = rendered_range.0,
            rendered_range_end = rendered_range.1,
            rendered_row_count,
            is_processing,
            elapsed_ms,
            "message_area_render_time"
        );
    }
}

pub(super) fn log_message_row_render_timing(
    session_id: Option<&str>,
    provider: ModelProvider,
    message_index: usize,
    total_messages: usize,
    message: &crate::data::ChatMessage,
    rendered_as_markdown: bool,
    is_processing: bool,
    available_width: f32,
    elapsed: Duration,
) {
    let always_log = render_timing_log_enabled();
    let slow_threshold = slow_render_warn_threshold();
    let is_slow = elapsed >= slow_threshold;
    if !always_log && !is_slow {
        return;
    }

    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let is_tail_message = message_index + 1 == total_messages;
    let content_len = message.content.len();
    let rendered_as_markdown = rendered_as_markdown && !message.is_user;

    if is_slow {
        warn!(
            session_id = session_id.unwrap_or("none"),
            provider = %provider.as_str(),
            message_index,
            total_messages,
            is_user = message.is_user,
            rendered_as_markdown,
            is_processing,
            is_tail_message,
            available_width,
            content_len,
            elapsed_ms,
            slow_threshold_ms = slow_threshold.as_secs_f64() * 1000.0,
            "message_row_render_time_slow"
        );
    } else {
        let preview = preview_text(&message.content, 96);
        debug!(
            session_id = session_id.unwrap_or("none"),
            provider = %provider.as_str(),
            message_index,
            total_messages,
            is_user = message.is_user,
            rendered_as_markdown,
            is_processing,
            is_tail_message,
            available_width,
            content_len,
            preview = %preview,
            elapsed_ms,
            "message_row_render_time"
        );
    }
}

fn render_timing_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag("RUST_CHATBOT_LOG_RENDER_TIMES"))
}

fn slow_render_warn_threshold() -> Duration {
    static THRESHOLD: OnceLock<Duration> = OnceLock::new();
    *THRESHOLD.get_or_init(|| {
        std::env::var("RUST_CHATBOT_SLOW_RENDER_WARN_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_SLOW_RENDER_WARN_THRESHOLD)
    })
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            let value = value.to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

static LAST_MESSAGE_AREA_RENDER_TIMING_SIGNATURE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
