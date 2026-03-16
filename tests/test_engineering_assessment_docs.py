import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
README_DOC = ROOT / "docs" / "README.md"
ASSESSMENT_DOC = ROOT / "docs" / "architecture" / "engineering-assessment.md"
UI_LEAN_MAP_DOC = ROOT / "docs" / "architecture" / "ui-lean-integration-map.md"
UI_LEAN_SPEC_DOC = (
    ROOT / "docs" / "architecture" / "ui-lean-send-draft-stream-spec.md"
)
VERIFY_DOC = ROOT / "docs" / "verification.md"
LIFECYCLE_DOC = ROOT / "docs" / "architecture" / "session-worker-lifecycle-spec.md"
VERSIONING_DOC = (
    ROOT / "docs" / "architecture" / "spool-session-contract-versioning.md"
)
PROPERTY_PLAN_DOC = ROOT / "docs" / "architecture" / "property-test-plan.md"
SEGMENTATION_DOC = ROOT / "docs" / "architecture" / "state-module-segmentation.md"
EVIDENCE_GATE_DOC = ROOT / "docs" / "architecture" / "evidence-gate.md"
REGISTRY = ROOT / "contracts" / "engineering-gate.toml"


class EngineeringAssessmentDocTests(unittest.TestCase):
    def test_docs_index_links_ui_lean_follow_on_docs(self) -> None:
        content = README_DOC.read_text()
        self.assertIn(
            "[UI-Lean Integration Map](architecture/ui-lean-integration-map.md)",
            content,
        )
        self.assertIn(
            "[UI-Lean Send/Draft/Stream Spec](architecture/ui-lean-send-draft-stream-spec.md)",
            content,
        )
        self.assertIn(
            "[Session And Worker Lifecycle Spec](architecture/session-worker-lifecycle-spec.md)",
            content,
        )
        self.assertIn(
            "[Spool And Session Contract Versioning](architecture/spool-session-contract-versioning.md)",
            content,
        )
        self.assertIn("[Property Test Plan](architecture/property-test-plan.md)", content)
        self.assertIn(
            "[State Module Segmentation Plan](architecture/state-module-segmentation.md)",
            content,
        )
        self.assertIn("[Evidence Gate](architecture/evidence-gate.md)", content)

    def test_assessment_references_repo_local_ui_lean_artifacts(self) -> None:
        content = ASSESSMENT_DOC.read_text()
        self.assertIn("chatbot-ui", content)
        self.assertIn("chatbot-adapter", content)
        self.assertIn("chatbot-history", content)
        self.assertIn("docs/architecture/ui-lean-integration-map.md", content)
        self.assertIn("docs/architecture/ui-lean-send-draft-stream-spec.md", content)
        self.assertIn("These candidates are now mapped", content)
        self.assertIn("UI-Lean Research Translation Contract", content)

    def test_ui_lean_integration_map_points_to_native_regressions(self) -> None:
        content = UI_LEAN_MAP_DOC.read_text()
        self.assertIn("Send/draft/stream/terminal lifecycle", content)
        self.assertIn(
            "src/app/state/session_runtime_tests.rs::apply_stream_to_current_clears_pending_draft_on_first_chunk",
            content,
        )
        self.assertIn(
            "src/app/state/ui_io_tests.rs::selected_session_reload_skips_replacement_when_pending_draft_active",
            content,
        )
        self.assertIn(
            "src/app/state/messaging_flow_tests.rs::check_for_response_drains_worker_workspace_and_history_background_channels_together",
            content,
        )
        self.assertIn("scripts/verify_ui_thread_contract.py", content)
        self.assertIn(".githooks/pre-commit", content)

    def test_ui_lean_send_draft_spec_captures_current_behavior_contracts(self) -> None:
        content = UI_LEAN_SPEC_DOC.read_text()
        self.assertIn("pending_draft_for_active_chat", content)
        self.assertIn("stream_chunk_non_empty", content)
        self.assertIn(
            "src/app/state/session_runtime_tests.rs::apply_stream_to_current_empty_response_keeps_pending_draft",
            content,
        )
        self.assertIn(
            "src/app/state/ui_io_tests.rs::send_failed_clears_pending_draft_and_appends_error",
            content,
        )
        self.assertIn(
            "src/app/ui/message_list/tests/render_regressions.rs::render_with_pending_draft_does_not_clear_or_panic",
            content,
        )
        self.assertIn("scripts/verify_ui_spec_contract.py", content)
        self.assertIn("scripts/verify-ui-spec-regressions.sh", content)

    def test_verification_doc_records_ui_lean_translation_contract(self) -> None:
        content = VERIFY_DOC.read_text()
        self.assertIn("chatbot-ui", content)
        self.assertIn("chatbot-adapter", content)
        self.assertIn("chatbot-history", content)
        self.assertIn("agent-memory", content)
        self.assertIn("## UI-Lean Research Translation Contract", content)
        self.assertIn("## Session/Worker Lifecycle Contract", content)
        self.assertIn("## Spool/Session Contract", content)
        self.assertIn("## History/Checkpoint Contract", content)
        self.assertIn("## Memory Contract", content)
        self.assertIn("ui-lean model", content)
        self.assertIn("docs/architecture/ui-lean-send-draft-stream-spec.md", content)
        self.assertIn(
            "src/app/state/session_runtime_tests.rs::apply_stream_to_current_clears_pending_draft_on_first_chunk",
            content,
        )
        self.assertIn(
            "src/app/ui/message_list/tests/render_regressions.rs::render_with_pending_draft_does_not_clear_or_panic",
            content,
        )
        self.assertIn("scripts/verify_ui_thread_contract.py", content)
        self.assertIn("src/app/ui/layout_log.rs", content)
        self.assertIn("scripts/verify_ui_spec_contract.py", content)
        self.assertIn("scripts/verify-ui-spec-regressions.sh", content)

    def test_assessment_records_follow_through_artifacts(self) -> None:
        content = ASSESSMENT_DOC.read_text()
        self.assertIn("## Current Status: Implemented Vs Enforced Vs Still Needed", content)
        self.assertIn("### Implemented And Enforced Together", content)
        self.assertIn("### Implemented But Only Partially Enforced", content)
        self.assertIn("### Still Needed", content)
        self.assertIn("scripts/verify_ui_spec_contract.py", content)
        self.assertIn("scripts/verify_ui_thread_contract.py", content)
        self.assertIn("scripts/verify_session_lifecycle_contract.py", content)
        self.assertIn("scripts/verify_spool_session_contract.py", content)
        self.assertIn("scripts/verify_history_contract.py", content)
        self.assertIn("scripts/verify_memory_contract.py", content)
        self.assertIn("property-test-plan.md", content)
        self.assertIn("command_group", content)
        self.assertIn("### Follow-Through Artifacts", content)
        self.assertIn("docs/architecture/session-worker-lifecycle-spec.md", content)
        self.assertIn(
            "docs/architecture/spool-session-contract-versioning.md",
            content,
        )
        self.assertIn("docs/architecture/property-test-plan.md", content)
        self.assertIn("docs/architecture/state-module-segmentation.md", content)
        self.assertIn("docs/architecture/evidence-gate.md", content)
        self.assertIn("chatbot-history", content)

    def test_lifecycle_versioning_and_evidence_docs_capture_concrete_contracts(self) -> None:
        lifecycle = LIFECYCLE_DOC.read_text()
        self.assertIn("### 2. `PendingDraft`", lifecycle)
        self.assertIn("### 6. `TerminalPendingReconcile`", lifecycle)
        self.assertIn("single-writer", lifecycle)
        self.assertIn("scripts/verify_session_lifecycle_contract.py", lifecycle)
        self.assertIn("scripts/verify-session-lifecycle-regressions.sh", lifecycle)

        versioning = VERSIONING_DOC.read_text()
        self.assertIn("spool/session contract version = v1", versioning)
        self.assertIn("Compatible additive", versioning)
        self.assertIn("Breaking contract", versioning)
        self.assertIn("scripts/verify_spool_session_contract.py", versioning)
        self.assertIn("scripts/verify-spool-session-regressions.sh", versioning)

        evidence = EVIDENCE_GATE_DOC.read_text()
        self.assertIn("{ contract, regression, replay assertion, sibling-boundary check, implementation }", evidence)
        self.assertIn("../chatbot-worker/scripts/verify-sibling-builds.sh", evidence)
        self.assertIn("chatbot-adapter", evidence)
        self.assertIn("agent-memory", evidence)
        self.assertIn("scripts/verify_history_contract.py", evidence)
        self.assertIn("scripts/verify_memory_contract.py", evidence)
        self.assertIn("contracts/engineering-gate.toml", evidence)
        self.assertIn("scripts/verify-engineering-gate.sh", evidence)

    def test_property_and_segmentation_docs_target_current_repo_hotspots(self) -> None:
        property_plan = PROPERTY_PLAN_DOC.read_text()
        self.assertIn("Pending-Draft Lifecycle Permutations", property_plan)
        self.assertIn("Worker-Spool Recovery Permutations", property_plan)
        self.assertIn(
            "src/app/state/session_runtime_tests.rs::apply_stream_to_current_clears_pending_draft_on_first_chunk",
            property_plan,
        )

        segmentation = SEGMENTATION_DOC.read_text()
        self.assertIn("src/bin/chatbot_ctl/messaging.rs` at 936 lines", segmentation)
        self.assertIn("src/app/state/session_runtime.rs` at 768 lines", segmentation)
        self.assertIn("send_pipeline.rs", segmentation)
        self.assertIn("terminal_reconcile.rs", segmentation)


if __name__ == "__main__":
    unittest.main()
