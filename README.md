# Rust Chatbot

A Rust desktop chat application with an egui interface, detached workers,
session history, and adapters for multiple coding-assistant providers.

## Build the repository set

Keep these repositories as siblings: `rust-chatbot`, `chatbot-ui`,
`chatbot-worker`, `ai-providers`, `chatbot-history`, `history-auditor`,
`the-markdown-commons`, and `chatbot-adapter`. The Cargo path dependencies use
this layout. Install the toolchain selected by `rust-toolchain.toml`.

From this repository:

```bash
cargo build --release
cargo build --release --manifest-path ../chatbot-adapter/Cargo.toml -p agent-adapter-cli
cargo run --release --bin rcc -- codex
```

Provider CLIs and their authentication are separate installations. Configure
only the providers you intend to use. `rcc` is the launcher; `rust-chatbot` is
the desktop binary. `chatbot-ctl` manages instances and workers. The shared
worker implementation lives in the sibling `chatbot-worker` repository.

## Configuration and runtime data

Shared configuration and session data live under `~/.rust-chatbot/`. The main
configuration is `~/.rust-chatbot/config.toml`; see
[chatbot-config.example.toml](chatbot-config.example.toml). Provider-specific
configuration and credentials belong in your local runtime directory and must
not be committed to a source checkout. `.codex/auth.json` is explicitly
ignored; configure authentication in the local runtime directory.
Some older home and repository-local
locations remain compatibility fallbacks.

Optional MCP configuration is illustrated in [mcp.example.json](mcp.example.json).
Install the referenced binary on PATH, restrict allowed roots to your intended
workspaces, and put your local configuration in the runtime directory. The
example does not install external MCP services.

## Development

```bash
cargo test --release
python3 -m unittest discover -s tests
```

The Python contract scripts require Python 3.11 or newer. Shell automation
uses Bash; scripts using `readarray` need Bash 4 or newer.

The [verification contract](docs/verification.md) lists the focused regression
scripts and sibling checks. Browser/UI automation needs the additional tools
specified by its scripts. Install repository hooks with
`./scripts/install-hooks.sh` when desired.

## Documentation

- [Documentation index](docs/README.md)
- [Architecture](docs/architecture/overview.md)
- [Controller commands](docs/chatbot-ctl.md)
- [Adding a provider](docs/adding-a-provider.md)
- [Memory MCP](docs/chatbot-memory-mcp.md)

## License

First-party code is available under [0BSD](LICENSE). Third-party components
and assets retain their own licenses and notices.
