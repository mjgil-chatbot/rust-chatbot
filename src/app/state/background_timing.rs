use std::sync::OnceLock;
use std::time::Duration;

use rust_debug::tracing::{debug, warn};

const DEFAULT_SLOW_BACKGROUND_CHANNEL_WARN_THRESHOLD: Duration = Duration::from_millis(75);

pub(super) fn finish_background_channel_timing(
    channel: &'static str,
    started_at: &mut Option<std::time::Instant>,
) {
    let Some(started_at) = started_at.take() else {
        return;
    };

    log_background_channel_timing(channel, started_at.elapsed());
}

fn log_background_channel_timing(channel: &'static str, elapsed: Duration) {
    let always_log = background_channel_timing_log_enabled();
    let slow_threshold = slow_background_channel_warn_threshold();
    let is_slow = elapsed >= slow_threshold;
    if !always_log && !is_slow {
        return;
    }

    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    if is_slow {
        warn!(
            channel,
            elapsed_ms,
            slow_threshold_ms = slow_threshold.as_secs_f64() * 1000.0,
            "background_channel_latency_slow"
        );
    } else {
        debug!(channel, elapsed_ms, "background_channel_latency");
    }
}

fn background_channel_timing_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag("RUST_CHATBOT_LOG_BACKGROUND_CHANNEL_TIMES"))
}

fn slow_background_channel_warn_threshold() -> Duration {
    static THRESHOLD: OnceLock<Duration> = OnceLock::new();
    *THRESHOLD.get_or_init(|| {
        std::env::var("RUST_CHATBOT_SLOW_BACKGROUND_CHANNEL_WARN_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_SLOW_BACKGROUND_CHANNEL_WARN_THRESHOLD)
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
