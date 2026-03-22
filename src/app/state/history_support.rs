use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use ai_history_core::{Actor, ContentHash};
use ai_history_snapshot::manifest::Manifest;
use ai_history_store::HistoryStore;
use anyhow::Result;
const AUTO_TURN_CHECKPOINT_PREFIX: &str = "auto-turn:";
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
fn auto_turn_checkpoint_label(reason: &str) -> Option<&str> {
    let rest = reason.strip_prefix(AUTO_TURN_CHECKPOINT_PREFIX)?;
    let (_, turn_number) = rest.rsplit_once(':')?;
    let turn_number = turn_number.trim();
    if turn_number.is_empty() {
        return None;
    }
    Some(turn_number)
}
