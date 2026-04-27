use anyhow::{bail, Context, Result};
use rust_debug::tracing::warn;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;
use crate::data::{ChatSession, SessionMetadata};
use crate::model::ModelProvider;
use crate::{stream, worker};

#[derive(Clone, Debug)]
pub struct SessionMcpService {
    stores: Vec<SessionStore>,
}

#[derive(Clone, Debug)]
pub struct SessionStore {
    provider: ModelProvider,
    data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectSessionResponse {
    pub input_session_id: String,
    pub matched_by: String,
    pub provider: String,
    pub provider_display_name: String,
    pub session_id: String,
    pub data_dir: String,
    pub session_path: String,
    pub hidden: bool,
    pub message_count: usize,
    pub user_message_count: usize,
    pub assistant_message_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_spool: Option<InspectWorkerSpool>,
    pub session: ChatSession,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectWorkerSpool {
    pub worker_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_error: Option<String>,
    pub event_count: usize,
    pub truncated_tool_payload_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub truncated_tool_payloads: Vec<TruncatedToolPayload>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TruncatedToolPayload {
    pub event_type: String,
    pub tool: String,
    pub original_bytes: u64,
    pub preview: String,
}

#[derive(Clone, Debug)]
struct SessionMatch {
    provider: ModelProvider,
    data_dir: PathBuf,
    session_id: String,
    hidden: bool,
    matched_by: MatchKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchKind {
    Exact,
    Prefix,
}

impl MatchKind {
    fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Prefix => "prefix",
        }
    }
}

impl SessionMcpService {
    pub fn from_default_data_dirs() -> Result<Self> {
        let home = dirs::home_dir().context("Could not find home directory")?;
        Ok(Self::new(
            ModelProvider::SUPPORTED
                .into_iter()
                .map(|provider| {
                    SessionStore::new(
                        provider,
                        config::resolve_data_dir_under_home(provider, &home),
                    )
                })
                .collect(),
        ))
    }

    pub fn new(stores: Vec<SessionStore>) -> Self {
        Self { stores }
    }

    pub fn inspect_session(&self, session_id: &str) -> Result<InspectSessionResponse> {
        let query = normalize_session_id_query(session_id)?;
        let matches = self.resolve_matches(&query)?;

        let matched = match matches.as_slice() {
            [] => bail!(
                "No rust-chatbot session found matching `{}`",
                session_id.trim()
            ),
            [matched] => matched.clone(),
            many => bail!(format_ambiguous_matches(session_id.trim(), many)),
        };

        let session_path = session_file_path(&matched.data_dir, &matched.session_id);
        let raw = fs::read_to_string(&session_path)
            .with_context(|| format!("failed to read session {}", session_path.display()))?;
        let session = serde_json::from_str::<ChatSession>(&raw)
            .with_context(|| format!("failed to parse session {}", session_path.display()))?;
        let user_message_count = session
            .messages
            .iter()
            .filter(|message| message.is_user)
            .count();
        let message_count = session.messages.len();
        let worker_spool = inspect_worker_spool(&matched.session_id)?;

        Ok(InspectSessionResponse {
            input_session_id: session_id.trim().to_string(),
            matched_by: matched.matched_by.label().to_string(),
            provider: matched.provider.as_str().to_string(),
            provider_display_name: matched.provider.display_name().to_string(),
            session_id: matched.session_id,
            data_dir: matched.data_dir.display().to_string(),
            session_path: session_path.display().to_string(),
            hidden: matched.hidden,
            message_count,
            user_message_count,
            assistant_message_count: message_count.saturating_sub(user_message_count),
            worker_spool,
            session,
        })
    }

    fn resolve_matches(&self, query: &str) -> Result<Vec<SessionMatch>> {
        let exact_matches = self
            .stores
            .iter()
            .map(|store| store.find_exact(query))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if !exact_matches.is_empty() {
            return Ok(exact_matches);
        }

        let mut prefix_matches = Vec::new();
        for store in &self.stores {
            prefix_matches.extend(store.find_prefix_matches(query)?);
        }
        prefix_matches.sort_by(|left, right| {
            left.provider
                .as_str()
                .cmp(right.provider.as_str())
                .then_with(|| left.session_id.cmp(&right.session_id))
        });
        Ok(prefix_matches)
    }
}

impl SessionStore {
    pub fn new(provider: ModelProvider, data_dir: PathBuf) -> Self {
        Self { provider, data_dir }
    }

    fn find_exact(&self, query: &str) -> Result<Option<SessionMatch>> {
        let exact_path = session_file_path(&self.data_dir, query);
        if !exact_path.is_file() {
            return Ok(None);
        }

        let hidden = self
            .metadata_sessions()?
            .into_iter()
            .find(|(session_id, _)| normalize_id(session_id) == query)
            .map(|(_, info)| info.hidden)
            .unwrap_or(false);

        Ok(Some(SessionMatch {
            provider: self.provider,
            data_dir: self.data_dir.clone(),
            session_id: query.to_string(),
            hidden,
            matched_by: MatchKind::Exact,
        }))
    }

    fn find_prefix_matches(&self, query: &str) -> Result<Vec<SessionMatch>> {
        let sessions_dir = self.data_dir.join("sessions");
        let metadata = self.metadata_sessions()?;
        let entries = match fs::read_dir(&sessions_dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to read {}", sessions_dir.display()));
            }
        };

        let mut matches = Vec::new();
        let mut errors = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                        continue;
                    }
                    let Some(file_stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                        continue;
                    };
                    if !normalize_id(file_stem).starts_with(query) {
                        continue;
                    }
                    let hidden = metadata
                        .get(file_stem)
                        .map(|info| info.hidden)
                        .unwrap_or(false);
                    matches.push(SessionMatch {
                        provider: self.provider,
                        data_dir: self.data_dir.clone(),
                        session_id: file_stem.to_string(),
                        hidden,
                        matched_by: MatchKind::Prefix,
                    });
                }
                Err(err) => {
                    errors.push(err);
                }
            }
        }

        // Log any IO errors that occurred during directory iteration
        for err in errors {
            warn!("failed to read session directory entry: {}", err);
        }

        Ok(matches)
    }

    fn metadata_sessions(
        &self,
    ) -> Result<std::collections::HashMap<String, crate::data::SessionInfo>> {
        let metadata_path = self.data_dir.join("sessions.json");
        if !metadata_path.is_file() {
            return Ok(std::collections::HashMap::new());
        }

        let raw = fs::read_to_string(&metadata_path)
            .with_context(|| format!("failed to read metadata {}", metadata_path.display()))?;
        let metadata = serde_json::from_str::<SessionMetadata>(&raw)
            .with_context(|| format!("failed to parse metadata {}", metadata_path.display()))?;
        Ok(metadata.sessions)
    }
}

fn session_file_path(data_dir: &Path, session_id: &str) -> PathBuf {
    data_dir.join("sessions").join(format!("{session_id}.json"))
}

fn normalize_session_id_query(session_id: &str) -> Result<String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        bail!("session_id cannot be empty");
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        bail!("session_id must use only ASCII letters, digits, and '-'");
    }
    Ok(normalize_id(trimmed))
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn format_ambiguous_matches(input: &str, matches: &[SessionMatch]) -> String {
    let mut lines = vec![format!(
        "Session id `{}` is ambiguous; matched {} sessions:",
        input,
        matches.len()
    )];
    for matched in matches {
        lines.push(format!(
            "- {} {} ({})",
            matched.provider.display_name(),
            matched.session_id,
            matched.matched_by.label()
        ));
    }
    lines.join("\n")
}

fn inspect_worker_spool(session_id: &str) -> Result<Option<InspectWorkerSpool>> {
    let worker_dir = worker::worker_dir(session_id);
    if !worker_dir.exists() {
        return Ok(None);
    }

    let state = worker::read_job_state(session_id)?.map(|job| job.state.name().to_string());
    let terminal_error = read_terminal_error(session_id)?;
    let events = stream::read_events(&worker::events_path(session_id));
    let truncated_tool_payloads = collect_truncated_tool_payloads(&events);

    Ok(Some(InspectWorkerSpool {
        worker_dir: worker_dir.display().to_string(),
        state,
        terminal_error,
        event_count: events.len(),
        truncated_tool_payload_count: truncated_tool_payloads.len(),
        truncated_tool_payloads,
    }))
}

fn read_terminal_error(session_id: &str) -> Result<Option<String>> {
    let result_path = worker::result_path(session_id);
    let raw = match fs::read_to_string(&result_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", result_path.display()));
        }
    };
    let result = serde_json::from_str::<worker::WorkerResult>(&raw)
        .with_context(|| format!("failed to parse {}", result_path.display()))?;
    Ok(result.error)
}

fn collect_truncated_tool_payloads(events: &[stream::StreamEvent]) -> Vec<TruncatedToolPayload> {
    let mut payloads = Vec::new();
    for event in events {
        match event {
            stream::StreamEvent::ToolStart { tool, input, .. } => {
                if let Some(payload) = truncated_payload("tool_start", tool, input) {
                    payloads.push(payload);
                }
            }
            stream::StreamEvent::ToolEnd { tool, output, .. } => {
                if let Some(payload) = truncated_payload("tool_end", tool, output) {
                    payloads.push(payload);
                }
            }
            _ => {}
        }
    }
    payloads
}

fn truncated_payload(event_type: &str, tool: &str, value: &Value) -> Option<TruncatedToolPayload> {
    let object = value.as_object()?;
    if object.get("truncated").and_then(Value::as_bool) != Some(true) {
        return None;
    }

    Some(TruncatedToolPayload {
        event_type: event_type.to_string(),
        tool: tool.to_string(),
        original_bytes: object.get("original_bytes").and_then(Value::as_u64)?,
        preview: object.get("preview").and_then(Value::as_str)?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{InspectSessionResponse, SessionMcpService, SessionStore};
    use crate::data::{ChatMessage, ChatSession, SessionInfo, SessionMetadata};
    use crate::model::ModelProvider;
    use crate::stream::StreamEvent;
    use crate::worker::{self, WorkerJobSnapshot, WorkerResult, WorkerState};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rust-chatbot-session-mcp-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ))
    }

    fn write_store(
        root: &Path,
        provider: ModelProvider,
        session_id: &str,
        hidden: bool,
    ) -> PathBuf {
        let data_dir = root.join(provider.as_str());
        fs::create_dir_all(data_dir.join("sessions")).unwrap();
        let now = Utc::now();
        let session = ChatSession {
            id: session_id.to_string(),
            name: format!("{} session", provider.display_name()),
            system_prompt: Some("system".to_string()),
            json_summary: None,
            claude_session_id: None,
            minimax_session_id: None,
            qwen_session_id: None,
            codex_session_id: None,
            nvidia_session_id: None,
            gemini_session_id: None,
            openrouter_session_id: None,
            omniforge_session_id: None,
            omniforge_model_id: None,
            kimi_session_id: None,
            opencode_session_id: None,
            hermes_session_id: None,
            forge_session_id: None,
            claw_session_id: None,
            launched_from: Some("/tmp/workspace".to_string()),
            runsc_dirs: Vec::new(),
            runsc_image: None,
            created_at: now,
            updated_at: now,
            messages: vec![
                ChatMessage {
                    content: "hello".to_string(),
                    is_user: true,
                    timestamp: now,
                    command: Some("codex hello".to_string()),
                    retrieved_memory: None,
                },
                ChatMessage {
                    content: "world".to_string(),
                    is_user: false,
                    timestamp: now,
                    command: None,
                    retrieved_memory: None,
                },
            ],
        };
        fs::write(
            data_dir.join("sessions").join(format!("{session_id}.json")),
            serde_json::to_string_pretty(&session).unwrap(),
        )
        .unwrap();

        let mut sessions = HashMap::new();
        sessions.insert(
            session_id.to_string(),
            SessionInfo {
                id: session_id.to_string(),
                name: session.name.clone(),
                created_at: now,
                updated_at: now,
                message_count: 1,
                hidden,
            },
        );
        fs::write(
            data_dir.join("sessions.json"),
            serde_json::to_string_pretty(&SessionMetadata { sessions }).unwrap(),
        )
        .unwrap();

        data_dir
    }

    fn inspect(service: &SessionMcpService, session_id: &str) -> InspectSessionResponse {
        service.inspect_session(session_id).unwrap()
    }

    fn restore_env_var(key: &str, previous: Option<OsString>) {
        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn inspects_session_by_exact_id() {
        let root = temp_dir("exact");
        let data_dir = write_store(
            &root,
            ModelProvider::Codex,
            "e829f2f6-92ab-4d5a-923c-d48fd9fbe00a",
            false,
        );
        let service =
            SessionMcpService::new(vec![SessionStore::new(ModelProvider::Codex, data_dir)]);

        let response = inspect(&service, "e829f2f6-92ab-4d5a-923c-d48fd9fbe00a");

        assert_eq!(response.matched_by, "exact");
        assert_eq!(response.provider, "codex");
        assert_eq!(
            response.session_id,
            "e829f2f6-92ab-4d5a-923c-d48fd9fbe00a".to_string()
        );
        assert_eq!(response.message_count, 2);
        assert_eq!(response.user_message_count, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inspects_session_by_unique_prefix() {
        let root = temp_dir("prefix");
        let data_dir = write_store(
            &root,
            ModelProvider::Claude,
            "e829f2f6-92ab-4d5a-923c-d48fd9fbe00a",
            true,
        );
        let service =
            SessionMcpService::new(vec![SessionStore::new(ModelProvider::Claude, data_dir)]);

        let response = inspect(&service, "e829f2f6");

        assert_eq!(response.matched_by, "prefix");
        assert_eq!(response.provider, "claude");
        assert!(response.hidden);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_ambiguous_prefixes() {
        let root = temp_dir("ambiguous");
        let claude_dir = write_store(
            &root,
            ModelProvider::Claude,
            "e829f2f6-92ab-4d5a-923c-d48fd9fbe00a",
            false,
        );
        let codex_dir = write_store(
            &root,
            ModelProvider::Codex,
            "e829f2f6-1111-2222-3333-444444444444",
            false,
        );
        let service = SessionMcpService::new(vec![
            SessionStore::new(ModelProvider::Claude, claude_dir),
            SessionStore::new(ModelProvider::Codex, codex_dir),
        ]);

        let error = service.inspect_session("e829f2f6").unwrap_err().to_string();

        assert!(error.contains("ambiguous"));
        assert!(error.contains("Claude"));
        assert!(error.contains("Codex"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_session_ids_with_path_characters() {
        let root = temp_dir("invalid-id");
        let data_dir = write_store(
            &root,
            ModelProvider::Claude,
            "e829f2f6-92ab-4d5a-923c-d48fd9fbe00a",
            false,
        );
        let service =
            SessionMcpService::new(vec![SessionStore::new(ModelProvider::Claude, data_dir)]);

        let error = service
            .inspect_session("../etc/passwd")
            .unwrap_err()
            .to_string();

        assert!(error.contains("session_id must use only ASCII letters, digits, and '-'"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inspect_session_surfaces_worker_spool_truncation_metadata() {
        let _lock = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let root = temp_dir("worker-spool");
        let home = temp_dir("worker-spool-home");
        let previous_home = std::env::var_os("HOME");
        let previous_config = std::env::var_os("RUST_CHATBOT_CONFIG_PATH");
        std::env::set_var("HOME", &home);
        std::env::remove_var("RUST_CHATBOT_CONFIG_PATH");

        let session_id = "e829f2f6-92ab-4d5a-923c-d48fd9fbe00a";
        let data_dir = write_store(&root, ModelProvider::Codex, session_id, false);
        fs::create_dir_all(worker::worker_dir(session_id)).unwrap();
        crate::stream::append_event(
            &worker::events_path(session_id),
            &StreamEvent::tool_end(
                "ListDir",
                serde_json::json!({ "output": "x".repeat(200000) }),
            ),
        )
        .unwrap();

        let mut job = WorkerJobSnapshot::new(session_id, "codex", 111, 222);
        job.state = WorkerState::Error;
        worker::write_json_atomic(&worker::state_path(session_id), &job).unwrap();
        worker::write_json_atomic(
            &worker::result_path(session_id),
            &WorkerResult {
                session_id: None,
                summary: None,
                error: Some("exit_code:7: tool replay overflow".to_string()),
            },
        )
        .unwrap();

        let service =
            SessionMcpService::new(vec![SessionStore::new(ModelProvider::Codex, data_dir)]);
        let response = inspect(&service, session_id);
        let spool = response
            .worker_spool
            .expect("worker spool metadata should be present");

        restore_env_var("RUST_CHATBOT_CONFIG_PATH", previous_config);
        restore_env_var("HOME", previous_home);

        assert_eq!(spool.state.as_deref(), Some("ERROR"));
        assert_eq!(
            spool.terminal_error.as_deref(),
            Some("exit_code:7: tool replay overflow")
        );
        assert_eq!(spool.event_count, 1);
        assert_eq!(spool.truncated_tool_payload_count, 1);
        assert_eq!(spool.truncated_tool_payloads[0].event_type, "tool_end");
        assert_eq!(spool.truncated_tool_payloads[0].tool, "ListDir");
        assert!(spool.truncated_tool_payloads[0].original_bytes > 32 * 1024);
        assert!(spool.truncated_tool_payloads[0]
            .preview
            .contains("\"output\""));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(home).unwrap();
    }
}
