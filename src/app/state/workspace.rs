impl SessionState {
    pub(crate) fn effective_workspace_dir(&self) -> Option<String> {
        self.current_session
            .as_ref()
            .and_then(|session| session.launched_from.clone())
            .or_else(|| self.workspace_dir.clone())
    }}
impl SessionState {
    pub(crate) fn workspace_selection_locked(&self) -> bool {
        self.current_session
            .as_ref()
            .map(|session| session.messages.iter().any(|message| message.is_user))
            .unwrap_or(false)
    }}
impl SessionState {
    pub(crate) fn runsc_selection_locked(&self) -> bool {
        self.current_session
            .as_ref()
            .map(|session| session.messages.iter().any(|message| message.is_user))
            .unwrap_or(false)
    }}
impl AppState {
    fn next_path_resolution_request_id(&mut self) -> u64 {
        self.session.latest_path_resolution_request_id = self
            .session
            .latest_path_resolution_request_id
            .saturating_add(1);
        self.session.latest_path_resolution_request_id
    }}
impl AppState {
    fn normalize_path_request(&mut self, request: PathResolutionRequest) {
        if cfg!(test) && self.session.ui_io.repaint_ctx.is_none() {
            let request_id = self.next_path_resolution_request_id();
            let result = resolve_path_request(request_id, request);
            self.handle_path_resolution_result(result);
            return;
        }

        let request_id = self.next_path_resolution_request_id();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.session.path_resolution_started_at = Some(Instant::now());
        self.session.path_resolution_rx = Some(rx);
        thread::spawn(move || {
            let result = resolve_path_request(request_id, request);
            let _ = tx.send(result);
            if let Some(ctx) = repaint_ctx {
                ctx.request_repaint();
            }
        });
    }}
impl AppState {
    pub(crate) fn drain_path_resolution_results(&mut self) {
        let Some(rx) = self.session.path_resolution_rx.take() else {
            return;
        };

        match rx.try_recv() {
            Ok(result) => {
                background_timing::finish_background_channel_timing(
                    "path_resolution",
                    &mut self.session.path_resolution_started_at,
                );
                self.handle_path_resolution_result(result);
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.session.path_resolution_rx = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                background_timing::finish_background_channel_timing(
                    "path_resolution",
                    &mut self.session.path_resolution_started_at,
                );
            }
        }
    }}
impl AppState {
    fn handle_path_resolution_result(&mut self, result: PathResolutionResult) {
        if result.request_id < self.session.latest_path_resolution_request_id {
            return;
        }

        match result.request {
            PathResolutionRequest::ApplyWorkspaceInput { .. } => match result.normalized_path {
                Ok(path) => self.apply_workspace_dir_normalized(path),
                Err(error) => {
                    self.layout.status_feedback = Some(UiStatusFeedback {
                        at: Instant::now(),
                        is_error: true,
                        message: format!("Workspace error: {error}"),
                    });
                }
            },
            PathResolutionRequest::SelectWorkspaceDir { .. } => match result.normalized_path {
                Ok(path) => self.apply_workspace_dir_normalized(path),
                Err(error) => {
                    self.layout.status_feedback = Some(UiStatusFeedback {
                        at: Instant::now(),
                        is_error: true,
                        message: format!("Workspace error: {error}"),
                    });
                }
            },
            PathResolutionRequest::ApplyRunscInput { .. } => match result.normalized_path {
                Ok(path) => self.apply_runsc_dir_normalized(path),
                Err(error) => {
                    self.layout.status_feedback = Some(UiStatusFeedback {
                        at: Instant::now(),
                        is_error: true,
                        message: format!("Runsc error: {error}"),
                    });
                }
            },
            PathResolutionRequest::PreviewRunscInput { .. } => match result.normalized_path {
                Ok(path) => self.session.runsc_input_text = path,
                Err(error) => {
                    self.layout.status_feedback = Some(UiStatusFeedback {
                        at: Instant::now(),
                        is_error: true,
                        message: format!("Runsc error: {error}"),
                    });
                }
            },
            PathResolutionRequest::SelectRunscDir { .. } => match result.normalized_path {
                Ok(path) => self.apply_runsc_dir_normalized(path),
                Err(error) => {
                    self.layout.status_feedback = Some(UiStatusFeedback {
                        at: Instant::now(),
                        is_error: true,
                        message: format!("Runsc error: {error}"),
                    });
                }
            },
            PathResolutionRequest::ReplaceRunscDir { existing_path, .. } => {
                match result.normalized_path {
                    Ok(path) => self.apply_replaced_runsc_dir(&existing_path, path),
                    Err(error) => {
                        self.layout.status_feedback = Some(UiStatusFeedback {
                            at: Instant::now(),
                            is_error: true,
                            message: format!("Runsc error: {error}"),
                        });
                    }
                }
            }
        }
    }}
impl AppState {
    fn clear_workspace_completion_state(&mut self) {
        self.session.workspace_completions.clear();
        self.session.workspace_completion_query = None;
        self.session.workspace_completion_rx = None;
        self.session.workspace_completion_started_at = None;
    }}
impl AppState {
    fn persist_workspace_history(&self) {
        if let Err(err) = crate::app::workspace_picker::save_provider_workspace_snapshot(
            self.session.provider,
            self.session.workspace_dir.as_deref(),
            &self.session.workspace_recent_dirs,
        ) {
            warn!(
                provider = %self.session.provider.as_str(),
                error = %err,
                "failed to persist workspace history"
            );
        }
    }}
impl AppState {
    fn sync_workspace_state_from_current_session(&mut self) {
        if let Some(path) = self
            .session
            .current_session
            .as_ref()
            .and_then(|session| session.launched_from.clone())
        {
            crate::app::workspace_picker::push_recent_dir(
                &mut self.session.workspace_recent_dirs,
                &path,
            );
            self.session.workspace_dir = Some(path.clone());
            self.session.workspace_input_text = path;
            self.clear_workspace_completion_state();
            self.persist_workspace_history();
            self.refresh_history();
            return;
        }

        self.session.workspace_input_text = self.session.workspace_dir.clone().unwrap_or_default();
        self.clear_workspace_completion_state();
        self.refresh_history();
    }}
impl AppState {
    pub(crate) fn drain_workspace_completion_results(&mut self) {
        let result = self
            .session
            .workspace_completion_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some(result) = result {
            background_timing::finish_background_channel_timing(
                "workspace_completion",
                &mut self.session.workspace_completion_started_at,
            );
            if self.session.workspace_completion_query.as_deref() == Some(result.query.as_str()) {
                self.session.workspace_completions = result.matches;
            }
            self.session.workspace_completion_rx = None;
        }
    }}
impl AppState {
    pub(crate) fn request_workspace_completion_refresh(&mut self) {
        let query = self.session.workspace_input_text.trim().to_string();
        if query.is_empty() {
            self.clear_workspace_completion_state();
            return;
        }

        if self.session.workspace_completion_query.as_deref() == Some(query.as_str())
            && (self.session.workspace_completion_rx.is_some()
                || !self.session.workspace_completions.is_empty())
        {
            return;
        }

        self.session.workspace_completion_query = Some(query.clone());
        self.session.workspace_completions.clear();

        if cfg!(test) && self.session.ui_io.repaint_ctx.is_none() {
            self.session.workspace_completions =
                crate::app::workspace_picker::workspace_completion_matches(&query, 8);
            self.session.workspace_completion_rx = None;
            return;
        }

        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.session.workspace_completion_started_at = Some(Instant::now());
        self.session.workspace_completion_rx = Some(rx);
        thread::spawn(move || {
            let matches = crate::app::workspace_picker::workspace_completion_matches(&query, 8);
            let _ = tx.send(WorkspaceCompletionResult { query, matches });
            if let Some(ctx) = repaint_ctx {
                ctx.request_repaint();
            }
        });
    }}
impl AppState {
    pub(crate) fn workspace_completion_matches(&self) -> Vec<String> {
        if self.session.workspace_completion_query.as_deref()
            != Some(self.session.workspace_input_text.trim())
        {
            return Vec::new();
        }
        self.session.workspace_completions.clone()
    }}
impl AppState {
    pub(crate) fn autocomplete_workspace_input(&mut self) {
        if let Some(completed) = crate::app::workspace_picker::autocomplete_workspace_input_from_matches(
            &self.session.workspace_input_text,
            &self.session.workspace_completions,
        )
        .or_else(|| {
            crate::app::workspace_picker::autocomplete_workspace_input(
                &self.session.workspace_input_text,
            )
        }) {
            self.session.workspace_input_text = completed;
            self.request_workspace_completion_refresh();
        }
    }}
impl AppState {
    pub(crate) fn apply_workspace_input(&mut self) {
        self.normalize_path_request(PathResolutionRequest::ApplyWorkspaceInput {
            input: self.session.workspace_input_text.clone(),
        });
    }}
impl AppState {
    pub(crate) fn select_workspace_dir(&mut self, path: String) {
        if self.session.workspace_selection_locked() {
            self.layout.status_feedback = Some(UiStatusFeedback {
                at: Instant::now(),
                is_error: true,
                message: "Workspace is locked after the first message".to_string(),
            });
            return;
        }

        self.normalize_path_request(PathResolutionRequest::SelectWorkspaceDir { path });
    }}
impl AppState {
    fn apply_workspace_dir_normalized(&mut self, normalized: String) {
        crate::app::workspace_picker::push_recent_dir(
            &mut self.session.workspace_recent_dirs,
            &normalized,
        );
        self.session.workspace_dir = Some(normalized.clone());
        self.session.workspace_input_text = normalized.clone();
        self.clear_workspace_completion_state();
        self.persist_workspace_history();

        let mut session_to_save = None;
        if let Some(current_session) = self.session.current_session.as_mut() {
            if current_session.launched_from.as_deref() != Some(normalized.as_str()) {
                current_session.launched_from = Some(normalized);
                current_session.updated_at = chrono::Utc::now();
                session_to_save = Some(current_session.clone());
            }
        }

        if self.session.current_session.is_some() {
            self.session.sync_current_view(&mut self.layout);
            self.refresh_current_session_tracking();
        }

        if let Some(session) = session_to_save {
            self.spawn_save_session_async(session, false, None, false);
        }

        self.refresh_history();
    }}
impl AppState {
    pub(crate) fn apply_runsc_input(&mut self) {
        self.normalize_path_request(PathResolutionRequest::ApplyRunscInput {
            input: self.session.runsc_input_text.clone(),
        });
    }}
impl AppState {
    pub(crate) fn preview_runsc_input(&mut self, path: String) {
        self.normalize_path_request(PathResolutionRequest::PreviewRunscInput { path });
    }}
impl AppState {
    pub(crate) fn apply_runsc_image_input(&mut self) {
        let runsc_image = self.session.runsc_image_input_text.trim().to_string();
        self.session.runsc_image_input_text = runsc_image.clone();
        let runsc_image = (!runsc_image.is_empty()).then_some(runsc_image);

        let mut session_to_save = None;
        if let Some(current) = self.session.current_session.as_mut() {
            if current.runsc_image != runsc_image {
                current.runsc_image = runsc_image;
                current.updated_at = chrono::Utc::now();
                session_to_save = Some(current.clone());
            }
        }

        if let Some(session) = session_to_save {
            self.spawn_save_session_async(session, false, None, false);
        }
    }}
impl AppState {
    pub(crate) fn select_runsc_dir(&mut self, path: String) {
        self.normalize_path_request(PathResolutionRequest::SelectRunscDir { path });
    }}
impl AppState {
    fn apply_runsc_dir_normalized(&mut self, normalized: String) {
        let first_runsc_dir = self.session.runsc_dirs.is_empty();
        crate::app::workspace_picker::push_recent_dir(
            &mut self.session.runsc_recent_dirs,
            &normalized,
        );
        if first_runsc_dir {
            crate::app::workspace_picker::push_recent_dir(
                &mut self.session.workspace_recent_dirs,
                &normalized,
            );
            self.session.workspace_dir = Some(normalized.clone());
            self.session.workspace_input_text = normalized.clone();
            self.persist_workspace_history();
        }
        if !self.session.runsc_dirs.contains(&normalized) {
            self.session.runsc_dirs.push(normalized.clone());
        }
        self.session.runsc_input_text = normalized.clone();

        let mut session_to_save = None;
        // Sync runsc state to current_session so it's included in the cloned
        // session when spawning background send operations.
        if let Some(ref mut current) = self.session.current_session {
            let mut changed = false;
            if first_runsc_dir && current.launched_from.as_deref() != Some(normalized.as_str()) {
                current.launched_from = Some(normalized.clone());
                changed = true;
            }
            if !current.runsc_dirs.contains(&normalized) {
                current.runsc_dirs.push(normalized.clone());
                changed = true;
            }
            if changed {
                current.updated_at = chrono::Utc::now();
                session_to_save = Some(current.clone());
            }
        }

        if let Some(session) = session_to_save {
            self.spawn_save_session_async(session, false, None, false);
        }
    }}
impl AppState {
    pub(crate) fn replace_runsc_dir(&mut self, existing_path: &str, replacement: String) {
        self.normalize_path_request(PathResolutionRequest::ReplaceRunscDir {
            existing_path: existing_path.to_string(),
            replacement,
        });
    }}
impl AppState {
    fn apply_replaced_runsc_dir(&mut self, existing_path: &str, normalized: String) {
        let Some(existing_index) = self
            .session
            .runsc_dirs
            .iter()
            .position(|path| path == existing_path)
        else {
            self.layout.status_feedback = Some(UiStatusFeedback {
                at: Instant::now(),
                is_error: true,
                message: "Runsc row is unavailable".to_string(),
            });
            return;
        };

        let replacing_first_dir = existing_index == 0;
        crate::app::workspace_picker::push_recent_dir(
            &mut self.session.runsc_recent_dirs,
            &normalized,
        );

        match self
            .session
            .runsc_dirs
            .iter()
            .position(|path| path == &normalized)
        {
            Some(index) if index != existing_index => {
                self.session.runsc_dirs.remove(existing_index);
            }
            _ => {
                self.session.runsc_dirs[existing_index] = normalized.clone();
            }
        }
        self.session.runsc_input_text = normalized;

        let mut session_to_save = None;
        let mut changed = false;
        let mut first_dir_state = None;

        if let Some(current) = self.session.current_session.as_mut() {
            if current.runsc_dirs != self.session.runsc_dirs {
                current.runsc_dirs = self.session.runsc_dirs.clone();
                changed = true;
            }

            if replacing_first_dir {
                if let Some(first_dir) = self.session.runsc_dirs.first().cloned() {
                    first_dir_state = Some((first_dir, current.launched_from.clone()));
                }
            }
        }

        if let Some((first_dir, launched_from)) = first_dir_state {
            crate::app::workspace_picker::push_recent_dir(
                &mut self.session.workspace_recent_dirs,
                &first_dir,
            );
            self.session.workspace_dir = Some(first_dir.clone());
            self.session.workspace_input_text = first_dir.clone();
            self.persist_workspace_history();

            if launched_from.as_deref() != Some(first_dir.as_str()) {
                if let Some(current) = self.session.current_session.as_mut() {
                    current.launched_from = Some(first_dir);
                    changed = true;
                }
            }
        }

        if changed {
            if let Some(current) = self.session.current_session.as_mut() {
                current.updated_at = chrono::Utc::now();
                session_to_save = Some(current.clone());
            }
        }

        if let Some(session) = session_to_save {
            self.spawn_save_session_async(session, false, None, false);
        }
    }}
impl AppState {
    pub(crate) fn remove_runsc_dir(&mut self, path: &str) {
        self.session.runsc_dirs.retain(|p| p != path);

        let mut session_to_save = None;
        // Also remove from current_session to keep them in sync
        if let Some(ref mut current) = self.session.current_session {
            let original_len = current.runsc_dirs.len();
            current.runsc_dirs.retain(|p| p != path);
            if current.runsc_dirs.len() != original_len {
                current.updated_at = chrono::Utc::now();
                session_to_save = Some(current.clone());
            }
        }

        if let Some(session) = session_to_save {
            self.spawn_save_session_async(session, false, None, false);
        }
    }}
fn resolve_path_request(request_id: u64, request: PathResolutionRequest) -> PathResolutionResult {
    let normalized_path = match &request {
        PathResolutionRequest::ApplyWorkspaceInput { input } => {
            resolve_workspace_input_request(input)
        }
        PathResolutionRequest::SelectWorkspaceDir { path }
        | PathResolutionRequest::PreviewRunscInput { path }
        | PathResolutionRequest::SelectRunscDir { path } => {
            crate::app::workspace_picker::normalize_workspace_input(path)
                .map_err(|err| format!("{err}"))
        }
        PathResolutionRequest::ApplyRunscInput { input } => {
            crate::app::workspace_picker::normalize_workspace_input(input)
                .map_err(|err| format!("{err}"))
        }
        PathResolutionRequest::ReplaceRunscDir { replacement, .. } => {
            crate::app::workspace_picker::normalize_workspace_input(replacement)
                .map_err(|err| format!("{err}"))
        }
    };

    PathResolutionResult {
        request_id,
        request,
        normalized_path,
    }
}
fn resolve_workspace_input_request(input: &str) -> Result<String, String> {
    let direct_match = crate::app::workspace_picker::normalize_workspace_input(input);
    match direct_match {
        Ok(path) => Ok(path),
        Err(err) => crate::app::workspace_picker::autocomplete_workspace_input(input)
            .ok_or_else(|| format!("{err}"))
            .and_then(|completed| {
                crate::app::workspace_picker::normalize_workspace_input(&completed)
                    .map_err(|error| format!("{error}"))
            }),
    }
}
