# Autonomous Design Docs

This is the default entry point for `background_design_doc_path`. It describes
the boundaries used by the current code, without a separate migration backlog.

- [Autonomous coordination](../architecture/autonomous-coordination.md): the
  runtime provider catalog and the smaller autonomous policy subset.
- [Background provider runs](../architecture/background-provider-runs.md):
  transport execution, worker lifecycle, and optional runsc isolation.
- [Adapter integration](../architecture/chatbot-adapter-integration.md):
  normalized requests and events at the worker/provider boundary.
- [Send, draft, and stream behavior](../architecture/ui-lean-send-draft-stream-spec.md):
  session selection, optimistic drafts, stream updates, and terminal handling.
- [History integration](../architecture/chatbot-history-integration.md):
  checkpoints, restore operations, and limits on replay artifacts.
- [Verification](../verification.md): executable contracts and regression suites.

The application hosts the UI and session state. The worker owns request
execution, and the adapter owns provider CLI and transport normalization.
Higher-level autonomous workflow services are separate optional integrations;
they are not required to build this release set.
