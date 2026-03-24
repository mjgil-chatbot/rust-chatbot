use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use anyhow::{bail, Context, Result};
use rust_debug::tracing::{info, warn};
use crate::data::{ChatSession, DataManager};
use crate::memory;
use crate::model::ModelProvider;
use crate::storage;
const MEMORY_JOB_FLAG: &str = "--memory-job";
pub fn spawn_backfill_process(provider: ModelProvider) -> Result<()> {
    let provider_name = provider.as_str().to_string();
    spawn_background_memory_job(
        |command| {
            command
                .arg("backfill")
                .arg("--provider")
                .arg(provider_name.as_str());
        },
        move || {
            if let Err(err) = backfill_workspace_memory(provider) {
                warn!(
                    error=%err,
                    provider=%provider.as_str(),
                    "workspace-memory backfill in-process fallback failed"
                );
            }
        },
    )
    .with_context(|| {
        format!(
            "spawn workspace-memory backfill helper for {}",
            provider_name
        )
    })
}
pub fn spawn_index_chat_process(
    provider: ModelProvider,
    chat_id: &str,
    final_state: &str,
) -> Result<()> {
    let provider_name = provider.as_str().to_string();
    let chat_id = chat_id.to_string();
    let final_state = final_state.to_string();
    let provider_name_for_command = provider_name.clone();
    let chat_id_for_command = chat_id.clone();
    let final_state_for_command = final_state.clone();
    let chat_id_for_context = chat_id.clone();
    let chat_id_for_fallback = chat_id.clone();
    let final_state_for_fallback = final_state.clone();
    spawn_background_memory_job(
        move |command| {
            command
                .arg("index-chat")
                .arg("--provider")
                .arg(provider_name_for_command.as_str())
                .arg("--chat-id")
                .arg(&chat_id_for_command)
                .arg("--final-state")
                .arg(&final_state_for_command);
        },
        move || {
            if let Err(err) =
                index_memory_for_chat(provider, &chat_id_for_fallback, &final_state_for_fallback)
            {
                warn!(
                    error=%err,
                    provider=%provider.as_str(),
                    chat_id=%chat_id_for_fallback,
                    "workspace-memory index in-process fallback failed"
                );
            }
        },
    )
    .with_context(|| {
        format!(
            "spawn workspace-memory index helper for chat {}",
            chat_id_for_context
        )
    })
}
fn spawn_background_memory_job<F, G>(configure: F, fallback: G) -> Result<()>
where
    F: FnOnce(&mut Command),
    G: FnOnce() + Send + 'static,
{
    let Some(mut command) = memory_job_command()? else {
        std::thread::spawn(fallback);
        return Ok(());
    };
    configure(&mut command);
    let mut child = match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            warn!(
                error=%err,
                "memory helper launch failed; falling back to in-process execution"
            );
            std::thread::spawn(fallback);
            return Ok(());
        }
    };

    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}
fn memory_job_command() -> Result<Option<Command>> {
    let current_exe = current_executable_path()?;
    let Some(memory_job_exe) = resolve_memory_job_target(&current_exe)? else {
        return Ok(None);
    };
    let mut command = Command::new(memory_job_exe);
    command.arg(MEMORY_JOB_FLAG);
    Ok(Some(command))
}
fn current_executable_path() -> Result<PathBuf> {
    let current_exe =
        std::env::current_exe().context("resolve current executable for memory job")?;
    Ok(current_exe.canonicalize().ok().unwrap_or(current_exe))
}
fn resolve_memory_job_binary_from(current_exe: &Path) -> Result<PathBuf> {
    if let Some(path) = resolve_memory_job_binary_from_cargo_env() {
        return Ok(path);
    }

    let current_dir = current_exe.parent().unwrap_or(Path::new("."));
    let mut search_dirs = Vec::new();
    push_unique_path(&mut search_dirs, current_dir.to_path_buf());

    if current_dir.file_name().and_then(|name| name.to_str()) == Some("deps") {
        if let Some(profile_dir) = current_dir.parent() {
            push_unique_path(&mut search_dirs, profile_dir.to_path_buf());
        }
    }

    for search_dir in search_dirs {
        let canonical = search_dir.join(crate::config::UI_BINARY_NAME);
        if canonical.is_file() {
            return Ok(canonical);
        }
    }

    let current_name = current_exe.file_name().and_then(|name| name.to_str());
    if matches!(current_name, Some(crate::config::UI_BINARY_NAME)) {
        return Ok(current_exe.to_path_buf());
    }

    bail!(
        "could not locate `{}` alongside `{}` for memory helper launch",
        crate::config::UI_BINARY_NAME,
        current_exe.display()
    );
}
fn resolve_memory_job_binary_from_cargo_env() -> Option<PathBuf> {
    for env_name in [
        format!("CARGO_BIN_EXE_{}", crate::config::UI_BINARY_NAME),
        format!(
            "CARGO_BIN_EXE_{}",
            crate::config::UI_BINARY_NAME.replace('-', "_")
        ),
    ] {
        let Some(path) = std::env::var_os(&env_name).map(PathBuf::from) else {
            continue;
        };
        if path.is_file() {
            return Some(path.canonicalize().ok().unwrap_or(path));
        }
    }
    None
}
fn is_test_host_executable(current_exe: &Path) -> bool {
    current_exe
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        == Some("deps")
}
fn resolve_memory_job_target(current_exe: &Path) -> Result<Option<PathBuf>> {
    match resolve_memory_job_binary_from(current_exe) {
        Ok(path) => Ok(Some(path)),
        Err(err) if is_test_host_executable(current_exe) => {
            warn!(
                error=%err,
                executable=%current_exe.display(),
                "memory helper binary unavailable; falling back to in-process execution"
            );
            Ok(None)
        }
        Err(err) => Err(err),
    }
}
fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|existing| existing == &candidate) {
        paths.push(candidate);
    }
}
fn backfill_workspace_memory(provider: ModelProvider) -> Result<()> {
    if !memory::memory_enabled() {
        return Ok(());
    }

    let data_manager = DataManager::new(provider)
        .with_context(|| format!("initialize data manager for {}", provider.as_str()))?;

    if let Some(workspaces) = indexed_workspaces(provider) {
        for workspace in workspaces {
            let sessions = match load_sessions_for_workspace_id(
                &data_manager,
                provider,
                &workspace.workspace_id,
            ) {
                Ok(sessions) => sessions,
                Err(err) => {
                    warn!(
                        error=%err,
                        workspace=%workspace.workspace_path,
                        "failed to load indexed workspace sessions for memory backfill"
                    );
                    continue;
                }
            };
            if sessions.is_empty() {
                continue;
            }

            index_workspace_sessions_logged(
                &workspace.workspace_path,
                &sessions,
                &BTreeMap::new(),
                None,
                "memory backfill completed",
                "memory backfill failed; continuing without indexed history",
            );
        }
        return Ok(());
    }

    let session_infos = data_manager
        .list_sessions()
        .context("list sessions for workspace-memory backfill")?;

    let mut sessions_by_workspace: BTreeMap<String, Vec<ChatSession>> = BTreeMap::new();
    for session_info in session_infos {
        let session = match data_manager.load_session(&session_info.id) {
            Ok(session) => session,
            Err(err) => {
                warn!(
                    error=%err,
                    session_id=%session_info.id,
                    "failed to load session for memory backfill"
                );
                continue;
            }
        };

        let Some(launched_from) = session.launched_from.clone() else {
            continue;
        };

        match memory::workspace_memory_paths(&launched_from) {
            Ok(paths) => {
                let workspace = paths.workspace_root.to_string_lossy().into_owned();
                sessions_by_workspace
                    .entry(workspace)
                    .or_default()
                    .push(session);
            }
            Err(err) => {
                warn!(
                    error=%err,
                    session_id=%session_info.id,
                    workspace=%launched_from,
                    "failed to resolve workspace for memory backfill"
                );
            }
        }
    }

    for (workspace, sessions) in sessions_by_workspace {
        index_workspace_sessions_logged(
            &workspace,
            &sessions,
            &BTreeMap::new(),
            None,
            "memory backfill completed",
            "memory backfill failed; continuing without indexed history",
        );
    }

    Ok(())
}
fn index_memory_for_chat(provider: ModelProvider, chat_id: &str, final_state: &str) -> Result<()> {
    if !memory::memory_enabled() {
        return Ok(());
    }

    let data_manager = DataManager::new(provider)
        .with_context(|| format!("initialize data manager for {}", provider.as_str()))?;
    let session = data_manager
        .load_session(chat_id)
        .with_context(|| format!("load session {chat_id} for memory indexing"))?;
    let Some(launched_from) = session.launched_from.clone() else {
        return Ok(());
    };

    let workspace_root = memory::workspace_memory_paths(&launched_from)
        .with_context(|| format!("resolve workspace for {launched_from}"))?
        .workspace_root;
    let workspace_sessions = load_sessions_for_workspace(&data_manager, provider, &workspace_root)
        .with_context(|| format!("load workspace sessions for {}", workspace_root.display()))?;

    let mut final_state_overrides = BTreeMap::new();
    final_state_overrides.insert(chat_id.to_string(), final_state.to_string());

    let workspace = workspace_root.to_string_lossy().into_owned();
    index_workspace_sessions_logged(
        &workspace,
        &workspace_sessions,
        &final_state_overrides,
        Some(chat_id),
        "memory indexing completed",
        "memory indexing failed; continuing without blocking chat",
    );
    Ok(())
}
fn load_sessions_for_workspace(
    data_manager: &DataManager,
    provider: ModelProvider,
    workspace_root: &std::path::Path,
) -> Result<Vec<ChatSession>> {
    if let Ok(workspace_id) = memory::workspace_id_for_path(workspace_root) {
        if let Ok(sessions) = load_sessions_for_workspace_id(data_manager, provider, &workspace_id)
        {
            if !sessions.is_empty() {
                return Ok(sessions);
            }
        }
    }

    let mut sessions = Vec::new();
    for session_info in data_manager.list_sessions()? {
        let session = match data_manager.load_session(&session_info.id) {
            Ok(session) => session,
            Err(err) => {
                warn!(
                    error=%err,
                    session_id=%session_info.id,
                    "failed to load session while scanning workspace sessions"
                );
                continue;
            }
        };
        let Some(launched_from) = session.launched_from.clone() else {
            continue;
        };
        let Ok(paths) = memory::workspace_memory_paths(&launched_from) else {
            continue;
        };
        if paths.workspace_root == workspace_root {
            sessions.push(session);
        }
    }

    Ok(sessions)
}
fn indexed_workspaces(provider: ModelProvider) -> Option<Vec<storage::IndexedWorkspace>> {
    let index = storage::get_index()?;
    let conn = index.connection();
    let workspaces = storage::list_workspaces_from_index(&conn, provider.as_str()).ok()?;
    (!workspaces.is_empty()).then_some(workspaces)
}
fn load_sessions_for_workspace_id(
    data_manager: &DataManager,
    provider: ModelProvider,
    workspace_id: &str,
) -> Result<Vec<ChatSession>> {
    let index =
        storage::get_index().ok_or_else(|| anyhow::anyhow!("SQLite session index unavailable"))?;
    let conn = index.connection();
    let session_ids =
        storage::list_session_ids_for_workspace(&conn, provider.as_str(), workspace_id)?;
    drop(conn);
    load_sessions_by_id(data_manager, &session_ids)
}
fn load_sessions_by_id(
    data_manager: &DataManager,
    session_ids: &[String],
) -> Result<Vec<ChatSession>> {
    let mut sessions = Vec::with_capacity(session_ids.len());
    for session_id in session_ids {
        match data_manager.load_session(session_id) {
            Ok(session) => sessions.push(session),
            Err(err) => {
                warn!(
                    error=%err,
                    session_id=%session_id,
                    "failed to load session while iterating indexed workspace sessions"
                );
            }
        }
    }
    Ok(sessions)
}
fn index_workspace_sessions_logged(
    workspace: &str,
    sessions: &[ChatSession],
    final_state_overrides: &BTreeMap<String, String>,
    chat_id: Option<&str>,
    success_log: &str,
    failure_log: &str,
) {
    match memory::index_workspace_sessions(workspace, sessions, final_state_overrides) {
        Ok(summary) => {
            if summary.indexed_messages == 0 && !summary.rebuilt {
                return;
            }

            if let Some(chat_id) = chat_id {
                info!(
                    message = success_log,
                    chat_id=%chat_id,
                    workspace=%workspace,
                    indexed_sessions=summary.indexed_sessions,
                    indexed_messages=summary.indexed_messages,
                    rebuilt=summary.rebuilt,
                    "workspace-memory helper completed"
                );
            } else {
                info!(
                    message = success_log,
                    workspace=%workspace,
                    indexed_sessions=summary.indexed_sessions,
                    indexed_messages=summary.indexed_messages,
                    rebuilt=summary.rebuilt,
                    "workspace-memory helper completed"
                );
            }
        }
        Err(err) => {
            if let Some(chat_id) = chat_id {
                warn!(
                    message = failure_log,
                    error=%err,
                    chat_id=%chat_id,
                    workspace=%workspace,
                    "workspace-memory helper failed"
                );
            } else {
                warn!(
                    message = failure_log,
                    error=%err,
                    workspace=%workspace,
                    "workspace-memory helper failed"
                );
            }
        }
    }
}
