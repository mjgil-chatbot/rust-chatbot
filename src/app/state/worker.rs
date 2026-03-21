pub struct AppWorkerState {
    pub processing_chat_ids: HashSet<String>,
    pub local_processing_chat_ids: HashSet<String>,
    pub local_worker_pids: HashMap<String, u32>,
    pub response_cache_by_chat: HashMap<String, String>,
    pub pending_worker_pids: HashMap<String, u32>,
    pub response_started_chat_ids: HashSet<String>,
    pub pending_delete_chat_ids: HashSet<String>,
    pub forge_loop_active_chat_ids: HashSet<String>,
    pub forge_loop_continue_ids: HashMap<String, String>,
    pub forge_loop_acknowledged_ids: HashMap<String, String>,
    pub last_seen_worker_seq: HashMap<String, u64>,
    pub last_seen_worker_events: HashMap<String, (u64, u64)>,
    pub last_worker_poll: Instant,
    pub worker_poll_started_at: Option<Instant>,
    pub worker_discovery_rx: Option<mpsc::Receiver<DiscoveredWorkerJob>>,
    pub worker_poll_rx: Option<mpsc::Receiver<WorkerPollSnapshot>>,
    /// Chat IDs that were finalized locally (worker completed in this process)
    /// but whose StreamEnd socket echo has not yet been processed. Used to
    /// suppress stale disk reloads that would overwrite the in-memory state
    /// before the async save has completed.
    pub locally_finalized_chat_ids: HashSet<String>,
}
