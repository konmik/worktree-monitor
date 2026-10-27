use super::{Coordinator, idle_percent};
use std::time::{Duration, Instant};

impl Coordinator {
    pub(super) fn try_scan_pass(&mut self, now: Instant) {
        if !self.pending() || !self.passes.ready(now) { return; }
        if idle_percent(self.polling_time.total(now)) <= 97 {
            self.passes.next = Some(now + Duration::from_secs(5));
            return;
        }
        self.scan_pass();
    }
}
