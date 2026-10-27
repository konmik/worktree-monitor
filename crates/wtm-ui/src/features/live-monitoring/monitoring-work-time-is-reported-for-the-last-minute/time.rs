use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

pub(crate) fn idle_percent(polling_time_last_minute: Duration) -> u128 {
    100 - (polling_time_last_minute.as_millis().saturating_add(300) / 600).min(100)
}

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
        self.state.lock().unwrap_or_else(|error| error.into_inner()).completed.push((start, end));
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
