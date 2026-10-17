use super::WorktreeKey;
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub(super) const PASS_DELAY: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct ScanPasses {
    pub(super) changed: HashSet<WorktreeKey>,
    pub(super) next: Option<Instant>,
}

impl ScanPasses {
    pub(super) fn ready(&self, now: Instant) -> bool { self.next.is_none_or(|next| now >= next) }

    pub(super) fn finish(&mut self, now: Instant) { self.next = Some(now + PASS_DELAY); }
}
