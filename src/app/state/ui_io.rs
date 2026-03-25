use std::collections::{HashMap, HashSet};
use anyhow::{Context, Result};
use super::*;
#[path = "ui_io_background.rs"]
mod background;
use background::load_selected_session;
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
include!("ui_io_async_impl.rs");
