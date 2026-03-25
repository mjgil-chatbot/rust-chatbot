use anyhow::Result;
use super::*;
pub(super) fn resolve_snapshot_current_session_id<'a>(
    sessions: &'a [SessionInfo],
    preferred_session_id: Option<&str>,
    fallback_to_first: bool,
) -> Option<&'a str> {
    preferred_session_id
        .and_then(|session_id| sessions.iter().find(|session| session.id == session_id))
        .or_else(|| {
            fallback_to_first
                .then_some(())
                .and_then(|()| sessions.first())
        })
        .map(|session| session.id.as_str())
}
fn resolve_snapshot_current_session(
    data_manager: &DataManager,
    sessions: &[SessionInfo],
    preferred_session_id: Option<&str>,
    explicit_current: Option<ChatSession>,
    fallback_to_first: bool,
) -> Option<ChatSession> {
    if let Some(session) = explicit_current {
        return Some(session);
    }

    let session_id =
        resolve_snapshot_current_session_id(sessions, preferred_session_id, fallback_to_first)?;
    data_manager.load_session(session_id).ok()
}
fn load_session_snapshot_parts(
    provider: ModelProvider,
    preferred_session_id: Option<String>,
    explicit_current: Option<ChatSession>,
    fallback_to_first: bool,
) -> Result<(Vec<SessionInfo>, Option<ChatSession>)> {
    let data_manager = DataManager::new(provider)?;
    let sessions = data_manager.list_sessions()?;
    let current_session = resolve_snapshot_current_session(
        &data_manager,
        &sessions,
        preferred_session_id.as_deref(),
        explicit_current,
        fallback_to_first,
    );

    Ok((sessions, current_session))
}
pub(super) fn load_selected_session(
    provider: ModelProvider,
    preferred_session_id: Option<String>,
    explicit_current: Option<ChatSession>,
    fallback_to_first: bool,
    requested_session_id: Option<String>,
) -> Result<SelectedSessionLoad> {
    let (sessions, current_session) = load_session_snapshot_parts(
        provider,
        preferred_session_id,
        explicit_current,
        fallback_to_first,
    )?;
    Ok(SelectedSessionLoad {
        sessions: Some(sessions),
        current_session,
        requested_session_id,
    })
}
