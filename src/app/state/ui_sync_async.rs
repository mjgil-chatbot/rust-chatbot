use super::*;
impl AppState {
    pub(crate) fn poll_shared_session_changes_async(&mut self) {
        let sessions_changed = self
            .sync_shm
            .as_mut()
            .map(|shm| shm.sessions_changed())
            .unwrap_or(false);
        if sessions_changed {
            debug!("sync shm: sessions changed notification");
            // Passive refresh: sidebar-only. Explicit current-session reloads
            // come through StreamBegin / explicit user actions instead.
            self.spawn_refresh_sessions_async();
        }
    }}
impl AppState {
    pub(crate) fn drain_sync_events_async(&mut self) {
        let events: Vec<SyncEvent> = self.sync_events.try_iter().collect();

        for event in events {
            self.enqueue_event(AppEvent::Sync(event));
        }
    }}
