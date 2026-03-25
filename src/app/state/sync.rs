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
    }}
impl AppState {
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
    }}
