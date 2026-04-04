use eframe::egui;

use crate::summary::JsonSummary;

pub(super) fn render_json_summary_area(
    ui: &mut egui::Ui,
    summary: Option<&JsonSummary>,
) -> egui::Rect {
    let response = ui.group(|ui| {
        ui.label("Conversation Summary:");
        if let Some(summary) = summary {
            ui.horizontal(|ui| {
                ui.label(format!("Responses: {}", summary.response_count));
                if let Some(ref model) = summary.model {
                    ui.label(format!("Model: {}", model));
                }
            });

            if let Some(ref session_id) = summary.session_id {
                ui.label(format!(
                    "Session: {}...",
                    &session_id[..8.min(session_id.len())]
                ));
            }

            ui.horizontal(|ui| {
                ui.label(format!("Input: {} tokens", summary.total_input_tokens));
                ui.label(format!("Output: {} tokens", summary.total_output_tokens));
            });

            ui.horizontal(|ui| {
                if summary.total_duration_ms > 0 {
                    ui.label(format!("Total time: {}ms", summary.total_duration_ms));
                }
                if summary.total_cost_usd > 0.0 {
                    ui.label(format!("Total cost: ${:.4}", summary.total_cost_usd));
                }
            });

            if !summary.tools_used.is_empty() {
                ui.label(format!("Tools: {}", summary.tools_used.join(", ")));
            }

            if !summary.turn_history.is_empty() {
                ui.separator();
                ui.label(format!("Turn History ({} calls):", summary.turn_history.len()));

                use egui::text::LayoutJob;
                let mono = egui::TextFormat {
                    font_id: egui::FontId::monospace(11.0),
                    ..Default::default()
                };

                let header = format!(
                    "{:>4}  {:>8}  {:>8}  {:>10}  {:>10}  {:>8}  {:>9}  {:>6}",
                    "#", "Output", "Input", "CacheWr", "CacheRd", "Ctx", "Cost", "Dur"
                );
                let mut job = LayoutJob::default();
                job.append(&header, 0.0, mono.clone());
                ui.label(job);

                let dim = egui::TextFormat {
                    font_id: egui::FontId::monospace(10.0),
                    color: ui.visuals().weak_text_color(),
                    ..Default::default()
                };

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (i, turn) in summary.turn_history.iter().enumerate() {
                            let line = format!(
                                "{:>4}  {:>8}  {:>8}  {:>10}  {:>10}  {:>8}  ${:>8.2}  {:>5}s  {}tc",
                                i + 1,
                                fmt_tokens(turn.output_tokens),
                                fmt_tokens(turn.input_tokens),
                                fmt_tokens(turn.cache_creation_tokens),
                                fmt_tokens(turn.cache_read_tokens),
                                fmt_tokens(turn.context_window_tokens),
                                turn.cost_usd,
                                turn.duration_ms / 1000,
                                turn.tool_calls.len(),
                            );
                            let mut job = LayoutJob::default();
                            job.append(&line, 0.0, mono.clone());
                            ui.label(job);

                            for tc in &turn.tool_calls {
                                let preview = if tc.input_preview.len() > 80 {
                                    let mut end = 80.min(tc.input_preview.len());
                                    while end > 0 && !tc.input_preview.is_char_boundary(end) {
                                        end -= 1;
                                    }
                                    format!("{}...", &tc.input_preview[..end])
                                } else {
                                    tc.input_preview.clone()
                                };
                                let tc_line = format!("        {} {}", tc.name, preview);
                                let mut job = LayoutJob::default();
                                job.append(&tc_line, 0.0, dim.clone());
                                ui.label(job);
                            }
                        }
                    });
            }
        } else {
            ui.label("No conversation data available yet.");
        }
    });
    response.response.rect
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
