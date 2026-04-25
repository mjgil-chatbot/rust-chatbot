use chatbot_lib::memory_mcp::config::ServerConfig;
use chatbot_lib::memory_mcp::handler::ChatbotMemoryMcpHandler;
use chatbot_lib::memory_mcp::service::MemoryMcpService;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    chatbot_lib::process_limits::apply_default_process_memory_limit("chatbot-memory-mcp");
    let _log_guard = chatbot_lib::debug::init();
    let _debug_guard = rust_debug::init_debug_defaults("chatbot-memory-mcp");
    let config = ServerConfig::from_env()?;
    let handler = ChatbotMemoryMcpHandler::new(MemoryMcpService::new(config.allowed_roots));

    let service = handler.serve(rmcp::transport::io::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
