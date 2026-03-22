use std::sync::mpsc::TryRecvError;
use ai_history_core::{Actor, BranchId, CheckpointId, SessionId, TurnId};
use ai_history_store::{
    CheckpointInfo as HistoryCheckpointInfo, ManifestInfo as HistoryManifestInfo, MessageRecord,
    HistoryStore as SqliteStore, SessionInfo as HistorySessionInfo,
    TurnInfo as HistoryTurnInfo,
};
use ai_history_ui_model::{
    BranchItem, BranchTimelineView, CheckpointItem, CheckpointListView, TimelineItem, TimelineView,
};
const HISTORY_PANEL_TURN_LIMIT: usize = 24;
use self::history_support::checkpoint_display_name;
use self::history_support::deserialize_manifest;
use self::history_support::is_auto_turn_checkpoint_reason;
use self::history_support::truncate_preview;
pub(crate) use self::history_repo_capture::{
    CheckpointRepoSummary, RepoPatchCaptureState, RepoPatchSummary,
};
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HistoryRequestKey {
    workspace_root: String,
    chat_id: Option<String>,
    chat_name: Option<String>,
    chat_fingerprint: Option<u64>,
    force_create: bool,
}
#[derive(Clone, Debug)]
struct HistoryRefreshRequest {
    key: HistoryRequestKey,
    chat: Option<ChatSession>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct HistorySnapshot {
    pub(crate) workspace_root: Option<String>,
    pub(crate) store_path: Option<String>,
    pub(crate) has_store: bool,
    pub(crate) linked_session_title: Option<String>,
    pub(crate) total_sessions: usize,
    pub(crate) branch_view: Option<BranchTimelineView>,
    pub(crate) checkpoint_view: Option<CheckpointListView>,
    pub(crate) checkpoint_repo_summaries: Vec<CheckpointRepoSummary>,
    pub(crate) timeline_view: Option<TimelineView>,
    pub(crate) object_store_bytes: u64,
    pub(crate) object_store_files: usize,
    pub(crate) auto_checkpoint_count: usize,
    pub(crate) notice: Option<String>,
    pub(crate) error: Option<String>,
}
#[derive(Debug)]
struct HistoryRefreshResult {
    key: HistoryRequestKey,
    snapshot: HistorySnapshot,
}
#[derive(Debug)]
struct HistoryCheckpointResult {
    success_message: String,
}
#[derive(Debug, Default)]
pub(crate) struct HistoryUiState {
    pub(crate) snapshot: HistorySnapshot,
    pub(crate) last_request_key: Option<HistoryRequestKey>,
    pub(crate) refresh_in_flight: bool,
    refresh_started_at: Option<Instant>,
    refresh_rx: Option<mpsc::Receiver<Result<HistoryRefreshResult, (HistoryRequestKey, String)>>>,
    pub(crate) show_checkpoint_dialog: bool,
    pub(crate) checkpoint_name: String,
    pub(crate) checkpoint_in_flight: bool,
    checkpoint_rx: Option<mpsc::Receiver<Result<HistoryCheckpointResult, String>>>,
}
#[derive(Clone, Debug)]
struct ParsedTurn {
    actor: Actor,
    user_content: Option<String>,
    assistant_content: Option<String>,
    timestamp: String,
}
impl HistoryUiState {
    fn set_snapshot(&mut self, key: HistoryRequestKey, snapshot: HistorySnapshot) {
        self.last_request_key = Some(key);
        self.snapshot = snapshot;
    }}
impl HistoryUiState {
    fn reset_for_missing_workspace(&mut self) {
        self.last_request_key = None;
        self.snapshot = HistorySnapshot {
            notice: Some("Select a workspace to enable history.".to_string()),
            ..HistorySnapshot::default()
        };
    }}
fn history_request_key(
    workspace_root: &str,
    chat: Option<&ChatSession>,
    force_create: bool,
) -> HistoryRequestKey {
    HistoryRequestKey {
        workspace_root: workspace_root.to_string(),
        chat_id: chat.map(|chat| chat.id.clone()),
        chat_name: chat.map(|chat| chat.name.clone()),
        chat_fingerprint: chat.map(ChatSession::history_fingerprint),
        force_create,
    }
}
fn merge_history_notice(
    primary: Option<String>,
    secondary: impl Into<Option<String>>,
) -> Option<String> {
    match (primary, secondary.into()) {
        (Some(primary), Some(secondary)) if !secondary.is_empty() => {
            Some(format!("{primary} {secondary}"))
        }
        (Some(primary), _) => Some(primary),
        (None, other) => other,
    }
}
fn open_history_store(path: &Path, create_parent: bool) -> Result<SqliteStore> {
    if create_parent {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(SqliteStore::open(path)?)
}
fn build_branch_view(
    store: &SqliteStore,
    session: &HistorySessionInfo,
    active_branch_id: BranchId,
) -> Result<BranchTimelineView> {
    let branches = store.list_branches(session.id)?;
    let mut view = BranchTimelineView::new(session.id.to_string());
    for branch in branches {
        let turn_count = store.get_turns_for_branch(branch.id)?.len();
        view.add_branch(BranchItem {
            branch_id: branch.id,
            name: branch.name,
            parent_branch_id: branch.parent_branch_id,
            head_turn_id: branch.head_turn_id.map(|turn_id| turn_id.to_string()),
            created_at: branch.created_at,
            is_active: branch.id == active_branch_id,
            turn_count,
        });
    }
    view.set_active(active_branch_id);
    Ok(view)
}
fn build_checkpoint_view(
    store: &SqliteStore,
    session: &HistorySessionInfo,
    active_branch_id: BranchId,
) -> Result<CheckpointListView> {
    let checkpoints = store.list_checkpoints(session.id)?;
    let mut view = CheckpointListView::new(session.id.to_string(), active_branch_id.to_string());
    for checkpoint in checkpoints {
        let manifest = store.get_manifest(checkpoint.manifest_id)?;
        let file_count = manifest
            .as_ref()
            .and_then(|manifest| deserialize_manifest(&manifest.manifest_data).ok())
            .map(|manifest| manifest.file_count)
            .unwrap_or(0);
        let (name, is_named) = checkpoint_display_name(&checkpoint.reason);
        view.add_checkpoint(CheckpointItem {
            checkpoint_id: checkpoint.id,
            name,
            reason: checkpoint.reason.clone(),
            file_count,
            created_at: checkpoint.created_at,
            turn_id: checkpoint.turn_id.map(|turn_id| turn_id.to_string()),
            is_safety: checkpoint.reason.contains("safety"),
            is_named,
        });
    }
    Ok(view)
}
fn build_timeline_view(
    store: &SqliteStore,
    session: &HistorySessionInfo,
    active_branch_id: BranchId,
) -> Result<TimelineView> {
    let turns = store.get_turns_for_branch(active_branch_id)?;
    let total_turns = turns.len();
    let start = total_turns.saturating_sub(HISTORY_PANEL_TURN_LIMIT);
    let mut view = TimelineView::new(session.id.to_string());
    view.total_turns = total_turns;
    for (offset, turn) in turns.into_iter().enumerate().skip(start) {
        let messages = store.get_messages_for_turn(turn.id)?;
        let preview = messages
            .iter()
            .find(|message| message.role == "user")
            .or_else(|| messages.first())
            .map(|message| truncate_preview(&message.content))
            .unwrap_or_default();
        let actor = match turn.actor.to_ascii_lowercase().as_str() {
            "assistant" => Actor::Assistant,
            "system" => Actor::System,
            _ => Actor::User,
        };
        let has_tool_calls = messages.iter().any(|message| message.role == "tool");
        view.add_item(TimelineItem {
            turn_id: turn.id,
            turn_number: offset + 1,
            actor,
            content_preview: preview,
            timestamp: turn.created_at.clone(),
            message_count: messages.len(),
            has_tool_calls,
            is_finalized: turn.finalized_at.is_some(),
        });
    }
    view.current_turn_id = view.items.last().map(|item| item.turn_id);
    Ok(view)
}
fn find_linked_history_session<'a>(
    sessions: &'a [HistorySessionInfo],
    chat_id: &str,
) -> Option<&'a HistorySessionInfo> {
    let prefix = format!("rust-chatbot:{chat_id}:");
    sessions
        .iter()
        .find(|session| session.title.starts_with(prefix.as_str()))
}
#[cfg(test)]
#[path = "history_maintenance_tests.rs"]
mod history_maintenance_tests;
