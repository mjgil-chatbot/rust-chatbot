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

pub fn list_memory_retrieval_audits(
    workspace_path: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<MemoryRetrievalAuditEntry>> {
    let paths = memory::workspace_memory_paths(workspace_path)?;
    let path = retrieval_audit_path_for_paths(&paths.root_dir);
    let mut entries = load_memory_retrieval_audit(&path)?.entries;
    entries.sort_by(|left, right| {
        right
            .requested_at
            .cmp(&left.requested_at)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    entries.truncate(limit.max(1));
    Ok(entries)
}

pub fn retrieval_audit_path(workspace_path: impl AsRef<Path>) -> Result<PathBuf> {
    let paths = memory::workspace_memory_paths(workspace_path)?;
    Ok(retrieval_audit_path_for_paths(&paths.root_dir))
}

pub fn count_memory_retrieval_audits(workspace_path: impl AsRef<Path>) -> Result<usize> {
    Ok(list_memory_retrieval_audits(workspace_path, usize::MAX)?.len())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryHit;
    use chrono::TimeZone;
    use std::ffi::OsString;

    struct RetrievalAuditTestGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        _home_dir: tempfile::TempDir,
        previous_home: Option<OsString>,
    }

    impl RetrievalAuditTestGuard {
        fn new() -> Self {
            let lock = crate::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let home_dir = tempfile::TempDir::new().unwrap();
            let previous_home = std::env::var_os("HOME");
            std::env::set_var("HOME", home_dir.path());
            Self {
                _lock: lock,
                _home_dir: home_dir,
                previous_home,
            }
        }
    }

    impl Drop for RetrievalAuditTestGuard {
        fn drop(&mut self) {
            match self.previous_home.as_ref() {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    fn temp_workspace(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rust-chatbot-memory-audit-{label}-{nanos}"))
    }

    #[test]
    fn append_and_list_memory_retrieval_audits_round_trip() {
        let _guard = RetrievalAuditTestGuard::new();
        let workspace = temp_workspace("roundtrip");
        std::fs::create_dir_all(&workspace).unwrap();

        let context = MemoryContext {
            workspace_id: "ws".to_string(),
            query: "grep field".to_string(),
            hits: vec![MemoryHit {
                doc_key: "chat-1:1:assistant".to_string(),
                chat_id: "chat-1".to_string(),
                role: "assistant".to_string(),
                session_name: "Retry".to_string(),
                timestamp: Utc.with_ymd_and_hms(2026, 5, 21, 1, 1, 0).unwrap(),
                snippet: "Grep calls require the case_sensitive field.".to_string(),
                title: None,
                message_index: Some(1),
                turn_index: Some(0),
                score: Some(0.9),
            }],
        };

        append_memory_retrieval_audit(
            &workspace,
            AppendMemoryRetrievalAudit {
                provider: "codex",
                provider_display_name: "Codex",
                session_id: "chat-1",
                session_name: "Retry Session",
                requested_at_ms: 1_747_790_860_000,
                query: "Why does grep keep failing?",
                user_message: "Why does grep keep failing?",
                memory_context: Some(&context),
                injected_preview: Some(
                    "- [session=Retry role=assistant time=2026-05-21T01:01:00+00:00] Grep calls require the case_sensitive field.",
                ),
            },
        )
        .unwrap();

        let entries = list_memory_retrieval_audits(&workspace, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].candidate_hit_count, 1);
        assert!(entries[0]
            .candidate_preview
            .as_deref()
            .unwrap_or_default()
            .contains("case_sensitive field"));
        assert!(entries[0]
            .injected_preview
            .as_deref()
            .unwrap_or_default()
            .contains("case_sensitive field"));

        let _ = std::fs::remove_dir_all(workspace);
    }
}
