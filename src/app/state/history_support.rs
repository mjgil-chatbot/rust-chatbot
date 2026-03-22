use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use ai_history_core::{Actor, ContentHash};
use ai_history_snapshot::manifest::Manifest;
use ai_history_store::HistoryStore;
use anyhow::Result;
use super::memory;
const HISTORY_STORE_ROOT_DIR: &str = "ai-history";
const HISTORY_STORE_BACKUP_ROOT_DIR: &str = "ai-history-backup";
const HISTORY_OBJECTS_DIR: &str = "objects";
const HISTORY_OBJECT_HASH_DIR: &str = "sha256";
const AUTO_TURN_CHECKPOINT_PREFIX: &str = "auto-turn:";
pub(super) fn history_object_store_dir(workspace_root: &Path) -> Result<PathBuf> {
    Ok(history_store_dir(workspace_root)?
        .join(HISTORY_OBJECTS_DIR)
        .join(HISTORY_OBJECT_HASH_DIR))
}
pub(super) fn history_object_store_backup_dir(workspace_root: &Path) -> Result<PathBuf> {
    Ok(history_store_backup_dir(workspace_root)?
        .join(HISTORY_OBJECTS_DIR)
        .join(HISTORY_OBJECT_HASH_DIR))
}
pub(super) fn write_history_object(
    workspace_root: &Path,
    blob_hash: &ContentHash,
    contents: &[u8],
) -> Result<()> {
    let final_path = history_object_path(workspace_root, blob_hash)?;
    if final_path.is_file() {
        return Ok(());
    }

    let parent = final_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("history object path missing parent"))?;
    fs::create_dir_all(parent)?;

    let temp_path = parent.join(format!(
        ".tmp-{}-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        blob_hash.to_hex()
    ));
    let mut file = fs::File::create(&temp_path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(&temp_path, &final_path)?;
    Ok(())
}
pub(super) fn read_history_object(
    workspace_root: &Path,
    blob_hash: &ContentHash,
) -> Result<Option<Vec<u8>>> {
    let path = history_object_path(workspace_root, blob_hash)?;
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(fs::read(path)?))
}
pub(super) fn read_or_restore_history_object(
    workspace_root: &Path,
    blob_hash: &ContentHash,
) -> Result<Option<Vec<u8>>> {
    if let Some(bytes) = read_history_object(workspace_root, blob_hash)? {
        return Ok(Some(bytes));
    }

    let backup_path = history_object_backup_path(workspace_root, blob_hash)?;
    if !backup_path.is_file() {
        return Ok(None);
    }

    let bytes = fs::read(backup_path)?;
    write_history_object(workspace_root, blob_hash, &bytes)?;
    Ok(Some(bytes))
}
pub(super) fn checkpoint_display_name(reason: &str) -> (Option<String>, bool) {
    if let Some(name) = reason.strip_prefix("named:") {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return (Some(trimmed.to_string()), true);
        }
    }

    if let Some(turn_number) = auto_turn_checkpoint_label(reason) {
        return (Some(format!("Turn {turn_number}")), false);
    }

    (None, false)
}
pub(super) fn truncate_preview(content: &str) -> String {
    let preview = content.trim().replace('\n', " ");
    if preview.len() <= 80 {
        return preview;
    }
    format!("{}...", &preview[..77])
}
pub(super) fn deserialize_manifest(raw: &str) -> Result<Manifest> {
    Ok(serde_json::from_str(raw)?)
}
pub(super) fn is_auto_turn_checkpoint_reason(reason: &str) -> bool {
    reason.starts_with(AUTO_TURN_CHECKPOINT_PREFIX)
}
fn shared_history_root_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("git")
        .join(".rust-chatbot")
}
fn history_store_dir(workspace_root: &Path) -> Result<PathBuf> {
    history_store_root_dir(workspace_root, HISTORY_STORE_ROOT_DIR)
}
fn history_store_backup_dir(workspace_root: &Path) -> Result<PathBuf> {
    history_store_root_dir(workspace_root, HISTORY_STORE_BACKUP_ROOT_DIR)
}
fn history_store_root_dir(workspace_root: &Path, root_dir_name: &str) -> Result<PathBuf> {
    let workspace_id = memory::workspace_id_for_path(workspace_root)?;
    Ok(shared_history_root_dir()
        .join(root_dir_name)
        .join(workspace_id))
}
fn history_object_path(workspace_root: &Path, blob_hash: &ContentHash) -> Result<PathBuf> {
    let hash_hex = blob_hash.to_hex();
    let prefix_a = &hash_hex[0..2];
    let prefix_b = &hash_hex[2..4];
    Ok(history_object_store_dir(workspace_root)?
        .join(prefix_a)
        .join(prefix_b)
        .join(hash_hex))
}
fn history_object_backup_path(workspace_root: &Path, blob_hash: &ContentHash) -> Result<PathBuf> {
    let hash_hex = blob_hash.to_hex();
    let prefix_a = &hash_hex[0..2];
    let prefix_b = &hash_hex[2..4];
    Ok(history_object_store_backup_dir(workspace_root)?
        .join(prefix_a)
        .join(prefix_b)
        .join(hash_hex))
}
fn auto_turn_checkpoint_label(reason: &str) -> Option<&str> {
    let rest = reason.strip_prefix(AUTO_TURN_CHECKPOINT_PREFIX)?;
    let (_, turn_number) = rest.rsplit_once(':')?;
    let turn_number = turn_number.trim();
    if turn_number.is_empty() {
        return None;
    }
    Some(turn_number)
}
