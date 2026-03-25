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

pub(super) fn load_session_list_patch(provider: ModelProvider) -> Result<SessionListPatch> {
    let data_manager = DataManager::new(provider)?;
    Ok(SessionListPatch {
        sessions: data_manager.list_sessions()?,
    })
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

pub(super) fn load_selected_session_after_delete(
    provider: ModelProvider,
    deleted_session_id: &str,
    preferred_session_id: Option<String>,
    requested_session_id: Option<String>,
) -> Result<SelectedSessionLoad> {
    let data_manager = DataManager::new(provider)?;
    let mut sessions = data_manager.list_sessions()?;
    sessions.retain(|session| session.id != deleted_session_id);

    let current_session = resolve_snapshot_current_session(
        &data_manager,
        &sessions,
        preferred_session_id.as_deref(),
        None,
        true,
    );
    let requested_session_id = requested_session_id
        .filter(|session_id| session_id != deleted_session_id)
        .or_else(|| current_session.as_ref().map(|session| session.id.clone()));

    Ok(SelectedSessionLoad {
        sessions: Some(sessions),
        current_session,
        requested_session_id,
    })
}

pub(super) fn save_session_in_background(
    provider: ModelProvider,
    session: ChatSession,
) -> Result<()> {
    let data_manager = DataManager::new(provider)?;
    data_manager.save_session(&session)
}
