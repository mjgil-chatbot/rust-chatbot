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
pub(crate) const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(200);
const STARTUP_SESSION_PRELOAD_LIMIT: usize = 0;
const PREALLOCATED_SESSION_POOL_SIZE: usize = 5;
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
