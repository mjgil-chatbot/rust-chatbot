//! Adapters bridging chatbot-worker domain types to chatbot-ui traits.

pub mod chatbot_ui_actions;
pub mod chatbot_ui_adapters;

#[cfg(test)]
mod adapter_tests;

pub use chatbot_ui_actions::AppStateUiActions;
pub use chatbot_ui_adapters::{
    AppModelCatalog,
    AppProcessingState,
    AppWorkspaceList,
    SessionViewWrapper,
    // Re-export for convenience
    chatbot_lib::data::{ChatMessage, ChatSession, SessionInfo},
    chatbot_lib::model::ModelProvider,
    chatbot_lib::summary::JsonSummary,
};