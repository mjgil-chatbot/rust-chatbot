use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ai_history_core::{Actor, ContentHash};
use ai_history_snapshot::manifest::Manifest;
use ai_history_store::HistoryStore;
use anyhow::Result;

use super::{memory, ChatSession, ParsedTurn};

const HISTORY_SESSION_PREFIX: &str = "rust-chatbot:";
const HISTORY_STORE_ROOT_DIR: &str = "ai-history";
const HISTORY_STORE_BACKUP_ROOT_DIR: &str = "ai-history-backup";
const HISTORY_OBJECTS_DIR: &str = "objects";
const HISTORY_OBJECT_HASH_DIR: &str = "sha256";
const AUTO_TURN_CHECKPOINT_PREFIX: &str = "auto-turn:";
const HISTORY_BACKUP_KIND_FILE: &str = "backup-kind";
const HISTORY_BACKUP_KIND_EXACT: &str = "exact-db";
const HISTORY_BACKUP_KIND_METADATA: &str = "metadata-snapshot";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum HistoryRestoreKind {
    #[default]
    None,
    ExactCopy,
    MetadataSnapshot,
}

impl HistoryRestoreKind {
    pub(super) fn restored(self) -> bool {
        !matches!(self, Self::None)
    }
}

pub(super) fn history_missing_notice() -> String {
    "No ai-history store exists under the shared history root for this workspace yet.".to_string()
}

pub(super) fn history_restored_notice(kind: HistoryRestoreKind) -> String {
    match kind {
        HistoryRestoreKind::ExactCopy => {
            "History store was restored byte-for-byte from the deleted shared backup.".to_string()
        }
        HistoryRestoreKind::MetadataSnapshot => {
            "History store metadata and CAS objects were restored from the shared backup snapshot."
                .to_string()
        }
        HistoryRestoreKind::None => String::new(),
    }
}

pub(super) fn history_store_path(workspace_root: &Path) -> Result<PathBuf> {
    Ok(history_store_dir(workspace_root)?.join("store.db"))
}

pub(super) fn history_store_backup_path(workspace_root: &Path) -> Result<PathBuf> {
    Ok(history_store_backup_dir(workspace_root)?.join("store.db"))
}

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

pub(super) fn history_store_is_available(workspace_root: &Path) -> Result<bool> {
    Ok(history_store_path(workspace_root)?.is_file()
        || history_store_backup_path(workspace_root)?.is_file())
}

pub(super) fn restore_history_store_if_needed(workspace_root: &Path) -> Result<HistoryRestoreKind> {
    let store_path = history_store_path(workspace_root)?;
    let backup_path = history_store_backup_path(workspace_root)?;
    let object_dir = history_object_store_dir(workspace_root)?;
    let backup_object_dir = history_object_store_backup_dir(workspace_root)?;
    let backup_kind = read_history_backup_kind(workspace_root)?;
    let mut restored = HistoryRestoreKind::None;

    if !store_path.is_file() && backup_path.is_file() {
        if let Some(parent) = store_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&backup_path, &store_path)?;
        restored = backup_kind;
    }

    if !object_dir.exists() && backup_object_dir.exists() {
        copy_dir_recursive(&backup_object_dir, &object_dir)?;
        if !restored.restored() {
            restored = backup_kind;
        }
    }

    Ok(restored)
}

pub(super) fn mirror_history_store(workspace_root: &Path) -> Result<()> {
    let store_path = history_store_path(workspace_root)?;
    if !store_path.is_file() {
        return Ok(());
    }
    let backup_path = history_store_backup_path(workspace_root)?;
    let store = HistoryStore::open(&store_path)?;
    let backup_kind = if store.uses_metadata_snapshots()? {
        store.write_metadata_snapshot(&backup_path)?;
        HistoryRestoreKind::MetadataSnapshot
    } else {
        if let Some(parent) = backup_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&store_path, &backup_path)?;
        HistoryRestoreKind::ExactCopy
    };
    write_history_backup_kind(workspace_root, backup_kind)?;

    let object_dir = history_object_store_dir(workspace_root)?;
    let backup_object_dir = history_object_store_backup_dir(workspace_root)?;
    if object_dir.exists() {
        copy_dir_recursive(&object_dir, &backup_object_dir)?;
    } else if backup_object_dir.exists() {
        fs::remove_dir_all(backup_object_dir)?;
    }
    Ok(())
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

pub(super) fn linked_history_session_title(chat: &ChatSession) -> String {
    format!("{HISTORY_SESSION_PREFIX}{}:{}", chat.id, chat.name)
}

pub(super) fn display_history_session_title(title: &str) -> String {
    if let Some(rest) = title.strip_prefix(HISTORY_SESSION_PREFIX) {
        if let Some((_, display_name)) = rest.split_once(':') {
            return display_name.to_string();
        }
    }
    title.to_string()
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

pub(super) fn parse_chat_turns(chat: &ChatSession) -> Vec<ParsedTurn> {
    let mut turns = Vec::new();
    let mut current: Option<ParsedTurn> = None;

    for message in &chat.messages {
        if message.is_user {
            if let Some(turn) = current.take() {
                turns.push(turn);
            }
            current = Some(ParsedTurn {
                actor: Actor::User,
                user_content: Some(message.content.clone()),
                assistant_content: None,
                timestamp: message.timestamp.to_rfc3339(),
            });
            continue;
        }

        match current.as_mut() {
            Some(turn) => append_assistant_message(turn, &message.content),
            None => {
                current = Some(ParsedTurn {
                    actor: Actor::Assistant,
                    user_content: None,
                    assistant_content: Some(message.content.clone()),
                    timestamp: message.timestamp.to_rfc3339(),
                });
            }
        }
    }

    if let Some(turn) = current {
        turns.push(turn);
    }

    turns
}

pub(super) fn auto_turn_checkpoint_reason(chat: &ChatSession, turn_index: usize) -> String {
    format!(
        "{AUTO_TURN_CHECKPOINT_PREFIX}{}:{}",
        chat.id,
        turn_index + 1
    )
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

fn history_store_backup_kind_path(workspace_root: &Path) -> Result<PathBuf> {
    Ok(history_store_backup_dir(workspace_root)?.join(HISTORY_BACKUP_KIND_FILE))
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

fn read_history_backup_kind(workspace_root: &Path) -> Result<HistoryRestoreKind> {
    let marker_path = history_store_backup_kind_path(workspace_root)?;
    if !marker_path.is_file() {
        return Ok(HistoryRestoreKind::ExactCopy);
    }
    let marker = fs::read_to_string(marker_path)?;
    Ok(match marker.trim() {
        HISTORY_BACKUP_KIND_METADATA => HistoryRestoreKind::MetadataSnapshot,
        _ => HistoryRestoreKind::ExactCopy,
    })
}

fn write_history_backup_kind(workspace_root: &Path, kind: HistoryRestoreKind) -> Result<()> {
    let marker_path = history_store_backup_kind_path(workspace_root)?;
    if let Some(parent) = marker_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let marker = match kind {
        HistoryRestoreKind::MetadataSnapshot => HISTORY_BACKUP_KIND_METADATA,
        _ => HISTORY_BACKUP_KIND_EXACT,
    };
    fs::write(marker_path, marker)?;
    Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
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

fn append_assistant_message(turn: &mut ParsedTurn, content: &str) {
    match turn.assistant_content.as_mut() {
        Some(existing) if !existing.is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(content);
        }
        Some(existing) => existing.push_str(content),
        None => turn.assistant_content = Some(content.to_string()),
    }
}
