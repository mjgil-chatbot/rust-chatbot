use anyhow::Result;
use chatbot_lib::data::ChatSession;
use history_auditor_core::service::HistoryAuditorService;
use history_auditor_core::session::{
    InspectedSession, SessionMessage, SessionSnapshot, SessionSource,
};
use history_auditor_mcp::handler::HistoryAuditorMcpHandler;
use rmcp::ServiceExt;
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
fn main() {}
