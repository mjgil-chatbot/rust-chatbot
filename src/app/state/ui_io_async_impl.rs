fn send_ui_io_result(
    tx: &mpsc::Sender<UiIoResult>,
    repaint_ctx: Option<&eframe::egui::Context>,
    result: UiIoResult,
) {
    if tx.send(result).is_ok() {
        if let Some(ctx) = repaint_ctx {
            ctx.request_repaint();
        }
    }
}
impl AppState {
    pub(crate) fn spawn_reload_current_session_async(&mut self, session_id: String) {
        let provider = self.session.provider;
        let tx = self.session.ui_io.tx.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();

        thread::spawn(move || {
            let result = (|| -> Result<SelectedSessionLoad> {
                let data_manager = DataManager::new(provider)?;
                let session = data_manager.load_session(&session_id)?;
                load_selected_session(
                    provider,
                    Some(session_id.clone()),
                    Some(session),
                    true,
                    Some(session_id.clone()),
                )
            })();

            match result {
                Ok(load) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SelectedSessionLoad {
                        request_id: None,
                        requested_session_id: Some(session_id.clone()),
                        load,
                    },
                ),
                Err(err) => send_ui_io_result(
                    &tx,
                    repaint_ctx.as_ref(),
                    UiIoResult::SessionLoadFailed {
                        request_id: None,
                        session_id: session_id.clone(),
                        requested_session_id: Some(session_id),
                        error: format!("reload current session failed: {err:#}"),
                    },
                ),
            }
        });
    }}
