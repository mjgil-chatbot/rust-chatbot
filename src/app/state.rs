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

use crate::config;
use crate::data::{ChatMessage, ChatSession, DataManager, SessionHeightCache, SessionInfo};
use crate::markdown::MarkdownCache;
use crate::memory;
use crate::model::ModelProvider;
use crate::socket::{self, SocketHandle, SyncEvent, WireMessage};
use crate::summary::{JsonSummary, JsonSummaryPreview};
use crate::sync::SyncShm;
use crate::worker::{self, WorkerResult, WorkerState};
use crate::workspace;

#[path = "state/background_timing.rs"]
mod background_timing;
#[path = "state/history_maintenance.rs"]
mod history_maintenance;
#[path = "state/history_repo_capture.rs"]
mod history_repo_capture;
#[path = "state/history_support.rs"]
mod history_support;
#[path = "state/history_sync.rs"]
mod history_sync;
#[path = "state/ui_io.rs"]
mod ui_io;
#[path = "state/ui_sync_async.rs"]
mod ui_sync_async;

pub(crate) use self::history_maintenance::human_bytes;

use ui_io::{
    save_operation_label, update_message_request_metadata, PendingUiSend, UiIoResult,
    UiIoSaveOperation, UiIoState,
};

pub(crate) const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(200);
// Preloading full sessions at startup retains large transcripts before the
// user ever opens them. Keep startup residency pinned to the current session.
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

/// Main application state
pub struct AppState {
    pub session: SessionState,
    pub worker: AppWorkerState,
    pub layout: LayoutState,

    /// Shared memory handle for cross-instance sync
    pub sync_shm: Option<SyncShm>,
    /// Socket handle for real-time cross-instance stream sync.
    pub sync_socket: SocketHandle,
    /// Receiver for sync events from the socket reader thread.
    pub sync_events: mpsc::Receiver<SyncEvent>,
    /// Instance number (e.g. 1, 2, 3) for multi-instance identification
    #[allow(dead_code)]
    pub instance_number: Option<u32>,
    /// Wrapper for native window handle for modal dialog parent
    #[cfg(not(target_arch = "wasm32"))]
    pub dialog_parent: Option<DialogParent>,
    #[cfg(target_arch = "wasm32")]
    pub dialog_parent: Option<()>,
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
pub(crate) struct PolledWorkerJob {
    pub(crate) job: worker::WorkerJobSnapshot,
    pub(crate) worker_pid_alive: bool,
    pub(crate) response: Option<String>,
    pub(crate) result: WorkerResult,
    pub(crate) worker_events: Vec<crate::stream::StreamEvent>,
    pub(crate) worker_events_offset: u64,
}

#[derive(Debug)]
pub(crate) struct OrphanedLocalWorker {
    pub(crate) chat_id: String,
    pub(crate) error_message: String,
}

#[derive(Debug)]
pub(crate) struct WorkerPollSnapshot {
    pub(crate) active_chat_ids: HashSet<String>,
    pub(crate) jobs: Vec<PolledWorkerJob>,
    pub(crate) orphaned_local_workers: Vec<OrphanedLocalWorker>,
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
    }
}

impl AppWorkerState {
    fn new() -> Self {
        Self {
            processing_chat_ids: HashSet::new(),
            local_processing_chat_ids: HashSet::new(),
            local_worker_pids: HashMap::new(),
            response_cache_by_chat: HashMap::new(),
            pending_worker_pids: HashMap::new(),
            response_started_chat_ids: HashSet::new(),
            pending_delete_chat_ids: HashSet::new(),
            forge_loop_active_chat_ids: HashSet::new(),
            forge_loop_continue_ids: HashMap::new(),
            forge_loop_acknowledged_ids: HashMap::new(),
            last_seen_worker_seq: HashMap::new(),
            last_seen_worker_events: HashMap::new(),
            last_worker_poll: Instant::now(),
            worker_poll_started_at: None,
            worker_discovery_rx: None,
            worker_poll_rx: None,
            locally_finalized_chat_ids: HashSet::new(),
        }
    }
}

impl LayoutState {
    fn new() -> Self {
        Self {
            input_text: String::new(),
            should_focus_input: !config::auto_ui_launch_background_requested(),
            show_json_summary: false,
            current_json_summary_preview: None,
            current_json_summary_full: None,
            pending_layout_log: false,
            layout_log_rx: None,
            markdown_cache: MarkdownCache::default(),
            copy_feedback: HashMap::new(),
            message_row_heights: Vec::new(),
            message_row_heights_session_id: None,
            message_row_heights_width: 0.0,
            session_height_cache: HashMap::new(),
            layout_log_feedback: None,
            last_auto_ui_debug_signature: None,
            last_auto_ui_sidebar_debug_signature: None,
            message_scroll_offset_override: None,
            message_scroll_offset_session_id: None,
            deferred_message_scroll_adjustment: 0.0,
            deferred_message_scroll_adjustment_session_id: None,
            pending_height_cache_save: None,
            height_cache_scan: None,
            status_feedback: None,
        }
    }
}

impl AppState {
    /// Create a new application state
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        provider: ModelProvider,
        instance: Option<u32>,
    ) -> Self {
        info!(instance=?instance, "initializing app state");

        // Initialize cross-instance sync (mmap)
        let sync_shm = match SyncShm::open(provider) {
            Ok(shm) => {
                info!("sync shm initialized");
                Some(shm)
            }
            Err(e) => {
                error!(error=%e, "failed to initialize sync shm, cross-instance sync disabled");
                None
            }
        };

        // Initialize cross-instance sync (socket) — always succeeds,
        // connection to the broker happens asynchronously with reconnect.
        let (sync_socket, sync_events) = socket::start(cc.egui_ctx.clone());
        info!("sync socket initialized (connecting in background)");

        let mut app = Self {
            session: SessionState::new(provider),
            worker: AppWorkerState::new(),
            layout: LayoutState::new(),
            sync_shm,
            sync_socket,
            sync_events,
            instance_number: instance,
            dialog_parent: None,
        };
        app.session.ui_io.set_repaint_ctx(cc.egui_ctx.clone());

        #[cfg(not(target_arch = "wasm32"))]
        {
            use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
            if let (Ok(wh), Ok(dh)) = (cc.window_handle(), cc.display_handle()) {
                app.dialog_parent = Some(DialogParent {
                    window: wh.as_raw(),
                    display: dh.as_raw(),
                });
            }
        }

        // Initialize data manager
        match DataManager::new(provider) {
            Ok(data_manager) => {
                info!("data manager initialized");
                if let Err(e) =
                    data_manager.ensure_preallocated_sessions(PREALLOCATED_SESSION_POOL_SIZE)
                {
                    warn!(error=%e, "failed to prepare preallocated session pool");
                }
                app.session.data_manager = Some(data_manager);

                // Load existing sessions
                if let Err(e) = app.load_sessions() {
                    error!(error=%e, "error loading sessions");
                }

                // Ensure we have a current session (create if needed)
                if app.session.current_session.is_none() {
                    if app.session.sessions.is_empty() {
                        if let Err(e) =
                            app.create_new_session(config::DEFAULT_SESSION_NAME.to_string(), None)
                        {
                            error!(error=%e, "error creating default session");
                        }
                    } else {
                        // Sessions exist but none loaded - try loading the first one
                        if let Err(e) = app.load_first_available_session() {
                            error!(error=%e, "error loading first available session, creating new one");
                            if let Err(e) = app
                                .create_new_session(config::DEFAULT_SESSION_NAME.to_string(), None)
                            {
                                error!(error=%e, "error creating fallback session");
                            }
                        }
                    }
                }

                if let Ok(session_id) = std::env::var("RUST_CHATBOT_START_SESSION_ID") {
                    if !session_id.trim().is_empty() {
                        if let Err(e) = app.load_session(session_id.trim()) {
                            error!(error=%e, session_id=%session_id, "error loading startup override session");
                        }
                    }
                }

                app.backfill_all_workspace_memory_async();

                if STARTUP_SESSION_PRELOAD_LIMIT > 0 && !app.session.sessions.is_empty() {
                    app.preload_recent_sessions_in_background(STARTUP_SESSION_PRELOAD_LIMIT);
                }
            }
            Err(e) => {
                error!(error=%e, "error initializing data manager");
                // Create a temporary session for this session only
                app.session.current_session = Some(ChatSession {
                    id: "temp".to_string(),
                    name: "Temporary Session".to_string(),
                    system_prompt: None,
                    json_summary: None,
                    claude_session_id: None,
                    minimax_session_id: None,
                    qwen_session_id: None,
                    codex_session_id: None,
                    nvidia_session_id: None,
                    gemini_session_id: None,
                    openrouter_session_id: None,
                    omniforge_session_id: None,
                    omniforge_model_id: None,
                    kimi_session_id: None,
                    opencode_session_id: None,
                    hermes_session_id: None,
                    forge_session_id: None,
                    claw_session_id: None,
                    launched_from: std::env::current_dir()
                        .ok()
                        .map(|p| p.to_string_lossy().into_owned()),
                    runsc_dirs: Vec::new(),
                    runsc_image: None,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    messages: vec![crate::data::ChatMessage {
                        content: config::TEMP_SESSION_MESSAGE.to_string(),
                        is_user: false,
                        timestamp: chrono::Utc::now(),
                        command: None,
                        retrieved_memory: None,
                    }],
                });
            }
        }

        // Discover any workers that survived a previous UI restart — in the
        // background so the window appears immediately.
        app.discover_workers_in_background();
        app.refresh_mcp_config_status_in_background();

        app
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(provider: ModelProvider) -> Self {
        let (_tx, rx) = mpsc::channel();
        let mut state = Self {
            session: SessionState::new(provider),
            worker: AppWorkerState::new(),
            layout: LayoutState::new(),
            sync_shm: None,
            sync_socket: SocketHandle::noop(),
            sync_events: rx,
            instance_number: None,
            dialog_parent: None,
        };
        state.refresh_mcp_config_status_in_background();
        state
    }

    fn next_height_cache_load_request_id(&mut self) -> u64 {
        self.session.latest_height_cache_load_request_id = self
            .session
            .latest_height_cache_load_request_id
            .saturating_add(1);
        self.session.latest_height_cache_load_request_id
    }

    fn next_mcp_config_request_id(&mut self) -> u64 {
        self.session.latest_mcp_config_request_id =
            self.session.latest_mcp_config_request_id.saturating_add(1);
        self.session.latest_mcp_config_request_id
    }

    pub(crate) fn refresh_mcp_config_status_in_background(&mut self) {
        let provider = self.session.provider;
        let mcp_json_path = provider_mcp_json_path(provider);
        let cache_key = format!("{}:{}", provider.as_str(), mcp_json_path.display());
        if self.session.mcp_config_cache_key.as_deref() == Some(cache_key.as_str())
            && self.session.mcp_config_status_rx.is_some()
        {
            return;
        }
        if self.session.mcp_config_cache_key.as_deref() == Some(cache_key.as_str())
            && self.session.fff_mcp_server_explicitly_configured.is_some()
        {
            return;
        }

        #[cfg(test)]
        {
            self.session.mcp_config_cache_key = Some(cache_key);
            self.session.fff_mcp_server_explicitly_configured = Some(
                read_explicit_mcp_server_configured(provider, &mcp_json_path, "fff"),
            );
            self.session.mcp_config_status_rx = None;
            self.session.mcp_config_status_started_at = None;
            return;
        }

        #[cfg(not(test))]
        {
            let request_id = self.next_mcp_config_request_id();
            let repaint_ctx = self.session.ui_io.repaint_ctx.clone();
            let (tx, rx) = mpsc::channel();
            self.session.mcp_config_cache_key = Some(cache_key.clone());
            self.session.fff_mcp_server_explicitly_configured = None;
            self.session.mcp_config_status_started_at = Some(Instant::now());
            self.session.mcp_config_status_rx = Some(rx);

            thread::spawn(move || {
                let fff_explicitly_configured =
                    read_explicit_mcp_server_configured(provider, &mcp_json_path, "fff");
                let _ = tx.send(McpConfigStatusResult {
                    request_id,
                    cache_key,
                    fff_explicitly_configured,
                });
                if let Some(ctx) = repaint_ctx {
                    ctx.request_repaint();
                }
            });
        }
    }

    pub(crate) fn spawn_height_cache_load_for_session(&mut self, session_id: String) {
        let Some(data_manager) = self.session.data_manager.clone() else {
            return;
        };
        if self.layout.session_height_cache.contains_key(&session_id) {
            return;
        }

        let request_id = self.next_height_cache_load_request_id();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.session.height_cache_load_started_at = Some(Instant::now());
        self.session.height_cache_load_rx = Some(rx);

        thread::spawn(move || {
            let cache = data_manager
                .load_height_cache(&session_id)
                .map_err(|error| error.to_string());
            let _ = tx.send(HeightCacheLoadResult {
                request_id,
                session_id,
                cache,
            });
            if let Some(ctx) = repaint_ctx {
                ctx.request_repaint();
            }
        });
    }

    pub(crate) fn drain_height_cache_load_results(&mut self) {
        let Some(rx) = self.session.height_cache_load_rx.take() else {
            return;
        };

        match rx.try_recv() {
            Ok(result) => {
                background_timing::finish_background_channel_timing(
                    "height_cache_load",
                    &mut self.session.height_cache_load_started_at,
                );
                self.handle_height_cache_load_result(result);
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.session.height_cache_load_rx = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                background_timing::finish_background_channel_timing(
                    "height_cache_load",
                    &mut self.session.height_cache_load_started_at,
                );
            }
        }
    }

    pub(crate) fn drain_mcp_config_status_results(&mut self) {
        let Some(rx) = self.session.mcp_config_status_rx.take() else {
            return;
        };

        match rx.try_recv() {
            Ok(result) => {
                background_timing::finish_background_channel_timing(
                    "mcp_config_status",
                    &mut self.session.mcp_config_status_started_at,
                );
                if result.request_id < self.session.latest_mcp_config_request_id {
                    return;
                }
                self.session.mcp_config_cache_key = Some(result.cache_key);
                self.session.fff_mcp_server_explicitly_configured =
                    Some(result.fff_explicitly_configured);
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.session.mcp_config_status_rx = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                background_timing::finish_background_channel_timing(
                    "mcp_config_status",
                    &mut self.session.mcp_config_status_started_at,
                );
            }
        }
    }

    fn handle_height_cache_load_result(&mut self, result: HeightCacheLoadResult) {
        if result.request_id < self.session.latest_height_cache_load_request_id {
            return;
        }

        match result.cache {
            Ok(Some(mut cache)) => {
                if self
                    .session
                    .current_session
                    .as_ref()
                    .is_some_and(|session| session.id == result.session_id)
                {
                    if let Some(session) = self.session.current_session.as_ref() {
                        validate_height_cache_entries(&mut cache, &session.messages);
                    }
                }
                self.layout
                    .session_height_cache
                    .insert(result.session_id, cache);
            }
            Ok(None) => {
                self.layout.session_height_cache.remove(&result.session_id);
            }
            Err(error) => {
                error!(
                    error=%error,
                    session_id=%result.session_id,
                    "failed to load height cache"
                );
            }
        }
    }
}

fn validate_height_cache_entries(cache: &mut SessionHeightCache, messages: &[ChatMessage]) {
    for entry in cache.widths.values_mut() {
        if entry.message_count == messages.len() && entry.row_heights.len() == messages.len() {
            entry.message_count = 0;
        }
    }
}

fn provider_mcp_json_path(provider: ModelProvider) -> PathBuf {
    crate::provider_bridge::background_provider_overrides(provider)
        .mcp_json_path
        .or_else(|| std::env::var_os("RUST_CHATBOT_TEST_MCP_JSON_PATH").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(config::mcp_json_path()))
}

fn read_explicit_mcp_server_configured(
    provider: ModelProvider,
    mcp_json_path: &Path,
    server_name: &str,
) -> bool {
    let Ok(json) = std::fs::read_to_string(mcp_json_path) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json) else {
        return false;
    };
    configured_provider_mcp_servers(&parsed, provider)
        .and_then(|servers| servers.get(server_name))
        .is_some()
}

fn configured_provider_mcp_servers<'a>(
    parsed: &'a serde_json::Value,
    provider: ModelProvider,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    provider_mcp_profile(provider)
        .and_then(|profile| {
            parsed
                .get("providers")
                .and_then(|value| value.as_object())
                .and_then(|providers| providers.get(profile))
                .and_then(|value| value.as_object())
                .and_then(|provider| provider.get("mcpServers"))
                .and_then(|value| value.as_object())
        })
        .or_else(|| parsed.get("mcpServers").and_then(|value| value.as_object()))
}

fn provider_mcp_profile(provider: ModelProvider) -> Option<&'static str> {
    match provider {
        ModelProvider::Claude
        | ModelProvider::MiniMax
        | ModelProvider::OmniForge
        | ModelProvider::GeminiForge
        | ModelProvider::GeminiCliForge
        | ModelProvider::MiniMaxForge
        | ModelProvider::Hermes
        | ModelProvider::Forge
        | ModelProvider::Claw => Some("claude"),
        ModelProvider::Codex | ModelProvider::Nvidia => Some("codex"),
        ModelProvider::Gemini => Some("gemini"),
        ModelProvider::Qwen
        | ModelProvider::OpenRouter
        | ModelProvider::Kimi
        | ModelProvider::OpenCode => None,
    }
}

#[cfg(test)]
#[path = "state/ui_spec_lifecycle_tests.rs"]
mod ui_spec_lifecycle_tests;
