//! Adapters bridging chatbot-worker domain types to chatbot-ui traits.
//!
//! This module is ONLY for implementing chatbot_ui traits on domain types.
//! No chatbot-ui code should live here — only the impl blocks.

use std::sync::Arc;
use chrono::{DateTime, Utc};

use chatbot_lib::data::{ChatMessage, ChatSession, SessionInfo};
use chatbot_lib::model::ModelProvider;
use chatbot_lib::summary::JsonSummary;

// ============================================================================
// SessionListItem for SessionInfo
// ============================================================================

impl chatbot_ui::traits::SessionListItem for SessionInfo {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn message_count(&self) -> usize {
        self.message_count
    }
    fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    fn hidden(&self) -> bool {
        self.hidden
    }
}

// ============================================================================
// MessageView for ChatMessage
// ============================================================================

impl chatbot_ui::traits::MessageView for ChatMessage {
    fn content(&self) -> &str {
        &self.content
    }
    fn is_user(&self) -> bool {
        self.is_user
    }
    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
    fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }
}

// ============================================================================
// SessionView for ChatSession
// ============================================================================

/// Wrapper to convert a ChatSession into a SessionView trait object.
pub struct SessionViewWrapper {
    pub session: ChatSession,
}

impl From<ChatSession> for SessionViewWrapper {
    fn from(session: ChatSession) -> Self {
        Self { session }
    }
}

impl chatbot_ui::traits::SessionView for SessionViewWrapper {
    fn id(&self) -> &str {
        &self.session.id
    }
    fn name(&self) -> &str {
        &self.session.name
    }
    fn messages(&self) -> Vec<Arc<dyn chatbot_ui::traits::MessageView>> {
        self.session
            .messages
            .iter()
            .map(|m| Arc::new(m.clone()) as Arc<dyn chatbot_ui::traits::MessageView>)
            .collect()
    }
    fn system_prompt(&self) -> Option<&str> {
        self.session.system_prompt.as_deref()
    }
    fn json_summary(&self) -> Option<Arc<dyn chatbot_ui::traits::JsonSummaryView>> {
        self.session
            .json_summary
            .as_ref()
            .map(|s| Arc::new(s.clone()) as Arc<dyn chatbot_ui::traits::JsonSummaryView>)
    }
}

// ============================================================================
// JsonSummaryView for JsonSummary
// ============================================================================

impl chatbot_ui::traits::JsonSummaryView for JsonSummary {
    fn turn_count(&self) -> usize {
        self.response_count as usize
    }
    fn total_tokens(&self) -> u64 {
        self.total_input_tokens.saturating_add(self.total_output_tokens)
    }
    fn estimated_cost(&self) -> Option<f64> {
        if self.total_cost_usd > 0.0 {
            Some(self.total_cost_usd)
        } else {
            None
        }
    }
    fn model(&self) -> &str {
        self.model.as_deref().unwrap_or("unknown")
    }
    fn to_full_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

// ============================================================================
// ProviderInfo for ModelProvider
// ============================================================================

impl chatbot_ui::traits::ProviderInfo for ModelProvider {
    fn display_name(&self) -> &str {
        self.display_name()
    }
    fn provider_key(&self) -> &str {
        self.as_str()
    }
}

// ============================================================================
// ProcessingState for AppWorkerState
// ============================================================================

/// Adapter that exposes AppWorkerState processing status as ProcessingState.
pub struct AppProcessingState<'a> {
    pub worker: &'a app::state::AppWorkerState,
}

impl<'a> chatbot_ui::traits::ProcessingState for AppProcessingState<'a> {
    fn is_processing(&self) -> bool {
        self.worker.any_processing()
    }

    fn is_local_processing(&self) -> bool {
        !self.worker.local_processing_chat_ids.is_empty()
    }
}

// ============================================================================
// WorkspaceList for AppState
// ============================================================================

/// Adapter that exposes workspace selection as WorkspaceList.
/// Used as Arc<dyn WorkspaceList> so requires interior mutability.
pub struct AppWorkspaceList {
    /// Returns (workspaces, selected)
    pub get_workspaces: Box<dyn Fn() -> (Vec<String>, Option<String>) + Send + Sync>,
    /// Select a workspace dir
    pub select_workspace: Box<dyn Fn(String) + Send>,
}

impl AppWorkspaceList {
    pub fn new(
        get_workspaces: impl Fn() -> (Vec<String>, Option<String>) + Send + Sync + 'static,
        select_workspace: impl Fn(String) + Send + 'static,
    ) -> Self {
        Self {
            get_workspaces: Box::new(get_workspaces),
            select_workspace: Box::new(select_workspace),
        }
    }
}

impl chatbot_ui::traits::WorkspaceList for AppWorkspaceList {
    fn workspaces(&self) -> Vec<String> {
        (self.get_workspaces)().0
    }

    fn selected(&self) -> Option<&str> {
        (self.get_workspaces)().1.as_deref()
    }

    fn select(&mut self, dir: &str) {
        (self.select_workspace)(dir.to_string())
    }
}

// ============================================================================
// ModelCatalog for OmniForge model selection
// ============================================================================

/// Adapter that exposes OmniForge model catalog for model selection.
/// Requires &mut self for select_model since it enqueues an AppUiAction.
pub struct AppModelCatalog {
    /// Returns list of (model_id, model_label) tuples
    pub available_models: Box<dyn Fn() -> Vec<(&'static str, &'static str)> + Send + Sync>,
    /// Returns the currently selected model_id
    pub selected_model_id: Box<dyn Fn() -> String + Send + Sync>,
    /// Select a model (enqueues UiAction)
    pub select_model: Box<dyn Fn(String) + Send>,
}

impl AppModelCatalog {
    pub fn new(
        available_models: impl Fn() -> Vec<(&'static str, &'static str)> + Send + Sync + 'static,
        selected_model_id: impl Fn() -> String + Send + Sync + 'static,
        select_model: impl Fn(String) + Send + 'static,
    ) -> Self {
        Self {
            available_models: Box::new(available_models),
            selected_model_id: Box::new(selected_model_id),
            select_model: Box::new(select_model),
        }
    }
}

impl chatbot_ui::traits::ModelCatalog for AppModelCatalog {
    fn available_models(&self) -> Vec<(&'static str, &'static str)> {
        (self.available_models)()
    }

    fn selected_model_id(&self) -> &str {
        (self.selected_model_id)().as_str()
    }

    fn select_model(&mut self, model_id: &str) {
        (self.select_model)(model_id.to_string())
    }
}