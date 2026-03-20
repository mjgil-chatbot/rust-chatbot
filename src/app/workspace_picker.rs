use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use crate::{config, model::ModelProvider};
const WORKSPACE_HISTORY_FILE: &str = "workspace-history.json";
const MAX_RECENT_WORKSPACES: usize = 5;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProviderWorkspaceSnapshot {
    pub(crate) current_dir: Option<String>,
    pub(crate) recent_dirs: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct WorkspaceHistoryFile {
    #[serde(default)]
    providers: BTreeMap<String, ProviderWorkspacePrefs>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ProviderWorkspacePrefs {
    #[serde(default)]
    current_dir: Option<String>,
    #[serde(default)]
    recent_dirs: Vec<String>,
}
pub(crate) fn load_provider_workspace_snapshot(
    provider: ModelProvider,
) -> ProviderWorkspaceSnapshot {
    let history = read_workspace_history().unwrap_or_default();
    let mut prefs = history
        .providers
        .get(provider.as_str())
        .cloned()
        .unwrap_or_default();
    let current_dir = prefs
        .current_dir
        .clone()
        .or_else(|| prefs.recent_dirs.first().cloned())
        .or_else(default_workspace_dir);
    if let Some(current_dir) = current_dir.as_deref() {
        push_recent_dir(&mut prefs.recent_dirs, current_dir);
    }
    ProviderWorkspaceSnapshot {
        current_dir,
        recent_dirs: prefs.recent_dirs,
    }
}
pub(crate) fn push_recent_dir(recent_dirs: &mut Vec<String>, path: &str) {
    recent_dirs.retain(|candidate| candidate != path);
    recent_dirs.insert(0, path.to_string());
    recent_dirs.truncate(MAX_RECENT_WORKSPACES);
}
pub(crate) fn default_workspace_dir() -> Option<String> {
    if let Some(home) = dirs::home_dir() {
        let git_dir = home.join("git");
        if git_dir.is_dir() {
            return Some(git_dir.to_string_lossy().into_owned());
        }
    }

    std::env::current_dir()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}
fn workspace_history_path() -> PathBuf {
    config::canonical_shared_root_dir().join(WORKSPACE_HISTORY_FILE)
}
fn read_workspace_history() -> Result<WorkspaceHistoryFile> {
    let path = workspace_history_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(WorkspaceHistoryFile::default())
        }
        Err(err) => Err(err).with_context(|| format!("failed to read {}", path.display())),
    }
}
