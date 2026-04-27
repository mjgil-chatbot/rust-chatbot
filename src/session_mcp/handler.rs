use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, InitializeResult, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use serde::Serialize;
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
