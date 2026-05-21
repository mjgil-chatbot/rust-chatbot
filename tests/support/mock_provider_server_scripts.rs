use super::*;

pub fn write_fake_claude_cli(path: &Path) {
    let script = r#"#!/usr/bin/env python3
import json
import os
import sys
import time
import urllib.request

def parse_args(argv):
    parsed = {
        "system_prompt": None,
        "resume": None,
        "model": None,
        "output_format": None,
        "query": None,
    }
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg == "--append-system-prompt":
            parsed["system_prompt"] = argv[i + 1]
            i += 2
        elif arg == "--dangerously-skip-permissions":
            i += 1
        elif arg == "--resume":
            parsed["resume"] = argv[i + 1]
            i += 2
        elif arg == "--verbose":
            i += 1
        elif arg == "--model":
            parsed["model"] = argv[i + 1]
            i += 2
        elif arg == "--output-format":
            parsed["output_format"] = argv[i + 1]
            i += 2
        elif arg == "--input-format":
            i += 2
        elif arg == "--context-dirs":
            i += 2
        elif arg == "-p":
            if i + 2 < len(argv) and argv[i + 1] == "--":
                parsed["query"] = argv[i + 2]
                i += 3
            else:
                parsed["query"] = argv[i + 1]
                i += 2
        else:
            raise SystemExit(f"unexpected arg: {arg}")
    if parsed["query"] is None:
        raise SystemExit("missing query")
    return parsed

def read_optional_json(path):
    try:
        with open(path, "r", encoding="utf-8") as handle:
            return json.load(handle)
    except FileNotFoundError:
        return None

def post_message(base_url, auth_token, payload):
    request = urllib.request.Request(
        base_url.rstrip("/") + "/v1/messages",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "content-type": "application/json",
            "anthropic-version": "2023-06-01",
            "x-api-key": auth_token,
        },
        method="POST",
    )
    with urllib.request.urlopen(request) as response:
        return json.loads(response.read().decode("utf-8"))

def emit(event):
    sys.stdout.write(json.dumps(event) + "\n")
    sys.stdout.flush()

def assistant_event(model, session_id, content, usage):
    return {
        "type": "assistant",
        "session_id": session_id,
        "message": {
            "model": model,
            "usage": usage,
            "content": content,
        },
    }

def main():
    parsed = parse_args(sys.argv[1:])
    base_url = os.environ["RUST_CHATBOT_TEST_ANTHROPIC_BASE_URL"]
    auth_token = os.environ.get("RUST_CHATBOT_TEST_ANTHROPIC_AUTH_TOKEN", "")
    config_dir = os.environ.get("CLAUDE_CONFIG_DIR")
    claude_json = None
    if config_dir:
        claude_json = read_optional_json(os.path.join(config_dir, ".claude.json"))

    messages = [{
        "role": "user",
        "content": [{"type": "text", "text": parsed["query"]}],
    }]
    session_id = parsed["resume"]
    system_emitted = False
    tool_result_count = 0

    while True:
        payload = {
            "model": parsed["model"],
            "system": parsed["system_prompt"],
            "messages": messages,
            "resume_session_id": session_id,
            "stream": False,
            "metadata": {
                "config_dir": config_dir,
                "output_format": parsed["output_format"],
                "claude_json": claude_json,
                "tool_result_count": tool_result_count,
                "cli_path": os.path.realpath(sys.argv[0]),
            },
        }
        response = post_message(base_url, auth_token, payload)
        session_id = response.get("session_id", session_id)
        model = response.get("model", parsed["model"])
        usage = response.get("usage", {})
        if not system_emitted:
            emit({
                "type": "system",
                "model": model,
                "session_id": session_id,
            })
            system_emitted = True

        stream_chunks = response.get("stream_chunks") or []
        if stream_chunks:
            for chunk in stream_chunks:
                emit(assistant_event(
                    model,
                    session_id,
                    [{"type": "text", "text": chunk}],
                    usage,
                ))
        else:
            emit(assistant_event(
                model,
                session_id,
                response.get("content", []),
                usage,
            ))

        if response.get("stop_reason") == "tool_use":
            tool_use = None
            for item in response.get("content", []):
                if item.get("type") == "tool_use":
                    tool_use = item
                    break
            if tool_use is None:
                raise SystemExit("tool_use response omitted tool block")
            messages.append({"role": "assistant", "content": response.get("content", [])})
            messages.append({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_use.get("id"),
                    "content": response.get("tool_result_text", "tool-ok"),
                }],
            })
            emit({
                "type": "user",
                "session_id": session_id,
                "message": messages[-1],
            })
            tool_result_count += 1
            continue

        sleep_ms = response.get("sleep_before_result_ms", 0)
        if sleep_ms:
            time.sleep(sleep_ms / 1000.0)
        if response.get("emit_malformed_result"):
            sys.stdout.write('{"type":"result","session_id"\n')
            sys.stdout.flush()
        else:
            emit({
                "type": "result",
                "session_id": session_id,
                "model": model,
                "duration_ms": response.get("duration_ms", 0),
                "num_turns": response.get("num_turns", 1),
                "usage": usage,
                "modelUsage": response.get("modelUsage", {}),
                "is_error": response.get("is_error", False),
                "result": response.get("result_text", ""),
            })
        break

if __name__ == "__main__":
    main()
"#;
    fs::write(path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

pub fn write_fake_agent_adapter_cli(path: &Path) {
    ensure_agent_adapter_cli_built();
    let binary_path = agent_adapter_cli_binary_path();
    let binary = binary_path
        .to_str()
        .expect("adapter CLI path must be UTF-8");
    let binary = format!("'{}'", binary.replace('\'', "'\\''"));
    let script = format!("#!/usr/bin/env bash\nset -euo pipefail\nexec {binary} \"$@\"\n");
    fs::write(path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

fn agent_adapter_test_target_dir() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        }
        None => root.join("target"),
    }
}

fn agent_adapter_cli_binary_path() -> PathBuf {
    agent_adapter_test_target_dir()
        .join("release")
        .join("agent-adapter-cli")
}

fn ensure_agent_adapter_cli_built() {
    static BUILD: std::sync::Once = std::sync::Once::new();
    BUILD.call_once(|| {
        let status = Command::new("cargo")
            .args(["build", "-p", "agent-adapter-cli", "--release", "--locked"])
            .env("CARGO_TARGET_DIR", agent_adapter_test_target_dir())
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../chatbot-adapter"))
            .status()
            .expect("failed to start agent-adapter-cli build");
        assert!(status.success(), "agent-adapter-cli build failed");
    });
}
