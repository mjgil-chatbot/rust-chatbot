use std::borrow::Cow;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StandardTranscriptSplit {
    pub transcript: String,
    pub markdown_body: String,
    pub line_count: usize,
}
pub(crate) fn split_standard_transcript_and_markdown(
    content: &str,
) -> Option<StandardTranscriptSplit> {
    let lines: Vec<&str> = content.split('\n').collect();
    let transcript_line_count = leading_standard_transcript_line_count(content);

    if transcript_line_count == 0 || transcript_line_count >= lines.len() {
        return None;
    }

    let markdown_body = lines[transcript_line_count..]
        .join("\n")
        .trim_start_matches(['\r', '\n'])
        .to_string();
    if markdown_body.trim().is_empty() {
        return None;
    }

    Some(StandardTranscriptSplit {
        transcript: lines[..transcript_line_count].join("\n"),
        markdown_body,
        line_count: transcript_line_count,
    })
}
pub(crate) fn hard_break_standard_transcript_lines(content: &str) -> Cow<'_, str> {
    let transcript_lines = leading_standard_transcript_line_count(content);

    if transcript_lines == 0 {
        return Cow::Borrowed(content);
    }

    let mut normalized = String::with_capacity(content.len() + transcript_lines * 6);
    for (index, line) in content.split('\n').enumerate() {
        if index > 0 {
            normalized.push('\n');
        }
        if index < transcript_lines {
            normalized.push_str(&escape_transcript_markdown_line(line));
        } else {
            normalized.push_str(line);
        }
        if index < transcript_lines {
            normalized.push_str("  ");
        }
    }
    Cow::Owned(normalized)
}
fn leading_standard_transcript_line_count(content: &str) -> usize {
    content
        .lines()
        .take_while(|line| {
            let trimmed = line.trim_start();
            !trimmed.is_empty() && is_standard_markdown_transcript_line(trimmed)
        })
        .count()
}
fn escape_transcript_markdown_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut escaped = String::with_capacity(line.len() + 8);
    let mut open_ticks: Option<usize> = None;
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '`' {
            let start = index;
            while index < chars.len() && chars[index] == '`' {
                escaped.push(chars[index]);
                index += 1;
            }
            let run_len = index - start;
            match open_ticks {
                Some(open_len) if open_len == run_len => open_ticks = None,
                None => open_ticks = Some(run_len),
                _ => {}
            }
            continue;
        }

        let ch = chars[index];
        if open_ticks.is_none() && matches!(ch, '\\' | '*' | '_' | '<' | '>') {
            escaped.push('\\');
        }
        escaped.push(ch);
        index += 1;
    }

    escaped
}
fn is_standard_markdown_transcript_line(line: &str) -> bool {
    let stripped = strip_standard_transcript_timestamp_prefix(line);
    if stripped.len() != line.len() {
        return !stripped.is_empty();
    }
    if let Some(model) = stripped.strip_suffix(" agent started") {
        return looks_like_standard_model_label(model.trim_end());
    }
    if let Some((model, _detail)) = stripped.split_once(" | ") {
        return looks_like_standard_model_label(model.trim());
    }
    false
}
fn strip_standard_transcript_timestamp_prefix(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.len() < 16 || bytes[0] != b'[' {
        return line;
    }
    if !bytes[1].is_ascii_alphabetic()
        || !bytes[2].is_ascii_alphabetic()
        || !bytes[3].is_ascii_alphabetic()
    {
        return line;
    }
    if bytes[4] != b' ' {
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
    {
        return line;
    }
    if bytes[index + 8] != b']' || index + 9 >= bytes.len() || bytes[index + 9] != b' ' {
        return line;
    }
    &line[index + 10..]
}
fn looks_like_standard_model_label(label: &str) -> bool {
    if label.is_empty()
        || label.len() > 24
        || !label
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '.' | '-' | '+'))
    {
        return false;
    }

    let lower = label.to_ascii_lowercase();
    label.chars().any(|ch| ch.is_ascii_digit())
        || [
            "gpt", "gemini", "sonnet", "haiku", "opus", "claude", "minimax", "kimi", "codex",
            "qwen", "deepseek", "mistral", "llama",
        ]
        .iter()
        .any(|token| lower.contains(token))
}
