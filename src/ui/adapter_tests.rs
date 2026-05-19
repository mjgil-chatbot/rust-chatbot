//! Tests for chatbot-ui trait adapters.

use chrono::{DateTime, Utc};
use std::sync::Arc;

use chatbot_lib::data::{ChatMessage, ChatSession, SessionInfo};
use chatbot_lib::model::ModelProvider;
use chatbot_lib::summary::JsonSummary;

use crate::app::state::AppWorkerState;
use crate::ui::chatbot_ui_adapters::{
    AppModelCatalog, AppProcessingState, AppWorkspaceList, SessionViewWrapper,
};

/// Test SessionListItem adapter for SessionInfo
#[test]
fn session_list_item_adapter_returns_correct_id() {
    let info = SessionInfo {
        id: "test-session-123".to_string(),
        name: "Test Session".to_string(),
        message_count: 5,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        hidden: false,
    };

    assert_eq!(info.id(), "test-session-123");
    assert_eq!(info.name(), "Test Session");
    assert_eq!(info.message_count(), 5);
    assert!(!info.hidden());
}

/// Test MessageView adapter for ChatMessage
#[test]
fn message_view_adapter_returns_correct_content() {
    let msg = ChatMessage {
        content: "Hello world".to_string(),
        is_user: true,
        timestamp: Utc::now(),
        command: Some("/test".to_string()),
        retrieved_memory: None,
    };

    assert_eq!(msg.content(), "Hello world");
    assert!(msg.is_user());
    assert_eq!(msg.command(), Some("/test"));
}

/// Test MessageView adapter for non-user message
#[test]
fn message_view_adapter_non_user_message() {
    let msg = ChatMessage {
        content: "AI response".to_string(),
        is_user: false,
        timestamp: Utc::now(),
        command: None,
        retrieved_memory: None,
    };

    assert_eq!(msg.content(), "AI response");
    assert!(!msg.is_user());
    assert_eq!(msg.command(), None);
}

/// Test SessionViewWrapper adapter
#[test]
fn session_view_wrapper_returns_correct_session_id() {
    let session = ChatSession {
        id: "session-abc".to_string(),
        name: "My Session".to_string(),
        system_prompt: Some("You are helpful".to_string()),
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
        launched_from: None,
        runsc_dirs: Vec::new(),
        messages: vec![
            ChatMessage {
                content: "User message".to_string(),
                is_user: true,
                timestamp: Utc::now(),
                command: None,
                retrieved_memory: None,
            },
            ChatMessage {
                content: "AI response".to_string(),
                is_user: false,
                timestamp: Utc::now(),
                command: None,
                retrieved_memory: None,
            },
        ],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let wrapper = SessionViewWrapper { session };
    assert_eq!(wrapper.id(), "session-abc");
    assert_eq!(wrapper.name(), "My Session");
    assert_eq!(wrapper.messages().len(), 2);
    assert_eq!(wrapper.system_prompt(), Some("You are helpful"));
}

/// Test SessionViewWrapper with json_summary
#[test]
fn session_view_wrapper_with_json_summary() {
    let mut session = ChatSession {
        id: "session-xyz".to_string(),
        name: "Summary Test".to_string(),
        system_prompt: None,
        json_summary: Some(JsonSummary {
            model: Some("claude-3-5-sonnet".to_string()),
            response_count: 10,
            total_input_tokens: 1000,
            total_output_tokens: 2000,
            total_cost_usd: 0.05,
            ..JsonSummary::default()
        }),
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
        launched_from: None,
        runsc_dirs: Vec::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        messages: vec![],
    };

    let wrapper = SessionViewWrapper { session };
    let summary = wrapper.json_summary();
    assert!(summary.is_some());
    assert_eq!(summary.as_ref().unwrap().turn_count(), 10);
    assert_eq!(summary.as_ref().unwrap().total_tokens(), 3000);
    assert_eq!(summary.as_ref().unwrap().estimated_cost(), Some(0.05));
    assert_eq!(summary.as_ref().unwrap().model(), "claude-3-5-sonnet");
}

/// Test JsonSummaryView adapter
#[test]
fn json_summary_view_adapter() {
    let summary = JsonSummary {
        model: Some("claude-3-7-sonnet".to_string()),
        response_count: 5,
        total_input_tokens: 500,
        total_output_tokens: 1500,
        total_cost_usd: 0.03,
        ..JsonSummary::default()
    };

    assert_eq!(summary.turn_count(), 5);
    assert_eq!(summary.total_tokens(), 2000);
    assert_eq!(summary.estimated_cost(), Some(0.03));
    assert_eq!(summary.model(), "claude-3-7-sonnet");

    let json = summary.to_full_json();
    assert!(json.contains("claude-3-7-sonnet"));
    assert!(json.contains("2000"));
}

/// Test JsonSummaryView with zero cost returns None
#[test]
fn json_summary_view_zero_cost_returns_none() {
    let summary = JsonSummary {
        model: Some("test".to_string()),
        response_count: 1,
        total_input_tokens: 100,
        total_output_tokens: 100,
        total_cost_usd: 0.0,
        ..JsonSummary::default()
    };

    assert_eq!(summary.estimated_cost(), None);
}

/// Test ProviderInfo adapter for ModelProvider
#[test]
fn provider_info_adapter_claude() {
    let provider = ModelProvider::Claude;
    assert_eq!(provider.display_name(), "Claude");
    assert_eq!(provider.provider_key(), "claude");
}

#[test]
fn provider_info_adapter_minimax() {
    let provider = ModelProvider::MiniMax;
    assert_eq!(provider.display_name(), "MiniMax");
    assert_eq!(provider.provider_key(), "minimax");
}

#[test]
fn provider_info_adapter_omniforge() {
    let provider = ModelProvider::OmniForge;
    assert_eq!(provider.display_name(), "OmniForge");
    assert_eq!(provider.provider_key(), "omniforge");
}

/// Test ProcessingState adapter via AppWorkerState
#[test]
fn processing_state_empty_is_not_processing() {
    let worker = AppWorkerState::new();
    let state = AppProcessingState { worker: &worker };

    assert!(!state.is_processing());
    assert!(!state.is_local_processing());
}

#[test]
fn processing_state_with_local_processing() {
    let mut worker = AppWorkerState::new();
    worker.mark_processing("chat-1");

    let state = AppProcessingState { worker: &worker };
    assert!(state.is_processing());
    assert!(state.is_local_processing());
}

/// Test WorkspaceList adapter
#[test]
fn workspace_list_adapter_empty() {
    let ws = AppWorkspaceList::new(|| (vec![], None), |_| {});

    assert!(ws.workspaces().is_empty());
    assert_eq!(ws.selected(), None);
}

#[test]
fn workspace_list_adapter_with_recent_dirs() {
    let ws = AppWorkspaceList::new(
        || {
            (
                vec!["/home/user/code".to_string(), "/tmp".to_string()],
                Some("/home/user/code".to_string()),
            )
        },
        |_| {},
    );

    let workspaces = ws.workspaces();
    assert_eq!(workspaces.len(), 2);
    assert_eq!(workspaces[0], "/home/user/code");
    assert_eq!(ws.selected(), Some("/home/user/code"));
}

#[test]
fn workspace_list_adapter_select() {
    let mut selected = None;
    let ws = AppWorkspaceList::new(
        || (vec![], None),
        |dir| {
            selected = Some(dir);
        },
    );

    ws.select("/new/workspace");
    assert_eq!(selected, Some("/new/workspace".to_string()));
}

/// Test ModelCatalog adapter
#[test]
fn model_catalog_adapter_empty() {
    let catalog = AppModelCatalog::new(|| vec![], || "".to_string(), |_| {});

    assert!(catalog.available_models().is_empty());
    assert_eq!(catalog.selected_model_id(), "");
}

#[test]
fn model_catalog_adapter_with_models() {
    let catalog = AppModelCatalog::new(
        || {
            vec![
                ("claude-3-5-sonnet", "Claude 3.5 Sonnet"),
                ("claude-3-7-sonnet", "Claude 3.7 Sonnet"),
            ]
        },
        || "claude-3-5-sonnet".to_string(),
        |_| {},
    );

    let models = catalog.available_models();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].0, "claude-3-5-sonnet");
    assert_eq!(models[0].1, "Claude 3.5 Sonnet");
    assert_eq!(catalog.selected_model_id(), "claude-3-5-sonnet");
}

#[test]
fn model_catalog_adapter_select_model() {
    let mut selected = None;
    let catalog = AppModelCatalog::new(
        || vec![("model-1", "Model 1")],
        || "model-1".to_string(),
        |id| {
            selected = Some(id);
        },
    );

    catalog.select_model("model-2");
    assert_eq!(selected, Some("model-2".to_string()));
}
