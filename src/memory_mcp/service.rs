use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use rust_debug::tracing::warn;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::data::DataManager;
use crate::memory::{self, ManualMemoryFact, ManualMemoryPriority, MemoryInvalidation};
use crate::memory_audit;
use crate::model::ModelProvider;

#[derive(Clone, Debug)]
pub struct MemoryMcpService {
    allowed_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceMemoryStatus {
    pub workspace_root: String,
    pub workspace_id: String,
    pub memmachine_base_url: String,
    pub memmachine_org_id: String,
    pub memmachine_project_id: String,
    pub ingest_state_path: String,
    pub memmachine_project_exists: bool,
    pub ingest_state_exists: bool,
    pub indexed_sessions: usize,
    pub invalidation_count: usize,
    pub manual_memory_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchMemoryResponse {
    pub workspace_root: String,
    pub workspace_id: String,
    pub query: String,
    pub hit_count: usize,
    pub hits: Vec<memory::MemoryHit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryInjectionEntry {
    pub provider: String,
    pub provider_display_name: String,
    pub session_id: String,
    pub session_name: String,
    pub message_index: usize,
    pub injected_at: DateTime<Utc>,
    pub user_message: String,
    pub retrieved_memory: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryInjectionsResponse {
    pub workspace_root: String,
    pub workspace_id: String,
    pub injection_count: usize,
    pub injections: Vec<MemoryInjectionEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryRetrievalsResponse {
    pub workspace_root: String,
    pub workspace_id: String,
    pub retrieval_count: usize,
    pub retrieval_audit_path: String,
    pub retrievals: Vec<memory_audit::MemoryRetrievalAuditEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvalidatedDocKeyEntry {
    pub doc_key: String,
    pub invalidated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvalidationsResponse {
    pub workspace_root: String,
    pub workspace_id: String,
    pub invalidation_count: usize,
    pub invalidations: Vec<InvalidatedDocKeyEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocKeyMutationResponse {
    pub workspace_root: String,
    pub workspace_id: String,
    pub doc_key: String,
    pub action: String,
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RememberFactResponse {
    pub workspace_root: String,
    pub workspace_id: String,
    pub memory_id: String,
    pub doc_key: String,
    pub text: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgetFactResponse {
    pub workspace_root: String,
    pub workspace_id: String,
    pub memory_id: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManualMemoriesResponse {
    pub workspace_root: String,
    pub workspace_id: String,
    pub memory_count: usize,
    pub memories: Vec<ManualMemoryFact>,
}

impl MemoryMcpService {
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        Self { allowed_roots }
    }

    pub fn workspace_memory_status(&self, workspace_path: &str) -> Result<WorkspaceMemoryStatus> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let state = memory::load_ingest_state(&paths.ingest_state_path)?;
        let manual_memories = memory::load_manual_memories(&paths.manual_memories_path)?;
        let memmachine = memory::workspace_memmachine_status(&workspace_root)?;

        Ok(WorkspaceMemoryStatus {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            memmachine_base_url: memmachine.base_url,
            memmachine_org_id: memmachine.org_id,
            memmachine_project_id: memmachine.project_id,
            ingest_state_path: paths.ingest_state_path.display().to_string(),
            memmachine_project_exists: memmachine.project_exists,
            ingest_state_exists: paths.ingest_state_path.is_file(),
            indexed_sessions: state.sessions.len(),
            invalidation_count: state.invalidated_doc_keys.len(),
            manual_memory_count: manual_memories.memories.len(),
        })
    }

    pub fn search_memory(&self, workspace_path: &str, query: &str) -> Result<SearchMemoryResponse> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let context = memory::retrieve_memory_context(&workspace_root, query)?;
        let hits = context
            .as_ref()
            .map(|context| context.hits.clone())
            .unwrap_or_default();

        Ok(SearchMemoryResponse {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            query: query.to_string(),
            hit_count: hits.len(),
            hits,
        })
    }

    pub fn list_memory_injections(
        &self,
        workspace_path: &str,
        limit: usize,
    ) -> Result<MemoryInjectionsResponse> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let mut injections = self.collect_workspace_memory_injections(&workspace_root);
        injections.sort_by(|left, right| {
            right
                .injected_at
                .cmp(&left.injected_at)
                .then_with(|| left.session_id.cmp(&right.session_id))
                .then_with(|| left.message_index.cmp(&right.message_index))
        });

        let limit = limit.max(1);
        injections.truncate(limit);

        Ok(MemoryInjectionsResponse {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            injection_count: injections.len(),
            injections,
        })
    }

    pub fn list_memory_retrievals(
        &self,
        workspace_path: &str,
        limit: usize,
    ) -> Result<MemoryRetrievalsResponse> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let retrieval_audit_path = memory_audit::retrieval_audit_path(&workspace_root)?;
        let retrievals = memory_audit::list_memory_retrieval_audits(&workspace_root, limit)?;

        Ok(MemoryRetrievalsResponse {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            retrieval_count: retrievals.len(),
            retrieval_audit_path: retrieval_audit_path.display().to_string(),
            retrievals,
        })
    }

    pub fn list_invalidations(&self, workspace_path: &str) -> Result<InvalidationsResponse> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let state = memory::load_ingest_state(&paths.ingest_state_path)?;
        let invalidations = state
            .invalidated_doc_keys
            .into_iter()
            .map(|(doc_key, invalidation)| InvalidatedDocKeyEntry::from_pair(doc_key, invalidation))
            .collect::<Vec<_>>();

        Ok(InvalidationsResponse {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            invalidation_count: invalidations.len(),
            invalidations,
        })
    }

    pub fn invalidate_doc_key(
        &self,
        workspace_path: &str,
        doc_key: &str,
        reason: Option<String>,
    ) -> Result<DocKeyMutationResponse> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let changed = memory::invalidate_memory_doc_key(&workspace_root, doc_key, reason.clone())?;

        Ok(DocKeyMutationResponse {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            doc_key: doc_key.trim().to_string(),
            action: "invalidate".to_string(),
            changed,
            reason: normalize_reason(reason),
        })
    }

    pub fn remember_fact(
        &self,
        workspace_path: &str,
        fact: &str,
        priority: &str,
    ) -> Result<RememberFactResponse> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let priority = parse_priority(priority)?;
        let memory = memory::remember_manual_memory(&workspace_root, fact, priority)?;

        Ok(RememberFactResponse {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            memory_id: memory.id.clone(),
            doc_key: format!("manual:{}", memory.id),
            text: memory.text,
            priority: priority.label().to_string(),
        })
    }

    pub fn forget_fact(&self, workspace_path: &str, memory_id: &str) -> Result<ForgetFactResponse> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let changed = memory::forget_manual_memory(&workspace_root, memory_id)?;

        Ok(ForgetFactResponse {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            memory_id: memory_id.trim().to_string(),
            changed,
        })
    }

    pub fn list_manual_memories(&self, workspace_path: &str) -> Result<ManualMemoriesResponse> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let memories = memory::list_manual_memories(&workspace_root)?;

        Ok(ManualMemoriesResponse {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            memory_count: memories.len(),
            memories,
        })
    }

    pub fn restore_doc_key(
        &self,
        workspace_path: &str,
        doc_key: &str,
    ) -> Result<DocKeyMutationResponse> {
        let workspace_root = self.resolve_workspace_path(workspace_path)?;
        let paths = memory::workspace_memory_paths(&workspace_root)?;
        let changed = memory::restore_memory_doc_key(&workspace_root, doc_key)?;

        Ok(DocKeyMutationResponse {
            workspace_root: workspace_root.display().to_string(),
            workspace_id: paths.workspace_id,
            doc_key: doc_key.trim().to_string(),
            action: "restore".to_string(),
            changed,
            reason: None,
        })
    }

    fn resolve_workspace_path(&self, workspace_path: &str) -> Result<PathBuf> {
        let path = Path::new(workspace_path);
        if !path.is_absolute() {
            bail!("workspace_path must be absolute: {}", path.display());
        }

        let resolved = path
            .canonicalize()
            .with_context(|| format!("Cannot resolve workspace_path {}", path.display()))?;

        if self
            .allowed_roots
            .iter()
            .any(|root| resolved.starts_with(root))
        {
            return Ok(resolved);
        }

        bail!(
            "Workspace path {} is outside CHATBOT_MEMORY_MCP_ALLOWED_ROOTS",
            resolved.display()
        )
    }

    fn collect_workspace_memory_injections(
        &self,
        workspace_root: &Path,
    ) -> Vec<MemoryInjectionEntry> {
        let mut injections = Vec::new();

        for provider in ModelProvider::SUPPORTED {
            let data_manager = match DataManager::new(provider) {
                Ok(data_manager) => data_manager,
                Err(err) => {
                    warn!(
                        error=%err,
                        provider=%provider.as_str(),
                        "failed to initialize data manager while scanning memory injections"
                    );
                    continue;
                }
            };

            let sessions = match data_manager.list_sessions() {
                Ok(sessions) => sessions,
                Err(err) => {
                    warn!(
                        error=%err,
                        provider=%provider.as_str(),
                        "failed to list sessions while scanning memory injections"
                    );
                    continue;
                }
            };

            for session_info in sessions {
                let session = match data_manager.load_session(&session_info.id) {
                    Ok(session) => session,
                    Err(err) => {
                        warn!(
                            error=%err,
                            provider=%provider.as_str(),
                            session_id=%session_info.id,
                            "failed to load session while scanning memory injections"
                        );
                        continue;
                    }
                };

                let Some(launched_from) = session.launched_from.as_deref() else {
                    continue;
                };
                let Ok(paths) = memory::workspace_memory_paths(launched_from) else {
                    continue;
                };
                if paths.workspace_root != workspace_root {
                    continue;
                }

                injections.extend(
                    session
                        .messages
                        .iter()
                        .enumerate()
                        .filter(|(_, message)| message.is_user)
                        .filter_map(|(message_index, message)| {
                            let retrieved_memory =
                                message.retrieved_memory.as_deref()?.trim().to_string();
                            if retrieved_memory.is_empty() {
                                return None;
                            }

                            Some(MemoryInjectionEntry {
                                provider: provider.as_str().to_string(),
                                provider_display_name: provider.display_name().to_string(),
                                session_id: session.id.clone(),
                                session_name: session.name.clone(),
                                message_index,
                                injected_at: message.timestamp,
                                user_message: message.content.clone(),
                                retrieved_memory,
                            })
                        }),
                );
            }
        }

        injections
    }
}

impl InvalidatedDocKeyEntry {
    fn from_pair(doc_key: String, invalidation: MemoryInvalidation) -> Self {
        Self {
            doc_key,
            invalidated_at: invalidation.invalidated_at,
            reason: invalidation.reason,
        }
    }
}

fn normalize_reason(reason: Option<String>) -> Option<String> {
    reason.and_then(|reason| {
        let trimmed = reason.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_priority(priority: &str) -> Result<ManualMemoryPriority> {
    match priority.trim().to_ascii_lowercase().as_str() {
        "" | "high" => Ok(ManualMemoryPriority::High),
        "normal" => Ok(ManualMemoryPriority::Normal),
        other => bail!("unsupported priority '{}'; use 'high' or 'normal'", other),
    }
}
