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
