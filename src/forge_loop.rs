pub(crate) fn extract_continue_id(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(extract_continue_id_from_line)
        .map(str::to_string)
}

pub(crate) fn is_forge_plaintext_response(content: &str) -> bool {
    let content = sanitize_stream_plaintext(content);
    content
        .lines()
        .any(|line| is_transcript_status_line(line.trim_start()))
}

pub(crate) fn sanitize_stream_plaintext(content: &str) -> String {
    let mut sanitized_lines = Vec::new();

    for raw_line in split_control_lines(content) {
        let stripped = strip_ansi_sequences(raw_line);
        let normalized = normalize_plaintext_line(trim_stream_prefix_artifacts(stripped.trim()));
        if normalized.is_empty() || is_spinner_status_line(&normalized) {
            continue;
        }
        sanitized_lines.push(normalized);
    }

    sanitized_lines.join("\n")
}

pub(crate) fn transcript_preamble(content: &str) -> Option<String> {
    let sanitized = sanitize_stream_plaintext(content);
    let mut transcript_lines = Vec::new();
    for line in sanitized.lines() {
        let trimmed = line.trim_start();
        if !is_transcript_status_line(trimmed) {
            break;
        }
        transcript_lines.push(trimmed.to_string());
    }
    (!transcript_lines.is_empty()).then(|| transcript_lines.join("\n"))
}

fn extract_continue_id_from_line(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let marker = "] Continue ";
    let suffix = trimmed.split_once(marker)?.1;
    let continue_id = suffix.split_whitespace().next()?;
    is_probable_continue_id(continue_id).then_some(continue_id)
}

fn is_probable_continue_id(value: &str) -> bool {
    value.len() >= 8 && value.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
}

fn is_transcript_status_line(line: &str) -> bool {
    line.starts_with("● [")
        || line.starts_with("○ [")
        || line.starts_with("◦ [")
        || line.starts_with("• [")
}

fn split_control_lines(content: &str) -> impl Iterator<Item = &str> {
    content.split(['\n', '\r'])
}

fn is_spinner_status_line(line: &str) -> bool {
    line.contains("Ctrl+C to interrupt")
        || line.starts_with("Migrating credentials ")
        || line.starts_with("Analyzing ")
}

fn trim_stream_prefix_artifacts(line: &str) -> &str {
    let trimmed = line.trim_start();
    let Some(first) = trimmed.chars().next() else {
        return trimmed;
    };
    if matches!(first, '\u{2800}'..='\u{28ff}' | '\u{fffd}' | '□')
        && trimmed[first.len_utf8()..].starts_with(' ')
    {
        trimmed[first.len_utf8()..].trim_start()
    } else {
        trimmed
    }
}

fn normalize_plaintext_line(line: &str) -> String {
    let mut normalized = String::with_capacity(line.len());
    for ch in line.chars() {
        if ('\u{E000}'..='\u{F8FF}').contains(&ch) {
            continue;
        }
        normalized.push(map_plaintext_glyph(ch));
    }

    let trimmed = normalized.trim();
    if !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '-') {
        "---".to_string()
    } else {
        normalized
    }
}

fn map_plaintext_glyph(ch: char) -> char {
    match ch {
        '─' | '━' | '┄' | '┅' | '┈' | '┉' | '╌' | '╍' | '═' => '-',
        '│' | '┃' | '┆' | '┇' | '┊' | '┋' | '╎' | '╏' | '║' => '|',
        '┌' | '┍' | '┎' | '┏' | '┐' | '┑' | '┒' | '┓' | '└' | '┕' | '┖' | '┗' | '┘' | '┙' | '┚'
        | '┛' | '├' | '┝' | '┞' | '┟' | '┠' | '┡' | '┢' | '┣' | '┤' | '┥' | '┦' | '┧' | '┨'
        | '┩' | '┪' | '┫' | '┬' | '┭' | '┮' | '┯' | '┰' | '┱' | '┲' | '┳' | '┴' | '┵' | '┶'
        | '┷' | '┸' | '┹' | '┺' | '┻' | '┼' | '┽' | '┾' | '┿' | '╀' | '╁' | '╂' | '╃' | '╄'
        | '╅' | '╆' | '╇' | '╈' | '╉' | '╊' | '╋' | '╒' | '╓' | '╔' | '╕' | '╖' | '╗' | '╘'
        | '╙' | '╚' | '╛' | '╜' | '╝' | '╞' | '╟' | '╠' | '╡' | '╢' | '╣' | '╤' | '╥' | '╦'
        | '╧' | '╨' | '╩' | '╪' | '╫' | '╬' => '+',
        _ => ch,
    }
}

fn strip_ansi_sequences(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i >= bytes.len() {
                break;
            }

            match bytes[i] {
                b'[' => {
                    i += 1;
                    while i < bytes.len() {
                        let byte = bytes[i];
                        i += 1;
                        if (0x40..=0x7e).contains(&byte) {
                            break;
                        }
                    }
                }
                b']' => {
                    i += 1;
                    while i < bytes.len() {
                        let byte = bytes[i];
                        i += 1;
                        if byte == 0x07 {
                            break;
                        }
                        if byte == 0x1b && i < bytes.len() && bytes[i] == b'\\' {
                            i += 1;
                            break;
                        }
                    }
                }
                _ => {
                    i += 1;
                }
            }
            continue;
        }

        if let Some(ch) = content[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{
        extract_continue_id, is_forge_plaintext_response, sanitize_stream_plaintext,
        transcript_preamble,
    };

    #[test]
    fn extracts_continue_id_from_forge_status_line() {
        let content = "● [00:52:32] Continue 9c58fe00-d47b-4b45-8faa-d2d908100bce";
        assert_eq!(
            extract_continue_id(content).as_deref(),
            Some("9c58fe00-d47b-4b45-8faa-d2d908100bce")
        );
    }

    #[test]
    fn ignores_non_continue_lines() {
        assert_eq!(extract_continue_id("plain assistant reply"), None);
    }

    #[test]
    fn detects_plaintext_forge_transcript_lines() {
        assert!(is_forge_plaintext_response(
            "● [00:52:45] Execute [/bin/bash] ls -la crates/"
        ));
    }

    #[test]
    fn ignores_regular_markdown_messages() {
        assert!(!is_forge_plaintext_response("## Heading\n\n- item"));
    }

    #[test]
    fn strips_ansi_and_spinner_lines_from_plaintext_stream() {
        let content = "\u{1b}[2K⠋ Migrating credentials 00s · Ctrl+C to interrupt\r\u{1b}[2K⠙ Analyzing 01s · Ctrl+C to interrupt\r● [00:52:32] Continue 9c58fe00-d47b-4b45-8faa-d2d908100bce\rActual line";
        assert_eq!(
            sanitize_stream_plaintext(content),
            "● [00:52:32] Continue 9c58fe00-d47b-4b45-8faa-d2d908100bce\nActual line"
        );
    }

    #[test]
    fn strips_private_use_glyphs_and_box_drawing_rules() {
        let content = "• \u{e640} Tier 1: Complete the Term Model\n────────────────────";
        assert_eq!(
            sanitize_stream_plaintext(content),
            "•  Tier 1: Complete the Term Model\n---"
        );
    }

    #[test]
    fn converts_box_drawing_tables_to_ascii() {
        let content = "┌───────┬────────┐\n│ Phase │ Focus  │\n└───────┴────────┘";
        assert_eq!(
            sanitize_stream_plaintext(content),
            "+-------+--------+\n| Phase | Focus  |\n+-------+--------+"
        );
    }

    #[test]
    fn sanitizes_full_muse_transcript_with_table_and_fenced_code() {
        let content = "● [03:11:37] MUSE [Agent] Create a concise rollout plan\n● [03:11:39] Read README.md\nThe muse agent completed the task.\n┌───────┬────────┐\n│ Phase │ Focus  │\n└───────┴────────┘\n```rust\nfn main() {}\n```";
        assert_eq!(
            sanitize_stream_plaintext(content),
            "● [03:11:37] MUSE [Agent] Create a concise rollout plan\n● [03:11:39] Read README.md\nThe muse agent completed the task.\n+-------+--------+\n| Phase | Focus  |\n+-------+--------+\n```rust\nfn main() {}\n```"
        );
    }

    #[test]
    fn extracts_transcript_preamble_from_streamed_plaintext() {
        let content = "● [03:11:37] MUSE [Agent] Create a concise rollout plan\n● [03:11:39] Read README.md\n## Summary\n- item";
        assert_eq!(
            transcript_preamble(content).as_deref(),
            Some(
                "● [03:11:37] MUSE [Agent] Create a concise rollout plan\n● [03:11:39] Read README.md"
            )
        );
    }

    #[test]
    fn transcript_preamble_ignores_plain_markdown_without_status_lines() {
        assert_eq!(transcript_preamble("## Summary\n- item"), None);
    }
}
