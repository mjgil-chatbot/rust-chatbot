//! Message rendering UI components
//!
//! This module handles rendering individual chat messages, including
//! markdown formatting for assistant responses and copy functionality.

use eframe::egui;
use egui_extras::syntax_highlighting::{self, CodeTheme};
use std::borrow::Cow;
use std::collections::HashMap;
use std::time::Instant;
use crate::app::ui::transcript_summary::{
    hard_break_standard_transcript_lines, split_standard_transcript_and_markdown,
};
use crate::data::ChatMessage;
use crate::forge_loop;
use crate::markdown::{self, MarkdownCache};
/// Render a single chat message.
///
/// The message group fills the available width.  Markdown content inside
/// Assistant responses are rendered through `render_markdown`, which uses a
/// hard-clipping child UI so code blocks / tables can never push the
/// layout wider.
pub fn render_message(
    ui: &mut egui::Ui,
    message: &ChatMessage,
    previous_message: Option<&ChatMessage>,
    markdown_cache: &mut MarkdownCache,
    copy_feedback: &mut HashMap<i64, std::time::Instant>,
) -> egui::Rect {
    let response = ui.group(|ui| {
        // IMPORTANT: measure available_width() INSIDE the group callback.
        // The Frame::group() already subtracted its inner_margin (6px each
        // side = 12px total) from the parent’s available rect to produce
        // this child UI’s max_rect.  Using the outer width here would
        // expand the inner UI beyond what the frame can contain, causing
        // the group’s outer rect to overflow.
        let inner_w = ui.available_width();
        ui.set_width(inner_w);
        ui.set_min_width(inner_w);
        ui.set_max_width(inner_w);

        ui.horizontal(|ui| {
            if message.is_user {
                ui.label("🧑 You:");
            } else {
                ui.label("🤖 Assistant:");
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Copy button with feedback
                let msg_id = message.timestamp.timestamp_millis();
                let show_checkmark = match copy_feedback.get(&msg_id) {
                    Some(t) if t.elapsed().as_secs_f32() < 2.0 => true,
                    Some(_) => {
                        copy_feedback.remove(&msg_id);
                        false
                    }
                    None => false,
                };

                let button_text = if show_checkmark {
                    "✓ Copied"
                } else {
                    "📋 Copy"
                };
                if ui.small_button(button_text).clicked() {
                    ui.ctx().copy_text(message.content.clone());
                    copy_feedback.insert(msg_id, std::time::Instant::now());
                }
                ui.label(
                    message
                        .timestamp
                        .with_timezone(&chrono::Local)
                        .format("%b %-d %I:%M%P")
                        .to_string(),
                );
            });
        });

        ui.separator();

        let base_content = base_markdown_content(&message.content);

        if message.is_user {
            // User messages: plain text with color
            let color = egui::Color32::from_rgb(70, 130, 180);
            ui.add(
                egui::Label::new(egui::RichText::new(&message.content).color(color))
                    .wrap_mode(egui::TextWrapMode::Wrap),
            );
        } else {
            if let Some(retrieved_memory) =
                assistant_retrieved_memory_preview(message, previous_message)
            {
                render_retrieved_memory_summary(
                    ui,
                    markdown_cache,
                    message,
                    inner_w,
                    retrieved_memory,
                );
                ui.add_space(6.0);
            }

            if let Some(split) = split_standard_transcript_and_markdown(base_content.as_ref()) {
                render_collapsed_transcript_summary(ui, markdown_cache, message, inner_w, &split);
                ui.add_space(6.0);
                render_assistant_markdown(
                    ui,
                    markdown_cache,
                    message,
                    inner_w,
                    &split.markdown_body,
                );
            } else if let Some(split_stream) = split_transcript_and_markdown(base_content.as_ref())
            {
                if let Some(preamble) = split_stream.plaintext_preamble.as_deref() {
                    render_plaintext_stream(ui, preamble);
                    if split_stream.markdown_body.is_some() {
                        ui.add_space(6.0);
                    }
                }
                if let Some(markdown_body) = split_stream.markdown_body.as_deref() {
                    render_assistant_markdown(ui, markdown_cache, message, inner_w, markdown_body);
                }
            } else if should_render_as_plaintext_stream(base_content.as_ref()) {
                render_plaintext_stream(ui, base_content.as_ref());
            } else if let Some(sanitized_stream_markdown) =
                sanitized_stream_markdown_content(base_content.as_ref())
            {
                render_assistant_markdown(
                    ui,
                    markdown_cache,
                    message,
                    inner_w,
                    &sanitized_stream_markdown,
                );
            } else {
                let normalized = normalized_markdown_content(base_content.as_ref());
                render_assistant_markdown(
                    ui,
                    markdown_cache,
                    message,
                    inner_w,
                    normalized.as_ref(),
                );
            }
        }
    });
    response.response.rect
}
fn assistant_retrieved_memory_preview<'a>(
    message: &ChatMessage,
    previous_message: Option<&'a ChatMessage>,
) -> Option<&'a str> {
    (!message.is_user)
        .then_some(previous_message)
        .flatten()
        .filter(|previous| previous.is_user)
        .and_then(|previous| previous.retrieved_memory.as_deref())
        .filter(|retrieved_memory| !retrieved_memory.trim().is_empty())
}
fn should_render_as_plaintext_stream(content: &str) -> bool {
    let sanitized = forge_loop::sanitize_stream_plaintext(content);
    forge_loop::is_forge_plaintext_response(&sanitized)
}
fn split_transcript_and_markdown(content: &str) -> Option<StreamRenderSplit> {
    let sanitized = forge_loop::sanitize_stream_plaintext(content);
    if sanitized.is_empty() || !forge_loop::is_forge_plaintext_response(&sanitized) {
        return None;
    }

    let lines: Vec<&str> = sanitized.lines().collect();
    let last_transcript_idx = lines
        .iter()
        .rposition(|line| is_transcript_status_line(line.trim_start()))?;
    let body_lines = &lines[last_transcript_idx + 1..];
    if body_lines.is_empty() {
        return None;
    }

    let body = body_lines.join("\n");
    if !looks_like_markdown_body(&body) {
        return None;
    }

    let plaintext_preamble = (!lines[..=last_transcript_idx].is_empty())
        .then(|| lines[..=last_transcript_idx].join("\n"));
    Some(StreamRenderSplit {
        plaintext_preamble,
        markdown_body: Some(body),
    })
}
fn sanitized_stream_markdown_content(content: &str) -> Option<String> {
    let sanitized = forge_loop::sanitize_stream_plaintext(content);
    if sanitized.is_empty() || sanitized == content {
        return None;
    }
    (!forge_loop::is_forge_plaintext_response(&sanitized)).then_some(sanitized)
}
fn render_plaintext_stream(ui: &mut egui::Ui, content: &str) {
    for block in prepare_plaintext_render_blocks(content) {
        match block {
            PlaintextRenderBlock::Text(text) => render_plaintext_block(ui, &text),
            PlaintextRenderBlock::Code { language, code } => {
                render_highlighted_code_block(ui, Some(&language), &code)
            }
        }
    }
}
fn render_assistant_markdown(
    ui: &mut egui::Ui,
    markdown_cache: &mut MarkdownCache,
    message: &ChatMessage,
    inner_w: f32,
    markdown_content: &str,
) {
    let width_key = inner_w.round().max(0.0) as i64;
    let id_salt = format!(
        "msg_{}_w{}_len{}",
        message.timestamp.timestamp_millis(),
        width_key,
        markdown_content.len()
    );
    if let Some(pretty_json) = format_standalone_json_message(markdown_content) {
        let fenced_json = format!("```json\n{}\n```", pretty_json);
        let render_start = Instant::now();
        markdown::render_markdown(ui, markdown_cache, &fenced_json, &id_salt);
        markdown::log_markdown_render_timing(
            &id_salt,
            &fenced_json,
            inner_w,
            render_start.elapsed(),
        );
    } else {
        let render_start = Instant::now();
        let escaped = escape_underscores_in_tool_names(markdown_content);
        markdown::render_markdown(ui, markdown_cache, &escaped, &id_salt);
        markdown::log_markdown_render_timing(
            &id_salt,
            markdown_content,
            inner_w,
            render_start.elapsed(),
        );
    }
}
fn render_collapsed_transcript_summary(
    ui: &mut egui::Ui,
    markdown_cache: &mut MarkdownCache,
    message: &ChatMessage,
    inner_w: f32,
    transcript: &crate::app::ui::transcript_summary::StandardTranscriptSplit,
) {
    let line_label = if transcript.line_count == 1 {
        "1 line".to_string()
    } else {
        format!("{} lines", transcript.line_count)
    };
    egui::CollapsingHeader::new(format!(
        "Tool calls and intermediate responses ({line_label})"
    ))
    .id_salt((
        "intermediate-activity",
        message.timestamp.timestamp_millis(),
    ))
    .default_open(false)
    .show(ui, |ui| {
        let normalized = normalized_markdown_content(&transcript.transcript);
        render_assistant_markdown(ui, markdown_cache, message, inner_w, normalized.as_ref());
    });
}
fn render_retrieved_memory_summary(
    ui: &mut egui::Ui,
    markdown_cache: &mut MarkdownCache,
    message: &ChatMessage,
    inner_w: f32,
    retrieved_memory: &str,
) {
    let item_label = retrieved_memory_item_label(retrieved_memory);
    egui::CollapsingHeader::new(format!("Retrieved memory ({item_label})"))
        .id_salt(("retrieved-memory", message.timestamp.timestamp_millis()))
        .default_open(false)
        .show(ui, |ui| {
            render_assistant_markdown(ui, markdown_cache, message, inner_w, retrieved_memory);
        });
}
fn retrieved_memory_item_label(retrieved_memory: &str) -> String {
    let item_count = retrieved_memory
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .count();
    if item_count == 1 {
        "1 item".to_string()
    } else {
        format!("{item_count} items")
    }
}
fn render_plaintext_block(ui: &mut egui::Ui, text: &str) {
    if text.is_empty() {
        return;
    }
    for line in plaintext_lines_for_render(text) {
        if line.is_empty() {
            ui.add_space(4.0);
            continue;
        }
        let monospace = egui::RichText::new(line).monospace();
        ui.add(egui::Label::new(monospace).wrap_mode(egui::TextWrapMode::Wrap));
    }
}
fn render_highlighted_code_block(ui: &mut egui::Ui, language: Option<&str>, code: &str) {
    let language = normalize_code_language(language);
    let theme = CodeTheme::from_style(ui.style().as_ref());
    egui::Frame::none()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
        .show(ui, |ui| {
            for line in code.split('\n') {
                let highlighted = syntax_highlighting::highlight(
                    ui.ctx(),
                    ui.style().as_ref(),
                    &theme,
                    line,
                    language,
                );
                ui.add(egui::Label::new(highlighted).wrap_mode(egui::TextWrapMode::Wrap));
            }
        });
}
fn normalize_code_language(language: Option<&str>) -> &str {
    match language.map(str::trim).filter(|lang| !lang.is_empty()) {
        Some("text" | "txt") | None => "sh",
        Some(lang) => lang,
    }
}
fn base_markdown_content(content: &str) -> Cow<'_, str> {
    let trimmed = content.trim_start();
    let without_codex_prefix = if let Some(rest) = trimmed.strip_prefix("Codex: turn complete") {
        let rest = rest.trim_start_matches(['\r', '\n', ' ']);
        if rest.is_empty() {
            content
        } else {
            rest
        }
    } else {
        content
    };

    if let Some((preamble, separator, body)) =
        split_leading_transcript_preamble_for_render(without_codex_prefix)
    {
        if let Some(stripped_body) = strip_leading_think_block(body) {
            if stripped_body.is_empty() {
                return Cow::Owned(preamble.trim_end_matches(['\r', '\n']).to_string());
            }
            return Cow::Owned(format!("{preamble}{separator}{stripped_body}"));
        }
        return Cow::Borrowed(without_codex_prefix);
    }

    strip_leading_think_block(without_codex_prefix)
        .map(Cow::Owned)
        .unwrap_or_else(|| Cow::Borrowed(without_codex_prefix))
}
fn is_transcript_status_line(line: &str) -> bool {
    line.starts_with("● [")
        || line.starts_with("○ [")
        || line.starts_with("◦ [")
        || line.starts_with("• [")
}
fn looks_like_markdown_body(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('#')
            || trimmed.starts_with("```")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("> ")
            || trimmed.starts_with("| ")
            || trimmed.starts_with("1. ")
            || trimmed.starts_with("2. ")
            || trimmed.starts_with("3. ")
    })
}
struct StreamRenderSplit {
    plaintext_preamble: Option<String>,
    markdown_body: Option<String>,
}
enum PlaintextBlock {
    Text(String),
    Code {
        language: Option<String>,
        code: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum PlaintextRenderBlock {
    Text(String),
    Code { language: String, code: String },
}
impl PlaintextBlock {
    fn parse(content: &str) -> Vec<Self> {
        let mut blocks = Vec::new();
        let mut text = String::new();
        let mut in_code = false;
        let mut code = String::new();
        let mut language = None;

        for line in content.split_inclusive('\n') {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("```") {
                if in_code {
                    if !code.is_empty() {
                        blocks.push(Self::Code {
                            language: language.take(),
                            code: std::mem::take(&mut code),
                        });
                    }
                    in_code = false;
                } else {
                    if !text.is_empty() {
                        blocks.push(Self::Text(std::mem::take(&mut text)));
                    }
                    let lang = rest.trim();
                    language = (!lang.is_empty()).then(|| lang.to_string());
                    in_code = true;
                }
                continue;
            }

            if in_code {
                code.push_str(line);
            } else {
                text.push_str(line);
            }
        }

        if !code.is_empty() {
            blocks.push(Self::Code { language, code });
        } else if in_code {
            text.push_str("```");
        }

        if !text.is_empty() {
            blocks.push(Self::Text(text));
        }

        blocks
    }}
fn prepare_plaintext_render_blocks(content: &str) -> Vec<PlaintextRenderBlock> {
    let sanitized = forge_loop::sanitize_stream_plaintext(content);
    let display = if sanitized.is_empty() {
        content
    } else {
        &sanitized
    };

    PlaintextBlock::parse(display)
        .into_iter()
        .map(|block| match block {
            PlaintextBlock::Text(text) => PlaintextRenderBlock::Text(text),
            PlaintextBlock::Code { language, code } => PlaintextRenderBlock::Code {
                language: normalize_code_language(language.as_deref()).to_string(),
                code,
            },
        })
        .collect()
}
fn plaintext_lines_for_render(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines
}
/// Render the processing indicator
pub fn render_processing_indicator(ui: &mut egui::Ui) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label("🤖 Assistant:");
            ui.spinner();
            ui.label("Thinking...");
        });
    });
}
#[cfg_attr(not(test), allow(dead_code))]
fn is_standalone_json_message(content: &str) -> bool {
    let trimmed = content.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return false;
    }

    serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
}
fn format_standalone_json_message(content: &str) -> Option<String> {
    let trimmed = content.trim();
    let parsed = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    serde_json::to_string_pretty(&parsed).ok()
}
/// Escape double underscores to prevent them being parsed as markdown emphasis.
///
/// Tool names like `mcp__bash__run` contain `__` separators which get interpreted
/// as emphasis markers (__bash__) when rendered as markdown.
fn escape_underscores_in_tool_names(content: &str) -> String {
    content.replace("__", "\\_\\_")
}
fn normalized_markdown_content(content: &str) -> Cow<'_, str> {
    let base = base_markdown_content(content);
    match hard_break_standard_transcript_lines(base.as_ref()) {
        Cow::Borrowed(_) => base,
        Cow::Owned(normalized) => Cow::Owned(normalized),
    }
}
fn split_leading_transcript_preamble_for_render(response: &str) -> Option<(&str, &str, &str)> {
    let mut preamble_end = 0usize;
    let mut transcript_lines = 0usize;

    for segment in response.split_inclusive('\n') {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let trimmed = line.trim_end_matches('\r').trim_start();
        if trimmed.is_empty() || !is_transcript_activity_line_for_render(trimmed) {
            break;
        }
        preamble_end += segment.len();
        transcript_lines += 1;
    }

    if transcript_lines == 0 || preamble_end >= response.len() {
        return None;
    }

    let mut body_start = preamble_end;
    while let Some(ch) = response[body_start..].chars().next() {
        if matches!(ch, '\n' | '\r') {
            body_start += ch.len_utf8();
        } else {
            break;
        }
    }

    if body_start >= response.len() {
        return None;
    }

    Some((
        &response[..preamble_end],
        &response[preamble_end..body_start],
        &response[body_start..],
    ))
}
fn is_transcript_activity_line_for_render(line: &str) -> bool {
    line.starts_with("● [")
        || line.starts_with("○ [")
        || line.starts_with("◦ [")
        || line.starts_with("• [")
        || strip_standard_transcript_timestamp_prefix_for_render(line).len() != line.len()
}
fn strip_standard_transcript_timestamp_prefix_for_render(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.len() < 16 || bytes[0] != b'[' {
        return line;
    }
    if !bytes[1].is_ascii_alphabetic()
        || !bytes[2].is_ascii_alphabetic()
        || !bytes[3].is_ascii_alphabetic()
        || bytes[4] != b' '
    {
        return line;
    }

    let mut index = 5usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 5 || index + 9 > bytes.len() || bytes[index] != b' ' {
        return line;
    }
    if !bytes[index + 1].is_ascii_digit()
        || !bytes[index + 2].is_ascii_digit()
        || bytes[index + 3] != b':'
        || !bytes[index + 4].is_ascii_digit()
        || !bytes[index + 5].is_ascii_digit()
        || !bytes[index + 6].is_ascii_lowercase()
        || !bytes[index + 7].is_ascii_lowercase()
        || bytes[index + 8] != b']'
        || index + 9 >= bytes.len()
        || bytes[index + 9] != b' '
    {
        return line;
    }

    &line[index + 10..]
}
fn strip_leading_think_block(content: &str) -> Option<String> {
    let trimmed = content.trim_start();
    let think_open = "<think>";
    let think_close = "</think>";
    if !trimmed.starts_with(think_open) {
        return None;
    }

    let close_index = trimmed.find(think_close)?;
    let remainder =
        trimmed[close_index + think_close.len()..].trim_start_matches(['\r', '\n', ' ']);
    Some(remainder.to_string())
}
