# Documentation

Current documentation for `rust-chatbot`.

This tree describes the selected code, its public interfaces, and executable
behavior contracts. Completed migration logs and development scratch notes
are excluded.

## Start Here

- [Architecture Overview](architecture/overview.md)
- [Autonomous Design Index](autonomous/design-doc.md)
- [Verification Contract](verification.md)

## Architecture

- [Overview](architecture/overview.md)
- [Engineering Assessment](architecture/engineering-assessment.md)
- [UI-Lean Integration Map](architecture/ui-lean-integration-map.md)
- [UI-Lean Send/Draft/Stream Spec](architecture/ui-lean-send-draft-stream-spec.md)
- [Session And Worker Lifecycle Spec](architecture/session-worker-lifecycle-spec.md)
- [Spool And Session Contract Versioning](architecture/spool-session-contract-versioning.md)
- [Property Test Plan](architecture/property-test-plan.md)
- [State Module Segmentation Plan](architecture/state-module-segmentation.md)
- [Evidence Gate](architecture/evidence-gate.md)
- [Broker And Sync](architecture/broker-sync.md)
- [Worker Spool](architecture/worker-spool.md)
- [Background Provider Runs](architecture/background-provider-runs.md)
- [MemMachine Integration](architecture/memmachine-integration.md)
- [History Auditor v1](architecture/history-auditor-v1.md)
- [Autonomous Coordination](architecture/autonomous-coordination.md)

## Workflow Design

- [Autonomous Design Index](autonomous/design-doc.md)
- [Verification Contract](verification.md)

## Operator And Usage

- [Adding a Provider](adding-a-provider.md)
- [chatbot-ctl](chatbot-ctl.md)
- [Memory MCP](chatbot-memory-mcp.md)
- [Input Completion](input-completion.md)
- [Pricing Computation](pricing-computation.md)

## Maintenance

- Keep docs in the same change as behavior changes.
- Prefer one canonical doc per current feature or workflow.
- Put generated research, dated review output, and superseded migration notes
  outside this tree or delete them after consolidation.
- Keep cross-cutting behavior here; keep module-local detail in code comments
  and tests.
