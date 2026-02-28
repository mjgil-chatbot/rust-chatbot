#!/usr/bin/env python3
"""Minimal mock Anthropic-compatible HTTP server for auto-ui testing.

Serves a single canned response per request. Writes the bound port to a
file so callers can discover it. Usage:

    mock-provider-server.py --port-file /tmp/mock-port.txt [--response-text "hello"]

The server listens on 127.0.0.1:0 (OS-assigned port), writes the port to
--port-file, then serves until killed.
"""
import argparse
import json
import sys
from http.server import HTTPServer, BaseHTTPRequestHandler
from pathlib import Path

RESPONSE_TEXT = "This is a mock assistant response for testing."


class MockHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length)
        try:
            request = json.loads(body)
        except json.JSONDecodeError:
            request = {}

        response = {
            "id": "msg_mock_001",
            "type": "message",
            "role": "assistant",
            "model": "claude-mock-test",
            "content": [{"type": "text", "text": self.server.response_text}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
            },
        }
        payload = json.dumps(response).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format, *args):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port-file", type=Path, required=True)
    parser.add_argument("--response-text", default=RESPONSE_TEXT)
    args = parser.parse_args()

    server = HTTPServer(("127.0.0.1", 0), MockHandler)
    server.response_text = args.response_text
    port = server.server_address[1]
    args.port_file.write_text(str(port))
    print(f"[mock-provider] listening on 127.0.0.1:{port}", file=sys.stderr)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_shutdown()


if __name__ == "__main__":
    main()
