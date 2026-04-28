use crate::session_mcp::service::SessionMcpService;
use crate::session_mcp::tools::InspectSessionInput;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, InitializeResult, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use serde::Serialize;

#[derive(Clone)]
pub struct ChatbotSessionMcpHandler {
    service: SessionMcpService,
    tool_router: ToolRouter<Self>,
}

impl ChatbotSessionMcpHandler {
    pub fn new(service: SessionMcpService) -> Self {
        Self {
            service,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl ChatbotSessionMcpHandler {
    /// Inspect a rust-chatbot session by full id or unique prefix.
    #[tool(name = "inspect_session")]
    async fn inspect_session(&self, Parameters(input): Parameters<InspectSessionInput>) -> String {
        let service = self.service.clone();
        run_blocking_json(move || service.inspect_session(&input.session_id)).await
    }
}

#[tool_handler]
impl ServerHandler for ChatbotSessionMcpHandler {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "chatbot-session-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Session inspection MCP for rust-chatbot. Tools:\n\
                 - inspect_session: inspect a rust-chatbot session by full id or unique prefix",
            )
    }
}

async fn run_blocking_json<F, T>(f: F) -> String
where
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    T: Serialize + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(result)) => to_pretty_json(&result),
        Ok(Err(error)) => format!("Request failed: {error}"),
        Err(error) => format!("Request task panicked: {error}"),
    }
}

fn to_pretty_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::{ChatbotSessionMcpHandler, SessionMcpService};
    use crate::data::{ChatMessage, ChatSession, SessionInfo, SessionMetadata};
    use crate::model::ModelProvider;
    use crate::session_mcp::service::SessionStore;
    use chrono::Utc;
    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rust-chatbot-session-handler-test-{}-{}",
            label, nanos
        ))
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

    // -------------------------------------------------------------------------
    // Success formatting tests — verify handler returns valid JSON on success
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn inspect_session_handler_returns_valid_json_on_success() {
        let root = temp_dir("handler-success");
        let data_dir = root.join("claude");
        write_session(&data_dir, "handler-test-session-001", false);

        let service = SessionMcpService::new(vec![SessionStore::new(
            ModelProvider::Claude,
            data_dir.clone(),
        )]);
        let handler = ChatbotSessionMcpHandler::new(service);

        let input = Parameters(crate::session_mcp::tools::InspectSessionInput {
            session_id: "handler-test-session-001".to_string(),
        });

        let response = handler.inspect_session(input).await;

        // Response should be valid JSON
        let value: Value =
            serde_json::from_str(&response).expect("Handler success response should be valid JSON");

        // Should have all expected fields
        assert!(value.get("input_session_id").is_some());
        assert!(value.get("matched_by").is_some());
        assert!(value.get("provider").is_some());
        assert!(value.get("session_id").is_some());
        assert!(value.get("hidden").is_some());
        assert!(value.get("message_count").is_some());
        assert!(value.get("session").is_some());

        // matched_by should indicate exact match
        assert_eq!(value.get("matched_by").unwrap().as_str(), Some("exact"));

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn inspect_session_handler_routes_by_prefix() {
        let root = temp_dir("handler-prefix");
        let data_dir = root.join("gemini");
        write_session(&data_dir, "handler-prefix-unique-session-999", false);

        let service = SessionMcpService::new(vec![SessionStore::new(
            ModelProvider::Gemini,
            data_dir.clone(),
        )]);
        let handler = ChatbotSessionMcpHandler::new(service);

        let input = Parameters(crate::session_mcp::tools::InspectSessionInput {
            session_id: "handler-prefix-unique".to_string(),
        });

        let response = handler.inspect_session(input).await;

        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value.get("matched_by").unwrap().as_str(), Some("prefix"));
        assert_eq!(value.get("provider").unwrap().as_str(), Some("gemini"));

        fs::remove_dir_all(root).unwrap();
    }

    // -------------------------------------------------------------------------
    // Failure formatting tests — verify handler returns PLAIN STRING errors,
    // NOT structured JSON (contrast with memory_mcp which returns {"error": "..."})
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn inspect_session_handler_returns_plain_string_error_for_nonexistent() {
        let root = temp_dir("handler-notfound");
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
        let handler = ChatbotSessionMcpHandler::new(service);

        let input = Parameters(crate::session_mcp::tools::InspectSessionInput {
            session_id: "nonexistent-session-id".to_string(),
        });

        let response = handler.inspect_session(input).await;

        // session_mcp handler returns plain string errors, NOT JSON
        // The response should NOT be valid JSON with an "error" key
        let parsed: Result<Value, _> = serde_json::from_str(&response);
        assert!(
            parsed.is_err(),
            "session_mcp error should NOT be valid JSON: {}",
            response
        );

        // But it should contain a meaningful error message
        assert!(
            response.contains("Request failed:"),
            "Error should be plain string with 'Request failed:', got: {}",
            response
        );
        assert!(
            response.contains("No rust-chatbot session found"),
            "Error should mention session not found, got: {}",
            response
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn inspect_session_handler_returns_plain_string_error_for_empty_session_id() {
        let service = SessionMcpService::new(vec![]);
        let handler = ChatbotSessionMcpHandler::new(service);

        let input = Parameters(crate::session_mcp::tools::InspectSessionInput {
            session_id: "".to_string(),
        });

        let response = handler.inspect_session(input).await;

        // Should NOT be valid JSON
        let parsed: Result<Value, _> = serde_json::from_str(&response);
        assert!(
            parsed.is_err(),
            "session_mcp error should NOT be valid JSON: {}",
            response
        );

        // Should be a plain string error
        assert!(
            response.contains("Request failed:"),
            "Error should be plain string with 'Request failed:', got: {}",
            response
        );

        // Empty session_id should produce an error mentioning "empty"
        assert!(
            response.contains("empty"),
            "Error should mention empty session_id, got: {}",
            response
        );
    }

    // -------------------------------------------------------------------------
    // Contrast test: session_mcp vs memory_mcp error formatting
    // This test documents the architectural difference.
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn session_mcp_error_differs_from_memory_mcp_error_format() {
        // This test verifies the key architectural difference:
        // - session_mcp handler: errors are plain strings like "Request failed: ..."
        // - memory_mcp handler: errors are structured JSON like {"error": "..."}
        //
        // The session_mcp run_blocking_json does:
        //   Ok(Err(error)) => format!("Request failed: {error}")
        //
        // NOT:
        //   Ok(Err(error)) => to_pretty_json(&ErrorResponse { error: error.to_string() })
        //
        // So attempting to parse session_mcp error as JSON should fail.

        let root = temp_dir("handler-contrast");
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
        let handler = ChatbotSessionMcpHandler::new(service);

        let input = Parameters(crate::session_mcp::tools::InspectSessionInput {
            session_id: "nonexistent".to_string(),
        });

        let response = handler.inspect_session(input).await;

        // Verify it's a plain string error, not JSON
        let is_json = serde_json::from_str::<Value>(&response).is_ok();
        assert!(
            !is_json,
            "session_mcp error should NOT be JSON (contrast with memory_mcp)"
        );

        // Verify it has the expected plain string format
        assert!(
            response.starts_with("Request failed:"),
            "session_mcp error should start with 'Request failed:', got: {}",
            response
        );

        fs::remove_dir_all(root).unwrap();
    }
}
