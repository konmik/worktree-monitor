use super::Coordinator;
use std::time::{Duration, Instant};

impl Coordinator {
    pub(super) fn try_scan_pass(&mut self, now: Instant) {
        if !self.pending() || !self.passes.ready(now) { return; }
        if self.polling_time.idle(now).percent() <= 97 {
            self.passes.next = Some(now + Duration::from_secs(5));
            return;
        }
        self.scan_pass();
    }
}
