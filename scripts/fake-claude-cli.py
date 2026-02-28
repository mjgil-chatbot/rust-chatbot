#!/usr/bin/env python3
"""Fake Claude CLI for auto-ui testing.

Mimics the real claude CLI's --output-format stream-json interface by
forwarding the user query to a mock Anthropic server (via ANTHROPIC_BASE_URL)
and emitting assistant/result events on stdout.

Required env vars:
    ANTHROPIC_BASE_URL   e.g. http://127.0.0.1:PORT/anthropic
    ANTHROPIC_API_KEY    any non-empty string (not validated)
"""
import json
import os
import sys
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
        elif arg == "-p":
            if i + 2 < len(argv) and argv[i + 1] == "--":
                parsed["query"] = argv[i + 2]
                i += 3
            else:
                parsed["query"] = argv[i + 1]
                i += 2
        else:
            i += 1
    if parsed["query"] is None:
        raise SystemExit("missing query")
    return parsed


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


def main():
    parsed = parse_args(sys.argv[1:])
    base_url = os.environ["ANTHROPIC_BASE_URL"]
    auth_token = os.environ.get("ANTHROPIC_API_KEY", "mock-token")
    model = parsed["model"] or "claude-mock-test"
    session_id = parsed["resume"] or "mock-session-001"

    messages = [{"role": "user", "content": [{"type": "text", "text": parsed["query"]}]}]
    payload = {
        "model": model,
        "max_tokens": 4096,
        "messages": messages,
    }
    if parsed["system_prompt"]:
        payload["system"] = parsed["system_prompt"]

    response = post_message(base_url, auth_token, payload)
    content = response.get("content", [])
    usage = response.get("usage", {"input_tokens": 0, "output_tokens": 0})

    emit({
        "type": "assistant",
        "session_id": session_id,
        "message": {
            "model": model,
            "usage": usage,
            "content": content,
        },
    })

    result_text = ""
    for block in content:
        if block.get("type") == "text":
            result_text += block.get("text", "")

    emit({
        "type": "result",
        "session_id": session_id,
        "model": model,
        "duration_ms": 18,
        "num_turns": 1,
        "usage": usage,
        "modelUsage": {},
        "is_error": False,
        "result": result_text,
    })


if __name__ == "__main__":
    main()
