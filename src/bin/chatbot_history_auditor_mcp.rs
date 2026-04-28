use anyhow::Result;
use chatbot_lib::data::ChatSession;
use chatbot_lib::session_mcp::service::SessionMcpService;
use history_auditor_core::service::HistoryAuditorService;
use history_auditor_core::session::{
    InspectedSession, SessionMessage, SessionSnapshot, SessionSource,
};
use history_auditor_mcp::handler::HistoryAuditorMcpHandler;
use rmcp::ServiceExt;

#[derive(Clone, Debug)]
struct RustChatbotSessionSource {
    inner: SessionMcpService,
}

impl RustChatbotSessionSource {
    fn from_default_data_dirs() -> Result<Self> {
        Ok(Self {
            inner: SessionMcpService::from_default_data_dirs()?,
        })
    }
}

impl SessionSource for RustChatbotSessionSource {
    fn inspect_session(&self, session_id: &str) -> Result<InspectedSession> {
        let inspected = self.inner.inspect_session(session_id)?;
        Ok(InspectedSession {
            provider: inspected.provider,
            provider_display_name: inspected.provider_display_name,
            session_id: inspected.session_id,
            session_path: inspected.session_path,
            session: normalize_session_snapshot(inspected.session),
        })
    }
}

fn normalize_session_snapshot(session: ChatSession) -> SessionSnapshot {
    SessionSnapshot {
        id: session.id,
        name: session.name,
        launched_from: session.launched_from,
        messages: session
            .messages
            .into_iter()
            .map(|message| SessionMessage {
                content: message.content,
                is_user: message.is_user,
                timestamp: message.timestamp,
            })
            .collect(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    chatbot_lib::process_limits::apply_default_process_memory_limit("chatbot-history-auditor-mcp");
    let _log_guard = chatbot_lib::debug::init();
    let _debug_guard = rust_debug::init_debug_defaults("chatbot-history-auditor-mcp");
    let handler = HistoryAuditorMcpHandler::new(HistoryAuditorService::new(
        RustChatbotSessionSource::from_default_data_dirs()?,
    ));

    let service = handler.serve(rmcp::transport::io::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
