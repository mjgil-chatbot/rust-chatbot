use eframe::egui;
use ai_history_core::Actor;
use crate::app::state::CheckpointRepoSummary;
use crate::app::state::RepoPatchCaptureState;
use crate::app::state::RepoPatchSummary;
const HISTORY_PANEL_WIDTH: f32 = 320.0;
pub fn panel_config() -> egui::SidePanel {
    egui::SidePanel::right("history_panel")
        .resizable(true)
        .min_width(240.0)
        .default_width(HISTORY_PANEL_WIDTH)
        .max_width(520.0)
}
fn repo_summary_for_checkpoint(
    summaries: &[CheckpointRepoSummary],
    checkpoint_id: ai_history_core::CheckpointId,
) -> Option<&CheckpointRepoSummary> {
    summaries
        .iter()
        .find(|summary| summary.checkpoint_id == checkpoint_id)
}
fn repo_patch_summary_line(repo: &RepoPatchSummary) -> String {
    let kind = if repo.role == "workspace" {
        "workspace"
    } else {
        "companion"
    };
    let commit = if repo.base_commit == repo.apply_commit {
        format!("commit {}", short_commit(&repo.base_commit))
    } else {
        format!(
            "base {} -> apply {}",
            short_commit(&repo.base_commit),
            short_commit(&repo.apply_commit)
        )
    };
    let untracked = if repo.untracked_file_count == 0 {
        String::new()
    } else {
        format!(" • {} untracked", repo.untracked_file_count)
    };
    let patch = match repo.capture_state {
        RepoPatchCaptureState::Captured if repo.patch_is_empty => "clean patch".to_string(),
        RepoPatchCaptureState::Captured => format!("{} B patch", repo.patch_bytes),
        RepoPatchCaptureState::OmittedOversize => format!(
            "patch omitted ({})",
            repo.omitted_reason
                .as_deref()
                .unwrap_or("dirty repo patch exceeded the capture limit")
        ),
        RepoPatchCaptureState::OmittedQuota => format!(
            "patch omitted ({})",
            repo.omitted_reason
                .as_deref()
                .unwrap_or("history blob quota had no room for the dirty repo patch")
        ),
    };
    format!(
        "{} • {} • {} • {}{}",
        repo.repo_name, kind, commit, patch, untracked
    )
}
fn short_commit(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}
