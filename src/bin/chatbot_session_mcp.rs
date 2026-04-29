use chatbot_lib::session_mcp::handler::ChatbotSessionMcpHandler;
use chatbot_lib::session_mcp::service::SessionMcpService;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    chatbot_lib::process_limits::apply_default_process_memory_limit("chatbot-session-mcp");
    let _log_guard = chatbot_lib::debug::init();
    let _debug_guard = rust_debug::init_debug_defaults("chatbot-session-mcp");
    let handler = ChatbotSessionMcpHandler::new(SessionMcpService::from_default_data_dirs()?);

    let service = handler.serve(rmcp::transport::io::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
