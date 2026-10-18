use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct Cancel {
    state: Arc<(Mutex<CancelState>, Condvar)>,
}

#[derive(Default)]
struct CancelState {
    cancelled: bool,
    generation: u64,
}

impl Cancel {
    pub fn new() -> Self { Self::default() }

    pub fn cancel(&self) {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.cancelled = true;
        state.generation = state.generation.wrapping_add(1);
        wake.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.0.lock().unwrap_or_else(|error| error.into_inner()).cancelled
    }

    pub fn wait(&self, duration: Duration) {
        let generation = self.generation();
        self.wait_since(generation, duration);
    }

    pub(crate) fn wait_since(&self, generation: u64, duration: Duration) {
        let (lock, wake) = &*self.state;
        let state = lock.lock().unwrap_or_else(|error| error.into_inner());
        let _ = wake.wait_timeout_while(state, duration,
            |state| !state.cancelled && state.generation == generation);
    }

    pub(crate) fn generation(&self) -> u64 {
        self.state.0.lock().unwrap_or_else(|error| error.into_inner()).generation
    }

    pub(crate) fn wake(&self) {
        let mut state = self.state.0.lock().unwrap_or_else(|error| error.into_inner());
        state.generation = state.generation.wrapping_add(1);
        self.state.1.notify_all();
    }
}
