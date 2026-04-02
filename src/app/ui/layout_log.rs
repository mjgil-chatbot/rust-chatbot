//! Layout debug logging — writes detailed layout snapshots to disk.
//!
//! Extracted from main_panel.rs to keep the main panel module focused on rendering.

use std::fmt::Write as _;
use std::sync::mpsc;

use chrono::Utc;
use eframe::egui;

use crate::app::state::AppState;
use crate::app::ui::main_panel::LayoutSnapshot;
use crate::debug;

#[derive(Clone)]
pub(crate) struct LayoutLogRequest {
    snapshot: LayoutSnapshot,
    session_id: Option<String>,
    session_name: Option<String>,
    provider: String,
    session_message_count: Option<usize>,
    provider_session_id: Option<String>,
}

pub(crate) struct LayoutLogCompletion {
    pub(crate) result: Result<std::path::PathBuf, String>,
}

pub(crate) fn capture_request(state: &AppState, snapshot: &LayoutSnapshot) -> LayoutLogRequest {
    let (session_id, session_name, session_message_count, provider_session_id) =
        match state.session.current_session.as_ref() {
            Some(session) => (
                Some(session.id.clone()),
                Some(session.name.clone()),
                Some(session.messages.len()),
                state
                    .session
                    .provider
                    .session_id_from_chat(session)
                    .cloned(),
            ),
            None => (None, None, None, None),
        };

    LayoutLogRequest {
        snapshot: snapshot.clone(),
        session_id,
        session_name,
        provider: state.session.provider.as_str().to_string(),
        session_message_count,
        provider_session_id,
    }
}

pub(crate) fn spawn_write(
    request: LayoutLogRequest,
    repaint_ctx: egui::Context,
) -> mpsc::Receiver<LayoutLogCompletion> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = write_layout_log(request).map_err(|err| err.to_string());
        let _ = tx.send(LayoutLogCompletion { result });
        repaint_ctx.request_repaint();
    });
    rx
}

// ui-thread-guard: allow-blocking-start
fn write_layout_log(request: LayoutLogRequest) -> std::io::Result<std::path::PathBuf> {
    let log_dir = debug::log_dir();
    std::fs::create_dir_all(&log_dir)?;

    let timestamp = Utc::now();
    let pid = std::process::id();
    let filename = format!(
        "layout-{}-{}.log",
        timestamp.format("%Y%m%d-%H%M%S%.3f"),
        pid
    );
    let path = log_dir.join(filename);

    let mut output = String::new();
    let _ = writeln!(output, "layout_log_version=1");
    let _ = writeln!(output, "timestamp_utc={}", timestamp.to_rfc3339());
    let snapshot = &request.snapshot;
    let _ = writeln!(output, "pixels_per_point={:.3}", snapshot.pixels_per_point);
    let _ = writeln!(output, "content_width={:.1}", snapshot.content_width);
    let _ = writeln!(
        output,
        "main_available_width_start={:.1}",
        snapshot.main_available_width_start
    );

    if let Some(session_id) = request.session_id.as_deref() {
        let _ = writeln!(output, "session_id={session_id}");
        if let Some(session_name) = request.session_name.as_deref() {
            let _ = writeln!(output, "session_name={session_name}");
        }
        let _ = writeln!(output, "provider={}", request.provider);
        if let Some(message_count) = request.session_message_count {
            let _ = writeln!(output, "session_message_count={message_count}");
        }
        if let Some(provider_session_id) = request.provider_session_id.as_deref() {
            let _ = writeln!(output, "provider_session_id={provider_session_id}");
        }
    } else {
        let _ = writeln!(output, "session_id=<none>");
    }

    write_rect_line(&mut output, "viewport_rect", snapshot.viewport_rect);
    write_rect_line(&mut output, "content_rect", snapshot.content_rect);
    write_rect_line(
        &mut output,
        "globally_used_rect",
        snapshot.globally_used_rect,
    );

    write_optional_rect_line(&mut output, "header_rect", snapshot.header_rect);
    write_optional_rect_line(
        &mut output,
        "system_prompt_rect",
        snapshot.system_prompt_rect,
    );
    write_optional_rect_line(&mut output, "json_summary_rect", snapshot.json_summary_rect);
    write_optional_rect_line(
        &mut output,
        "main_available_rect_before_input",
        Some(snapshot.main_available_rect_before_input),
    );
    let _ = writeln!(
        output,
        "main_available_width_before_input={:.1}",
        snapshot.main_available_width_before_input
    );
    write_optional_rect_line(&mut output, "input_rect", snapshot.input_rect);
    write_optional_rect_line(&mut output, "status_rect", snapshot.status_rect);

    if let Some(input_area) = snapshot.input_area.as_ref() {
        let _ = writeln!(
            output,
            "input_area.available_width={:.1}",
            input_area.available_width
        );
        write_rect_line(
            &mut output,
            "input_area.available_rect_before_wrap",
            input_area.available_rect_before_wrap,
        );
        write_rect_line(&mut output, "input_area.min_rect", input_area.min_rect);
        write_rect_line(&mut output, "input_area.max_rect", input_area.max_rect);
        let _ = writeln!(output, "input_area.row_width={:.1}", input_area.row_width);
        let _ = writeln!(output, "input_area.spacing={:.1}", input_area.spacing);
        let _ = writeln!(
            output,
            "input_area.button_width={:.1}",
            input_area.button_width
        );
        let _ = writeln!(output, "input_area.text_width={:.1}", input_area.text_width);
        write_rect_line(&mut output, "input_area.text_rect", input_area.text_rect);
        write_rect_line(
            &mut output,
            "input_area.button_rect",
            input_area.button_rect,
        );
    }

    if let Some(message_area) = snapshot.message_area.as_ref() {
        let _ = writeln!(
            output,
            "message_area.available_width={:.1}",
            message_area.available_width
        );
        let _ = writeln!(
            output,
            "message_area.available_height={:.1}",
            message_area.available_height
        );
        write_rect_line(
            &mut output,
            "message_area.scroll_inner_rect",
            message_area.scroll_inner_rect,
        );
        let _ = writeln!(
            output,
            "message_area.scroll_content_size=({:.1},{:.1})",
            message_area.scroll_content_size.x, message_area.scroll_content_size.y
        );
        write_rect_line(
            &mut output,
            "message_area.viewport_rect",
            message_area.viewport_rect,
        );
        let _ = writeln!(
            output,
            "message_area.message_count={}",
            message_area.message_count
        );
        let _ = writeln!(
            output,
            "message_area.row_heights_width={:.1}",
            message_area.row_heights_width
        );
        let _ = writeln!(
            output,
            "message_area.row_total_height={:.1}",
            message_area.row_total_height
        );
        let _ = writeln!(
            output,
            "message_area.visible_range={}-{}",
            message_area.visible_range.0, message_area.visible_range.1
        );
        let _ = writeln!(
            output,
            "message_area.message_group_min_width={:.1}",
            message_area.message_group_min_width
        );
        let _ = writeln!(
            output,
            "message_area.claude_content_width_max={:.1}",
            message_area.claude_content_width_max
        );
        let _ = writeln!(
            output,
            "message_area.max_rendered_right={:.1}",
            message_area.max_rendered_right
        );
        let _ = writeln!(
            output,
            "message_area.max_rendered_overflow={:.1}",
            message_area.max_rendered_overflow
        );

        for row in &message_area.message_rows {
            let _ = writeln!(
                output,
                "message_row[{}]: rendered={} is_user={} timestamp_ms={} content_len={} has_command={} raw_height={:.1} height={:.1} top={:.1} bottom={:.1} width={:.1}",
                row.index,
                row.rendered,
                row.is_user,
                row.timestamp_ms,
                row.content_len,
                row.has_command,
                row.raw_height,
                row.height,
                row.top,
                row.bottom,
                message_area.available_width
            );
            if let Some(ref preview) = row.content_preview {
                let escaped = preview
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r");
                let _ = writeln!(output, "message_row.content_preview={}", escaped);
            }
            if let Some(ref preview) = row.command_preview {
                let escaped = preview
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r");
                let _ = writeln!(output, "message_row.command_preview={}", escaped);
            }
            if let Some(rect) = row.message_rect {
                write_rect_line(&mut output, "message_row.message_rect", rect);
            }
            if let Some(rect) = row.command_rect {
                write_rect_line(&mut output, "message_row.command_rect", rect);
            }
        }
    } else {
        let _ = writeln!(output, "message_area=<not_captured>");
    }

    std::fs::write(&path, output)?;
    Ok(path)
}
// ui-thread-guard: allow-blocking-end

fn write_rect_line(output: &mut String, label: &str, rect: egui::Rect) {
    let _ = writeln!(
        output,
        "{label}: min=({:.1},{:.1}) max=({:.1},{:.1}) width={:.1} height={:.1}",
        rect.min.x,
        rect.min.y,
        rect.max.x,
        rect.max.y,
        rect.width(),
        rect.height()
    );
}

fn write_optional_rect_line(output: &mut String, label: &str, rect: Option<egui::Rect>) {
    match rect {
        Some(rect) => write_rect_line(output, label, rect),
        None => {
            let _ = writeln!(output, "{label}: <not_rendered>");
        }
    }
}
