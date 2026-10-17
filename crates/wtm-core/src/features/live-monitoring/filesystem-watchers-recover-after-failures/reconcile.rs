use super::Coordinator;
use crate::watch;

impl Coordinator {
    pub(super) fn recover_watch_status(&mut self, status: watch::WatchStatus) {
        if status.degraded {
            self.monitors_failed = true;
        } else if status.full || self.monitors_failed {
            self.monitors_failed = false;
            self.enqueue(watch::WatchBatch { full: true, ..Default::default() });
        }
    }
}
