use std::collections::HashMap;
use std::path::Path;

use ai_history_core::BranchId;
use ai_history_store::{CheckpointInfo as HistoryCheckpointInfo, HistoryStore as SqliteStore};
use anyhow::Result;
use chrono::{DateTime, Utc};

use super::is_auto_turn_checkpoint_reason;

const AUTO_CHECKPOINT_LIMIT_PER_SESSION: usize = 50;
const AUTO_CHECKPOINT_RETENTION_HOURS: i64 = 24;
const INLINE_BLOB_MIGRATION_BATCH_SIZE: usize = 512;

#[derive(Clone, Debug, Default)]
pub(crate) struct HistoryMaintenanceSummary {
    pub(crate) object_store_bytes: u64,
    pub(crate) object_store_files: usize,
    pub(crate) auto_checkpoint_count: usize,
    pub(crate) deleted_auto_checkpoints: usize,
    pub(crate) deleted_object_files: usize,
    pub(crate) reclaimed_bytes: u64,
    pub(crate) over_quota: bool,
    pub(crate) notice: Option<String>,
}

impl HistoryMaintenanceSummary {
    pub(crate) fn mutated(&self) -> bool {
        self.deleted_auto_checkpoints > 0
            || self.deleted_object_files > 0
            || self.reclaimed_bytes > 0
    }
}

pub(crate) fn maintain_history_store(
    store: &mut SqliteStore,
    _workspace_root: &Path,
) -> Result<HistoryMaintenanceSummary> {
    let sessions = store.list_sessions()?;
    let mut auto_checkpoint_count = 0usize;
    let mut checkpoint_ids_to_delete = Vec::new();

    for session in sessions {
        let checkpoints = store.list_checkpoints(session.id)?;
        auto_checkpoint_count += checkpoints
            .iter()
            .filter(|checkpoint| is_auto_turn_checkpoint_reason(&checkpoint.reason))
            .count();
        checkpoint_ids_to_delete.extend(checkpoints_to_prune(&checkpoints)?);
    }

    for checkpoint in &checkpoint_ids_to_delete {
        store.delete_checkpoint(checkpoint.id)?;
    }
    auto_checkpoint_count = auto_checkpoint_count.saturating_sub(checkpoint_ids_to_delete.len());

    let migration = store.migrate_legacy_inline_blobs(INLINE_BLOB_MIGRATION_BATCH_SIZE)?;
    let object_gc = store.garbage_collect_objects(true)?;
    let quota = store.quota_report()?;

    let mut notice_parts = Vec::new();
    if !checkpoint_ids_to_delete.is_empty() {
        notice_parts.push(format!(
            "Pruned {} auto checkpoints.",
            checkpoint_ids_to_delete.len()
        ));
    }
    if migration.migrated_blobs > 0 {
        notice_parts.push(format!(
            "Migrated {} legacy inline history blobs into CAS.",
            migration.migrated_blobs
        ));
    }
    if object_gc.deleted_objects > 0 {
        notice_parts.push(format!(
            "Reclaimed {} from {} history objects.",
            human_bytes(object_gc.reclaimed_bytes),
            object_gc.deleted_objects
        ));
    }
    if quota.over_quota() {
        notice_parts.push(format!(
            "History object store is over quota: {} used / {} target.",
            human_bytes(quota.stored_bytes),
            human_bytes(quota.max_total_blob_bytes)
        ));
    }

    Ok(HistoryMaintenanceSummary {
        object_store_bytes: quota.stored_bytes,
        object_store_files: quota.object_count,
        auto_checkpoint_count,
        deleted_auto_checkpoints: checkpoint_ids_to_delete.len(),
        deleted_object_files: object_gc.deleted_objects,
        reclaimed_bytes: object_gc.reclaimed_bytes,
        over_quota: quota.over_quota(),
        notice: (!notice_parts.is_empty()).then(|| notice_parts.join(" ")),
    })
}

fn checkpoints_to_prune(
    checkpoints: &[HistoryCheckpointInfo],
) -> Result<Vec<HistoryCheckpointInfo>> {
    let grace_cutoff = Utc::now() - chrono::Duration::hours(AUTO_CHECKPOINT_RETENTION_HOURS);
    let mut auto_checkpoints = checkpoints
        .iter()
        .filter(|checkpoint| is_auto_turn_checkpoint_reason(&checkpoint.reason))
        .cloned()
        .collect::<Vec<_>>();
    auto_checkpoints.sort_by(|left, right| {
        checkpoint_timestamp(&right.created_at)
            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
            .cmp(&checkpoint_timestamp(&left.created_at).unwrap_or(DateTime::<Utc>::UNIX_EPOCH))
    });

    let latest_by_branch = auto_checkpoints.iter().fold(
        HashMap::<BranchId, String>::new(),
        |mut latest, checkpoint| {
            latest
                .entry(checkpoint.branch_id)
                .or_insert_with(|| checkpoint.id.to_string());
            latest
        },
    );

    Ok(auto_checkpoints
        .into_iter()
        .enumerate()
        .filter_map(|(index, checkpoint)| {
            let checkpoint_time =
                checkpoint_timestamp(&checkpoint.created_at).unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
            let is_branch_latest = latest_by_branch
                .get(&checkpoint.branch_id)
                .map(|id| id == &checkpoint.id.to_string())
                .unwrap_or(false);
            if !is_branch_latest
                && index >= AUTO_CHECKPOINT_LIMIT_PER_SESSION
                && checkpoint_time < grace_cutoff
            {
                Some(checkpoint)
            } else {
                None
            }
        })
        .collect())
}

fn checkpoint_timestamp(raw: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(raw)?.with_timezone(&Utc))
}

pub(crate) fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit_index = 0usize;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}
