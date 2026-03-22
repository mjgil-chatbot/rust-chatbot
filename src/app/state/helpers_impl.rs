#[cfg(test)]
mod helpers_tests {
    use super::*;

    fn accumulate_prefixes(content: &str, prefix_char_lengths: &[usize]) -> String {
        let boundaries = content
            .char_indices()
            .map(|(idx, _)| idx)
            .chain(std::iter::once(content.len()))
            .collect::<Vec<_>>();
        let mut accumulated = String::new();

        for &char_len in prefix_char_lengths {
            accumulated.push_str(&content[..boundaries[char_len]]);
        }

        accumulated
    }

    fn test_session() -> ChatSession {
        ChatSession {
            id: "session-1".to_string(),
            name: "session-1".to_string(),
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
            omniforge_model_id: None,
            kimi_session_id: None,
            opencode_session_id: None,
            hermes_session_id: None,
            forge_session_id: None,
            claw_session_id: None,
            launched_from: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            messages: Vec::new(),
            runsc_dirs: Vec::new(),
            runsc_image: None,
        }
    }

    #[test]
    fn upsert_assistant_response_reuses_existing_assistant_message() {
        let mut session = test_session();
        session.add_message("hello".to_string(), false);

        assert!(upsert_assistant_response(&mut session, "updated"));
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "updated");
        assert!(!upsert_assistant_response(&mut session, "updated"));
    }

    #[test]
    fn append_terminal_message_once_skips_duplicate_tail_message() {
        let mut session = test_session();

        append_terminal_message_once(&mut session, "done");
        append_terminal_message_once(&mut session, "done");

        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "done");
    }

    #[test]
    fn replace_provider_session_id_overwrites_existing_value() {
        let mut session = test_session();
        session.minimax_session_id = Some("mm-stale".to_string());

        replace_provider_session_id(&mut session, ModelProvider::MiniMax, Some("mm-fresh"));

        assert_eq!(session.minimax_session_id.as_deref(), Some("mm-fresh"));
    }

    #[test]
    fn replace_provider_session_id_ignores_missing_or_blank_values() {
        let mut session = test_session();
        session.minimax_session_id = Some("mm-existing".to_string());

        replace_provider_session_id(&mut session, ModelProvider::MiniMax, None);
        replace_provider_session_id(&mut session, ModelProvider::MiniMax, Some("   "));

        assert_eq!(session.minimax_session_id.as_deref(), Some("mm-existing"));
    }

    #[test]
    fn upsert_assistant_response_preserves_forge_transcript_preamble() {
        let mut session = test_session();
        session.add_message(
            "● [00:00:01] Execute [/bin/bash] pwd\n● [00:00:02] Execute [/bin/bash] git status --short\n## Summary\n- pwd\n- git status --short"
                .to_string(),
            false,
        );

        assert!(upsert_assistant_response(
            &mut session,
            "## Summary\n- pwd\n- git status --short"
        ));
        assert_eq!(
            session.messages[0].content,
            "● [00:00:01] Execute [/bin/bash] pwd\n● [00:00:02] Execute [/bin/bash] git status --short\n## Summary\n- pwd\n- git status --short"
        );
    }

    #[test]
    fn upsert_assistant_response_does_not_duplicate_existing_transcript_preamble() {
        let mut session = test_session();
        session.add_message(
            "● [00:00:01] Execute [/bin/bash] pwd\n## Summary\n- pwd".to_string(),
            false,
        );

        assert!(upsert_assistant_response(
            &mut session,
            "● [00:00:01] Execute [/bin/bash] pwd\n## Summary\n- pwd\n- done"
        ));
        assert_eq!(
            session.messages[0].content,
            "● [00:00:01] Execute [/bin/bash] pwd\n## Summary\n- pwd\n- done"
        );
    }

    #[test]
    fn upsert_assistant_response_collapses_cumulative_textdelta_body() {
        let mut session = test_session();
        let final_body =
            "Patched the final response assembly.\n\nVerification is incomplete.".to_string();
        let accumulated_body =
            accumulate_prefixes(&final_body, &[1, 12, final_body.chars().count()]);

        assert!(upsert_assistant_response(
            &mut session,
            &format!(
                "[May 19 08:44am] GPT-5.4 agent started\n[May 19 08:45am] I’m tracing the response path.\n\n{}",
                accumulated_body
            )
        ));
        assert_eq!(
            session.messages[0].content,
            format!(
                "[May 19 08:44am] GPT-5.4 agent started\n[May 19 08:45am] I’m tracing the response path.\n\n{}",
                final_body
            )
        );
    }

    #[test]
    fn upsert_assistant_response_strips_leading_think_block() {
        let mut session = test_session();

        assert!(upsert_assistant_response(
            &mut session,
            "<think>Need to inspect the code first.</think>\n\nVisible final answer"
        ));
        assert_eq!(session.messages[0].content, "Visible final answer");
    }

    #[test]
    fn upsert_assistant_response_preserves_transcript_and_strips_think_block() {
        let mut session = test_session();

        assert!(upsert_assistant_response(
            &mut session,
            "[May 19 08:44am] GPT-5.4 | Bash `pwd`\n\n<think>Need to inspect the code first.</think>\n\n## Summary\n- Final answer"
        ));
        assert_eq!(
            session.messages[0].content,
            "[May 19 08:44am] GPT-5.4 | Bash `pwd`\n\n## Summary\n- Final answer"
        );
    }

    #[test]
    fn upsert_assistant_response_ignores_think_only_content() {
        let mut session = test_session();

        assert!(!upsert_assistant_response(
            &mut session,
            "<think>Need to inspect the code first.</think>"
        ));
        assert!(session.messages.is_empty());
    }

    #[test]
    fn read_worker_result_treats_unsupported_schema_version_as_malformed() {
        let _lock = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let home_dir = std::env::temp_dir().join(format!(
            "rust-chatbot-result-schema-home-{}",
            uuid::Uuid::new_v4()
        ));
        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home_dir);

        let chat_id = format!("schema-version-{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(crate::worker::worker_dir(&chat_id)).expect("worker dir");
        std::fs::write(
            crate::worker::result_path(&chat_id),
            r#"{"schema_version":99,"error":"stale"}"#,
        )
        .expect("result.json");

        let result = read_worker_result(&chat_id);
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|error| error.contains("unsupported result.json schema_version")),
            "unsupported schema version should surface as a malformed result"
        );

        match old_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(home_dir);
    }

    #[test]
    fn collapse_textdelta_accumulation_leaves_plain_repetition_unchanged() {
        let content = "Repeated sentence. Repeated sentence. Repeated sentence.";
        assert_eq!(collapse_textdelta_accumulation(content), content);
    }

    #[test]
    fn merge_session_summary_for_codex_preserves_explicit_compaction_counts() {
        let mut summary = Some(JsonSummary {
            total_input_tokens: 100,
            total_output_tokens: 20,
            total_cost_usd: 1.5,
            context_window_tokens: 80_000,
            num_compactions: 1,
            response_count: 1,
            ..Default::default()
        });

        let update = JsonSummary {
            total_input_tokens: 160,
            total_output_tokens: 35,
            total_cost_usd: 2.1,
            context_window_tokens: 42_000,
            num_compactions: 2,
            response_count: 1,
            ..Default::default()
        };

        merge_session_summary(&mut summary, Some(&update), ModelProvider::Codex);

        let merged = summary.expect("summary should be present");
        assert_eq!(merged.context_window_tokens, 42_000);
        assert_eq!(merged.num_compactions, 3);
        assert_eq!(merged.total_input_tokens, 160);
        assert_eq!(merged.total_output_tokens, 35);
        assert_eq!(merged.total_cost_usd, 2.1);
        assert_eq!(merged.response_count, 2);
        assert_eq!(merged.turn_history.len(), 1);
    }

    // ── fa802fe / 29ab45a: is_default_chat_number_name regression ────

    #[test]
    fn is_default_chat_number_name_matches_valid_patterns() {
        assert!(is_default_chat_number_name("Chat 1"));
        assert!(is_default_chat_number_name("Chat 42"));
        assert!(is_default_chat_number_name("Chat 9999"));
    }

    #[test]
    fn is_default_chat_number_name_rejects_invalid_patterns() {
        assert!(!is_default_chat_number_name("Chat "));
        assert!(!is_default_chat_number_name("Chat"));
        assert!(!is_default_chat_number_name("Chat abc"));
        assert!(!is_default_chat_number_name("chat 1")); // lowercase
        assert!(!is_default_chat_number_name("My Chat 1"));
        assert!(!is_default_chat_number_name(""));
        assert!(!is_default_chat_number_name("Chat 1a")); // trailing alpha
    }

    #[test]
    fn merge_codex_summary_propagates_context_window_size() {
        let mut summary = Some(JsonSummary {
            total_input_tokens: 100,
            total_output_tokens: 20,
            response_count: 1,
            ..Default::default()
        });

        let update = JsonSummary {
            total_input_tokens: 200,
            total_output_tokens: 40,
            context_window_tokens: 9_000,
            context_window_size: 258_400,
            response_count: 1,
            ..Default::default()
        };

        merge_session_summary(&mut summary, Some(&update), ModelProvider::Codex);

        let merged = summary.expect("summary should be present");
        assert_eq!(merged.context_window_tokens, 9_000);
        assert_eq!(merged.context_window_size, 258_400);
    }

    #[test]
    fn merge_codex_summary_updates_context_tokens_across_turns() {
        let mut summary = Some(JsonSummary {
            total_input_tokens: 100,
            total_output_tokens: 20,
            context_window_tokens: 9_000,
            context_window_size: 258_400,
            response_count: 1,
            ..Default::default()
        });

        // Second turn: context grew
        let update = JsonSummary {
            total_input_tokens: 300,
            total_output_tokens: 60,
            context_window_tokens: 18_000,
            context_window_size: 258_400,
            response_count: 2,
            ..Default::default()
        };

        merge_session_summary(&mut summary, Some(&update), ModelProvider::Codex);

        let merged = summary.expect("summary should be present");
        assert_eq!(merged.context_window_tokens, 18_000);
        assert_eq!(merged.context_window_size, 258_400);
    }

    #[test]
    fn merge_codex_summary_tracks_compaction_count_from_provider() {
        let mut summary = Some(JsonSummary {
            total_input_tokens: 500,
            total_output_tokens: 100,
            context_window_tokens: 120_000,
            context_window_size: 258_400,
            num_compactions: 0,
            response_count: 3,
            ..Default::default()
        });

        // Codex reports compaction via num_compactions field
        let update = JsonSummary {
            total_input_tokens: 600,
            total_output_tokens: 120,
            context_window_tokens: 40_000,
            context_window_size: 258_400,
            num_compactions: 1,
            response_count: 1,
            ..Default::default()
        };

        merge_session_summary(&mut summary, Some(&update), ModelProvider::Codex);

        let merged = summary.expect("summary should be present");
        assert_eq!(merged.context_window_tokens, 40_000);
        assert_eq!(merged.num_compactions, 1);
    }

    #[test]
    fn merge_codex_summary_overwrites_context_tokens_with_cumulative() {
        let mut summary = Some(JsonSummary {
            total_input_tokens: 300,
            total_output_tokens: 60,
            context_window_tokens: 18_000,
            context_window_size: 258_400,
            response_count: 2,
            ..Default::default()
        });

        // Codex always sends cumulative totals — even 0 overwrites
        let update = JsonSummary {
            total_input_tokens: 400,
            total_output_tokens: 80,
            context_window_tokens: 0,
            context_window_size: 258_400,
            response_count: 1,
            ..Default::default()
        };

        merge_session_summary(&mut summary, Some(&update), ModelProvider::Codex);

        let merged = summary.expect("summary should be present");
        assert_eq!(
            merged.context_window_tokens, 0,
            "codex cumulative summary overwrites context_window_tokens"
        );
    }
}

fn worker_job_provider_matches(job_provider: &str, provider: ModelProvider) -> bool {
    job_provider == provider.as_str()
}

fn socket_provider_matches(event_provider: &str, provider: ModelProvider) -> bool {
    event_provider == provider.as_str()
}

/// Matches auto-generated session names like "Chat 1", "Chat 42", etc.
#[cfg(test)]
fn is_default_chat_number_name(name: &str) -> bool {
    name.strip_prefix("Chat ")
        .map(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

fn read_worker_result(chat_id: &str) -> WorkerResult {
    let path = worker::result_path(chat_id);
    match std::fs::read_to_string(&path) {
        Ok(json) => match serde_json::from_str(&json) {
            Ok(result) => result,
            Err(e) => {
                warn!(chat_id=%chat_id, error=%e, path=%path.display(), "failed to parse worker result.json");
                WorkerResult {
                    session_id: None,
                    summary: None,
                    error: Some(format!("malformed result.json: {e}")),
                }
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => WorkerResult {
            session_id: None,
            summary: None,
            error: None,
        },
        Err(e) => {
            warn!(chat_id=%chat_id, error=%e, path=%path.display(), "failed to read worker result.json");
            WorkerResult {
                session_id: None,
                summary: None,
                error: Some(format!("could not read result.json: {e}")),
            }
        }
    }
}

fn read_worker_response(chat_id: &str) -> String {
    worker::read_output(chat_id)
}

/// Built-in Claude CLI slash commands.
/// Built-in Claude Code slash commands (v2.1.81).
const BUILTIN_SLASH_COMMANDS: &[&str] = &[
    "/add-dir",
    "/agents",
    "/branch",
    "/btw",
    "/chrome",
    "/clear",
    "/color",
    "/compact",
    "/config",
    "/context",
    "/copy",
    "/cost",
    "/desktop",
    "/diff",
    "/doctor",
    "/effort",
    "/exit",
    "/export",
    "/extra-usage",
    "/fast",
    "/feedback",
    "/help",
    "/hooks",
    "/ide",
    "/init",
    "/insights",
    "/install-github-app",
    "/install-slack-app",
    "/keybindings",
    "/login",
    "/logout",
    "/mcp",
    "/memory",
    "/mobile",
    "/model",
    "/passes",
    "/permissions",
    "/plan",
    "/plugin",
    "/pr-comments",
    "/privacy-settings",
    "/release-notes",
    "/reload-plugins",
    "/remote-control",
    "/remote-env",
    "/rename",
    "/resume",
    "/review",
    "/rewind",
    "/sandbox",
    "/security-review",
    "/skills",
    "/stats",
    "/status",
    "/statusline",
    "/stickers",
    "/tasks",
    "/terminal-setup",
    "/theme",
    "/upgrade",
    "/usage",
    "/vim",
    "/voice",
];

fn push_unique_command(out: &mut Vec<String>, command: String) {
    if !out.contains(&command) {
        out.push(command);
    }
}

fn collect_markdown_commands(base: &std::path::Path, relative_dir: &str, out: &mut Vec<String>) {
    let commands_dir = base.join(relative_dir);
    if let Ok(entries) = std::fs::read_dir(&commands_dir) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                push_unique_command(out, format!("/{}", name));
            }
        }
    }
}

fn collect_skill_dirs(base: &std::path::Path, relative_dir: &str, out: &mut Vec<String>) {
    let skills_dir = base.join(relative_dir);
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if !path.is_dir() || !path.join("SKILL.md").exists() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|file_name| file_name.to_str()) {
                push_unique_command(out, format!("/{}", name));
            }
        }
    }
}

fn collect_claude_commands_from(base: &std::path::Path, out: &mut Vec<String>) {
    collect_markdown_commands(base, ".claude/commands", out);
    collect_skill_dirs(base, ".claude/skills", out);
}

fn collect_codex_commands_from(base: &std::path::Path, out: &mut Vec<String>) {
    collect_skill_dirs(base, ".codex/skills", out);
}

/// Discover slash commands from provider-specific project/personal locations
/// and provider built-ins where applicable.
fn discover_slash_commands(provider: ModelProvider, working_dir: Option<&str>) -> Vec<String> {
    let mut commands = Vec::new();

    // Provider-specific project commands for the active session workspace.
    let project_root = workspace::project_root_for_working_dir(working_dir);
    provider.collect_commands_from(&project_root, &mut commands);

    // Personal provider commands in the home directory.
    if let Ok(home) = std::env::var("HOME") {
        let home = std::path::Path::new(&home);
        provider.collect_commands_from(home, &mut commands);
    }

    for command in provider.extra_slash_commands() {
        push_unique_command(&mut commands, (*command).to_string());
    }

    if provider.uses_builtin_slash_commands() {
        for command in BUILTIN_SLASH_COMMANDS {
            push_unique_command(&mut commands, command.to_string());
        }
    }

    info!(
        count = commands.len(),
        provider = provider.as_str(),
        "discovered slash commands"
    );
    commands
}

trait ModelProviderStateExt {
    fn collect_commands_from(self, root: &std::path::Path, commands: &mut Vec<String>);
    fn extra_slash_commands(self) -> &'static [&'static str];
    fn uses_builtin_slash_commands(self) -> bool;
}

impl ModelProviderStateExt for ModelProvider {
    fn collect_commands_from(self, root: &std::path::Path, commands: &mut Vec<String>) {
        match self {
            ModelProvider::Claude | ModelProvider::MiniMax | ModelProvider::Qwen => {
                collect_claude_commands_from(root, commands)
            }
            ModelProvider::Codex | ModelProvider::Nvidia => {
                collect_codex_commands_from(root, commands)
            }
            ModelProvider::Gemini
            | ModelProvider::OpenRouter
            | ModelProvider::OmniForge
            | ModelProvider::GeminiForge
            | ModelProvider::GeminiCliForge
            | ModelProvider::MiniMaxForge
            | ModelProvider::Kimi
            | ModelProvider::OpenCode
            | ModelProvider::Hermes
            | ModelProvider::Forge
            | ModelProvider::Claw => {}
        }
    }

    fn extra_slash_commands(self) -> &'static [&'static str] {
        match self {
            ModelProvider::Claude | ModelProvider::MiniMax | ModelProvider::Qwen => {
                &["/batch", "/claude-api", "/debug", "/loop", "/simplify"]
            }
            ModelProvider::Codex
            | ModelProvider::Nvidia
            | ModelProvider::Gemini
            | ModelProvider::OpenRouter
            | ModelProvider::OmniForge
            | ModelProvider::GeminiForge
            | ModelProvider::GeminiCliForge
            | ModelProvider::MiniMaxForge
            | ModelProvider::Kimi
            | ModelProvider::OpenCode
            | ModelProvider::Hermes
            | ModelProvider::Forge
            | ModelProvider::Claw => &[],
        }
    }

    fn uses_builtin_slash_commands(self) -> bool {
        matches!(self, ModelProvider::Claude | ModelProvider::MiniMax)
    }
}
