use super::*;
use crate::data::ChatSession;
use crate::model::ModelProvider;
use crate::TEST_ENV_LOCK;
use serde_json::json;

#[test]
fn status_bar_effort_label_format() {
    let effort = Some("high".to_string());
    let label = effort.map(|e| format!("Effort: {}", e));
    assert_eq!(label, Some("Effort: high".to_string()));

    let effort = Some("low".to_string());
    let label = effort.map(|e| format!("Effort: {}", e));
    assert_eq!(label, Some("Effort: low".to_string()));

    let effort: Option<String> = None;
    let label = effort.map(|e| format!("Effort: {}", e));
    assert_eq!(label, None);
}

#[test]
fn status_bar_mcp_label_counts_servers() {
    assert_eq!(
        &mcp_status_text(&vec!["alpha".to_string(), "beta".to_string()]),
        "MCP: 2"
    );
    assert_eq!(mcp_status_text(&[]), "MCP: 0");
}

#[test]
fn status_bar_mcp_hover_text_formats_sorted_names() {
    let mut names = vec!["beta".to_string(), "alpha".to_string()];
    names.sort_unstable();
    assert_eq!(
        mcp_hover_text(&names),
        "Connected MCP servers:\n- alpha\n- beta"
    );

    assert_eq!(mcp_hover_text(&[]), "No MCP servers configured");

    assert_eq!(
        mcp_hover_text(&["mcp-coordinator".to_string()]),
        "Connected MCP servers:\n- mcp-coordinator\n\nRouted upstream MCP tools remain callable through the `mcp-coordinator` namespace."
    );
}

#[test]
fn optimistic_create_session_name_tracks_pending_create_request() {
    let mut state = AppState::new_for_tests(ModelProvider::Codex);
    assert!(optimistic_create_session_name(&state).is_none());

    state
        .session
        .ui_io
        .mark_create_session_started(7, "Created".to_string());
    assert_eq!(optimistic_create_session_name(&state), Some("Created"));
}

#[test]
fn connected_mcp_servers_uses_provider_effective_runsc_filter() {
    let _lock = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    let mcp_json = tmp.path().join("mcp.json");
    std::fs::write(
        &mcp_json,
        serde_json::to_vec_pretty(&json!({
            "mcpServers": {
                "mcp-coordinator": {
                    "command": "cargo"
                }
            },
            "providers": {
                "claude": {
                    "mcpServers": {
                        "Context7": {
                            "command": "cargo",
                            "args": [
                                "run",
                                "--quiet",
                                "--manifest-path",
                                "/workspace/tools/context7-mcp/apps/stdio-server/Cargo.toml"
                            ]
                        },
                        "sequential-thinking": {
                            "command": "cargo",
                            "args": [
                                "run",
                                "--quiet",
                                "--manifest-path",
                                "/workspace/tools/mcp-server-sequential-thinking/apps/stdio-server/Cargo.toml"
                            ]
                        },
                        "lightpanda": {
                            "command": "/workspace/bin/lightpanda",
                            "args": [
                                "mcp"
                            ]
                        },
                        "lightweight-builds": {
                            "command": "cargo",
                            "args": [
                                "run",
                                "--quiet",
                                "--manifest-path",
                                "/workspace/tools/lightweight-mcp/Cargo.toml"
                            ]
                        }
                    }
                },
                "gemini": {
                    "mcpServers": {
                        "Context7": {
                            "command": "cargo",
                            "args": [
                                "run",
                                "--quiet",
                                "--manifest-path",
                                "/workspace/tools/context7-mcp/apps/stdio-server/Cargo.toml"
                            ]
                        },
                        "sequential-thinking": {
                            "command": "cargo",
                            "args": [
                                "run",
                                "--quiet",
                                "--manifest-path",
                                "/workspace/tools/mcp-server-sequential-thinking/apps/stdio-server/Cargo.toml"
                            ]
                        },
                        "lightpanda": {
                            "command": "/workspace/bin/lightpanda",
                            "args": [
                                "mcp"
                            ]
                        },
                        "lightweight-builds": {
                            "command": "cargo",
                            "args": [
                                "run",
                                "--quiet",
                                "--manifest-path",
                                "/workspace/tools/lightweight-mcp/Cargo.toml"
                            ]
                        }
                    }
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    std::env::set_var("RUST_CHATBOT_TEST_MCP_JSON_PATH", &mcp_json);

    let gemini_state = AppState::new_for_tests(ModelProvider::Gemini);
    let mut gemini_names = connected_mcp_servers(&gemini_state);
    gemini_names.sort();
    assert_eq!(
        gemini_names,
        vec![
            "Context7".to_string(),
            "lightpanda".to_string(),
            "lightweight-builds".to_string(),
            "sequential-thinking".to_string()
        ]
    );

    let claude_state = AppState::new_for_tests(ModelProvider::Claude);
    let mut claude_names = connected_mcp_servers(&claude_state);
    claude_names.sort();
    assert_eq!(
        claude_names,
        vec![
            "Context7".to_string(),
            "lightpanda".to_string(),
            "lightweight-builds".to_string(),
            "sequential-thinking".to_string()
        ]
    );

    std::env::remove_var("RUST_CHATBOT_TEST_MCP_JSON_PATH");
}

#[test]
fn session_effective_mcp_overrides_uses_session_runsc_image_override() {
    let mut state = AppState::new_for_tests(ModelProvider::Codex);
    state.session.runsc_image_input_text = "custom/runsc:latest".to_string();

    let overrides =
        session_effective_mcp_overrides(&state).expect("runsc image should create overrides");

    assert_eq!(
        overrides.runsc_image.as_deref(),
        Some("custom/runsc:latest")
    );
}

#[test]
fn session_effective_mcp_overrides_prefers_saved_session_runsc_image() {
    let mut state = AppState::new_for_tests(ModelProvider::Codex);
    state.set_current_session(ChatSession {
        id: "session-1".to_string(),
        name: "session-1".to_string(),
        system_prompt: None,
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
        runsc_image: Some("saved/runsc:stable".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: Vec::new(),
    });
    state.session.runsc_image_input_text = "draft/runsc:latest".to_string();

    let overrides =
        session_effective_mcp_overrides(&state).expect("saved runsc image should create overrides");

    assert_eq!(overrides.runsc_image.as_deref(), Some("saved/runsc:stable"));
}
