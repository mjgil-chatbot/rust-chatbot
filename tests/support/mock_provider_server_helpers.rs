use super::*;

pub fn usage(input_tokens: u64, output_tokens: u64) -> Value {
    json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "cache_creation_input_tokens": 0,
        "cache_read_input_tokens": 0,
    })
}

pub fn model_usage() -> Value {
    json!({
        "minimax": {
            "contextWindow": 8192
        }
    })
}

pub fn text_response(session_id: &str, text: &str) -> Value {
    json!({
        "session_id": session_id,
        "model": "MiniMax-M2.7",
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn",
        "usage": usage(21, 7),
        "duration_ms": 18,
        "num_turns": 1,
        "result_text": text,
        "modelUsage": model_usage(),
    })
}

// ---------------------------------------------------------------------------
// Provider-boundary assertion helpers
// ---------------------------------------------------------------------------

/// Assert that the mock server received exactly `expected` requests.
pub fn assert_request_count(server: &MockProviderServer, expected: usize) {
    let actual = server.request_count();
    assert_eq!(
        actual, expected,
        "expected {expected} provider requests, got {actual}"
    );
}

/// Assert that the mock server received requests in the given step order.
pub fn assert_request_ordering(server: &MockProviderServer, expected_steps: &[&str]) {
    let actual: Vec<String> = server.step_names();
    let expected: Vec<String> = expected_steps.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        actual, expected,
        "request ordering mismatch: expected {expected:?}, got {actual:?}"
    );
}

/// Assert that all requests hit the expected API path.
pub fn assert_all_requests_path(server: &MockProviderServer, expected_path: &str) {
    for req in server.requests() {
        assert_eq!(
            req.path, expected_path,
            "request for step '{}' hit path '{}', expected '{}'",
            req.step, req.path, expected_path
        );
    }
}

/// Assert that all requests carried the expected auth token.
pub fn assert_all_requests_auth(server: &MockProviderServer, expected_token: &str) {
    for req in server.requests() {
        assert_eq!(
            req.auth_token.as_deref(),
            Some(expected_token),
            "request for step '{}' missing or wrong auth token",
            req.step
        );
    }
}

/// Assert that the first request carried the expected system prompt.
pub fn assert_system_prompt(server: &MockProviderServer, expected: &str) {
    let requests = server.requests();
    assert!(
        !requests.is_empty(),
        "no requests recorded for system prompt assertion"
    );
    let actual = requests[0]
        .system_prompt()
        .expect("system prompt must be present");
    let policy = actual.strip_suffix(expected).unwrap_or_else(|| {
        panic!("custom instructions lost or reordered: expected {expected:?}, got {actual:?}")
    });
    let marker = "[agent-adapter-code-search-policy-v1]";
    assert!(
        policy.starts_with(&format!("{marker}\nCode search policy:\n")),
        "managed code-search policy is missing: {actual:?}"
    );
    assert_eq!(
        policy.matches(marker).count(),
        1,
        "managed policy must occur once"
    );
    assert!(
        policy.ends_with("\n\n"),
        "managed policy must be separated from custom instructions"
    );
}

/// Read all complete events currently available in the worker's JSONL spool.
pub fn read_worker_events(chat_id: &str) -> Vec<chatbot_lib::stream::StreamEvent> {
    worker::read_events_since(chat_id, 0).0
}

/// Require the expected tool calls to both start and finish in order.
pub fn assert_tool_events(chat_id: &str, expected: &[&str]) {
    use chatbot_lib::stream::StreamEvent;
    let events = read_worker_events(chat_id);
    let starts: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::ToolStart { tool, .. } => Some(tool.as_str()),
            _ => None,
        })
        .collect();
    let ends: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::ToolEnd {
                tool, live_update, ..
            } => Some(
                live_update
                    .as_ref()
                    .map(|update| update.raw_name.as_str())
                    .unwrap_or(tool.as_str()),
            ),
            _ => None,
        })
        .collect();
    assert_eq!(
        starts, expected,
        "tool start events differ from expected calls"
    );
    assert_eq!(ends, expected, "tool end events differ from expected calls");
}

/// Check exact assistant text while allowing the separately recorded transcript.
pub fn assert_worker_text_output(chat_id: &str, expected: &str) {
    let events = read_worker_events(chat_id);
    let text: String = events
        .iter()
        .filter_map(|event| match event {
            chatbot_lib::stream::StreamEvent::TextDelta { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, expected, "assistant text in the event spool differs");
    let persisted = fs::read_to_string(worker::output_path(chat_id)).unwrap();
    assert!(
        persisted.ends_with(expected),
        "persisted output lost or changed the assistant body: {persisted:?}"
    );
}
