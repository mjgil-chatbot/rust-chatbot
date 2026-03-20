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
