impl AppState {
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
    }}
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
