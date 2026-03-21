use std::collections::HashMap;
use std::path::Path;
use ai_history_core::BranchId;
use ai_history_store::{CheckpointInfo as HistoryCheckpointInfo, HistoryStore as SqliteStore};
use anyhow::Result;
use chrono::{DateTime, Utc};
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
