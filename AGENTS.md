# Contributor instructions

- Build and test with `--release`.
- Keep the eight repositories in the sibling layout described in README.md.
- Keep source changes, regression tests, and behavioral documentation together.
- Use `scripts/auto-ui*` for supported UI diagnostics when its dependencies are installed.
- Keep credentials, provider sessions, generated traces, and local MCP configuration out of Git.
- Prefer `semble` for semantic code search when available; otherwise use the available local search tools.
