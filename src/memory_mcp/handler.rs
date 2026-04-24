use crate::memory_mcp::service::MemoryMcpService;
use crate::memory_mcp::tools::{
    ForgetFactInput, InvalidateDocKeyInput, ListMemoryInjectionsInput, ListMemoryRetrievalsInput,
    RememberFactInput, RestoreDocKeyInput, SearchMemoryInput, WorkspaceMemoryStatusInput,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, InitializeResult, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use serde::Serialize;

#[derive(Clone)]
pub struct ChatbotMemoryMcpHandler {
    service: MemoryMcpService,
    tool_router: ToolRouter<Self>,
}

impl ChatbotMemoryMcpHandler {
    pub fn new(service: MemoryMcpService) -> Self {
        Self {
            service,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl ChatbotMemoryMcpHandler {
    /// Inspect the current MemMachine-backed memory state for a workspace.
    #[tool(name = "get_workspace_memory_status")]
    async fn get_workspace_memory_status(
        &self,
        Parameters(input): Parameters<WorkspaceMemoryStatusInput>,
    ) -> String {
        let service = self.service.clone();
        run_blocking_json(move || service.workspace_memory_status(&input.workspace_path)).await
    }

    /// Search the current workspace memory the same way the app does before prompt injection.
    #[tool(name = "search_memory")]
    async fn search_memory(&self, Parameters(input): Parameters<SearchMemoryInput>) -> String {
        let service = self.service.clone();
        run_blocking_json(move || service.search_memory(&input.workspace_path, &input.query)).await
    }

    /// List recent retrieved-memory injections persisted on workspace user turns.
    #[tool(name = "list_memory_injections")]
    async fn list_memory_injections(
        &self,
        Parameters(input): Parameters<ListMemoryInjectionsInput>,
    ) -> String {
        let service = self.service.clone();
        run_blocking_json(move || {
            service.list_memory_injections(&input.workspace_path, input.limit)
        })
        .await
    }

    /// List recent memory retrieval attempts, including candidate hits and injected preview.
    #[tool(name = "list_memory_retrievals")]
    async fn list_memory_retrievals(
        &self,
        Parameters(input): Parameters<ListMemoryRetrievalsInput>,
    ) -> String {
        let service = self.service.clone();
        run_blocking_json(move || {
            service.list_memory_retrievals(&input.workspace_path, input.limit)
        })
        .await
    }

    /// List doc_key invalidations currently applied to a workspace memory store.
    #[tool(name = "list_invalidations")]
    async fn list_invalidations(
        &self,
        Parameters(input): Parameters<WorkspaceMemoryStatusInput>,
    ) -> String {
        let service = self.service.clone();
        run_blocking_json(move || service.list_invalidations(&input.workspace_path)).await
    }

    /// Mark a memory doc_key as invalid so future retrieval skips it.
    #[tool(name = "invalidate_doc_key")]
    async fn invalidate_doc_key(
        &self,
        Parameters(input): Parameters<InvalidateDocKeyInput>,
    ) -> String {
        let service = self.service.clone();
        run_blocking_json(move || {
            service.invalidate_doc_key(&input.workspace_path, &input.doc_key, input.reason)
        })
        .await
    }

    /// Persist an explicit workspace fact so retrieval can surface it at high priority.
    #[tool(name = "remember_fact")]
    async fn remember_fact(&self, Parameters(input): Parameters<RememberFactInput>) -> String {
        let service = self.service.clone();
        run_blocking_json(move || {
            service.remember_fact(&input.workspace_path, &input.fact, &input.priority)
        })
        .await
    }

    /// Remove a previously pinned manual memory by its memory_id.
    #[tool(name = "forget_fact")]
    async fn forget_fact(&self, Parameters(input): Parameters<ForgetFactInput>) -> String {
        let service = self.service.clone();
        run_blocking_json(move || service.forget_fact(&input.workspace_path, &input.memory_id))
            .await
    }

    /// List explicit manual memories stored for a workspace.
    #[tool(name = "list_manual_memories")]
    async fn list_manual_memories(
        &self,
        Parameters(input): Parameters<WorkspaceMemoryStatusInput>,
    ) -> String {
        let service = self.service.clone();
        run_blocking_json(move || service.list_manual_memories(&input.workspace_path)).await
    }

    /// Remove a doc_key invalidation so the memory can be returned again.
    #[tool(name = "restore_doc_key")]
    async fn restore_doc_key(&self, Parameters(input): Parameters<RestoreDocKeyInput>) -> String {
        let service = self.service.clone();
        run_blocking_json(move || service.restore_doc_key(&input.workspace_path, &input.doc_key))
            .await
    }
}

#[tool_handler]
impl ServerHandler for ChatbotMemoryMcpHandler {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "chatbot-memory-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Workspace memory MCP for rust-chatbot. Tools:\\n\
                 - get_workspace_memory_status: inspect the workspace memory store\\n\
                 - search_memory: search workspace memory the same way the app does\\n\
                 - list_memory_injections: inspect which retrieved-memory previews were injected into recent user turns\\n\
                 - list_memory_retrievals: inspect candidate memory hits and injected previews for recent send attempts\\n\
                 - remember_fact: persist an explicit pinned memory fact\\n\
                 - forget_fact: remove a pinned manual memory fact\\n\
                 - list_manual_memories: inspect pinned manual memory facts\\n\
                 - list_invalidations: show invalidated doc_keys for a workspace\\n\
                 - invalidate_doc_key: hide a doc_key from future retrieval\\n\
                 - restore_doc_key: undo a prior invalidation",
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
        Ok(Err(error)) => {
            // Return structured error that MCP clients can distinguish from success
            #[derive(Serialize)]
            struct ErrorResponse {
                error: String,
            }
            to_pretty_json(&ErrorResponse {
                error: error.to_string(),
            })
        }
        Err(error) => {
            #[derive(Serialize)]
            struct ErrorResponse {
                error: String,
            }
            to_pretty_json(&ErrorResponse {
                error: format!("Request task panicked: {}", error),
            })
        }
    }
}

fn to_pretty_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::{ChatbotMemoryMcpHandler, MemoryMcpService, WorkspaceMemoryStatusInput};
    use crate::memory::{
        IngestState, ManualMemoriesState, INGEST_STATE_FILE_NAME, MANUAL_MEMORIES_FILE_NAME,
    };
    use crate::memory_mcp::tools::{
        InvalidateDocKeyInput, ListMemoryInjectionsInput, ListMemoryRetrievalsInput,
    };
    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::Value;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct MemoryMcpHandlerTestGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        _home_dir: tempfile::TempDir,
        previous_home: Option<OsString>,
    }

    impl MemoryMcpHandlerTestGuard {
        fn new() -> Self {
            let lock = crate::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|error| error.into_inner());
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

    impl Drop for MemoryMcpHandlerTestGuard {
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
        // Match ServerConfig's canonical roots on platforms with directory aliases.
        std::env::temp_dir()
            .canonicalize()
            .expect("canonical temporary directory")
            .join(format!(
                "rust-chatbot-memory-handler-test-{}-{}",
                label, nanos
            ))
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

    fn parse_json(s: &str) -> Value {
        serde_json::from_str(s).unwrap_or_else(|_| panic!("Expected valid JSON, got: {}", s))
    }

    fn assert_success_json(json_str: &str) -> Value {
        let value = parse_json(json_str);
        assert!(
            value.get("error").is_none(),
            "Success response should not have 'error' key: {}",
            json_str
        );
        value
    }

    fn assert_error_json(json_str: &str) -> Value {
        let value = parse_json(json_str);
        let error = value
            .get("error")
            .unwrap_or_else(|| panic!("Error response should have 'error' key: {}", json_str));
        assert!(
            error.is_string() && !error.as_str().unwrap_or("").is_empty(),
            "error field should be a non-empty string: {:?}",
            error
        );
        value
    }

    // -------------------------------------------------------------------------
    // Handler routing tests — verify each tool returns structured JSON on success
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn get_workspace_memory_status_routes_to_success_json() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-status");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .get_workspace_memory_status(Parameters(WorkspaceMemoryStatusInput {
                workspace_path: workspace.to_string_lossy().to_string(),
            }))
            .await;

        let value = assert_success_json(&response);
        assert!(value.get("workspace_root").is_some());
        assert!(value.get("workspace_id").is_some());
        assert!(value.get("memmachine_base_url").is_some());
        assert!(value.get("indexed_sessions").is_some());

        cleanup_workspace(&workspace);
    }

    #[tokio::test]
    async fn search_memory_routes_to_success_json() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-search");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .search_memory(Parameters(crate::memory_mcp::tools::SearchMemoryInput {
                workspace_path: workspace.to_string_lossy().to_string(),
                query: "test query".to_string(),
            }))
            .await;

        let value = assert_success_json(&response);
        assert_eq!(value.get("query").unwrap().as_str(), Some("test query"));
        assert!(value.get("hit_count").is_some());
        assert!(value.get("hits").is_some());

        cleanup_workspace(&workspace);
    }

    #[tokio::test]
    async fn list_memory_injections_routes_to_success_json() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-list-injections");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .list_memory_injections(Parameters(ListMemoryInjectionsInput {
                workspace_path: workspace.to_string_lossy().to_string(),
                limit: 5,
            }))
            .await;

        let value = assert_success_json(&response);
        assert!(value.get("injection_count").is_some());
        assert!(value.get("injections").is_some());

        cleanup_workspace(&workspace);
    }

    #[tokio::test]
    async fn list_memory_retrievals_routes_to_success_json() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-list-retrievals");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .list_memory_retrievals(Parameters(ListMemoryRetrievalsInput {
                workspace_path: workspace.to_string_lossy().to_string(),
                limit: 5,
            }))
            .await;

        let value = assert_success_json(&response);
        assert!(value.get("retrieval_count").is_some());
        assert!(value.get("retrieval_audit_path").is_some());
        assert!(value.get("retrievals").is_some());

        cleanup_workspace(&workspace);
    }

    #[tokio::test]
    async fn list_invalidations_routes_to_success_json() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-list-inval");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .list_invalidations(Parameters(WorkspaceMemoryStatusInput {
                workspace_path: workspace.to_string_lossy().to_string(),
            }))
            .await;

        let value = assert_success_json(&response);
        assert!(value.get("invalidations").is_some());

        cleanup_workspace(&workspace);
    }

    #[tokio::test]
    async fn invalidate_doc_key_routes_to_success_json() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-invalidate");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .invalidate_doc_key(Parameters(InvalidateDocKeyInput {
                workspace_path: workspace.to_string_lossy().to_string(),
                doc_key: "test-chat:0:user".to_string(),
                reason: Some("testing".to_string()),
            }))
            .await;

        let value = assert_success_json(&response);
        assert_eq!(value.get("action").unwrap().as_str(), Some("invalidate"));
        assert!(value.get("changed").is_some());

        cleanup_workspace(&workspace);
    }

    #[tokio::test]
    async fn remember_fact_routes_to_success_json() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-remember");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .remember_fact(Parameters(crate::memory_mcp::tools::RememberFactInput {
                workspace_path: workspace.to_string_lossy().to_string(),
                fact: "This is a pinned fact".to_string(),
                priority: "high".to_string(),
            }))
            .await;

        let value = assert_success_json(&response);
        assert!(value.get("memory_id").is_some());
        assert_eq!(
            value.get("text").unwrap().as_str(),
            Some("This is a pinned fact")
        );

        cleanup_workspace(&workspace);
    }

    // -------------------------------------------------------------------------
    // Structured error tests — verify run_blocking_json returns {"error": "..."}
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn handler_returns_structured_error_json_for_nonexistent_workspace() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-err-missing");

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .get_workspace_memory_status(Parameters(WorkspaceMemoryStatusInput {
                workspace_path: workspace.to_string_lossy().to_string(),
            }))
            .await;

        let value = assert_error_json(&response);
        let error_msg = value.get("error").unwrap().as_str().unwrap();
        assert!(
            error_msg.contains("outside") || error_msg.contains("Cannot resolve"),
            "Error should mention path issue, got: {}",
            error_msg
        );

        cleanup_workspace(&workspace);
    }

    #[tokio::test]
    async fn handler_returns_structured_error_json_for_relative_path() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-err-relative");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .get_workspace_memory_status(Parameters(WorkspaceMemoryStatusInput {
                workspace_path: "relative/path".to_string(),
            }))
            .await;

        let value = assert_error_json(&response);
        let error_msg = value.get("error").unwrap().as_str().unwrap();
        assert!(
            error_msg.contains("absolute"),
            "Error should mention absolute path, got: {}",
            error_msg
        );

        cleanup_workspace(&workspace);
    }

    #[tokio::test]
    async fn handler_returns_structured_error_json_for_invalid_priority() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-err-priority");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .remember_fact(Parameters(crate::memory_mcp::tools::RememberFactInput {
                workspace_path: workspace.to_string_lossy().to_string(),
                fact: "Test fact".to_string(),
                priority: "invalid".to_string(),
            }))
            .await;

        let value = assert_error_json(&response);
        let error_msg = value.get("error").unwrap().as_str().unwrap();
        assert!(
            error_msg.contains("priority"),
            "Error should mention priority, got: {}",
            error_msg
        );

        cleanup_workspace(&workspace);
    }

    #[tokio::test]
    async fn error_response_is_valid_json_with_error_key() {
        let _guard = MemoryMcpHandlerTestGuard::new();
        let workspace = temp_workspace("h-err-json");
        create_empty_workspace(&workspace);

        let service = MemoryMcpService::new(vec![workspace.parent().unwrap().to_path_buf()]);
        let handler = ChatbotMemoryMcpHandler::new(service);

        let response = handler
            .get_workspace_memory_status(Parameters(WorkspaceMemoryStatusInput {
                workspace_path: "/nonexistent/path".to_string(),
            }))
            .await;

        // Should be valid JSON
        let value: Value =
            serde_json::from_str(&response).expect("memory_mcp error should be valid JSON");
        assert!(
            value.get("error").is_some(),
            "memory_mcp error should have 'error' key"
        );

        cleanup_workspace(&workspace);
    }
}
