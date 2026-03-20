//! Application state management
//!
//! This module contains the main application state (AppState) and all
//! related methods for managing chat sessions, communicating with providers,
//! and handling UI state.

use std::collections::{HashMap, HashSet, VecDeque};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use anyhow::Result;
use rust_debug::tracing::{debug, error, info, warn};
use crate::data::ChatSession;
use crate::data::DataManager;
use crate::data::SessionHeightCache;
use crate::data::SessionInfo;
use crate::model::ModelProvider;
use crate::socket::SyncEvent;
use crate::worker;
use crate::worker::WorkerResult;
#[path = "state/history_repo_capture.rs"]
mod history_repo_capture;
#[path = "state/ui_io.rs"]
mod ui_io;
use ui_io::UiIoResult;
use ui_io::UiIoSaveOperation;
use ui_io::UiIoState;
pub(crate) const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(200);
const STARTUP_SESSION_PRELOAD_LIMIT: usize = 0;
const PREALLOCATED_SESSION_POOL_SIZE: usize = 5;
pub(crate) enum AppEvent {
    UiAction(AppUiAction),
    UiIo(UiIoResult),
    Sync(SyncEvent),
    WorkerDiscovery(DiscoveredWorkerJob),
}
#[derive(Debug)]
pub(crate) enum AppUiAction {
    SubmitMessage,
    StopCurrentProcess,
    LoadSession(String),
    CreateSession {
        name: String,
        system_prompt: Option<String>,
    },
    RenameSession {
        session_id: String,
        new_name: String,
    },
    DeleteSession(String),
    SaveCurrentSystemPrompt {
        session_id: String,
        system_prompt: Option<String>,
    },
    SaveCurrentOmniForgeModel {
        session_id: String,
        model_id: String,
    },
}
/// Wrapper for native window handle that implements HasWindowHandle + HasDisplayHandle
/// for use with rfd::FileDialog::set_parent()
#[cfg(not(target_arch = "wasm32"))]
pub struct DialogParent {
    window: raw_window_handle::RawWindowHandle,
    display: raw_window_handle::RawDisplayHandle,
}
#[cfg(not(target_arch = "wasm32"))]
impl raw_window_handle::HasWindowHandle for DialogParent {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.window) })
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl raw_window_handle::HasDisplayHandle for DialogParent {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(self.display) })
    }
}
#[derive(Clone, Debug)]
pub struct LayoutLogFeedback {
    pub at: Instant,
    pub success: bool,
    pub path: Option<std::path::PathBuf>,
}
#[derive(Clone, Debug)]
pub struct HeightCacheScan {
    pub session_id: String,
    pub width_key: u32,
    pub start_offset: Option<f32>,
    pub seeking_top: bool,
    pub at_end: bool,
}
/// Worker job data collected during background startup discovery.
#[derive(Debug)]
pub(crate) struct DiscoveredWorkerJob {
    pub(crate) job: worker::WorkerJobSnapshot,
    pub(crate) response: String,
    pub(crate) result: Option<WorkerResult>,
}
#[derive(Debug)]
pub(crate) struct OrphanedLocalWorker {
    pub(crate) chat_id: String,
    pub(crate) error_message: String,
}
pub(crate) struct SessionSearchResult {
    pub(crate) query: String,
    pub(crate) matched_session_ids: HashSet<String>,
}
pub(crate) struct WorkspaceCompletionResult {
    pub(crate) query: String,
    pub(crate) matches: Vec<String>,
}
pub(crate) struct HeightCacheLoadResult {
    pub(crate) request_id: u64,
    pub(crate) session_id: String,
    pub(crate) cache: Result<Option<SessionHeightCache>, String>,
}
pub(crate) enum PathResolutionRequest {
    ApplyWorkspaceInput {
        input: String,
    },
    SelectWorkspaceDir {
        path: String,
    },
    ApplyRunscInput {
        input: String,
    },
    PreviewRunscInput {
        path: String,
    },
    SelectRunscDir {
        path: String,
    },
    ReplaceRunscDir {
        existing_path: String,
        replacement: String,
    },
}
pub(crate) struct PathResolutionResult {
    pub(crate) request_id: u64,
    pub(crate) request: PathResolutionRequest,
    pub(crate) normalized_path: Result<String, String>,
}
pub(crate) struct McpConfigStatusResult {
    pub(crate) request_id: u64,
    pub(crate) cache_key: String,
    pub(crate) fff_explicitly_configured: bool,
}
include!("state/session.rs");
include!("state/worker.rs");
include!("state/layout.rs");
include!("state/history.rs");
include!("state/session_runtime.rs");
include!("state/process_state.rs");
include!("state/messaging.rs");
include!("state/sync_slots.rs");
include!("state/sync.rs");
include!("state/persistence.rs");
include!("state/workspace.rs");
include!("state/helpers.rs");
include!("state/helpers_impl.rs");
impl SessionState {
    fn new(provider: ModelProvider) -> Self {
        let workspace_snapshot =
            crate::app::workspace_picker::load_provider_workspace_snapshot(provider);
        Self {
            current_session: None,
            sessions: Vec::new(),
            data_manager: None,
            provider,
            show_new_session_dialog: false,
            new_session_name: String::new(),
            show_rename_session_dialog: false,
            rename_session_id: String::new(),
            rename_session_name: String::new(),
            search_filter: String::new(),
            search_matched_sessions: HashSet::new(),
            search_results_rx: None,
            search_results_query: None,
            session_cache: SessionCache::new(),
            session_preload_rx: None,
            height_cache_load_rx: None,
            height_cache_load_started_at: None,
            latest_height_cache_load_request_id: 0,
            mcp_config_status_rx: None,
            mcp_config_status_started_at: None,
            latest_mcp_config_request_id: 0,
            mcp_config_cache_key: None,
            fff_mcp_server_explicitly_configured: None,
            pending_post_load_session_id: None,
            selected_session_id: None,
            loading_session_id: None,
            pending_draft: None,
            input_history_index: None,
            input_history_draft: String::new(),
            slash_command_index: None,
            slash_commands: Vec::new(),
            slash_commands_rx: None,
            system_prompt_text: String::new(),
            system_prompt_dirty: false,
            system_prompt_dirty_session_id: None,
            workspace_dir: workspace_snapshot.current_dir.clone(),
            workspace_input_text: workspace_snapshot.current_dir.unwrap_or_default(),
            workspace_recent_dirs: workspace_snapshot.recent_dirs,
            workspace_completions: Vec::new(),
            workspace_completion_query: None,
            workspace_completion_rx: None,
            workspace_completion_started_at: None,
            path_resolution_rx: None,
            path_resolution_started_at: None,
            latest_path_resolution_request_id: 0,
            runsc_dirs: Vec::new(),
            runsc_input_text: String::new(),
            runsc_recent_dirs: Vec::new(),
            runsc_image_input_text: String::new(),
            show_system_prompt: false,
            history: HistoryUiState::default(),
            ui_io: UiIoState::new(),
            event_queue: VecDeque::new(),
        }
    }}
