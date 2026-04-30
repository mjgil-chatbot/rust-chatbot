impl AppState {
    /// Spawn background thread to collect worker discovery data (I/O-heavy),
    /// then process results on the main thread via channel.
    fn discover_workers_in_background(&mut self) {
        let provider = self.session.provider;
        let (tx, rx) = mpsc::channel();
        self.worker.worker_discovery_rx = Some(rx);

        thread::spawn(move || {
            let jobs = match worker::list_job_states() {
                Ok(jobs) => jobs,
                Err(err) => {
                    warn!(error=%err, "failed to list worker spool states during startup discovery");
                    return;
                }
            };

            for job in jobs {
                if !worker_job_provider_matches(&job.provider, provider) {
                    continue;
                }
                if job.state == WorkerState::Free {
                    continue;
                }

                // Pre-read all file I/O on this thread.
                let response = read_worker_response(&job.chat_id);
                let result = match job.state {
                    WorkerState::Done | WorkerState::Error => {
                        Some(read_worker_result(&job.chat_id))
                    }
                    _ => None,
                };

                if tx
                    .send(DiscoveredWorkerJob {
                        job,
                        response,
                        result,
                    })
                    .is_err()
                {
                    break; // receiver dropped
                }
            }
        });
    }

    /// Drain background worker discovery results and apply state changes.
    /// Called from the update loop.
    pub(crate) fn drain_worker_discovery_events(&mut self) {
        let Some(rx) = self.worker.worker_discovery_rx.take() else {
            return;
        };

        let mut items = Vec::new();
        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok(item) => items.push(item),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if !disconnected {
            self.worker.worker_discovery_rx = Some(rx);
        }

        for item in items {
            self.enqueue_event(AppEvent::WorkerDiscovery(item));
        }
    }

    pub(crate) fn handle_worker_discovery_event(&mut self, item: DiscoveredWorkerJob) {
        let DiscoveredWorkerJob {
            job,
            response,
            result,
        } = item;

        if !response.is_empty() {
            self.worker
                .cache_response_snapshot(&job.chat_id, response.clone());
        }

        self.worker
            .last_seen_worker_seq
            .insert(job.chat_id.clone(), job.sequence);
        let discovered_sequence = job.sequence;
        let discovered_chat_id = job.chat_id.clone();

        match job.state {
            WorkerState::Running => {
                if crate::sync::is_pid_alive(job.worker_pid) {
                    info!(
                        chat_id = %job.chat_id,
                        worker_pid = job.worker_pid,
                        "discovered running worker"
                    );
                    self.worker.processing_chat_ids.insert(job.chat_id.clone());
                    self.worker.pending_worker_pids.remove(&job.chat_id);
                    self.worker.update_forge_loop_state(
                        self.session.provider,
                        &job.chat_id,
                        &response,
                    );
                    self.worker.last_seen_worker_seq.remove(&job.chat_id);

                    if self.can_control_running_job(job.parent_pid) {
                        self.worker
                            .local_processing_chat_ids
                            .insert(job.chat_id.clone());
                        self.worker
                            .local_worker_pids
                            .insert(job.chat_id.clone(), job.worker_pid);
                    }
                } else {
                    if !self.should_reconcile_job(job.parent_pid) {
                        return;
                    }
                    warn!(
                        chat_id = %job.chat_id,
                        worker_pid = job.worker_pid,
                        "discovered dead worker, recovering partial output"
                    );

                    if let Err(err) = self.with_latest_session_mut_and_save_now(
                        &job.chat_id,
                        "rust_chatbot.worker_discovery_dead_worker",
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
                            chat_id=%job.chat_id,
                            "failed to persist discovered dead-worker recovery before finalization"
                        );
                        return;
                    }
                    self.worker.mark_locally_finalized(&job.chat_id);
                    self.sync_socket.send(&WireMessage::stopped(
                        &job.chat_id,
                        self.session.provider.as_str(),
                        &response,
                    ));
                    self.log_response_start_if_needed(&job.chat_id, &response);
                    self.log_response_end(&job.chat_id, "error");
                    self.finalize_worker_slot(&job.chat_id, None, Some("error"));
                }
            }
            WorkerState::Done => {
                if !self.should_reconcile_job(job.parent_pid) {
                    return;
                }
                self.worker.update_forge_loop_state(
                    self.session.provider,
                    &job.chat_id,
                    &response,
                );
                info!(chat_id = %job.chat_id, "discovered completed worker, finalizing");
                self.log_response_start_if_needed(&job.chat_id, &response);
                let result = result.unwrap_or_else(|| WorkerResult {
                    session_id: None,
                    summary: None,
                    error: None,
                });
                let provider = self.session.provider;

                self.handle_local_terminal_worker(
                    &job.chat_id,
                    &response,
                    WireMessage::end(
                        &job.chat_id,
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
                self.worker
                    .last_seen_worker_seq
                    .insert(discovered_chat_id.clone(), discovered_sequence);
                self.maybe_continue_forge_loop(&job.chat_id);
            }
            WorkerState::Stopped => {
                if !self.should_reconcile_job(job.parent_pid) {
                    return;
                }
                self.worker.update_forge_loop_state(
                    self.session.provider,
                    &job.chat_id,
                    &response,
                );
                self.log_response_start_if_needed(&job.chat_id, &response);

                self.handle_local_terminal_worker(
                    &job.chat_id,
                    &response,
                    WireMessage::stopped(&job.chat_id, self.session.provider.as_str(), &response),
                    "stopped",
                    "stopped",
                    false,
                    |session, response| {
                        let response_changed = apply_stream_response(session, response);
                        append_terminal_message_once(session, "Process was stopped by user.");
                        response_changed
                    },
                );
                self.worker
                    .last_seen_worker_seq
                    .insert(discovered_chat_id.clone(), discovered_sequence);
            }
            WorkerState::Error => {
                if !self.should_reconcile_job(job.parent_pid) {
                    return;
                }
                self.worker.update_forge_loop_state(
                    self.session.provider,
                    &job.chat_id,
                    &response,
                );
                self.log_response_start_if_needed(&job.chat_id, &response);
                let result = result.unwrap_or_else(|| WorkerResult {
                    session_id: None,
                    summary: None,
                    error: None,
                });
                let error_message = result
                    .error
                    .unwrap_or_else(|| "worker process failed".to_string());

                self.handle_local_terminal_worker(
                    &job.chat_id,
                    &response,
                    WireMessage::stopped(&job.chat_id, self.session.provider.as_str(), &response),
                    "error",
                    "error",
                    false,
                    |session, response| {
                        let mut response_changed = false;
                        if !response.is_empty() {
                            response_changed = apply_stream_response(session, response);
                        }
                        append_terminal_message_once(session, &format!("Error: {}", error_message));
                        response_changed
                    },
                );
                self.worker
                    .last_seen_worker_seq
                    .insert(discovered_chat_id.clone(), discovered_sequence);
            }
            WorkerState::Free => {}
        }
    }
}
