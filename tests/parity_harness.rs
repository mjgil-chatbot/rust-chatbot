// Deterministic parity harness: six canonical scenarios that exercise the
// provider boundary through the real worker binary against a mock provider
// server. Each test asserts request ordering, request counts, auth token
// propagation, and scenario-specific provider-boundary invariants.

#[path = "mock_provider_server.rs"]
mod mock_provider_server;

use std::fs;
use std::sync::Mutex;
use std::time::Duration;

use chatbot_lib::worker::{self, WorkerState};
use mock_provider_server::*;
use serde_json::{json, Value};

static ENV_LOCK: Mutex<()> = Mutex::new(());

// -------------------------------------------------------------------------
// Scenario 1: streaming_text
// -------------------------------------------------------------------------

#[test]
fn streaming_text() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let server = MockProviderServer::start(vec![ScenarioStep {
        name: "initial",
        response: text_response("stream-session", "streaming text completed"),
    }]);
    let _harness = WorkerHarness::new("streaming-text", &server);
    let chat_id = unique_chat_id("stream");
    let request = make_request(&chat_id, "scenario:streaming_text", None);

    let output = wait_for_worker(spawn_worker(&request));
    assert!(
        output.status.success(),
        "worker stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Provider-boundary: exactly one request, correct path and auth
    assert_request_count(&server, 1);
    assert_request_ordering(&server, &["initial"]);
    assert_all_requests_path(&server, "/anthropic/v1/messages");
    assert_all_requests_auth(&server, "parity-token");

    // Provider-boundary: system prompt and query propagated
    assert_system_prompt(&server, "Return deterministic parity output.");
    let requests = server.request_log();
    assert_eq!(
        requests[0].user_query(),
        Some("scenario:streaming_text"),
        "query text not propagated to provider"
    );
    assert_eq!(
        requests[0].tool_result_count(),
        Some(0),
        "initial request should have tool_result_count=0"
    );

    // Result assertions
    let result = read_worker_result(&chat_id);
    assert_eq!(result.session_id.as_deref(), Some("stream-session"));
    assert_worker_text_output(&chat_id, "streaming text completed");
    assert!(
        result.summary.is_none(),
        "adapter-backed worker should not persist a legacy summary payload"
    );
}

#[test]
fn fresh_chat_includes_claude_md_in_sent_minimax_command() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let server = MockProviderServer::start(vec![ScenarioStep {
        name: "fresh",
        response: text_response("fresh-session", "fresh chat completed"),
    }]);
    let harness = WorkerHarness::new("fresh-chat-claude-md", &server);

    let global_claude_dir = harness._temp.path().join("home").join(".claude");
    fs::create_dir_all(&global_claude_dir).unwrap();
    fs::write(global_claude_dir.join("CLAUDE.md"), "global rule").unwrap();

    let workspace = TempDir::new("fresh-chat-workspace");
    fs::write(workspace.path().join("CLAUDE.md"), "workspace rule").unwrap();
    let workspace_claude_dir = workspace.path().join(".claude");
    fs::create_dir_all(&workspace_claude_dir).unwrap();
    fs::write(
        workspace_claude_dir.join("CLAUDE.md"),
        "workspace nested rule",
    )
    .unwrap();

    let chat_id = unique_chat_id("fresh-claude-md");
    let request = worker::WorkerRequest {
        provider: "minimax".to_string(),
        query: "scenario:fresh_chat_includes_claude_md".to_string(),
        working_dir: Some(workspace.path().display().to_string()),
        system_prompt: Some("Return deterministic parity output.".to_string()),
        resume_session_id: None,
        memory_context: None,
        parent_pid_override: Some(std::process::id()),
        runsc_dirs: Vec::new(),
        runsc_image: None,
        model_override: None,
        command: "parity-harness".to_string(),
        chat_id: chat_id.clone(),
    };

    let output = wait_for_worker(spawn_worker(&request));
    assert!(
        output.status.success(),
        "worker stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_request_count(&server, 1);
    assert_request_ordering(&server, &["fresh"]);
    assert_all_requests_path(&server, "/anthropic/v1/messages");
    assert_all_requests_auth(&server, "parity-token");
    assert_system_prompt(
        &server,
        "global rule\n\nworkspace rule\n\nworkspace nested rule\n\nReturn deterministic parity output.",
    );

    let requests = server.requests();
    assert_eq!(requests[0].resume_session_id(), None);
    assert_eq!(
        requests[0].user_query(),
        Some("scenario:fresh_chat_includes_claude_md"),
        "query text not propagated to provider"
    );

    let result = read_worker_result(&chat_id);
    assert_eq!(result.session_id.as_deref(), Some("fresh-session"));
    assert_worker_text_output(&chat_id, "fresh chat completed");
}

// -------------------------------------------------------------------------
// Scenario 2: resume_session_id
// -------------------------------------------------------------------------

#[test]
fn resume_session_id() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let server = MockProviderServer::start(vec![ScenarioStep {
        name: "resume",
        response: text_response("resume-session-42", "resumed session output"),
    }]);
    let _harness = WorkerHarness::new("resume-session", &server);
    let chat_id = unique_chat_id("resume");
    let request = make_request(
        &chat_id,
        "scenario:resume_session_id",
        Some("resume-session-42"),
    );

    let output = wait_for_worker(spawn_worker(&request));
    assert!(
        output.status.success(),
        "worker stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = read_worker_result(&chat_id);
    assert_eq!(
        result.error, None,
        "worker reported adapter error: {:?}",
        result.error
    );

    // Provider-boundary: exactly one request
    assert_request_count(&server, 1);
    assert_request_ordering(&server, &["resume"]);
    assert_all_requests_path(&server, "/anthropic/v1/messages");
    assert_all_requests_auth(&server, "parity-token");

    // Provider-boundary: resume_session_id propagated through the CLI
    let requests = server.request_log();
    assert_eq!(
        requests[0].resume_session_id(),
        Some("resume-session-42"),
        "resume_session_id not propagated to provider"
    );

    // Result reflects the resumed session
    assert_eq!(result.session_id.as_deref(), Some("resume-session-42"));
    assert!(
        result.summary.is_none(),
        "adapter-backed worker should not persist a legacy summary payload"
    );
}

// -------------------------------------------------------------------------
// Scenario 3: worker_spool_persistence
// -------------------------------------------------------------------------

#[test]
fn worker_spool_persistence() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let server = MockProviderServer::start(vec![ScenarioStep {
        name: "spool",
        response: json!({
            "session_id": "spool-session",
            "model": "MiniMax-M2.7",
            "stream_chunks": ["partial spool", "complete spool"],
            "stop_reason": "end_turn",
            "usage": usage(34, 8),
            "duration_ms": 44,
            "num_turns": 1,
            "result_text": "complete spool",
            "modelUsage": model_usage(),
            "sleep_before_result_ms": 250,
        }),
    }]);
    let _harness = WorkerHarness::new("spool-persistence", &server);
    let chat_id = unique_chat_id("spool");
    let request = make_request(&chat_id, "scenario:worker_spool_persistence", None);

    let child = spawn_worker(&request);

    // While the worker is running (before the 250ms sleep completes), verify
    // partial text deltas are persisted to the event spool and state is Running.
    wait_until(Duration::from_secs(5), || {
        read_worker_events(&chat_id).iter().any(|event| {
            matches!(
                event,
                chatbot_lib::stream::StreamEvent::TextDelta { content, .. }
                    if content.contains("partial spool")
            )
        }) && worker::read_job_state(&chat_id)
            .ok()
            .flatten()
            .map(|job| job.state == WorkerState::Running)
            .unwrap_or(false)
    });

    let output = wait_for_worker(child);
    assert!(
        output.status.success(),
        "worker stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Provider-boundary: exactly one request
    assert_request_count(&server, 1);
    assert_request_ordering(&server, &["spool"]);
    assert_all_requests_auth(&server, "parity-token");

    // Final state transitions correctly
    let final_state = read_worker_state(&chat_id);
    assert_eq!(final_state.state, WorkerState::Done);
    let output = fs::read_to_string(worker::output_path(&chat_id)).unwrap();
    assert!(
        output.contains("complete spool"),
        "final response missing from persisted output: {output}"
    );
}

// -------------------------------------------------------------------------
// Scenario 4: malformed_json_summary
// -------------------------------------------------------------------------

#[test]
fn malformed_json_summary() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let server = MockProviderServer::start(vec![ScenarioStep {
        name: "malformed",
        response: json!({
            "session_id": "malformed-session",
            "model": "MiniMax-M2.7",
            "content": [{"type": "text", "text": "graceful malformed summary"}],
            "stop_reason": "end_turn",
            "usage": usage(0, 0),
            "duration_ms": 0,
            "num_turns": 1,
            "result_text": "graceful malformed summary",
            "emit_malformed_result": true,
        }),
    }]);
    let _harness = WorkerHarness::new("malformed-summary", &server);
    let chat_id = unique_chat_id("malformed");
    let request = make_request(&chat_id, "scenario:malformed_json_summary", None);

    let output = wait_for_worker(spawn_worker(&request));
    assert!(
        output.status.success(),
        "worker stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Provider-boundary: exactly one request reached the server
    assert_request_count(&server, 1);
    assert_request_ordering(&server, &["malformed"]);

    // Graceful degradation: worker succeeds, no error, and no summary payload
    // because the malformed terminal line was not parseable as an adapter event.
    let result = read_worker_result(&chat_id);
    assert_eq!(
        result.error, None,
        "malformed summary should not produce an error"
    );
    assert!(
        result.summary.is_none(),
        "malformed terminal event should not synthesize a summary payload"
    );
    assert_worker_text_output(&chat_id, "graceful malformed summary");
}

// -------------------------------------------------------------------------
// Scenario 5: mcp_tool_response
// -------------------------------------------------------------------------

#[test]
fn mcp_tool_response() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let server = MockProviderServer::start(vec![
        ScenarioStep {
            name: "initial",
            response: json!({
                "session_id": "tool-session",
                "model": "MiniMax-M2.7",
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "mcp__bash__run",
                    "input": {"command": "printf tool-ok"}
                }],
                "stop_reason": "tool_use",
                "usage": usage(18, 4),
                "tool_result_text": "tool-ok",
            }),
        },
        ScenarioStep {
            name: "tool_result",
            response: json!({
                "session_id": "tool-session",
                "model": "MiniMax-M2.7",
                "content": [{"type": "text", "text": "tool round-trip complete"}],
                "stop_reason": "end_turn",
                "usage": usage(29, 11),
                "duration_ms": 55,
                "num_turns": 2,
                "result_text": "tool round-trip complete",
                "modelUsage": model_usage(),
            }),
        },
    ]);
    let _harness = WorkerHarness::new("mcp-tool-response", &server);
    let chat_id = unique_chat_id("tool");
    let request = make_request(&chat_id, "scenario:mcp_tool_response", None);

    let output = wait_for_worker(spawn_worker(&request));
    assert!(
        output.status.success(),
        "worker stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Provider-boundary: exactly two requests in strict order
    assert_request_count(&server, 2);
    assert_request_ordering(&server, &["initial", "tool_result"]);
    assert_all_requests_path(&server, "/anthropic/v1/messages");
    assert_all_requests_auth(&server, "parity-token");

    // Provider-boundary: first request has tool_result_count=0, second has 1
    let requests = server.requests();
    assert_eq!(
        requests[0].tool_result_count(),
        Some(0),
        "initial request should have tool_result_count=0"
    );
    assert_eq!(
        requests[1].tool_result_count(),
        Some(1),
        "tool_result request should have tool_result_count=1"
    );

    // Provider-boundary: tool result content propagated in second request
    let tool_result = requests[1].body["messages"][2]["content"][0]
        .get("content")
        .and_then(Value::as_str);
    assert_eq!(
        tool_result,
        Some("tool-ok"),
        "tool result content not propagated in second request"
    );

    // Spool events reflect the adapter tool round-trip.
    let result = read_worker_result(&chat_id);
    assert!(result.summary.is_none());
    assert_tool_events(&chat_id, &["mcp__bash__run"]);
    let output = fs::read_to_string(worker::output_path(&chat_id)).unwrap();
    assert!(
        output.contains("tool round-trip complete"),
        "final tool response missing from persisted output: {output}"
    );
}

// -------------------------------------------------------------------------
// Scenario 6: provider_config_injection
// -------------------------------------------------------------------------

#[test]
fn provider_config_injection() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let server = MockProviderServer::start(vec![ScenarioStep {
        name: "config",
        response: text_response("config-session", "config injection complete"),
    }]);
    let harness = WorkerHarness::new("provider-config", &server);
    let temp = TempDir::new("provider-config-extra");
    let mcp_json_path = temp.path().join("workspace.mcp.json");
    let claude_json_path = harness.config_dir.join(".claude.json");
    let gemini_settings_path = temp.path().join("gemini/settings.json");
    let codex_toml_path = temp.path().join("codex/config.toml");
    fs::create_dir_all(gemini_settings_path.parent().unwrap()).unwrap();
    fs::create_dir_all(codex_toml_path.parent().unwrap()).unwrap();
    fs::write(
        &mcp_json_path,
        serde_json::to_vec_pretty(&json!({
            "mcpServers": {
                "memory": {
                    "command": "echo",
                    "args": ["provider-config"],
                    "env": {"MODE": "parity"}
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let _extra_env = ScopedEnv::set([
        (TEST_MCP_JSON_PATH_ENV, mcp_json_path.into_os_string()),
        (
            TEST_CLAUDE_JSON_PATH_ENV,
            claude_json_path.clone().into_os_string(),
        ),
        (
            TEST_GEMINI_SETTINGS_PATH_ENV,
            gemini_settings_path.clone().into_os_string(),
        ),
        (
            TEST_CODEX_TOML_PATH_ENV,
            codex_toml_path.clone().into_os_string(),
        ),
    ]);

    let chat_id = unique_chat_id("config");
    let request = make_request(&chat_id, "scenario:provider_config_injection", None);
    let output = wait_for_worker(spawn_worker(&request));
    assert!(
        output.status.success(),
        "worker stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Provider-boundary: exactly one request with correct auth
    assert_request_count(&server, 1);
    assert_request_ordering(&server, &["config"]);
    assert_all_requests_auth(&server, "parity-token");

    // Provider-boundary: config_dir injected into metadata
    let requests = server.requests();
    let request = &requests[0];
    assert_eq!(
        request.body["metadata"]["config_dir"].as_str(),
        Some(harness.config_dir.to_string_lossy().as_ref()),
        "config_dir not propagated in request metadata"
    );

    // Current adapter path does not synthesize provider-facing config files
    // from the legacy test override paths, so claude_json stays unset here.
    assert!(
        request.body["metadata"]["claude_json"].is_null(),
        "adapter path should not synthesize claude_json metadata from legacy override envs"
    );

    // Provider-boundary: CLI path points to the fake CLI
    let expected_cli_path = harness
        .fake_cli_path
        .canonicalize()
        .expect("canonical fake CLI");
    assert_eq!(
        request.body["metadata"]["cli_path"].as_str(),
        Some(expected_cli_path.to_string_lossy().as_ref()),
        "cli_path metadata does not match injected CLI"
    );

    // Legacy provider-config sidecars are not rendered by the adapter path.
    assert!(
        !codex_toml_path.exists(),
        "adapter path should not render Codex TOML from legacy override envs"
    );
    assert!(
        !gemini_settings_path.exists(),
        "adapter path should not render Gemini settings from legacy override envs"
    );
    let _ = claude_json_path;
}
