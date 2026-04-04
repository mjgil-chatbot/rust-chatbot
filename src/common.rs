use std::borrow::Cow;
use crate::data::ChatSession;
use crate::model::ModelProvider;
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
