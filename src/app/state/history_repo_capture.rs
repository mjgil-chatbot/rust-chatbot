use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use ai_history_core::{CheckpointId, ContentHash, ManifestId, SessionId, WorkspacePath};
use ai_history_snapshot::manifest::{FileEntry, Manifest};
use ai_history_snapshot::scanner::{Scanner, ScannerConfig};
use ai_history_store::{BlobStore, HistoryStore as SqliteStore};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepoPatchCaptureState {
    #[default]
    Captured,
    OmittedOversize,
    OmittedQuota,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct CheckpointRepoSummary {
    pub(crate) checkpoint_id: CheckpointId,
    pub(crate) repos: Vec<RepoPatchSummary>,
}
#[derive(Clone, Debug)]
pub(crate) struct RepoPatchSummary {
    pub(crate) slug: String,
    pub(crate) repo_name: String,
    pub(crate) role: String,
    pub(crate) repo_root: String,
    pub(crate) base_commit: String,
    pub(crate) apply_commit: String,
    pub(crate) patch_bytes: usize,
    pub(crate) tracked_patch_bytes: usize,
    pub(crate) untracked_file_count: usize,
    pub(crate) patch_is_empty: bool,
    pub(crate) total_patch_bytes: usize,
    pub(crate) capture_state: RepoPatchCaptureState,
    pub(crate) omitted_reason: Option<String>,
    pub(crate) captured_at: String,
}
