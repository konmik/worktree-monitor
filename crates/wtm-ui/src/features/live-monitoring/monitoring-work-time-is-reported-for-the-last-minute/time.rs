use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

use super::idle_time::IdlePercentage;

#[derive(Clone, Default)]
pub(super) struct PollingTime {
    state: Arc<Mutex<ReadingTime>>,
}

#[derive(Default)]
struct ReadingTime {
    completed: Vec<(Instant, Instant)>,
    active: Option<Instant>,
}

pub(super) struct Reading {
    polling_time: PollingTime,
    start: Instant,
}

impl PollingTime {
    pub(super) fn start(&self) -> Reading {
        let start = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        assert!(state.active.is_none(), "Reading operations must be sequential");
        state.active = Some(start);
        Reading { polling_time: self.clone(), start }
    }

    #[cfg(test)]
    pub(super) fn record(&self, start: Instant, end: Instant) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.completed.push((start, end));
    }

    pub(super) fn idle(&self, now: Instant) -> IdlePercentage {
        IdlePercentage::from_reading_time(self.total(now))
    }

    pub(super) fn total(&self, now: Instant) -> Duration {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let window = now.checked_sub(Duration::from_secs(60)).unwrap_or(now);
        state.completed.retain(|(_, end)| *end > window);
        let completed: Duration = state.completed.iter().map(|(start, end)| {
            end.min(&now).saturating_duration_since(*start.max(&window))
        }).sum();
        completed + state.active.map_or(Duration::ZERO, |start| now.saturating_duration_since(start.max(window)))
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        let end = Instant::now();
        let mut state = self.polling_time.state.lock().unwrap_or_else(|error| error.into_inner());
        state.active = None;
        state.completed.push((self.start, end));
    }
}

#[cfg(test)]
#[path = "time.test.rs"]
mod tests;
