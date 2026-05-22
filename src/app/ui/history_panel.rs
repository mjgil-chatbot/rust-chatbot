use eframe::egui;

use ai_history_core::Actor;

use crate::app::state::{
    human_bytes, AppState, CheckpointRepoSummary, RepoPatchCaptureState, RepoPatchSummary,
};

const HISTORY_PANEL_WIDTH: f32 = 320.0;

pub fn panel_config() -> egui::SidePanel {
    egui::SidePanel::right("history_panel")
        .resizable(true)
        .min_width(240.0)
        .default_width(HISTORY_PANEL_WIDTH)
        .max_width(520.0)
}

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.set_width(ui.available_width());
    ui.heading("History");
    ui.separator();

    let can_enable = state.session.current_session.is_some()
        && state.session.effective_workspace_dir().is_some()
        && !state.session.history.refresh_in_flight;
    let can_checkpoint = state.session.current_session.is_some()
        && state.session.effective_workspace_dir().is_some()
        && !state.session.history.checkpoint_in_flight;

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !state.session.history.refresh_in_flight,
                egui::Button::new("Refresh"),
            )
            .clicked()
        {
            state.refresh_history();
        }

        if !state.session.history.snapshot.has_store
            && ui
                .add_enabled(can_enable, egui::Button::new("Enable"))
                .clicked()
        {
            state.request_history_enable();
        }

        if ui
            .add_enabled(can_checkpoint, egui::Button::new("Checkpoint"))
            .clicked()
        {
            state.session.history.show_checkpoint_dialog = true;
            if state.session.history.checkpoint_name.is_empty() {
                state.session.history.checkpoint_name = "snapshot".to_string();
            }
        }
    });

    if let Some(workspace_root) = state.session.history.snapshot.workspace_root.as_deref() {
        ui.label(format!("Workspace: {}", workspace_root));
    }
    if let Some(store_path) = state.session.history.snapshot.store_path.as_deref() {
        ui.label(format!("Store: {}", store_path));
    }
    ui.label(format!(
        "History Sessions: {}",
        state.session.history.snapshot.total_sessions
    ));
    ui.label(format!(
        "Auto Checkpoints: {} retained",
        state.session.history.snapshot.auto_checkpoint_count
    ));
    ui.label("Checkpoint Storage: replayable base commit + repo patches");
    ui.label(format!(
        "Object Store: {} files • {}",
        state.session.history.snapshot.object_store_files,
        human_bytes(state.session.history.snapshot.object_store_bytes)
    ));
    ui.label("Delete Recovery: shared metadata snapshot + CAS backup restore");

    if let Some(title) = state
        .session
        .history
        .snapshot
        .linked_session_title
        .as_deref()
    {
        ui.label(format!("Current Chat: {}", title));
    }

    if state.session.history.refresh_in_flight {
        ui.label("Refreshing history...");
    }
    if state.session.history.checkpoint_in_flight {
        ui.label("Creating checkpoint...");
    }
    if let Some(error) = state.session.history.snapshot.error.as_deref() {
        ui.colored_label(ui.visuals().error_fg_color, error);
    } else if let Some(notice) = state.session.history.snapshot.notice.as_deref() {
        ui.label(notice);
    }

    if let Some(branch_view) = state.session.history.snapshot.branch_view.as_ref() {
        ui.separator();
        egui::CollapsingHeader::new(format!("Branches ({})", branch_view.total_count))
            .default_open(true)
            .show(ui, |ui| {
                for branch in &branch_view.branches {
                    let marker = if branch.is_active { "●" } else { "○" };
                    let head = branch.head_turn_id.as_deref().unwrap_or("none");
                    ui.label(format!(
                        "{} {} • {} turns • head {}",
                        marker, branch.name, branch.turn_count, head
                    ));
                }
            });
    }

    if let Some(checkpoint_view) = state.session.history.snapshot.checkpoint_view.as_ref() {
        ui.separator();
        egui::CollapsingHeader::new(format!("Checkpoints ({})", checkpoint_view.total_count))
            .default_open(true)
            .show(ui, |ui| {
                for checkpoint in checkpoint_view.checkpoints.iter().rev() {
                    let label = checkpoint
                        .name
                        .as_deref()
                        .unwrap_or(checkpoint.reason.as_str());
                    ui.label(format!(
                        "{} • {} files • {}",
                        label, checkpoint.file_count, checkpoint.created_at
                    ));
                    if let Some(summary) = repo_summary_for_checkpoint(
                        &state.session.history.snapshot.checkpoint_repo_summaries,
                        checkpoint.checkpoint_id,
                    ) {
                        for repo in &summary.repos {
                            ui.small(repo_patch_summary_line(repo));
                        }
                    }
                }
            });
    }

    if let Some(timeline_view) = state.session.history.snapshot.timeline_view.as_ref() {
        ui.separator();
        egui::CollapsingHeader::new(format!("Recent Turns ({})", timeline_view.total_turns))
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for item in timeline_view.items.iter().rev() {
                            let actor = match item.actor {
                                Actor::Assistant => "Assistant",
                                Actor::System => "System",
                                _ => "User",
                            };
                            ui.label(format!(
                                "#{} {} • {}",
                                item.turn_number, actor, item.timestamp
                            ));
                            ui.label(item.content_preview.as_str());
                            ui.add_space(6.0);
                        }
                    });
            });
    }
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

pub fn render_checkpoint_dialog(ctx: &egui::Context, state: &mut AppState) {
    if !state.session.history.show_checkpoint_dialog {
        return;
    }

    egui::Window::new("Create History Checkpoint")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Checkpoint Name:");
            let name_response = ui.text_edit_singleline(&mut state.session.history.checkpoint_name);
            let enter_pressed =
                name_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.session.history.checkpoint_in_flight,
                        egui::Button::new("Create"),
                    )
                    .clicked()
                    || enter_pressed
                {
                    state.spawn_history_checkpoint_async(
                        state.session.history.checkpoint_name.clone(),
                    );
                }

                if ui.button("Cancel").clicked() && !state.session.history.checkpoint_in_flight {
                    state.session.history.show_checkpoint_dialog = false;
                }
            });
        });
}
