impl AppState {
    fn finish_observed_terminal_worker(&mut self, chat_id: &str, status: &str) {
        if self.is_current_chat(chat_id) {
            self.reload_current_session_from_disk(chat_id);
        } else {
            self.spawn_refresh_sessions_async();
        }
        self.worker.clear_processing(chat_id);
        self.log_response_end(chat_id, status);
        self.finalize_pending_delete_if_requested(chat_id);
    }

    fn spawn_worker_poll_in_background(&mut self) {
        if self.worker.worker_poll_rx.is_some() {
            return;
        }

        let provider = self.session.provider;
        let last_seen_worker_events = self.worker.last_seen_worker_events.clone();
        let local_processing_chat_ids = self.worker.local_processing_chat_ids.clone();
        let repaint_ctx = self.session.ui_io.repaint_ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.worker.worker_poll_started_at = Some(Instant::now());
        self.worker.worker_poll_rx = Some(rx);

        if cfg!(test) && repaint_ctx.is_none() {
            if let Some(snapshot) = build_worker_poll_snapshot(
                provider,
                last_seen_worker_events,
                local_processing_chat_ids,
            ) {
                let _ = tx.send(snapshot);
            }
            return;
        }

        thread::spawn(move || {
            if let Some(snapshot) = build_worker_poll_snapshot(
                provider,
                last_seen_worker_events,
                local_processing_chat_ids,
            ) {
                let _ = tx.send(snapshot);
                if let Some(ctx) = repaint_ctx {
                    ctx.request_repaint();
                }
            }
        });
    }

    fn drain_worker_poll_results(&mut self) {
        let Some(rx) = self.worker.worker_poll_rx.take() else {
            return;
        };
        let snapshot = match rx.try_recv() {
            Ok(snapshot) => snapshot,
            Err(mpsc::TryRecvError::Empty) => {
                self.worker.worker_poll_rx = Some(rx);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                background_timing::finish_background_channel_timing(
                    "worker_poll",
                    &mut self.worker.worker_poll_started_at,
                );
                return;
            }
        };

        background_timing::finish_background_channel_timing(
            "worker_poll",
            &mut self.worker.worker_poll_started_at,
        );
        self.apply_worker_poll_snapshot(snapshot);
    }

    fn apply_worker_poll_snapshot(&mut self, snapshot: WorkerPollSnapshot) {
        let active_chat_ids: HashSet<&str> = snapshot
            .active_chat_ids
            .iter()
            .map(String::as_str)
            .collect();

        for item in snapshot.jobs {
            let PolledWorkerJob {
                job,
                worker_pid_alive,
                response,
                result,
                worker_events,
                worker_events_offset,
            } = item;
            let chat_id = &job.chat_id;
            let mut response = response;

            let response_cache_changed = if job.state == WorkerState::Running {
                self.worker.merge_running_response_snapshot(
                    chat_id,
                    response.take(),
                    &worker_events,
                )
            } else {
                false
            };

            if job.state == WorkerState::Running && !worker_pid_alive {
                if !self.should_reconcile_job(job.parent_pid) {
                    continue;
                }
                warn!(chat_id=%chat_id, worker_pid=job.worker_pid, "running worker died");
                let response = self
                    .worker
                    .cached_response(chat_id)
                    .map(str::to_string)
                    .unwrap_or_default();
                if let Err(err) = self.with_latest_session_mut_and_save_now(
                    chat_id,
                    "rust_chatbot.sync_slots_dead_worker",
                    |session| {
                        if !response.is_empty() {
                            apply_stream_response(session, &response);
                        }
                        append_terminal_message_once(
                            session,
                            "Error: worker process died unexpectedly.",
                        );
                    },
                ) {
                    error!(
                        error=%err,
                        chat_id=%chat_id,
                        "failed to persist dead-worker recovery before finalization"
                    );
                    continue;
                }
                self.session.clear_pending_draft_if_matches(chat_id);
                self.worker.mark_locally_finalized(chat_id);
                self.sync_socket.send(&WireMessage::stopped(
                    chat_id,
                    self.session.provider.as_str(),
                    &response,
                ));
                self.finalize_worker_slot(chat_id, None, Some("error"));
                continue;
            }

            let last_seen = self
                .worker
                .last_seen_worker_seq
                .get(chat_id)
                .copied()
                .unwrap_or(0);
            if job.state != WorkerState::Running && job.sequence == last_seen {
                continue;
            }
            if job.state != WorkerState::Running || job.sequence != last_seen {
                self.worker
                    .last_seen_worker_seq
                    .insert(chat_id.clone(), job.sequence);
            }

            if job.state == WorkerState::Running {
                self.worker
                    .last_seen_worker_events
                    .insert(chat_id.clone(), (job.sequence, worker_events_offset));
            }

            let is_local = self.can_control_running_job(job.parent_pid);
            let is_current_chat = self
                .session
                .current_session
                .as_ref()
                .map(|session| session.id == *chat_id)
                .unwrap_or(false);

            match job.state {
                WorkerState::Running => {
                    let is_new_worker = !self.worker.processing_chat_ids.contains(chat_id);
                    self.worker
                        .update_forge_loop_state_from_cache(self.session.provider, chat_id);
                    if !worker_events.is_empty() {
                        for event in &worker_events {
                            let feedback = match event {
                                crate::stream::StreamEvent::TextDelta { .. } => None,
                                crate::stream::StreamEvent::LiveSummary { .. } => None,
                                crate::stream::StreamEvent::ToolStart { tool, .. } => {
                                    Some((format!("Tool started: {tool}"), false))
                                }
                                crate::stream::StreamEvent::ToolEnd { tool, .. } => {
                                    Some((format!("Tool finished: {tool}"), false))
                                }
                                crate::stream::StreamEvent::ThinkingStart { .. } => {
                                    Some(("Thinking started".to_string(), false))
                                }
                                crate::stream::StreamEvent::ThinkingEnd { .. } => {
                                    Some(("Thinking ended".to_string(), false))
                                }
                                crate::stream::StreamEvent::CostUpdate {
                                    input_tokens,
                                    output_tokens,
                                    ..
                                } => Some((
                                    format!(
                                        "Cost update: input={input_tokens} output={output_tokens}"
                                    ),
                                    false,
                                )),
                                crate::stream::StreamEvent::Error { message, .. } => {
                                    Some((format!("Error: {message}"), true))
                                }
                            };

                            let Some((message, is_error)) = feedback else {
                                continue;
                            };

                            if is_current_chat {
                                self.layout.status_feedback = Some(UiStatusFeedback {
                                    at: Instant::now(),
                                    is_error,
                                    message: message.clone(),
                                });
                            }

                            if is_local {
                                self.sync_socket.send(&WireMessage::feedback(
                                    chat_id,
                                    self.session.provider.as_str(),
                                    &message,
                                    is_error,
                                ));
                            }
                        }
                    }
                    debug!(
                        chat_id=%chat_id,
                        worker_pid=job.worker_pid,
                        parent_pid=job.parent_pid,
                        sequence=job.sequence,
                        is_new_worker,
                        is_current_chat,
                        is_local,
                        response_cache_changed,
                        response_len=self.worker.cached_response(chat_id).map(str::len).unwrap_or(0),
                        worker_events_offset,
                        current_message_count=?self.session.current_session.as_ref().map(|session| session.messages.len()),
                        "poll_worker_slots: running worker snapshot"
                    );
                    if !self.worker.response_started_chat_ids.contains(chat_id) {
                        let response_for_log = self
                            .worker
                            .cached_response(chat_id)
                            .filter(|response| !response.is_empty())
                            .map(str::to_string);
                        if let Some(response_for_log) = response_for_log.as_deref() {
                            self.log_response_start_if_needed(chat_id, response_for_log);
                        }
                    }

                    if is_current_chat {
                        if is_new_worker {
                            let reloaded = self.sync_current_session_from_disk_if_safe(chat_id);
                            debug!(
                                chat_id=%chat_id,
                                reloaded,
                                "poll_worker_slots: running worker current-chat sync completed"
                            );
                        }

                        let mut response_changed = false;
                        let provider = self.session.provider;
                        if is_new_worker || response_cache_changed || !worker_events.is_empty() {
                            let cached_response = self
                                .worker
                                .cached_response(chat_id)
                                .map(str::to_string)
                                .unwrap_or_default();
                            if let Some(ref mut session) = self.session.current_session {
                                response_changed = apply_stream_response(session, &cached_response);
                                if result.summary.is_some() {
                                    merge_session_summary(
                                        &mut session.json_summary,
                                        result.summary.as_ref(),
                                        provider,
                                    );
                                }
                            }
                        }
                        debug!(
                            chat_id=%chat_id,
                            response_changed,
                            current_message_count=?self.session.current_session.as_ref().map(|session| session.messages.len()),
                            current_last_is_user=?self.session.current_session.as_ref().and_then(|session| session.messages.last().map(|message| message.is_user)),
                            "poll_worker_slots: running worker response applied to current chat"
                        );
                        if response_changed {
                            self.session.clear_pending_draft_if_matches(chat_id);
                            self.refresh_current_session_tracking();
                        }
                    }

                    self.worker.processing_chat_ids.insert(chat_id.clone());
                    self.worker.pending_worker_pids.remove(chat_id);
                    if is_local {
                        self.worker
                            .local_processing_chat_ids
                            .insert(chat_id.clone());
                        self.worker
                            .local_worker_pids
                            .insert(chat_id.clone(), job.worker_pid);
                    }

                    if is_local && (is_new_worker || response_cache_changed) {
                        let response_for_sync = self
                            .worker
                            .cached_response(chat_id)
                            .map(str::to_string)
                            .unwrap_or_default();
                        self.sync_socket.send(&WireMessage::data(
                            chat_id,
                            self.session.provider.as_str(),
                            &response_for_sync,
                        ));
                    }
                }
                WorkerState::Done => {
                    debug!(
                        chat_id=%chat_id,
                        worker_pid=job.worker_pid,
                        parent_pid=job.parent_pid,
                        sequence=job.sequence,
                        is_current_chat,
                        is_local_reconcile=self.should_reconcile_job(job.parent_pid),
                        "poll_worker_slots: terminal done worker snapshot"
                    );
                    if !self.should_reconcile_job(job.parent_pid) {
                        self.finish_observed_terminal_worker(chat_id, "done");
                        continue;
                    }
                    let response = response.unwrap_or_default();
                    self.worker
                        .update_forge_loop_state(self.session.provider, chat_id, &response);
                    self.log_response_start_if_needed(chat_id, &response);

                    let provider = self.session.provider;

                    self.handle_local_terminal_worker(
                        chat_id,
                        &response,
                        WireMessage::end(
                            chat_id,
                            self.session.provider.as_str(),
                            &response,
                            result.session_id.as_deref(),
                        ),
                        "done",
                        "done",
                        true,
                        |session, response| {
                            let response_changed = apply_stream_response(session, response);
                            replace_provider_session_id(
                                session,
                                provider,
                                result.session_id.as_deref(),
                            );
                            merge_session_summary(
                                &mut session.json_summary,
                                result.summary.as_ref(),
                                provider,
                            );
                            provider.normalize_loaded_session(session);
                            response_changed
                        },
                    );
                    self.maybe_continue_forge_loop(chat_id);
                    info!(chat_id = %chat_id, "worker completed");
                }
                WorkerState::Stopped => {
                    debug!(
                        chat_id=%chat_id,
                        worker_pid=job.worker_pid,
                        parent_pid=job.parent_pid,
                        sequence=job.sequence,
                        is_current_chat,
                        is_local_reconcile=self.should_reconcile_job(job.parent_pid),
                        "poll_worker_slots: terminal stopped worker snapshot"
                    );
                    if !self.should_reconcile_job(job.parent_pid) {
                        self.finish_observed_terminal_worker(chat_id, "stopped");
                        continue;
                    }
                    let response = response.unwrap_or_default();
                    self.worker
                        .update_forge_loop_state(self.session.provider, chat_id, &response);
                    self.log_response_start_if_needed(chat_id, &response);

                    self.handle_local_terminal_worker(
                        chat_id,
                        &response,
                        WireMessage::stopped(chat_id, self.session.provider.as_str(), &response),
                        "stopped",
                        "stopped",
                        false,
                        |session, response| {
                            let response_changed = apply_stream_response(session, response);
                            append_terminal_message_once(session, "Process was stopped by user.");
                            response_changed
                        },
                    );
                }
                WorkerState::Error => {
                    debug!(
                        chat_id=%chat_id,
                        worker_pid=job.worker_pid,
                        parent_pid=job.parent_pid,
                        sequence=job.sequence,
                        is_current_chat,
                        is_local_reconcile=self.should_reconcile_job(job.parent_pid),
                        "poll_worker_slots: terminal error worker snapshot"
                    );
                    if !self.should_reconcile_job(job.parent_pid) {
                        self.finish_observed_terminal_worker(chat_id, "error");
                        continue;
                    }
                    let response = response.unwrap_or_default();
                    self.worker
                        .update_forge_loop_state(self.session.provider, chat_id, &response);
                    self.log_response_start_if_needed(chat_id, &response);
                    let error_message = result
                        .error
                        .unwrap_or_else(|| "worker process failed".to_string());

                    self.handle_local_terminal_worker(
                        chat_id,
                        &response,
                        WireMessage::stopped(chat_id, self.session.provider.as_str(), &response),
                        "error",
                        "error",
                        false,
                        |session, response| {
                            let mut response_changed = false;
                            if !response.is_empty() {
                                response_changed = apply_stream_response(session, response);
                            }
                            append_terminal_message_once(
                                session,
                                &format!("Error: {}", error_message),
                            );
                            response_changed
                        },
                    );
                    error!(chat_id = %chat_id, "worker error");
                }
                WorkerState::Free => {}
            }
        }

        for orphaned in snapshot.orphaned_local_workers {
            warn!(
                chat_id=%orphaned.chat_id,
                error=%orphaned.error_message,
                "orphaned local worker detected"
            );

            if let Err(err) = self.with_latest_session_mut_and_save_now(
                &orphaned.chat_id,
                "rust_chatbot.sync_slots_orphaned_worker",
                |session| {
                    append_terminal_message_once(
                        session,
                        &format!("Error: {}", orphaned.error_message),
                    );
                },
            ) {
                error!(
                    error=%err,
                    chat_id=%orphaned.chat_id,
                    "failed to persist orphaned-worker recovery before cleanup"
                );
                continue;
            }
            self.index_memory_for_chat(&orphaned.chat_id, "error");
            self.worker.clear_processing(&orphaned.chat_id);
            if self.is_current_chat(&orphaned.chat_id) {
                self.layout.should_focus_input = true;
            }
            let _ = std::fs::remove_dir_all(worker::worker_dir(&orphaned.chat_id));
        }

        let stale_processing: Vec<String> = self
            .worker
            .processing_chat_ids
            .iter()
            .filter(|chat_id| !active_chat_ids.contains(chat_id.as_str()))
            .filter(|chat_id| !self.session.ui_io.pending_send_chat_ids.contains(*chat_id))
            .cloned()
            .collect();

        for chat_id in stale_processing {
            debug!(chat_id=%chat_id, "clearing stale processing flag for missing worker spool");
            self.worker.clear_processing(&chat_id);
            if self.is_current_chat(&chat_id) {
                self.layout.should_focus_input = true;
            }
        }
    }

    fn poll_worker_slots(&mut self) {
        self.drain_worker_poll_results();
        if self.worker.worker_poll_rx.is_none() && self.worker.should_poll() {
            self.worker.last_worker_poll = Instant::now();
            self.spawn_worker_poll_in_background();
            self.drain_worker_poll_results();
        }
    }
}

fn build_worker_poll_snapshot(
    provider: ModelProvider,
    last_seen_worker_events: HashMap<String, (u64, u64)>,
    local_processing_chat_ids: HashSet<String>,
) -> Option<WorkerPollSnapshot> {
    let jobs = match worker::list_job_states() {
        Ok(jobs) => jobs,
        Err(err) => {
            warn!(error=%err, "failed to list worker spool states");
            return None;
        }
    };

    let mut active_chat_ids = HashSet::new();
    let mut polled_jobs = Vec::new();

    for job in jobs {
        if !worker_job_provider_matches(&job.provider, provider) {
            continue;
        }

        active_chat_ids.insert(job.chat_id.clone());
        let worker_pid_alive =
            job.state != WorkerState::Running || crate::sync::is_pid_alive(job.worker_pid);
        let result = read_worker_result(&job.chat_id);
        let (response, worker_events, worker_events_offset) = if job.state == WorkerState::Running {
            let (last_seen_events_seq, mut last_offset) = last_seen_worker_events
                .get(&job.chat_id)
                .copied()
                .unwrap_or((0, 0));
            if job.sequence < last_seen_events_seq {
                last_offset = 0;
            }
            let should_read_full_response =
                job.sequence != last_seen_events_seq || last_offset == 0;
            let response = should_read_full_response.then(|| read_worker_response(&job.chat_id));
            let (worker_events, worker_events_offset) =
                worker::read_events_since(&job.chat_id, last_offset);
            (response, worker_events, worker_events_offset)
        } else {
            (Some(read_worker_response(&job.chat_id)), Vec::new(), 0)
        };

        polled_jobs.push(PolledWorkerJob {
            job,
            worker_pid_alive,
            response,
            result,
            worker_events,
            worker_events_offset,
        });
    }

    let orphaned_local_workers = local_processing_chat_ids
        .into_iter()
        .filter(|chat_id| !active_chat_ids.contains(chat_id))
        .filter_map(|chat_id| {
            let result_exists = worker::result_path(&chat_id).exists();
            let req_stale = worker::request_path(&chat_id)
                .metadata()
                .and_then(|metadata| metadata.modified())
                .map(|time| time.elapsed().unwrap_or_default() > std::time::Duration::from_secs(5))
                .unwrap_or(false);
            if !result_exists && !req_stale {
                return None;
            }

            let error_message = read_worker_result(&chat_id)
                .error
                .unwrap_or_else(|| "Worker failed to start.".to_string());
            Some(OrphanedLocalWorker {
                chat_id,
                error_message,
            })
        })
        .collect();

    Some(WorkerPollSnapshot {
        active_chat_ids,
        jobs: polled_jobs,
        orphaned_local_workers,
    })
}

#[cfg(test)]
#[path = "sync_slots_tests.rs"]
mod sync_slots_tests;
