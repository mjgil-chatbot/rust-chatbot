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
pub(crate) fn save_provider_workspace_snapshot(
    provider: ModelProvider,
    current_dir: Option<&str>,
    recent_dirs: &[String],
) -> Result<()> {
    let mut history = read_workspace_history().unwrap_or_default();
    let mut recent_dirs = recent_dirs.to_vec();
    if let Some(current_dir) = current_dir {
        push_recent_dir(&mut recent_dirs, current_dir);
    }
    history.providers.insert(
        provider.as_str().to_string(),
        ProviderWorkspacePrefs {
            current_dir: current_dir.map(str::to_string),
            recent_dirs,
        },
    );
    write_workspace_history(&history)
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
pub(crate) fn normalize_workspace_input(input: &str) -> Result<String> {
    let trimmed = input.trim();
    anyhow::ensure!(!trimmed.is_empty(), "workspace path is empty");
    let expanded = expand_home(trimmed);
    let path = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .context("failed to read current working directory")?
            .join(expanded)
    };
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", path.display()))?;
    anyhow::ensure!(
        canonical.is_dir(),
        "{} is not a directory",
        canonical.display()
    );
    Ok(canonical.to_string_lossy().into_owned())
}
pub(crate) fn workspace_completion_matches(input: &str, limit: usize) -> Vec<String> {
    let Some(context) = completion_context(input) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&context.search_dir) else {
        return Vec::new();
    };

    let mut matches = entries
        .flatten()
        .filter_map(|entry| {
            let entry_type = entry.file_type().ok()?;
            if !entry_type.is_dir() {
                return None;
            }
            let name = entry.file_name();
            let name = name.to_str()?;
            if !name.starts_with(context.typed_prefix.as_str()) {
                return None;
            }
            Some(format!("{}{name}/", context.display_prefix))
        })
        .collect::<Vec<_>>();
    matches.sort_unstable();
    matches.truncate(limit);
    matches
}
pub(crate) fn autocomplete_workspace_input(input: &str) -> Option<String> {
    if input.trim() == "~" {
        return Some("~/".to_string());
    }

    let matches = workspace_completion_matches(input, usize::MAX);
    autocomplete_workspace_input_from_matches(input, &matches)
}
pub(crate) fn autocomplete_workspace_input_from_matches(
    input: &str,
    matches: &[String],
) -> Option<String> {
    if input.trim() == "~" {
        return Some("~/".to_string());
    }

    if matches.is_empty() {
        return None;
    }
    if matches.len() == 1 {
        return Some(matches[0].clone());
    }

    let common = longest_common_prefix(&matches);
    if common.len() > input.len() {
        Some(common)
    } else {
        None
    }
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
fn write_workspace_history(history: &WorkspaceHistoryFile) -> Result<()> {
    let path = workspace_history_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let contents = serde_json::to_string_pretty(history)?;
    std::fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
}
fn expand_home(input: &str) -> PathBuf {
    if input == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(input)
}
fn longest_common_prefix(values: &[String]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };
    let mut prefix = first.clone();
    for value in &values[1..] {
        while !value.starts_with(prefix.as_str()) {
            prefix.pop();
            if prefix.is_empty() {
                return prefix;
            }
        }
    }
    prefix
}
struct CompletionContext {
    search_dir: PathBuf,
    typed_prefix: String,
    display_prefix: String,
}
fn completion_context(input: &str) -> Option<CompletionContext> {
    let current_dir = std::env::current_dir().ok()?;
    if input.is_empty() {
        return Some(CompletionContext {
            search_dir: current_dir,
            typed_prefix: String::new(),
            display_prefix: String::new(),
        });
    }

    let expanded = expand_home(input);
    let ends_with_separator = input.ends_with(std::path::MAIN_SEPARATOR);
    if ends_with_separator {
        let search_dir = if expanded.is_absolute() {
            expanded
        } else {
            current_dir.join(expanded)
        };
        return Some(CompletionContext {
            search_dir,
            typed_prefix: String::new(),
            display_prefix: input.to_string(),
        });
    }

    let typed_prefix = expanded
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let display_prefix = input
        .rfind(std::path::MAIN_SEPARATOR)
        .map(|index| input[..=index].to_string())
        .unwrap_or_default();
    let parent = expanded
        .parent()
        .map(Path::to_path_buf)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| current_dir.clone());
    let search_dir = if parent.is_absolute() {
        parent
    } else {
        current_dir.join(parent)
    };
    Some(CompletionContext {
        search_dir,
        typed_prefix,
        display_prefix,
    })
}
