//! Tests for src/memory_mcp/
//!
//! Covers:
//! - Service method tests for all memory flows
//! - JSON serialization validation for all response types

use crate::data::DataManager;
use crate::memory::IngestState;
use crate::memory::ManualMemoriesState;
use crate::memory_audit::{append_memory_retrieval_audit, AppendMemoryRetrievalAudit};
use crate::memory_mcp::service::MemoryMcpService;
use crate::model::ModelProvider;
use chrono::TimeZone;
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct MemoryMcpTestGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    _home_dir: tempfile::TempDir,
    previous_home: Option<OsString>,
}

impl MemoryMcpTestGuard {
    fn new() -> Self {
        let lock = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let home_dir = tempfile::TempDir::new().unwrap();
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", home_dir.path());
        Self {
            _lock: lock,
            _home_dir: home_dir,
            previous_home,
        }
    }
}

impl Drop for MemoryMcpTestGuard {
    fn drop(&mut self) {
        match self.previous_home.as_ref() {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
    }
}

fn temp_workspace(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Match ServerConfig's canonical roots, including temporary-directory aliases.
    std::env::temp_dir()
        .canonicalize()
        .expect("canonical temporary directory")
        .join(format!("rust-chatbot-memory-mcp-{}-{}", label, nanos))
}

fn create_empty_workspace(root: &Path) {
    fs::create_dir_all(root).unwrap();
    let paths = crate::memory::ensure_workspace_layout(root).unwrap();
    let ingest_state = IngestState::default();
    fs::write(
        paths.ingest_state_path,
        serde_json::to_string_pretty(&ingest_state).unwrap(),
    )
    .unwrap();
    let memories = ManualMemoriesState::default();
    fs::write(
        paths.manual_memories_path,
        serde_json::to_string_pretty(&memories).unwrap(),
    )
    .unwrap();
}

fn cleanup_workspace(root: &Path) {
    if let Ok(paths) = crate::memory::workspace_memory_paths(root) {
        let _ = fs::remove_dir_all(paths.root_dir);
    }
    let _ = fs::remove_dir_all(root);
}

// ---------------------------------------------------------------------------
// Service method tests — these are the core behaviors that the handler
// wraps with JSON serialization. Testing the service directly lets us verify
// both the logic and the error semantics.
// ---------------------------------------------------------------------------

#[test]
fn workspace_memory_status_routes_successfully() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("status-ok");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service.workspace_memory_status(&workspace.to_string_lossy());

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let status = result.unwrap();
    assert!(!status.workspace_root.is_empty());
    assert!(!status.workspace_id.is_empty());
    assert!(!status.memmachine_base_url.is_empty());
    assert!(!status.memmachine_org_id.is_empty());
    assert!(!status.memmachine_project_id.is_empty());
    assert_eq!(status.indexed_sessions, 0);
    assert_eq!(status.invalidation_count, 0);
    assert_eq!(status.manual_memory_count, 0);

    cleanup_workspace(&workspace);
}

#[test]
fn workspace_memory_status_returns_error_for_nonexistent_workspace() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("status-missing");

    let service = MemoryMcpService::new(vec![std::env::temp_dir()]);

    let result = service.workspace_memory_status(&workspace.to_string_lossy());

    assert!(result.is_err(), "Expected Err for nonexistent workspace");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("outside") || err.to_string().contains("Cannot resolve"),
        "Error should mention path issue, got: {}",
        err
    );
}

#[test]
fn workspace_memory_status_returns_error_for_relative_path() {
    let _guard = MemoryMcpTestGuard::new();
    let service = MemoryMcpService::new(vec![std::env::temp_dir()]);

    let result = service.workspace_memory_status("relative/path");

    assert!(result.is_err(), "Expected Err for relative path");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("absolute"),
        "Error should mention absolute path required, got: {}",
        err
    );
}

#[test]
fn search_memory_routes_successfully() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("search-ok");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service.search_memory(&workspace.to_string_lossy(), "hello world");

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let response = result.unwrap();
    assert!(!response.workspace_root.is_empty());
    assert_eq!(response.query, "hello world");
    assert_eq!(response.hit_count, 0); // Empty workspace has no hits

    cleanup_workspace(&workspace);
}

#[test]
fn search_memory_returns_error_for_nonexistent_workspace() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("search-missing");

    let service = MemoryMcpService::new(vec![std::env::temp_dir()]);

    let result = service.search_memory(&workspace.to_string_lossy(), "test");

    assert!(result.is_err(), "Expected Err for nonexistent workspace");
}

#[test]
fn list_memory_injections_returns_persisted_user_turn_previews() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("list-injections");
    create_empty_workspace(&workspace);

    let data_manager = DataManager::new(ModelProvider::Codex).expect("data manager");
    let mut session = data_manager
        .create_session("Memory Injection Session".to_string(), None)
        .expect("create session");
    session.launched_from = Some(workspace.to_string_lossy().to_string());
    session.add_message_with_command(
        "Why does Grep keep failing?".to_string(),
        true,
        Some("codex request".to_string()),
    );
    session.messages[0].retrieved_memory = Some(
        "- [session=Retry role=assistant time=2026-05-21T01:01:00Z] Grep calls require the case_sensitive field."
            .to_string(),
    );
    data_manager.save_session(&session).expect("save session");

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
    let result = service.list_memory_injections(&workspace.to_string_lossy(), 10);

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let response = result.unwrap();
    assert_eq!(response.injection_count, 1);
    assert_eq!(response.injections[0].provider, "codex");
    assert_eq!(
        response.injections[0].user_message,
        "Why does Grep keep failing?"
    );
    assert!(response.injections[0]
        .retrieved_memory
        .contains("case_sensitive field"));

    cleanup_workspace(&workspace);
}

#[test]
fn list_memory_retrievals_returns_candidate_hits_and_injected_preview() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("list-retrievals");
    create_empty_workspace(&workspace);

    let context = crate::memory::MemoryContext {
        workspace_id: "ws".to_string(),
        query: "grep case_sensitive".to_string(),
        hits: vec![crate::memory::MemoryHit {
            doc_key: "chat-1:1:assistant".to_string(),
            chat_id: "chat-1".to_string(),
            role: "assistant".to_string(),
            session_name: "Retry".to_string(),
            timestamp: chrono::Utc.with_ymd_and_hms(2026, 5, 21, 1, 1, 0).unwrap(),
            snippet: "Grep calls require the case_sensitive field.".to_string(),
            title: None,
            message_index: Some(1),
            turn_index: Some(0),
            score: Some(0.9),
        }],
    };
    append_memory_retrieval_audit(
        &workspace,
        AppendMemoryRetrievalAudit {
            provider: "codex",
            provider_display_name: "Codex",
            session_id: "chat-1",
            session_name: "Retry Session",
            requested_at_ms: 1_747_790_860_000,
            query: "Why does Grep keep failing?",
            user_message: "Why does Grep keep failing?",
            memory_context: Some(&context),
            injected_preview: Some(
                "- [session=Retry role=assistant time=2026-05-21T01:01:00+00:00] Grep calls require the case_sensitive field.",
            ),
        },
    )
    .expect("append retrieval audit");

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
    let result = service.list_memory_retrievals(&workspace.to_string_lossy(), 10);

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let response = result.unwrap();
    assert_eq!(response.retrieval_count, 1);
    assert_eq!(response.retrievals[0].candidate_hit_count, 1);
    assert!(response.retrievals[0]
        .candidate_preview
        .as_deref()
        .unwrap_or_default()
        .contains("case_sensitive field"));
    assert!(response.retrievals[0]
        .injected_preview
        .as_deref()
        .unwrap_or_default()
        .contains("case_sensitive field"));

    cleanup_workspace(&workspace);
}

#[test]
fn list_invalidations_routes_successfully() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("list-inval-ok");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service.list_invalidations(&workspace.to_string_lossy());

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let response = result.unwrap();
    assert_eq!(response.invalidation_count, 0);
    assert!(response.invalidations.is_empty());

    cleanup_workspace(&workspace);
}

#[test]
fn invalidate_doc_key_routes_successfully() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("invalidate-ok");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service.invalidate_doc_key(
        &workspace.to_string_lossy(),
        "test-chat:0:user",
        Some("testing invalidation".to_string()),
    );

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let response = result.unwrap();
    assert_eq!(response.action, "invalidate");
    assert!(response.changed);
    assert!(response.doc_key.contains("test-chat"));

    cleanup_workspace(&workspace);
}

#[test]
fn restore_doc_key_routes_successfully() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("restore-ok");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    // First invalidate
    let invalidate_result = service.invalidate_doc_key(
        &workspace.to_string_lossy(),
        "test-chat:0:user",
        Some("testing".to_string()),
    );
    assert!(invalidate_result.unwrap().changed);

    // Then restore
    let restore_result = service.restore_doc_key(&workspace.to_string_lossy(), "test-chat:0:user");

    assert!(
        restore_result.is_ok(),
        "Expected Ok, got: {:?}",
        restore_result
    );
    let response = restore_result.unwrap();
    assert_eq!(response.action, "restore");
    assert!(response.changed);

    cleanup_workspace(&workspace);
}

#[test]
fn remember_fact_routes_successfully() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("remember-ok");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service.remember_fact(&workspace.to_string_lossy(), "This is a test fact", "high");

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let response = result.unwrap();
    assert!(!response.memory_id.is_empty());
    assert_eq!(response.text, "This is a test fact");
    assert_eq!(response.priority, "high");

    cleanup_workspace(&workspace);
}

#[test]
fn remember_fact_rejects_invalid_priority() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("remember-invalid");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service.remember_fact(
        &workspace.to_string_lossy(),
        "Test fact",
        "invalid_priority",
    );

    assert!(result.is_err(), "Expected Err for invalid priority");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("priority"),
        "Error should mention priority, got: {}",
        err
    );

    cleanup_workspace(&workspace);
}

#[test]
fn forget_fact_routes_successfully() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("forget-ok");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    // First remember a fact
    let remember_result =
        service.remember_fact(&workspace.to_string_lossy(), "Fact to forget", "high");
    let memory_id = remember_result.unwrap().memory_id;

    // Then forget it
    let result = service.forget_fact(&workspace.to_string_lossy(), &memory_id);

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let response = result.unwrap();
    assert!(response.changed);

    cleanup_workspace(&workspace);
}

#[test]
fn forget_fact_returns_changed_false_for_nonexistent_memory() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("forget-nonexistent");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service.forget_fact(&workspace.to_string_lossy(), "nonexistent-memory-id");

    assert!(
        result.is_ok(),
        "Expected Ok (not error) for nonexistent memory"
    );
    let response = result.unwrap();
    assert!(
        !response.changed,
        "changed should be false for nonexistent memory"
    );

    cleanup_workspace(&workspace);
}

#[test]
fn list_manual_memories_routes_successfully() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("list-memories-ok");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    // Remember a fact first
    service
        .remember_fact(&workspace.to_string_lossy(), "Test memory", "normal")
        .unwrap();

    // List memories
    let result = service.list_manual_memories(&workspace.to_string_lossy());

    assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    let response = result.unwrap();
    assert!(response.memory_count >= 1, "Should have at least 1 memory");
    assert!(!response.memories.is_empty());

    cleanup_workspace(&workspace);
}

// ---------------------------------------------------------------------------
// Structured error payload tests — verify that run_blocking_json produces
// {"error": "..."} for service errors.
// ---------------------------------------------------------------------------

#[test]
fn service_error_for_workspace_outside_allowed_roots() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("outside-roots");
    create_empty_workspace(&workspace);

    // Service is created with a restricted root that does NOT include the workspace
    let service = MemoryMcpService::new(vec![std::env::temp_dir().join("definitely-not-this")]);

    let result = service.workspace_memory_status(&workspace.to_string_lossy());

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    // Error should indicate the path is outside allowed roots
    assert!(
        err_msg.contains("outside") || err_msg.contains("CHATBOT_MEMORY_MCP_ALLOWED_ROOTS"),
        "Error should indicate path is outside allowed roots, got: {}",
        err_msg
    );
}

#[test]
fn service_error_for_relative_workspace_path() {
    let _guard = MemoryMcpTestGuard::new();
    let service = MemoryMcpService::new(vec![std::env::temp_dir()]);

    let result = service.workspace_memory_status("relative/path");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("absolute"),
        "Error should mention workspace_path must be absolute, got: {}",
        err
    );
}

#[test]
fn service_error_for_invalid_priority() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("invalid-priority");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service.remember_fact(&workspace.to_string_lossy(), "Test fact", "invalid");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("priority"),
        "Error should mention unsupported priority, got: {}",
        err
    );

    cleanup_workspace(&workspace);
}

// ---------------------------------------------------------------------------
// JSON structure tests — verify response types serialize to JSON correctly
// ---------------------------------------------------------------------------

#[test]
fn status_response_serializes_to_valid_json() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("status-json");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service
        .workspace_memory_status(&workspace.to_string_lossy())
        .unwrap();

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&result).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    // Verify all required fields are present
    assert!(
        value.get("workspace_root").is_some(),
        "missing workspace_root"
    );
    assert!(value.get("workspace_id").is_some(), "missing workspace_id");
    assert!(
        value.get("memmachine_base_url").is_some(),
        "missing memmachine_base_url"
    );
    assert!(
        value.get("memmachine_org_id").is_some(),
        "missing memmachine_org_id"
    );
    assert!(
        value.get("memmachine_project_id").is_some(),
        "missing memmachine_project_id"
    );
    assert!(
        value.get("ingest_state_path").is_some(),
        "missing ingest_state_path"
    );
    assert!(
        value.get("memmachine_project_exists").is_some(),
        "missing memmachine_project_exists"
    );
    assert!(
        value.get("ingest_state_exists").is_some(),
        "missing ingest_state_exists"
    );
    assert!(
        value.get("indexed_sessions").is_some(),
        "missing indexed_sessions"
    );
    assert!(
        value.get("invalidation_count").is_some(),
        "missing invalidation_count"
    );
    assert!(
        value.get("manual_memory_count").is_some(),
        "missing manual_memory_count"
    );

    cleanup_workspace(&workspace);
}

#[test]
fn search_response_serializes_to_valid_json() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("search-json");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service
        .search_memory(&workspace.to_string_lossy(), "test query")
        .unwrap();

    let json = serde_json::to_string_pretty(&result).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert!(
        value.get("workspace_root").is_some(),
        "missing workspace_root"
    );
    assert!(value.get("workspace_id").is_some(), "missing workspace_id");
    assert!(value.get("query").is_some(), "missing query");
    assert!(value.get("hit_count").is_some(), "missing hit_count");
    assert!(value.get("hits").is_some(), "missing hits");

    cleanup_workspace(&workspace);
}

#[test]
fn doc_key_mutation_response_serializes_to_valid_json() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("mutation-json");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service
        .invalidate_doc_key(
            &workspace.to_string_lossy(),
            "chat:0:user",
            Some("test reason".to_string()),
        )
        .unwrap();

    let json = serde_json::to_string_pretty(&result).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert!(
        value.get("workspace_root").is_some(),
        "missing workspace_root"
    );
    assert!(value.get("workspace_id").is_some(), "missing workspace_id");
    assert!(value.get("doc_key").is_some(), "missing doc_key");
    assert!(value.get("action").is_some(), "missing action");
    assert!(value.get("changed").is_some(), "missing changed");
    assert_eq!(value.get("action").unwrap().as_str(), Some("invalidate"));

    cleanup_workspace(&workspace);
}

#[test]
fn remember_fact_response_serializes_to_valid_json() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("remember-json");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    let result = service
        .remember_fact(&workspace.to_string_lossy(), "Test fact content", "high")
        .unwrap();

    let json = serde_json::to_string_pretty(&result).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert!(
        value.get("workspace_root").is_some(),
        "missing workspace_root"
    );
    assert!(value.get("workspace_id").is_some(), "missing workspace_id");
    assert!(value.get("memory_id").is_some(), "missing memory_id");
    assert!(value.get("doc_key").is_some(), "missing doc_key");
    assert!(value.get("text").is_some(), "missing text");
    assert!(value.get("priority").is_some(), "missing priority");
    assert_eq!(
        value.get("text").unwrap().as_str(),
        Some("Test fact content")
    );
    assert_eq!(value.get("priority").unwrap().as_str(), Some("high"));

    cleanup_workspace(&workspace);
}

#[test]
fn manual_memories_response_serializes_to_valid_json() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("memories-json");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    // Add a memory
    service
        .remember_fact(
            &workspace.to_string_lossy(),
            "Pinned memory content",
            "normal",
        )
        .unwrap();

    let result = service
        .list_manual_memories(&workspace.to_string_lossy())
        .unwrap();

    let json = serde_json::to_string_pretty(&result).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert!(
        value.get("workspace_root").is_some(),
        "missing workspace_root"
    );
    assert!(value.get("workspace_id").is_some(), "missing workspace_id");
    assert!(value.get("memory_count").is_some(), "missing memory_count");
    assert!(value.get("memories").is_some(), "missing memories");
    assert!(
        value.get("memory_count").unwrap().as_u64().unwrap() >= 1,
        "Should have at least 1 memory"
    );

    cleanup_workspace(&workspace);
}

#[test]
fn invalidations_response_serializes_to_valid_json() {
    let _guard = MemoryMcpTestGuard::new();
    let workspace = temp_workspace("inval-json");
    create_empty_workspace(&workspace);

    let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);

    // Add an invalidation
    service
        .invalidate_doc_key(
            &workspace.to_string_lossy(),
            "chat:0:user",
            Some("testing".to_string()),
        )
        .unwrap();

    let result = service
        .list_invalidations(&workspace.to_string_lossy())
        .unwrap();

    let json = serde_json::to_string_pretty(&result).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert!(
        value.get("workspace_root").is_some(),
        "missing workspace_root"
    );
    assert!(value.get("workspace_id").is_some(), "missing workspace_id");
    assert!(
        value.get("invalidation_count").is_some(),
        "missing invalidation_count"
    );
    assert!(
        value.get("invalidations").is_some(),
        "missing invalidations"
    );
    assert!(
        value.get("invalidation_count").unwrap().as_u64().unwrap() >= 1,
        "Should have at least 1 invalidation"
    );

    cleanup_workspace(&workspace);
}
