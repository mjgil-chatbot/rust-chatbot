use std::collections::{HashMap, HashSet};
use anyhow::{Context, Result};
use super::*;
#[path = "ui_io_background.rs"]
mod background;
use background::{
    load_selected_session, load_selected_session_after_delete, load_session_list_patch,
    save_session_in_background,
};
#[derive(Clone)]
pub(crate) struct SessionListPatch {
    pub(crate) sessions: Vec<SessionInfo>,
}
#[derive(Clone)]
pub(crate) struct SelectedSessionLoad {
    pub(crate) sessions: Option<Vec<SessionInfo>>,
    pub(crate) current_session: Option<ChatSession>,
    pub(crate) requested_session_id: Option<String>,
}
#[derive(Clone)]
pub(crate) struct PendingUiSend {
    pub(crate) chat_id: String,
    pub(crate) message_timestamp_ms: i64,
}
#[derive(Clone)]
pub(crate) struct PendingCreateSession {
    pub(crate) request_id: u64,
    pub(crate) name: String,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum UiIoSaveOperation {
    CreateSession,
    RenameSession,
    DeleteSession,
    SaveSession,
    RefreshSessionList,
    SystemPromptSave,
}
pub(crate) fn save_operation_label(operation: UiIoSaveOperation) -> &'static str {
    match operation {
        UiIoSaveOperation::CreateSession => "create_session",
        UiIoSaveOperation::RenameSession => "rename_session",
        UiIoSaveOperation::DeleteSession => "delete_session",
        UiIoSaveOperation::SaveSession => "save_session",
        UiIoSaveOperation::RefreshSessionList => "refresh_session_list",
        UiIoSaveOperation::SystemPromptSave => "save_system_prompt",
    }
}
pub(crate) enum UiIoResult {
    SessionListPatch {
        request_id: Option<u64>,
        patch: SessionListPatch,
    },
    SelectedSessionLoad {
        request_id: Option<u64>,
        requested_session_id: Option<String>,
        load: SelectedSessionLoad,
    },
    SessionLoadFailed {
        request_id: Option<u64>,
        session_id: String,
        requested_session_id: Option<String>,
        error: String,
    },
    SessionRefreshFailed {
        error: String,
    },
    SessionSaveFailed {
        request_id: Option<u64>,
        session_id: String,
        operation: UiIoSaveOperation,
        is_system_prompt_save: bool,
        error: String,
    },
    SessionSaveSuccess {
        request_id: Option<u64>,
        session_id: String,
        is_system_prompt_save: bool,
    },
    SendCompleted {
        chat_id: String,
        message_timestamp_ms: i64,
        command_str: String,
        retrieved_memory: Option<String>,
        worker_pid: u32,
    },
    SendFailed {
        chat_id: String,
        error: String,
    },
}
pub(crate) struct UiIoState {
    pub(crate) tx: mpsc::Sender<UiIoResult>,
    pub(crate) rx: mpsc::Receiver<UiIoResult>,
    pub(crate) repaint_ctx: Option<eframe::egui::Context>,
    pub(crate) next_request_id: u64,
    pub(crate) latest_list_request_id: u64,
    pub(crate) latest_load_request_id: u64,
    pub(crate) latest_save_request_id: u64,
    pub(crate) latest_list_request_id_by_session: HashMap<String, u64>,
    pub(crate) latest_load_request_id_by_session: HashMap<String, u64>,
    pub(crate) latest_save_request_id_by_session: HashMap<String, u64>,
    pub(crate) session_refresh_in_flight: bool,
    pub(crate) pending_create_session: Option<PendingCreateSession>,
    pub(crate) pending_send_chat_ids: HashSet<String>,
}
impl UiIoState {
    pub(crate) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            repaint_ctx: None,
            next_request_id: 1,
            latest_list_request_id: 0,
            latest_load_request_id: 0,
            latest_save_request_id: 0,
            latest_list_request_id_by_session: HashMap::new(),
            latest_load_request_id_by_session: HashMap::new(),
            latest_save_request_id_by_session: HashMap::new(),
            session_refresh_in_flight: false,
            pending_create_session: None,
            pending_send_chat_ids: HashSet::new(),
        }
    }}
impl UiIoState {
    pub(crate) fn next_user_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }}
impl UiIoState {
    pub(crate) fn set_repaint_ctx(&mut self, ctx: eframe::egui::Context) {
        self.repaint_ctx = Some(ctx);
    }}
impl UiIoState {
    pub(crate) fn next_list_request_id(&mut self) -> u64 {
        let request_id = self.next_user_request_id();
        self.latest_list_request_id = request_id;
        request_id
    }}
impl UiIoState {
    pub(crate) fn next_load_request_id(&mut self) -> u64 {
        let request_id = self.next_user_request_id();
        self.latest_load_request_id = request_id;
        request_id
    }}
impl UiIoState {
    pub(crate) fn next_save_request_id(&mut self) -> u64 {
        let request_id = self.next_user_request_id();
        self.latest_save_request_id = request_id;
        request_id
    }}
impl UiIoState {
    pub(crate) fn record_list_request_id_for_session(&mut self, session_id: &str, request_id: u64) {
        self.latest_list_request_id_by_session
            .insert(session_id.to_string(), request_id);
    }}
impl UiIoState {
    pub(crate) fn record_load_request_id_for_session(&mut self, session_id: &str, request_id: u64) {
        self.latest_load_request_id_by_session
            .insert(session_id.to_string(), request_id);
    }}
impl UiIoState {
    pub(crate) fn record_save_request_id_for_session(&mut self, session_id: &str, request_id: u64) {
        self.latest_save_request_id = request_id;
        self.latest_save_request_id_by_session
            .insert(session_id.to_string(), request_id);
    }}
impl UiIoState {
    pub(crate) fn latest_list_request_id_for_session(&self, session_id: Option<&str>) -> u64 {
        session_id
            .and_then(|session_id| {
                self.latest_list_request_id_by_session
                    .get(session_id)
                    .copied()
            })
            .unwrap_or(self.latest_list_request_id)
    }}
impl UiIoState {
    pub(crate) fn latest_load_request_id_for_session(&self, session_id: Option<&str>) -> u64 {
        session_id
            .and_then(|session_id| {
                self.latest_load_request_id_by_session
                    .get(session_id)
                    .copied()
            })
            .unwrap_or(self.latest_load_request_id)
    }}
impl UiIoState {
    pub(crate) fn latest_save_request_id_for_session(&self, session_id: Option<&str>) -> u64 {
        session_id
            .and_then(|session_id| {
                self.latest_save_request_id_by_session
                    .get(session_id)
                    .copied()
            })
            .unwrap_or(self.latest_save_request_id)
    }}
impl UiIoState {
    pub(crate) fn mark_create_session_started(&mut self, request_id: u64, name: String) {
        self.pending_create_session = Some(PendingCreateSession { request_id, name });
    }}
impl UiIoState {
    #[allow(dead_code)]
    pub(crate) fn is_create_session_in_flight(&self) -> bool {
        self.pending_create_session.is_some()
    }}
impl UiIoState {
    pub(crate) fn pending_create_session_name(&self) -> Option<&str> {
        self.pending_create_session
            .as_ref()
            .map(|pending| pending.name.as_str())
    }}
impl UiIoState {
    pub(crate) fn clear_pending_create_session(&mut self, request_id: Option<u64>) {
        if self
            .pending_create_session
            .as_ref()
            .map(|pending| pending.request_id)
            == request_id
        {
            self.pending_create_session = None;
        }
    }}
impl UiIoState {
    pub(crate) fn cancel_pending_create_session(&mut self) {
        self.pending_create_session = None;
    }}
pub(crate) fn update_message_command(
    session: &mut ChatSession,
    message_timestamp_ms: i64,
    command_str: &str,
) -> Result<()> {
    update_message_request_metadata(session, message_timestamp_ms, command_str, None)
}
pub(crate) fn update_message_request_metadata(
    session: &mut ChatSession,
    message_timestamp_ms: i64,
    command_str: &str,
    retrieved_memory: Option<&str>,
) -> Result<()> {
    session.update_user_message_request_metadata(
        message_timestamp_ms,
        Some(command_str.to_string()),
        retrieved_memory.map(str::to_string),
    )
}
impl AppState {
    pub(crate) fn drain_ui_io_results(&mut self) {
        let mut results = Vec::new();
        loop {
            match self.session.ui_io.rx.try_recv() {
                Ok(result) => results.push(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        for result in results {
            self.enqueue_event(AppEvent::UiIo(result));
        }
    }}
impl AppState {
    pub(crate) fn apply_sidebar_patch(&mut self, patch: SessionListPatch) {
        let current_session_id = self
            .session
            .current_session
            .as_ref()
            .map(|session| session.id.clone());
        self.session.sessions = patch.sessions;

        self.session.session_cache.retain(|session_id, session| {
            self.session
                .sessions
                .iter()
                .find(|info| info.id == *session_id)
                .map(|info| info.updated_at == session.updated_at)
                .unwrap_or(false)
        });

        if let Some(current_session_id) = current_session_id {
            if let Some(info) = self
                .session
                .sessions
                .iter()
                .find(|session| session.id == current_session_id)
                .cloned()
            {
                if let Some(current_session) = self.session.current_session.as_mut() {
                    current_session.name = info.name;
                }
                return;
            }
        }

        if self.session.sessions.is_empty() {
            self.session.selected_session_id = None;
            self.session.loading_session_id = None;
            self.clear_current_session();
        }
    }}
include!("ui_io_async_impl.rs");
