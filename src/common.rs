use std::borrow::Cow;

use crate::data::ChatSession;
use crate::model::ModelProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum Alignment {
    Left,
    Center,
    Right,
}

pub fn fence_delimiter(line: &str) -> Option<char> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") {
        Some('`')
    } else if trimmed.starts_with("~~~") {
        Some('~')
    } else {
        None
    }
}

#[allow(dead_code)]
pub fn normalize_language_tag(lang: &str) -> Cow<'_, str> {
    if lang.bytes().all(|b| !b.is_ascii_uppercase()) {
        return Cow::Borrowed(lang);
    }
    Cow::Owned(lang.to_ascii_lowercase())
}

pub fn split_table_row(line: &str) -> Option<Vec<&str>> {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return None;
    }

    let trimmed = trimmed
        .strip_prefix('|')
        .unwrap_or(trimmed)
        .strip_suffix('|')
        .unwrap_or(trimmed);

    let cells = split_on_unquoted_pipe(trimmed);
    if cells.is_empty() {
        None
    } else {
        Some(cells)
    }
}

pub fn worker_request_model_override(
    provider: ModelProvider,
    session: Option<&ChatSession>,
) -> Option<String> {
    if provider != ModelProvider::OmniForge {
        return None;
    }

    Some(crate::omniforge::selected_model(session).model.to_string())
}

pub(crate) fn split_on_unquoted_pipe(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut cells = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'`' {
            let tick_start = i;
            while i < bytes.len() && bytes[i] == b'`' {
                i += 1;
            }
            let tick_len = i - tick_start;
            loop {
                if i >= bytes.len() {
                    break;
                }
                if bytes[i] == b'`' {
                    let close_start = i;
                    while i < bytes.len() && bytes[i] == b'`' {
                        i += 1;
                    }
                    if i - close_start == tick_len {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if bytes[i] == b'|' {
            cells.push(s[start..i].trim());
            start = i + 1;
            i += 1;
        } else {
            i += 1;
        }
    }

    cells.push(s[start..].trim());
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── fence_delimiter ──────────────────────────────────────────────

    #[test]
    fn fence_delimiter_backtick() {
        assert_eq!(fence_delimiter("```rust"), Some('`'));
    }

    #[test]
    fn fence_delimiter_tilde() {
        assert_eq!(fence_delimiter("~~~"), Some('~'));
    }

    #[test]
    fn fence_delimiter_with_leading_spaces() {
        assert_eq!(fence_delimiter("   ```"), Some('`'));
    }

    #[test]
    fn fence_delimiter_plain_text_returns_none() {
        assert_eq!(fence_delimiter("hello world"), None);
    }

    #[test]
    fn fence_delimiter_only_two_backticks_returns_none() {
        assert_eq!(fence_delimiter("``not a fence"), None);
    }

    #[test]
    fn fence_delimiter_empty_string() {
        assert_eq!(fence_delimiter(""), None);
    }

    // ── normalize_language_tag ────────────────────────────────────────

    #[test]
    fn normalize_language_tag_already_lowercase() {
        let result = normalize_language_tag("rust");
        assert_eq!(result, "rust");
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_language_tag_mixed_case() {
        let result = normalize_language_tag("Rust");
        assert_eq!(result, "rust");
        assert!(matches!(result, std::borrow::Cow::Owned(_)));
    }

    #[test]
    fn normalize_language_tag_all_upper() {
        assert_eq!(normalize_language_tag("JSON"), "json");
    }

    #[test]
    fn normalize_language_tag_empty() {
        assert_eq!(normalize_language_tag(""), "");
    }

    // ── split_table_row ──────────────────────────────────────────────

    #[test]
    fn split_table_row_standard_row() {
        let cells = split_table_row("| a | b | c |").unwrap();
        assert_eq!(cells, vec!["a", "b", "c"]);
    }

    #[test]
    fn split_table_row_no_outer_pipes() {
        let cells = split_table_row("a | b | c").unwrap();
        assert_eq!(cells, vec!["a", "b", "c"]);
    }

    #[test]
    fn split_table_row_no_pipe_returns_none() {
        assert!(split_table_row("no pipes here").is_none());
    }

    #[test]
    fn split_table_row_preserves_backtick_content() {
        let cells = split_table_row("| `a|b` | c |").unwrap();
        assert_eq!(cells, vec!["`a|b`", "c"]);
    }

    #[test]
    fn split_table_row_double_backtick_span() {
        let cells = split_table_row("| ``a|b`` | c |").unwrap();
        assert_eq!(cells, vec!["``a|b``", "c"]);
    }

    #[test]
    fn split_table_row_trims_whitespace() {
        let cells = split_table_row("|  hello  |  world  |").unwrap();
        assert_eq!(cells, vec!["hello", "world"]);
    }

    // ── split_on_unquoted_pipe ───────────────────────────────────────

    #[test]
    fn split_on_unquoted_pipe_simple() {
        let cells = split_on_unquoted_pipe("a | b | c");
        assert_eq!(cells, vec!["a", "b", "c"]);
    }

    #[test]
    fn split_on_unquoted_pipe_backtick_protects_pipe() {
        let cells = split_on_unquoted_pipe("`x|y` | z");
        assert_eq!(cells, vec!["`x|y`", "z"]);
    }

    #[test]
    fn split_on_unquoted_pipe_unclosed_backtick() {
        // Unclosed backtick: pipe after it should still be protected
        let cells = split_on_unquoted_pipe("`a|b");
        assert_eq!(cells, vec!["`a|b"]);
    }

    #[test]
    fn worker_request_model_override_uses_selected_omniforge_model() {
        let session = crate::data::ChatSession {
            id: "omni".to_string(),
            name: "omni".to_string(),
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
            omniforge_model_id: Some("minimax-m2.7".to_string()),
            kimi_session_id: None,
            opencode_session_id: None,
            hermes_session_id: None,
            forge_session_id: None,
            claw_session_id: None,
            launched_from: None,
            runsc_dirs: Vec::new(),
            runsc_image: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            messages: Vec::new(),
        };

        assert_eq!(
            worker_request_model_override(crate::model::ModelProvider::OmniForge, Some(&session))
                .as_deref(),
            Some("MiniMax-M2.7")
        );
        assert_eq!(
            worker_request_model_override(crate::model::ModelProvider::Claude, Some(&session)),
            None
        );
    }
}
