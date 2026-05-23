#[path = "mock_provider_server.rs"]
mod mock_provider_server;

use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc, Mutex};

use chatbot_lib::data::{ChatMessage, ChatSession};
use chatbot_lib::memory::{MemoryContext, MemoryHit};
use chatbot_lib::model::ModelProvider;
use chatbot_lib::provider_bridge::{
    call_provider_cli_stream_with_overrides, ProviderRequestOverrides,
};
use chrono::Utc;
use mock_provider_server::*;
use serde_json::json;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner())
}

fn make_session() -> ChatSession {
    ChatSession {
        id: "session-1".to_string(),
        name: "Provider bridge coverage".to_string(),
        system_prompt: Some("Persisted system prompt".to_string()),
        json_summary: None,
        claude_session_id: None,
        minimax_session_id: Some("resume-session-77".to_string()),
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
        launched_from: Some(std::env::current_dir().unwrap().display().to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        messages: vec![
            ChatMessage {
                content: "previous user message".to_string(),
                is_user: true,
                timestamp: Utc::now(),
                command: None,
                retrieved_memory: None,
            },
            ChatMessage {
                content: "previous assistant message".to_string(),
                is_user: false,
                timestamp: Utc::now(),
                command: None,
                retrieved_memory: None,
            },
        ],
        runsc_dirs: Vec::new(),
        runsc_image: None,
    }
}

fn make_memory_context() -> MemoryContext {
    MemoryContext {
        workspace_id: "workspace-1".to_string(),
        query: "coverage query".to_string(),
        hits: vec![MemoryHit {
            doc_key: "doc-1".to_string(),
            chat_id: "chat-1".to_string(),
            role: "user".to_string(),
            session_name: "Coverage".to_string(),
            timestamp: Utc::now(),
            snippet: "relevant memory snippet".to_string(),
            title: Some("Memory title".to_string()),
            message_index: Some(3),
            turn_index: Some(2),
            score: Some(0.75),
        }],
    }
}

fn collect_messages(
    rx: mpsc::Receiver<chatbot_lib::stream::StreamMessage>,
) -> Vec<chatbot_lib::stream::StreamMessage> {
    rx.iter().collect()
}

#[test]
fn provider_bridge_stream_builds_request_and_normalizes_success() {
    let _lock = env_lock();
    let server = MockProviderServer::start(vec![ScenarioStep {
        name: "success",
        response: json!({
            "session_id": "stream-session",
            "model": "MiniMax-M2.7",
            "stream_chunks": ["partial ", "output"],
            "stop_reason": "end_turn",
            "usage": usage(21, 7),
            "duration_ms": 18,
            "num_turns": 1,
            "result_text": "stream completed",
            "modelUsage": model_usage(),
        }),
    }]);
    let _harness = WorkerHarness::new("provider-bridge-stream", &server);
    let overrides = ProviderRequestOverrides {
        model: Some("coverage-model".to_string()),
        ..Default::default()
    };
    let (tx, rx) = mpsc::channel();

    let result = call_provider_cli_stream_with_overrides(
        ModelProvider::MiniMax,
        "coverage query",
        Some("Return deterministic provider output."),
        Some(&make_memory_context()),
        Some(make_session()),
        Some(&overrides),
        tx,
        Arc::new(AtomicBool::new(false)),
    );
    let messages = collect_messages(rx);

    assert!(result.is_ok(), "stream call should succeed");
    assert_request_count(&server, 1);
    assert_request_ordering(&server, &["success"]);
    assert_all_requests_path(&server, "/anthropic/v1/messages");
    assert_all_requests_auth(&server, "parity-token");

    let request = &server.requests()[0];
    assert_eq!(request.model(), Some("coverage-model"));
    assert_eq!(request.resume_session_id(), Some("resume-session-77"));
    let user_query = request.user_query().expect("user query");
    assert!(
        user_query.contains("coverage query"),
        "expected request to include original query"
    );
    assert!(
        user_query.contains("relevant memory snippet"),
        "expected request to include memory context"
    );
    assert_eq!(request.tool_result_count(), Some(0));
    assert!(
        request.body["metadata"]["config_dir"].is_string(),
        "expected provider request metadata to include config_dir"
    );

    assert!(
        !messages.is_empty(),
        "expected streamed messages from provider bridge"
    );
    let combined = messages
        .iter()
        .map(|message| message.response.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        (combined.contains("partial") && combined.contains("output"))
            || combined.contains("stream completed"),
        "expected normalized response text, got {combined:?}"
    );
    let final_message = messages
        .iter()
        .find(|message| message.is_final)
        .expect("final message");
    assert_eq!(final_message.session_id.as_deref(), Some("stream-session"));
    assert!(
        final_message.summary.is_some(),
        "expected normalized summary"
    );
}

#[test]
fn provider_bridge_stream_normalizes_provider_error_result() {
    let _lock = env_lock();
    let server = MockProviderServer::start(vec![ScenarioStep {
        name: "error",
        response: json!({
            "session_id": "error-session",
            "model": "MiniMax-M2.7",
            "content": [{"type": "text", "text": "provider reported failure"}],
            "stop_reason": "end_turn",
            "usage": usage(5, 2),
            "duration_ms": 9,
            "num_turns": 1,
            "is_error": true,
            "result_text": "provider reported failure",
        }),
    }]);
    let _harness = WorkerHarness::new("provider-bridge-error", &server);
    let (tx, rx) = mpsc::channel();

    let result = call_provider_cli_stream_with_overrides(
        ModelProvider::MiniMax,
        "error normalization query",
        Some("Return deterministic provider output."),
        None,
        Some(make_session()),
        None,
        tx,
        Arc::new(AtomicBool::new(false)),
    );
    let messages = collect_messages(rx);

    assert!(
        result.is_ok(),
        "provider error should be normalized into output"
    );
    assert_request_count(&server, 1);
    assert_request_ordering(&server, &["error"]);

    let combined = messages
        .iter()
        .map(|message| message.response.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(
        combined.contains("provider reported failure"),
        "expected provider error text in normalized output, got {combined:?}"
    );
    assert!(
        messages.iter().any(|message| message.is_final),
        "expected final message for provider error path"
    );
}
