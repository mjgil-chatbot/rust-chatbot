use super::*;

use std::sync::mpsc;

#[test]
fn send_ui_io_result_requests_repaint_when_context_is_present() {
    let (tx, rx) = mpsc::channel();
    let ctx = eframe::egui::Context::default();

    assert!(!ctx.has_requested_repaint());

    send_ui_io_result(
        &tx,
        Some(&ctx),
        UiIoResult::SessionRefreshFailed {
            error: "boom".to_string(),
        },
    );

    match rx.try_recv().expect("queued UI result") {
        UiIoResult::SessionRefreshFailed { error } => assert_eq!(error, "boom"),
        other => panic!("unexpected UI result: {:?}", std::mem::discriminant(&other)),
    }
    assert!(ctx.has_requested_repaint());
}
