use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct PollingTime {
    completed: Vec<(Instant, Instant)>,
}

impl PollingTime {
    pub(super) fn record(&mut self, start: Instant, end: Instant) {
        self.completed.push((start, end));
    }

    pub(super) fn total(&mut self, now: Instant) -> Duration {
        let window = now.checked_sub(Duration::from_secs(60)).unwrap_or(now);
        self.completed.retain(|(_, end)| *end > window);
        self.completed.iter().map(|(start, end)| {
            end.min(&now).saturating_duration_since(*start.max(&window))
        }).sum()
    }
}
