use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use rust_debug::tracing::{debug, warn};

#[allow(unused_imports)]
pub use tmc::egui::parse_markdown_segments;
#[allow(unused_imports)]
pub use tmc::{MarkdownCache, MarkdownRenderOptions, MarkdownSegment, MarkdownTable};

const DEFAULT_SLOW_RENDER_WARN_THRESHOLD: Duration = Duration::from_millis(50);

/// Default render options: treat untagged and plain-text code blocks as shell.
const RENDER_OPTIONS: MarkdownRenderOptions<'static> = MarkdownRenderOptions {
    default_code_language: Some("sh"),
    code_language_overrides: &[("text", "sh"), ("txt", "sh")],
};

pub fn render_markdown(ui: &mut egui::Ui, cache: &mut MarkdownCache, content: &str, id_salt: &str) {
    tmc::render_markdown_with_options(ui, cache, content, id_salt, &RENDER_OPTIONS);
}

pub fn log_markdown_render_timing(id_salt: &str, content: &str, width: f32, elapsed: Duration) {
    let content_len = content.len();
    let profile = MarkdownRenderProfile::from_content(content);
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let always_log = render_timing_log_enabled();
    let slow_threshold = slow_render_warn_threshold();
    let is_slow = elapsed >= slow_threshold;
    if !always_log && !is_slow {
        return;
    }

    if !is_slow {
        let signature = format!("{id_salt}|{content_len}|{width:.1}");
        let seen = LAST_MARKDOWN_RENDER_TIMING_SIGNATURE
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock();
        let Ok(mut seen) = seen else {
            return;
        };
        if !seen.insert(signature) {
            return;
        }
    }

    if is_slow {
        warn!(
            id_salt = %id_salt,
            content_len,
            width,
            line_count = profile.line_count,
            code_fence_count = profile.code_fence_count,
            table_row_count = profile.table_row_count,
            link_count = profile.link_count,
            list_marker_count = profile.list_marker_count,
            heading_count = profile.heading_count,
            quote_line_count = profile.quote_line_count,
            is_probably_json = profile.is_probably_json,
            preview = %profile.preview,
            elapsed_ms,
            slow_threshold_ms = slow_threshold.as_secs_f64() * 1000.0,
            "markdown_render_time_slow"
        );
    } else {
        debug!(
            id_salt = %id_salt,
            content_len,
            width,
            line_count = profile.line_count,
            code_fence_count = profile.code_fence_count,
            table_row_count = profile.table_row_count,
            link_count = profile.link_count,
            list_marker_count = profile.list_marker_count,
            heading_count = profile.heading_count,
            quote_line_count = profile.quote_line_count,
            is_probably_json = profile.is_probably_json,
            preview = %profile.preview,
            elapsed_ms,
            "markdown_render_time"
        );
    }
}

#[derive(Debug)]
struct MarkdownRenderProfile {
    preview: String,
    line_count: usize,
    code_fence_count: usize,
    table_row_count: usize,
    link_count: usize,
    list_marker_count: usize,
    heading_count: usize,
    quote_line_count: usize,
    is_probably_json: bool,
}

impl MarkdownRenderProfile {
    fn from_content(content: &str) -> Self {
        let mut line_count = 0usize;
        let mut code_fence_count = 0usize;
        let mut table_row_count = 0usize;
        let mut list_marker_count = 0usize;
        let mut heading_count = 0usize;
        let mut quote_line_count = 0usize;

        for line in content.lines() {
            line_count += 1;
            let trimmed = line.trim_start();
            if crate::common::fence_delimiter(trimmed).is_some() {
                code_fence_count += 1;
            }
            if trimmed.starts_with('>') {
                quote_line_count += 1;
            }
            if trimmed.starts_with('#') {
                heading_count += 1;
            }
            if is_likely_list_marker(trimmed) {
                list_marker_count += 1;
            }
            if is_likely_table_row(trimmed) {
                table_row_count += 1;
            }
        }

        Self {
            preview: preview_text(content, 96),
            line_count,
            code_fence_count,
            table_row_count,
            link_count: content.matches("](").count(),
            list_marker_count,
            heading_count,
            quote_line_count,
            is_probably_json: is_probably_json_content(content),
        }
    }
}

fn preview_text(text: &str, max_len: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count >= max_len {
            out.push_str("...");
            break;
        }
        if matches!(ch, '\n' | '\r' | '\t') {
            out.push(' ');
        } else {
            out.push(ch);
        }
        count += 1;
    }
    out.trim().to_string()
}

fn is_likely_list_marker(line: &str) -> bool {
    if ["- ", "* ", "+ "]
        .iter()
        .any(|marker| line.starts_with(marker))
    {
        return true;
    }

    let digit_count = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    digit_count > 0
        && line
            .get(digit_count..)
            .map(|rest| rest.starts_with(". "))
            .unwrap_or(false)
}

fn is_likely_table_row(line: &str) -> bool {
    if line.is_empty() || !line.contains('|') {
        return false;
    }

    crate::common::split_table_row(line)
        .map(|cells| cells.iter().filter(|cell| !cell.is_empty()).count() >= 2)
        .unwrap_or(false)
}

fn is_probably_json_content(content: &str) -> bool {
    let trimmed = content.trim();
    (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || content.contains("```json")
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

static LAST_MARKDOWN_RENDER_TIMING_SIGNATURE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;
    use egui_extras::syntax_highlighting::{self, CodeTheme};

    // ── preview_text ─────────────────────────────────────────────────

    #[test]
    fn preview_text_short_string_unchanged() {
        assert_eq!(preview_text("hello world", 96), "hello world");
    }

    #[test]
    fn preview_text_replaces_newlines_and_tabs() {
        assert_eq!(preview_text("a\nb\tc", 96), "a b c");
    }

    #[test]
    fn preview_text_truncates_long_string() {
        let long = "x".repeat(200);
        let result = preview_text(&long, 10);
        assert_eq!(result, format!("{}...", "x".repeat(10)));
    }

    #[test]
    fn preview_text_trims_result() {
        assert_eq!(preview_text("  hi  ", 96), "hi");
    }

    #[test]
    fn preview_text_empty_string() {
        assert_eq!(preview_text("", 96), "");
    }

    // ── is_likely_list_marker ────────────────────────────────────────

    #[test]
    fn is_likely_list_marker_dash() {
        assert!(is_likely_list_marker("- item"));
    }

    #[test]
    fn is_likely_list_marker_asterisk() {
        assert!(is_likely_list_marker("* item"));
    }

    #[test]
    fn is_likely_list_marker_plus() {
        assert!(is_likely_list_marker("+ item"));
    }

    #[test]
    fn is_likely_list_marker_numbered() {
        assert!(is_likely_list_marker("1. first"));
        assert!(is_likely_list_marker("12. twelfth"));
    }

    #[test]
    fn is_likely_list_marker_not_a_list() {
        assert!(!is_likely_list_marker("hello world"));
        assert!(!is_likely_list_marker(""));
        assert!(!is_likely_list_marker("-nospace"));
    }

    #[test]
    fn is_likely_list_marker_number_without_dot_space() {
        assert!(!is_likely_list_marker("1-not-a-list"));
        assert!(!is_likely_list_marker("1.nospace"));
    }

    // ── is_likely_table_row ──────────────────────────────────────────

    #[test]
    fn is_likely_table_row_standard() {
        assert!(is_likely_table_row("| a | b |"));
    }

    #[test]
    fn is_likely_table_row_single_cell_returns_false() {
        // Only one non-empty cell should return false (needs >= 2)
        assert!(!is_likely_table_row("| a |"));
    }

    #[test]
    fn is_likely_table_row_empty_returns_false() {
        assert!(!is_likely_table_row(""));
    }

    #[test]
    fn is_likely_table_row_no_pipe_returns_false() {
        assert!(!is_likely_table_row("just text"));
    }

    // ── is_probably_json_content ─────────────────────────────────────

    #[test]
    fn is_probably_json_object() {
        assert!(is_probably_json_content("{\"key\": \"value\"}"));
    }

    #[test]
    fn is_probably_json_array() {
        assert!(is_probably_json_content("[1, 2, 3]"));
    }

    #[test]
    fn is_probably_json_fenced_block() {
        assert!(is_probably_json_content("here is some ```json\n{}\n```"));
    }

    #[test]
    fn is_probably_json_plain_text() {
        assert!(!is_probably_json_content("just some text"));
    }

    #[test]
    fn is_probably_json_with_whitespace() {
        assert!(is_probably_json_content("  { \"a\": 1 }  "));
    }

    // ── MarkdownRenderProfile ────────────────────────────────────────

    #[test]
    fn profile_counts_markdown_features() {
        let content = "# Heading\n\n- item\n- item2\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n> quote\n\n[link](url)\n";
        let profile = MarkdownRenderProfile::from_content(content);

        assert_eq!(profile.heading_count, 1);
        assert_eq!(profile.list_marker_count, 2);
        assert_eq!(profile.code_fence_count, 2); // open + close
        assert!(profile.table_row_count >= 2); // header + data row at minimum
        assert_eq!(profile.quote_line_count, 1);
        assert_eq!(profile.link_count, 1);
        assert!(!profile.is_probably_json);
    }

    #[test]
    fn profile_detects_json() {
        let profile = MarkdownRenderProfile::from_content("{\"key\": \"value\"}");
        assert!(profile.is_probably_json);
    }

    #[test]
    fn render_options_remap_text_and_txt_to_shell() {
        assert_eq!(RENDER_OPTIONS.default_code_language, Some("sh"));
        assert_eq!(
            RENDER_OPTIONS.code_language_overrides,
            &[("text", "sh"), ("txt", "sh")]
        );
    }

    #[test]
    fn shell_highlighter_colors_rcc_usage_block() {
        let ctx = egui::Context::default();
        let style = egui::Style::default();
        let theme = CodeTheme::from_style(&style);
        let content = "Usage: rcc [codex|claude|minimax|minimaxforge|qwen|gemini|geminiforge|geminicliforge|omniforge] [--instance N] [--cwd DIR]\nAlso accepts: rcc [--codex|--claude|--minimax|--minimaxforge|--qwen|--gemini|--geminiforge|--geminicliforge|--omniforge] ...";
        let job = syntax_highlighting::highlight(&ctx, &style, &theme, content, "sh");
        let unique_colors: std::collections::HashSet<_> = job
            .sections
            .iter()
            .map(|section| section.format.color)
            .collect();

        assert!(
            unique_colors.len() > 1,
            "expected shell highlighting to produce more than one text color for the sample usage block, got {unique_colors:?}"
        );
    }
}
