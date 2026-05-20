#[path = "mock_provider_server.rs"]
mod mock_provider_server;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc, Mutex};
use chatbot_lib::data::{ChatMessage, ChatSession};
use chatbot_lib::memory::{MemoryContext, MemoryHit};
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
