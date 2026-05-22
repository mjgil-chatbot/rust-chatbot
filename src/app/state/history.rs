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

use self::history_maintenance::maintain_history_store;
use self::history_support::{
    auto_turn_checkpoint_reason, checkpoint_display_name, display_history_session_title,
    deserialize_manifest, history_missing_notice, history_restored_notice, history_store_path,
    is_auto_turn_checkpoint_reason, linked_history_session_title, mirror_history_store,
    parse_chat_turns, restore_history_store_if_needed, truncate_preview,
};
use self::history_repo_capture::{build_checkpoint_manifest, load_checkpoint_repo_summaries};
use self::history_sync::{backfill_missing_turn_checkpoints, reconcile_existing_turn};

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
    }

    fn reset_for_missing_workspace(&mut self) {
        self.last_request_key = None;
        self.snapshot = HistorySnapshot {
            notice: Some("Select a workspace to enable history.".to_string()),
            ..HistorySnapshot::default()
        };
    }
}

impl AppState {
    pub(crate) fn refresh_history(&mut self) {
        if cfg!(test) && self.session.ui_io.repaint_ctx.is_none() {
            return;
        }
        self.session.history.last_request_key = None;
        self.poll_history_sync();
    }

    pub(crate) fn request_history_enable(&mut self) {
        let Some(workspace_root) = self.session.effective_workspace_dir() else {
            self.session.history.reset_for_missing_workspace();
            return;
        };
        let Some(chat) = self.session.current_session.clone() else {
            return;
        };
        self.spawn_history_refresh_async(HistoryRefreshRequest {
            key: history_request_key(&workspace_root, Some(&chat), true),
            chat: Some(chat),
        });
    }

    pub(crate) fn spawn_history_checkpoint_async(&mut self, checkpoint_name: String) {
        if self.session.history.checkpoint_in_flight {
            return;
        }

        let Some(workspace_root) = self.session.effective_workspace_dir() else {
            self.layout.status_feedback = Some(UiStatusFeedback {
                at: Instant::now(),
                is_error: true,
                message: "History requires a workspace.".to_string(),
            });
            return;
        };
        let Some(chat) = self.session.current_session.clone() else {
            self.layout.status_feedback = Some(UiStatusFeedback {
                at: Instant::now(),
                is_error: true,
                message: "History requires an active chat.".to_string(),
            });
            return;
        };

        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.session.history.checkpoint_in_flight = true;
        self.session.history.checkpoint_rx = Some(rx);

        thread::spawn(move || {
            let result = create_history_checkpoint(&workspace_root, &chat, &checkpoint_name)
                .map(|created| HistoryCheckpointResult {
                    success_message: format!("History checkpoint created: {}", created),
                })
                .map_err(|err| format!("{err:#}"));
            let _ = tx.send(result);
            if let Some(ctx) = repaint_ctx {
                ctx.request_repaint();
            }
        });
    }

    fn spawn_history_refresh_async(&mut self, request: HistoryRefreshRequest) {
        if self.session.history.refresh_in_flight {
            return;
        }

        self.session.history.last_request_key = Some(request.key.clone());
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.session.history.refresh_in_flight = true;
        self.session.history.refresh_started_at = Some(Instant::now());
        self.session.history.refresh_rx = Some(rx);

        thread::spawn(move || {
            let result = load_history_snapshot(request.clone())
                .map(|snapshot| HistoryRefreshResult {
                    key: request.key.clone(),
                    snapshot,
                })
                .map_err(|err| (request.key, format!("{err:#}")));
            let _ = tx.send(result);
            if let Some(ctx) = repaint_ctx {
                ctx.request_repaint();
            }
        });
    }

    fn drain_history_results(&mut self) {
        let refresh_result = self
            .session
            .history
            .refresh_rx
            .as_ref()
            .map(|rx| rx.try_recv());
        match refresh_result {
            Some(Ok(Ok(result))) => {
                self.session.history.refresh_in_flight = false;
                background_timing::finish_background_channel_timing(
                    "history_refresh",
                    &mut self.session.history.refresh_started_at,
                );
                self.session.history.refresh_rx = None;
                self.session
                    .history
                    .set_snapshot(result.key, result.snapshot);
            }
            Some(Ok(Err((key, error)))) => {
                self.session.history.refresh_in_flight = false;
                background_timing::finish_background_channel_timing(
                    "history_refresh",
                    &mut self.session.history.refresh_started_at,
                );
                self.session.history.refresh_rx = None;
                let snapshot = HistorySnapshot {
                    workspace_root: self.session.effective_workspace_dir(),
                    error: Some(error),
                    ..HistorySnapshot::default()
                };
                self.session.history.set_snapshot(key, snapshot);
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.session.history.refresh_in_flight = false;
                background_timing::finish_background_channel_timing(
                    "history_refresh",
                    &mut self.session.history.refresh_started_at,
                );
                self.session.history.refresh_rx = None;
                let key = self.session.history.last_request_key.clone().unwrap_or_else(|| {
                    history_request_key(
                        self.session.effective_workspace_dir().as_deref().unwrap_or_default(),
                        None,
                        false,
                    )
                });
                let snapshot = HistorySnapshot {
                    workspace_root: self.session.effective_workspace_dir(),
                    error: Some("History refresh stopped unexpectedly.".to_string()),
                    ..HistorySnapshot::default()
                };
                self.session.history.set_snapshot(key, snapshot);
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }

        let checkpoint_result = self
            .session
            .history
            .checkpoint_rx
            .as_ref()
            .map(|rx| rx.try_recv());
        match checkpoint_result {
            Some(Ok(Ok(result))) => {
                self.session.history.checkpoint_in_flight = false;
                self.session.history.checkpoint_rx = None;
                self.layout.status_feedback = Some(UiStatusFeedback {
                    at: Instant::now(),
                    is_error: false,
                    message: result.success_message,
                });
                self.session.history.show_checkpoint_dialog = false;
                self.session.history.checkpoint_name.clear();
                self.request_history_enable();
            }
            Some(Ok(Err(error))) => {
                self.session.history.checkpoint_in_flight = false;
                self.session.history.checkpoint_rx = None;
                self.layout.status_feedback = Some(UiStatusFeedback {
                    at: Instant::now(),
                    is_error: true,
                    message: format!("History checkpoint failed: {error}"),
                });
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.session.history.checkpoint_in_flight = false;
                self.session.history.checkpoint_rx = None;
                self.layout.status_feedback = Some(UiStatusFeedback {
                    at: Instant::now(),
                    is_error: true,
                    message: "History checkpoint stopped unexpectedly.".to_string(),
                });
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }
    }

    fn poll_history_sync(&mut self) {
        if cfg!(test) && self.session.ui_io.repaint_ctx.is_none() {
            return;
        }

        let Some(workspace_root) = self.session.effective_workspace_dir() else {
            self.session.history.reset_for_missing_workspace();
            return;
        };

        let current_chat = self.session.current_session.clone();
        let should_force_create = current_chat
            .as_ref()
            .is_some_and(|chat| !chat.messages.is_empty())
            && !self.current_history_chat_is_processing();
        let key = history_request_key(
            &workspace_root,
            current_chat
                .as_ref()
                .filter(|_| should_force_create),
            should_force_create,
        );

        if self.session.history.last_request_key.as_ref() == Some(&key)
            || self.session.history.refresh_in_flight
        {
            return;
        }

        self.spawn_history_refresh_async(HistoryRefreshRequest {
            key,
            chat: current_chat,
        });
    }

    fn current_history_chat_is_processing(&self) -> bool {
        let Some(chat_id) = self.session.current_session_id() else {
            return false;
        };
        self.worker.processing_chat_ids.contains(chat_id)
    }
}

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

fn load_history_snapshot(request: HistoryRefreshRequest) -> Result<HistorySnapshot> {
    let workspace_root = PathBuf::from(&request.key.workspace_root);
    let restored_from_backup = restore_history_store_if_needed(&workspace_root)?;
    let store_path = history_store_path(&workspace_root)?;
    if !store_path.is_file() && !request.key.force_create {
        return Ok(HistorySnapshot {
            workspace_root: Some(request.key.workspace_root),
            store_path: Some(store_path.display().to_string()),
            has_store: false,
            notice: Some(history_missing_notice()),
            ..HistorySnapshot::default()
        });
    }

    let mut store = open_history_store(&store_path, request.key.force_create)?;
    let mut store_updated = false;
    let mut linked_session: Option<HistorySessionInfo> = None;

    if request.key.force_create {
        if let Some(chat) = request.chat.as_ref() {
            linked_session = Some(sync_chat_history(
                &mut store,
                workspace_root.as_path(),
                chat,
            )?);
            store_updated = true;
        }
    }

    let maintenance = maintain_history_store(&mut store, workspace_root.as_path())?;
    if maintenance.mutated() {
        store_updated = true;
    }

    let sessions = store.list_sessions()?;
    let linked_session = linked_session.or_else(|| {
        request
            .chat
            .as_ref()
            .and_then(|chat| find_linked_history_session(&sessions, &chat.id))
            .cloned()
    });

    let mut snapshot = HistorySnapshot {
        workspace_root: Some(request.key.workspace_root),
        store_path: Some(store_path.display().to_string()),
        has_store: true,
        total_sessions: sessions.len(),
        linked_session_title: linked_session
            .as_ref()
            .map(|session| display_history_session_title(&session.title)),
        object_store_bytes: maintenance.object_store_bytes,
        object_store_files: maintenance.object_store_files,
        auto_checkpoint_count: maintenance.auto_checkpoint_count,
        ..HistorySnapshot::default()
    };

    if let Some(session) = linked_session {
        let active_branch = store.resolve_active_branch(session.id)?.branch;
        store.set_active_session_id(session.id)?;
        store.set_active_branch_id(session.id, active_branch.id)?;
        snapshot.branch_view = Some(build_branch_view(&store, &session, active_branch.id)?);
        snapshot.checkpoint_view = Some(build_checkpoint_view(&store, &session, active_branch.id)?);
        snapshot.checkpoint_repo_summaries =
            load_checkpoint_repo_summaries(&store, workspace_root.as_path(), session.id)?;
        snapshot.timeline_view = Some(build_timeline_view(&store, &session, active_branch.id)?);
    } else if request.chat.is_some() {
        snapshot.notice = Some("History is enabled, but this chat has not been linked yet.".to_string());
    }

    snapshot.notice = merge_history_notice(snapshot.notice.take(), maintenance.notice.clone());

    if restored_from_backup.restored() {
        snapshot.notice = merge_history_notice(
            Some(history_restored_notice(restored_from_backup)),
            snapshot.notice.take(),
        );
    }

    drop(store);
    if store_updated {
        mirror_history_store(&workspace_root)?;
    }

    Ok(snapshot)
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

fn sync_chat_history(
    store: &mut SqliteStore,
    workspace_root: &Path,
    chat: &ChatSession,
) -> Result<HistorySessionInfo> {
    let sessions = store.list_sessions()?;
    let session = if let Some(existing) = find_linked_history_session(&sessions, &chat.id) {
        if existing.title != linked_history_session_title(chat) {
            store.update_session_title(existing.id, &linked_history_session_title(chat))?;
        }
        existing.clone()
    } else {
        let session_id = SessionId::new();
        store.create_session(session_id, workspace_root.to_string_lossy().as_ref())?;
        store.update_session_title(session_id, &linked_history_session_title(chat))?;
        store
            .get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("history session was not persisted"))?
    };

    store.set_active_session_id(session.id)?;
    let branch = store.resolve_active_branch(session.id)?.branch;
    store.set_active_branch_id(session.id, branch.id)?;

    append_missing_turns(store, workspace_root, session.id, branch.id, chat)?;

    store
        .get_session(session.id)?
        .ok_or_else(|| anyhow::anyhow!("history session disappeared after sync"))
}

fn append_missing_turns(
    store: &mut SqliteStore,
    workspace_root: &Path,
    session_id: SessionId,
    branch_id: BranchId,
    chat: &ChatSession,
) -> Result<()> {
    let parsed_turns = parse_chat_turns(chat);
    if parsed_turns.is_empty() {
        return Ok(());
    }

    let existing_turns = store.get_turns_for_branch(branch_id)?;
    if existing_turns.len() > parsed_turns.len() {
        return Ok(());
    }

    let mut checkpointed_turns = store
        .list_checkpoints(session_id)?
        .into_iter()
        .filter(|checkpoint| is_auto_turn_checkpoint_reason(&checkpoint.reason))
        .filter_map(|checkpoint| checkpoint.turn_id)
        .collect::<HashSet<_>>();
    let mut head_turn_id = existing_turns.last().map(|turn| turn.id);
    if let Some(existing_turn) = existing_turns.last() {
        let parsed = &parsed_turns[existing_turns.len() - 1];
        if reconcile_existing_turn(store, existing_turn.id, parsed)?
            && checkpointed_turns.insert(existing_turn.id)
        {
            create_turn_checkpoint(
                store,
                workspace_root,
                session_id,
                branch_id,
                existing_turn.id,
                auto_turn_checkpoint_reason(chat, existing_turns.len() - 1),
            )?;
        }
    }

    for (index, parsed) in parsed_turns.iter().enumerate().skip(existing_turns.len()) {
        let turn_id = TurnId::new();
        let turn = HistoryTurnInfo {
            id: turn_id,
            session_id,
            branch_id,
            parent_turn_id: head_turn_id,
            actor: match parsed.actor {
                Actor::User => "User".to_string(),
                Actor::Assistant => "Assistant".to_string(),
                _ => "System".to_string(),
            },
            created_at: parsed.timestamp.clone(),
            finalized_at: None,
        };
        store.create_turn(&turn)?;

        let mut sequence_num = 0;
        if let Some(user_content) = parsed.user_content.as_deref() {
            let message = MessageRecord::new(turn_id, "User", "user", user_content, sequence_num)
                .with_metadata("source_chat_id", &chat.id)
                .with_metadata("turn_index", &index.to_string());
            store.create_message(&message)?;
            sequence_num += 1;
        }
        if let Some(assistant_content) = parsed.assistant_content.as_deref() {
            let message = MessageRecord::new(
                turn_id,
                "Assistant",
                "assistant",
                assistant_content,
                sequence_num,
            )
            .with_metadata("source_chat_id", &chat.id)
            .with_metadata("turn_index", &index.to_string());
            store.create_message(&message)?;
            store.finalize_turn(turn_id)?;
            if checkpointed_turns.insert(turn_id) {
                create_turn_checkpoint(
                    store,
                    workspace_root,
                    session_id,
                    branch_id,
                    turn_id,
                    auto_turn_checkpoint_reason(chat, index),
                )?;
            }
        }

        head_turn_id = Some(turn_id);
    }

    backfill_missing_turn_checkpoints(
        store,
        workspace_root,
        session_id,
        branch_id,
        chat,
        &parsed_turns,
        &mut checkpointed_turns,
    )?;

    if let Some(head_turn_id) = head_turn_id {
        store.update_branch_head(branch_id, head_turn_id)?;
    }

    Ok(())
}

fn create_history_checkpoint(
    workspace_root: &str,
    chat: &ChatSession,
    checkpoint_name: &str,
) -> Result<String> {
    let workspace_root = PathBuf::from(workspace_root);
    let _ = restore_history_store_if_needed(&workspace_root)?;
    let store_path = history_store_path(&workspace_root)?;
    let mut store = open_history_store(&store_path, true)?;
    let session = sync_chat_history(&mut store, &workspace_root, chat)?;
    let branch = store.resolve_active_branch(session.id)?.branch;
    let reason = if checkpoint_name.trim().is_empty() {
        "manual".to_string()
    } else {
        format!("named:{}", checkpoint_name.trim())
    };
    let checkpoint_id = create_checkpoint(
        &mut store,
        &workspace_root,
        session.id,
        branch.id,
        branch.head_turn_id,
        reason,
    )?;

    let result = if checkpoint_name.trim().is_empty() {
        checkpoint_id.to_string()
    } else {
        checkpoint_name.trim().to_string()
    };

    drop(store);
    mirror_history_store(&workspace_root)?;

    Ok(result)
}

fn create_turn_checkpoint(
    store: &mut SqliteStore,
    workspace_root: &Path,
    session_id: SessionId,
    branch_id: BranchId,
    turn_id: TurnId,
    reason: String,
) -> Result<()> {
    create_checkpoint(
        store,
        workspace_root,
        session_id,
        branch_id,
        Some(turn_id),
        reason,
    )?;
    Ok(())
}

fn create_checkpoint(
    store: &mut SqliteStore,
    workspace_root: &Path,
    session_id: SessionId,
    branch_id: BranchId,
    turn_id: Option<TurnId>,
    reason: String,
) -> Result<CheckpointId> {
    let checkpoint_id = CheckpointId::new();
    let created_at = chrono::Utc::now().to_rfc3339();
    let manifest = build_checkpoint_manifest(store, workspace_root, &created_at)?;
    store.create_checkpoint(&HistoryCheckpointInfo {
        id: checkpoint_id,
        session_id,
        branch_id,
        turn_id,
        manifest_id: manifest.id,
        reason,
        created_at: created_at.clone(),
    })?;
    store.create_manifest(&HistoryManifestInfo {
        id: manifest.id,
        session_id,
        checkpoint_id,
        manifest_data: serde_json::to_string(&manifest)?,
        created_at,
    })?;
    Ok(checkpoint_id)
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
#[path = "history_tests.rs"]
mod history_tests;

#[cfg(test)]
#[path = "history_maintenance_tests.rs"]
mod history_maintenance_tests;
