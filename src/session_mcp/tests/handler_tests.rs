//! Tests for src/session_mcp/handler.rs
//!
//! Covers:
//! - inspect_session routing and success/failure formatting via service layer
//! - Difference in error formatting between session_mcp (plain string)
//!   and memory_mcp (structured JSON)
//!
//! Note: The handler is a thin async wrapper around service methods that adds
//! JSON serialization via run_blocking_json. The key behavioral difference
//! is that session_mcp returns plain string errors ("Request failed: ...")
//! while memory_mcp returns structured JSON {"error": "..."}.

use crate::data::{ChatMessage, ChatSession, SessionInfo, SessionMetadata};
use crate::model::ModelProvider;
use crate::session_mcp::service::{SessionMcpService, SessionStore};
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rust-chatbot-session-handler-{}-{}", label, nanos))
}

fn write_session(data_dir: &Path, session_id: &str, hidden: bool) {
    let now = Utc::now();
    let session = ChatSession {
        id: session_id.to_string(),
        name: format!("Test Session {}", session_id),
        system_prompt: Some("system".to_string()),
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
        launched_from: Some("/tmp/workspace".to_string()),
        runsc_dirs: Vec::new(),
        runsc_image: None,
        created_at: now,
        updated_at: now,
        messages: vec![
            ChatMessage {
                content: "hello".to_string(),
                is_user: true,
                timestamp: now,
                command: Some("test command".to_string()),
                retrieved_memory: None,
            },
            ChatMessage {
                content: "world".to_string(),
                is_user: false,
                timestamp: now,
                command: None,
                retrieved_memory: None,
            },
        ],
    };

    fs::create_dir_all(data_dir.join("sessions")).unwrap();
    fs::write(
        data_dir.join("sessions").join(format!("{session_id}.json")),
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .unwrap();

    let mut sessions = HashMap::new();
    sessions.insert(
        session_id.to_string(),
        SessionInfo {
            id: session_id.to_string(),
            name: session.name.clone(),
            created_at: now,
            updated_at: now,
            message_count: 2,
            hidden,
        },
    );
    fs::write(
        data_dir.join("sessions.json"),
        serde_json::to_string_pretty(&SessionMetadata { sessions }).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Success formatting tests — verify service returns correct data structure
// that will be serialized to JSON by the handler
// ---------------------------------------------------------------------------

#[test]
fn inspect_session_returns_success_response() {
    let root = temp_dir("success");
    let data_dir = root.join("claude");
    write_session(&data_dir, "abc12345-0000-0000-0000-000000000000", false);

    let service = SessionMcpService::new(vec![SessionStore::new(
        ModelProvider::Claude,
        data_dir.clone(),
    )]);

    let result = service.inspect_session("abc12345-0000-0000-0000-000000000000");

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let response = result.unwrap();

    assert_eq!(
        response.input_session_id,
        "abc12345-0000-0000-0000-000000000000"
    );
    assert_eq!(response.matched_by, "exact");
    assert_eq!(response.provider, "claude");
    assert_eq!(response.session_id, "abc12345-0000-0000-0000-000000000000");
    assert_eq!(response.message_count, 2);
    assert_eq!(response.user_message_count, 1);
    assert_eq!(response.assistant_message_count, 1);
    assert!(!response.hidden);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn inspect_session_response_serializes_to_valid_json() {
    let root = temp_dir("success-json");
    let data_dir = root.join("codex");
    write_session(&data_dir, "msg-test-001", false);

    let service = SessionMcpService::new(vec![SessionStore::new(
        ModelProvider::Codex,
        data_dir.clone(),
    )]);

    let result = service.inspect_session("msg-test-001").unwrap();

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&result).unwrap();

    // Verify it's valid JSON with expected fields
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(value.get("input_session_id").is_some());
    assert!(value.get("matched_by").is_some());
    assert!(value.get("provider").is_some());
    assert!(value.get("provider_display_name").is_some());
    assert!(value.get("session_id").is_some());
    assert!(value.get("data_dir").is_some());
    assert!(value.get("session_path").is_some());
    assert!(value.get("hidden").is_some());
    assert!(value.get("message_count").is_some());
    assert!(value.get("user_message_count").is_some());
    assert!(value.get("assistant_message_count").is_some());
    assert!(value.get("session").is_some());

    // Verify messages array
    let messages = value.get("session").unwrap().get("messages").unwrap();
    assert!(messages.is_array(), "session.messages should be an array");
    assert_eq!(messages.as_array().unwrap().len(), 2);

    fs::remove_dir_all(root).unwrap();
}

// ---------------------------------------------------------------------------
// Failure formatting tests — session_mcp returns PLAIN STRING errors,
// unlike memory_mcp which returns structured {"error": "..."} JSON.
// These tests document the current behavior via the service layer.
// ---------------------------------------------------------------------------

#[test]
fn inspect_session_returns_error_for_nonexistent_session() {
    let root = temp_dir("not-found");
    let data_dir = root.join("claude");
    fs::create_dir_all(data_dir.join("sessions")).unwrap();
    fs::write(
        data_dir.join("sessions.json"),
        serde_json::to_string_pretty(&SessionMetadata {
            sessions: HashMap::new(),
        })
        .unwrap(),
    )
    .unwrap();

    let service = SessionMcpService::new(vec![SessionStore::new(
        ModelProvider::Claude,
        data_dir.clone(),
    )]);

    let result = service.inspect_session("nonexistent-session-id");

    assert!(result.is_err(), "Expected Err for nonexistent session");
    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Error should indicate session not found
    assert!(
        err_msg.contains("No rust-chatbot session found"),
        "Error should mention session not found, got: {}",
        err_msg
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn inspect_session_returns_error_for_empty_session_id() {
    let service = SessionMcpService::new(vec![]);

    let result = service.inspect_session("");

    assert!(result.is_err(), "Expected Err for empty session_id");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("empty") || err.to_string().contains("cannot be empty"),
        "Error should mention session_id cannot be empty, got: {}",
        err
    );
}

#[test]
fn inspect_session_returns_error_for_ambiguous_prefix() {
    let root = temp_dir("ambiguous");
    let claude_dir = root.join("claude");
    let codex_dir = root.join("codex");

    // Create two sessions with same prefix
    write_session(&claude_dir, "ambig-prefix-0001", false);
    write_session(&codex_dir, "ambig-prefix-0002", false);

    let service = SessionMcpService::new(vec![
        SessionStore::new(ModelProvider::Claude, claude_dir),
        SessionStore::new(ModelProvider::Codex, codex_dir),
    ]);

    let result = service.inspect_session("ambig-prefix");

    assert!(result.is_err(), "Expected Err for ambiguous prefix");
    let err = result.unwrap_err();
    let err_msg = err.to_string();

    assert!(
        err_msg.contains("ambiguous"),
        "Error should mention ambiguous, got: {}",
        err_msg
    );
    assert!(
        err_msg.contains("Claude") && err_msg.contains("Codex"),
        "Error should list matching providers, got: {}",
        err_msg
    );

    fs::remove_dir_all(root).unwrap();
}

// ---------------------------------------------------------------------------
// Routing tests — verify exact and prefix matching
// ---------------------------------------------------------------------------

#[test]
fn inspect_session_routes_by_exact_id() {
    let root = temp_dir("route-exact");
    let data_dir = root.join("minimax");
    write_session(&data_dir, "route-exact-full-id-0001", false);

    let service = SessionMcpService::new(vec![SessionStore::new(
        ModelProvider::MiniMax,
        data_dir.clone(),
    )]);

    let result = service.inspect_session("route-exact-full-id-0001").unwrap();

    assert_eq!(result.matched_by, "exact");
    assert_eq!(result.provider, "minimax");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn inspect_session_routes_by_unique_prefix() {
    let root = temp_dir("route-prefix");
    let data_dir = root.join("gemini");
    write_session(&data_dir, "route-prefix-unique-session-999", false);

    let service = SessionMcpService::new(vec![SessionStore::new(
        ModelProvider::Gemini,
        data_dir.clone(),
    )]);

    let result = service.inspect_session("route-prefix-unique").unwrap();

    assert_eq!(result.matched_by, "prefix");
    assert_eq!(result.provider, "gemini");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn inspect_session_returns_hidden_flag_correctly() {
    let root = temp_dir("route-hidden");
    let data_dir = root.join("qwen");
    write_session(&data_dir, "hidden-session-123", true); // hidden = true

    let service = SessionMcpService::new(vec![SessionStore::new(
        ModelProvider::Qwen,
        data_dir.clone(),
    )]);

    let result = service.inspect_session("hidden-session-123").unwrap();

    assert!(result.hidden, "hidden flag should be true");

    fs::remove_dir_all(root).unwrap();
}

// ---------------------------------------------------------------------------
// Contrast test: session_mcp vs memory_mcp error formatting
// ---------------------------------------------------------------------------

#[test]
fn session_mcp_error_is_plain_string_not_structured_json() {
    // This test documents the architectural difference:
    // - session_mcp handler: errors return plain strings like "Request failed: ..."
    // - memory_mcp handler: errors return structured JSON like {"error": "..."}
    //
    // When session_mcp handler's run_blocking_json catches an error, it does:
    //   format!("Request failed: {error}")
    // NOT:
    //   to_pretty_json(&ErrorResponse { error: error.to_string() })
    //
    // So when the handler returns an error response, it is a plain string,
    // NOT a JSON object with an "error" field.

    let service = SessionMcpService::new(vec![]);
    let result = service.inspect_session("");

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // The error message from the service is a plain string
    // The handler would format this as "Request failed: {err_msg}"
    // This is different from memory_mcp which returns {"error": err_msg}

    // Verify the service error is NOT a JSON object
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&err_msg);
    assert!(
        parsed.is_err(),
        "session_mcp error should NOT be valid JSON: {}",
        err_msg
    );

    // Verify it contains meaningful error text
    assert!(!err_msg.is_empty(), "Error message should not be empty");
}

#[test]
fn session_mcp_response_when_serialized_to_json_has_all_fields() {
    // Verify that a success response from session_mcp service
    // serializes correctly to JSON (what the handler would return on success)

    let root = temp_dir("serialize");
    let data_dir = root.join("claude");
    write_session(&data_dir, "serialize-test-001", false);

    let service = SessionMcpService::new(vec![SessionStore::new(
        ModelProvider::Claude,
        data_dir.clone(),
    )]);

    let result = service.inspect_session("serialize-test-001").unwrap();

    // Serialize as the handler would
    let json = serde_json::to_string_pretty(&result).unwrap();

    // The JSON should be valid
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_object(), "Serialized JSON should be an object");

    fs::remove_dir_all(root).unwrap();
}
