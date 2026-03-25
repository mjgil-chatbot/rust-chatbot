use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::memory::{self, MemoryContext, MemoryHit};
pub const RETRIEVAL_AUDIT_FILE_NAME: &str = "retrieval-audit.json";
const RETRIEVAL_AUDIT_VERSION: u32 = 1;
const MAX_RETRIEVAL_AUDIT_ENTRIES: usize = 200;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRetrievalAuditEntry {
    pub provider: String,
    pub provider_display_name: String,
    pub session_id: String,
    pub session_name: String,
    pub requested_at: DateTime<Utc>,
    pub query: String,
    pub user_message: String,
    pub candidate_hit_count: usize,
    #[serde(default)]
    pub candidate_hits: Vec<MemoryHit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub injected_preview: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
struct MemoryRetrievalAuditLog {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<MemoryRetrievalAuditEntry>,
}
#[derive(Debug, Clone)]
pub struct AppendMemoryRetrievalAudit<'a> {
    pub provider: &'a str,
    pub provider_display_name: &'a str,
    pub session_id: &'a str,
    pub session_name: &'a str,
    pub requested_at_ms: i64,
    pub query: &'a str,
    pub user_message: &'a str,
    pub memory_context: Option<&'a MemoryContext>,
    pub injected_preview: Option<&'a str>,
}
pub fn append_memory_retrieval_audit(
    workspace_path: impl AsRef<Path>,
    append: AppendMemoryRetrievalAudit<'_>,
) -> Result<()> {
    let paths = memory::ensure_workspace_layout(workspace_path)?;
    let path = retrieval_audit_path_for_paths(&paths.root_dir);
    let mut audit = load_memory_retrieval_audit(&path)?;
    audit.version = RETRIEVAL_AUDIT_VERSION;
    audit.entries.push(build_entry(append));
    if audit.entries.len() > MAX_RETRIEVAL_AUDIT_ENTRIES {
        let overflow = audit.entries.len() - MAX_RETRIEVAL_AUDIT_ENTRIES;
        audit.entries.drain(0..overflow);
    }
    save_memory_retrieval_audit(&path, &audit)?;
    Ok(())
}
fn retrieval_audit_path_for_paths(root_dir: &Path) -> PathBuf {
    root_dir.join(RETRIEVAL_AUDIT_FILE_NAME)
}
fn load_memory_retrieval_audit(path: &Path) -> Result<MemoryRetrievalAuditLog> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(MemoryRetrievalAuditLog {
                version: RETRIEVAL_AUDIT_VERSION,
                ..MemoryRetrievalAuditLog::default()
            });
        }
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", path.display())),
    };

    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}
fn save_memory_retrieval_audit(path: &Path, audit: &MemoryRetrievalAuditLog) -> Result<()> {
    let json = serde_json::to_vec_pretty(audit).context("serialize memory retrieval audit")?;
    crate::storage::atomic::atomic_write(path, &json)
        .map(|_| ())
        .with_context(|| format!("failed to write {}", path.display()))
}
fn build_entry(append: AppendMemoryRetrievalAudit<'_>) -> MemoryRetrievalAuditEntry {
    let requested_at = Utc
        .timestamp_millis_opt(append.requested_at_ms)
        .single()
        .unwrap_or_else(Utc::now);
    let candidate_hits = append
        .memory_context
        .map(|context| context.hits.clone())
        .unwrap_or_default();
    let candidate_preview = append
        .memory_context
        .and_then(|context| memory::format_retrieved_memory_preview(Some(context)));
    let injected_preview = append
        .injected_preview
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    MemoryRetrievalAuditEntry {
        provider: append.provider.to_string(),
        provider_display_name: append.provider_display_name.to_string(),
        session_id: append.session_id.to_string(),
        session_name: append.session_name.to_string(),
        requested_at,
        query: append.query.to_string(),
        user_message: append.user_message.to_string(),
        candidate_hit_count: candidate_hits.len(),
        candidate_hits,
        candidate_preview,
        injected_preview,
    }
}
