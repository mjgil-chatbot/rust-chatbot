use super::{
    assistant_retrieved_memory_preview, base_markdown_content, escape_underscores_in_tool_names,
    format_standalone_json_message, is_standalone_json_message, normalize_code_language,
    normalized_markdown_content, plaintext_lines_for_render, prepare_plaintext_render_blocks,
    retrieved_memory_item_label, sanitized_stream_markdown_content,
    should_render_as_plaintext_stream, split_transcript_and_markdown, PlaintextBlock,
    PlaintextRenderBlock,
};
use crate::data::ChatMessage;
use chrono::Utc;

#[test]
fn standalone_json_object_is_detected() {
    assert!(is_standalone_json_message("{\"ok\":true}"));
}

#[test]
fn markdown_wrapped_json_is_not_treated_as_raw_json() {
    assert!(!is_standalone_json_message("```json\n{\"ok\":true}\n```"));
}

#[test]
fn normal_markdown_message_is_not_treated_as_raw_json() {
    assert!(!is_standalone_json_message(
        "Here is `code` and **markdown**."
    ));
}

#[test]
fn standalone_json_is_pretty_printed() {
    let formatted = format_standalone_json_message("{\"z\":true,\"a\":[1,2]}")
        .expect("valid standalone JSON should be formatted");
    assert!(formatted.contains("\n  "));
    assert!(
        formatted.find("\"z\"").unwrap() < formatted.find("\"a\"").unwrap(),
        "pretty formatting should preserve the original key order"
    );
}

#[test]
fn escape_underscores_in_tool_names_basic() {
    assert_eq!(
        escape_underscores_in_tool_names("mcp__bash__run"),
        "mcp\\_\\_bash\\_\\_run"
    );
}

#[test]
fn escape_underscores_in_tool_names_multiple() {
    assert_eq!(
        escape_underscores_in_tool_names("mcp__bash__run and mcp__read__file"),
        "mcp\\_\\_bash\\_\\_run and mcp\\_\\_read\\_\\_file"
    );
}

#[test]
fn escape_underscores_preserves_single_underscores() {
    assert_eq!(
        escape_underscores_in_tool_names("some_variable_name"),
        "some_variable_name"
    );
}

#[test]
fn escape_underscores_no_change_without_double() {
    assert_eq!(
        escape_underscores_in_tool_names("Hello world"),
        "Hello world"
    );
}

#[test]
fn plaintext_stream_detection_handles_ansi_spinner_noise() {
    let input = "\u{1b}[2K⠋ Analyzing 00s · Ctrl+C to interrupt\rplain";
    assert!(!should_render_as_plaintext_stream(input));
    assert_eq!(
        sanitized_stream_markdown_content(input).as_deref(),
        Some("plain")
    );
}

#[test]
fn plaintext_block_parser_extracts_fenced_code() {
    let blocks = PlaintextBlock::parse("alpha\n```rust\nfn main() {}\n```\nomega\n");
    assert_eq!(blocks.len(), 3);
    match &blocks[1] {
        PlaintextBlock::Code { language, code } => {
            assert_eq!(language.as_deref(), Some("rust"));
            assert_eq!(code, "fn main() {}\n");
        }
        PlaintextBlock::Text(_) => panic!("expected code block"),
    }
}

#[test]
fn normalize_code_language_defaults_plaintext_to_shell() {
    assert_eq!(normalize_code_language(None), "sh");
    assert_eq!(normalize_code_language(Some("text")), "sh");
    assert_eq!(normalize_code_language(Some("rust")), "rust");
}

#[test]
fn prepare_plaintext_render_blocks_extracts_fenced_code_for_ui_path() {
    let blocks = prepare_plaintext_render_blocks("intro\n```rust\nfn main() {}\n```\noutro\n");
    assert_eq!(
        blocks,
        vec![
            PlaintextRenderBlock::Text("intro\n".to_string()),
            PlaintextRenderBlock::Code {
                language: "rust".to_string(),
                code: "fn main() {}\n".to_string(),
            },
            PlaintextRenderBlock::Text("outro".to_string()),
        ]
    );
}

#[test]
fn prepare_plaintext_render_blocks_normalizes_plaintext_code_language() {
    let blocks = prepare_plaintext_render_blocks("```text\nls -la\n```");
    assert_eq!(
        blocks,
        vec![PlaintextRenderBlock::Code {
            language: "sh".to_string(),
            code: "ls -la\n".to_string(),
        }]
    );
}

#[test]
fn assistant_retrieved_memory_preview_uses_previous_user_message() {
    let previous = ChatMessage {
        content: "question".to_string(),
        is_user: true,
        timestamp: Utc::now(),
        command: None,
        retrieved_memory: Some(
            "- [session=chat role=assistant time=2026-05-19T12:00:00Z] prior fix".to_string(),
        ),
    };
    let assistant = ChatMessage {
        content: "answer".to_string(),
        is_user: false,
        timestamp: Utc::now(),
        command: None,
        retrieved_memory: None,
    };

    assert_eq!(
        assistant_retrieved_memory_preview(&assistant, Some(&previous)),
        Some("- [session=chat role=assistant time=2026-05-19T12:00:00Z] prior fix")
    );
}

#[test]
fn retrieved_memory_item_label_counts_non_empty_lines() {
    assert_eq!(
        retrieved_memory_item_label("- first\n\n- second\n"),
        "2 items".to_string()
    );
}

#[test]
fn transcript_style_streams_stay_plaintext() {
    let input = "● [00:52:45] Execute [/bin/bash] ls -la crates/";
    assert!(should_render_as_plaintext_stream(input));
    assert!(sanitized_stream_markdown_content(input).is_none());
}

#[test]
fn transcript_preamble_with_markdown_body_splits_rendering() {
    let input = "● [00:52:45] Execute [/bin/bash] ls -la crates/\nAll done! Let me provide a summary.\n## Summary\n- item\n```rust\nfn main() {}\n```";
    let split = split_transcript_and_markdown(input).expect("expected mixed render split");
    assert_eq!(
        split.plaintext_preamble.as_deref(),
        Some("● [00:52:45] Execute [/bin/bash] ls -la crates/")
    );
    assert_eq!(
        split.markdown_body.as_deref(),
        Some("All done! Let me provide a summary.\n## Summary\n- item\n```rust\nfn main() {}\n```")
    );
}

#[test]
fn transcript_preamble_with_ascii_table_and_fenced_code_splits_rendering() {
    let input = "● [03:11:37] MUSE [Agent] Create a concise rollout plan\n● [03:11:39] Read README.md\nThe muse agent completed the task.\n+-------+--------+\n| Phase | Focus  |\n+-------+--------+\n```rust\nfn main() {}\n```";
    let split = split_transcript_and_markdown(input).expect("expected mixed render split");
    assert_eq!(
        split.plaintext_preamble.as_deref(),
        Some(
            "● [03:11:37] MUSE [Agent] Create a concise rollout plan\n● [03:11:39] Read README.md"
        )
    );
    assert_eq!(
        split.markdown_body.as_deref(),
        Some("The muse agent completed the task.\n+-------+--------+\n| Phase | Focus  |\n+-------+--------+\n```rust\nfn main() {}\n```")
    );
}

#[test]
fn prepare_plaintext_render_blocks_keeps_transcript_lines_outside_code_block() {
    let blocks = prepare_plaintext_render_blocks(
        "● [03:11:37] MUSE [Agent] Create a concise rollout plan\n+-------+--------+\n| Phase | Focus  |\n+-------+--------+\n```rust\nfn main() {}\n```",
    );
    assert_eq!(blocks.len(), 2);
    assert_eq!(
        blocks[0],
        PlaintextRenderBlock::Text(
            "● [03:11:37] MUSE [Agent] Create a concise rollout plan\n+-------+--------+\n| Phase | Focus  |\n+-------+--------+\n"
                .to_string()
        )
    );
    assert_eq!(
        blocks[1],
        PlaintextRenderBlock::Code {
            language: "rust".to_string(),
            code: "fn main() {}\n".to_string(),
        }
    );
}

#[test]
fn plaintext_lines_for_render_preserves_transcript_newlines() {
    assert_eq!(
        plaintext_lines_for_render(
            "● [03:11:37] Execute [/bin/bash] pwd\n● [03:11:39] Execute [/bin/bash] uname -a\n"
        ),
        vec![
            "● [03:11:37] Execute [/bin/bash] pwd",
            "● [03:11:39] Execute [/bin/bash] uname -a",
        ]
    );
}

#[test]
fn standard_tool_summary_markdown_uses_hard_breaks() {
    assert_eq!(
        normalized_markdown_content(
            "[May 19 12:02am] MM-2.7 agent started\n[May 19 12:02am] MM-2.7 | Bash `pwd`\n[May 19 12:02am] MM-2.7 | Bash `pwd` | /tmp"
        )
        .as_ref(),
        "[May 19 12:02am] MM-2.7 agent started  \n[May 19 12:02am] MM-2.7 | Bash `pwd`  \n[May 19 12:02am] MM-2.7 | Bash `pwd` | /tmp  "
    );
}

#[test]
fn standard_tool_summary_markdown_preserves_body_markdown() {
    assert_eq!(
        normalized_markdown_content(
            "[May 19 12:02am] GPT-5.4 agent started\n[May 19 12:02am] GPT-5.4 | Bash `pwd`\n\n## Summary\n- item"
        )
        .as_ref(),
        "[May 19 12:02am] GPT-5.4 agent started  \n[May 19 12:02am] GPT-5.4 | Bash `pwd`  \n\n## Summary\n- item"
    );
}

#[test]
fn timestamped_text_delta_markdown_uses_hard_breaks() {
    assert_eq!(
        normalized_markdown_content(
            "[May 19 01:28am] I'll abide by `AGENTS.md`.\n[May 19 01:28am] Then I'll inspect the relevant code paths.\n\n## Summary\n- item"
        )
        .as_ref(),
        "[May 19 01:28am] I'll abide by `AGENTS.md`.  \n[May 19 01:28am] Then I'll inspect the relevant code paths.  \n\n## Summary\n- item"
    );
}

#[test]
fn ordinary_pipe_markdown_is_unchanged() {
    assert_eq!(
        normalized_markdown_content("Summary | details\nNext line").as_ref(),
        "Summary | details\nNext line"
    );
}

#[test]
fn base_markdown_content_strips_leading_think_block() {
    assert_eq!(
        base_markdown_content(
            "<think>Need to inspect the code first.</think>\n\nVisible final answer"
        )
        .as_ref(),
        "Visible final answer"
    );
}

#[test]
fn normalized_markdown_content_strips_think_after_transcript_preamble() {
    assert_eq!(
        normalized_markdown_content(
            "[May 19 12:02am] MM-2.7 agent started\n\n<think>Need to inspect the code first.</think>\n\n## Summary\n- item"
        )
        .as_ref(),
        "[May 19 12:02am] MM-2.7 agent started  \n\n## Summary\n- item"
    );
}

#[test]
fn normalized_markdown_content_sanitizes_real_extracted_tool_transcript_sample() {
    let normalized = normalized_markdown_content(include_str!(
        "../../../tests/fixtures/real_extracted_tool_transcript_sample.md"
    ));

    assert!(normalized.contains(r"\<!-- SPDX-License-Identifier: MPL-2.0 --\>"));
    assert!(normalized.contains(r"install\_page\_bootstrap\_globals"));
    assert!(normalized.contains(r"wasm\_loader"));
    assert!(normalized.contains("## Summary\n- Preserve transcript wrapping"));
}

#[test]
fn normalized_markdown_content_preserves_inline_code_inside_transcript_lines() {
    assert_eq!(
        normalized_markdown_content(
            "[May 23 06:34pm] GPT-5.4 | Bash `python -c \"print(alpha_beta < gamma)\"`\n\n## Summary\n- item"
        )
        .as_ref(),
        "[May 23 06:34pm] GPT-5.4 | Bash `python -c \"print(alpha_beta < gamma)\"`  \n\n## Summary\n- item"
    );
}

#[test]
fn normalized_markdown_content_escapes_emphasis_in_transcript_but_not_body() {
    assert_eq!(
        normalized_markdown_content(
            "[May 23 06:34pm] I saw *alpha* and __beta__ in the transcript.\n\n## Summary\n- keep **body markdown**"
        )
        .as_ref(),
        "[May 23 06:34pm] I saw \\*alpha\\* and \\_\\_beta\\_\\_ in the transcript.  \n\n## Summary\n- keep **body markdown**"
    );
}
