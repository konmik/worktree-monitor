
use wtm_ui::order::Order;
use wtm_ui::visibility::Visibility;
use std::time::{Duration, Instant};
use super::runtime::State;
pub(super) fn reload_preferences(state: &mut State) {
    if Instant::now() >= state.ordering.next_check {
        match Order::read(&state.ordering.path) {
            Ok(order) => {
                if !state.ordering.write_failed || order != state.ordering.value { state.ordering.warning = None; state.ordering.write_failed = false; }
                state.ordering.value = order;
            }
            Err(error) => { state.ordering.warning = Some(format!("Cannot load presentation order: {error}")); }
        }
        state.ordering.next_check = Instant::now() + Duration::from_secs(1);
    }
    if Instant::now() >= state.worktree_visibility.next_check {
        match Visibility::read(&state.worktree_visibility.path) {
            Ok(visibility) => {
                if !state.worktree_visibility.write_failed || visibility != state.worktree_visibility.value {
                    state.worktree_visibility.warning = None;
                    state.worktree_visibility.write_failed = false;
                }
                state.worktree_visibility.value = visibility;
            }
            Err(error) => { state.worktree_visibility.warning = Some(format!("Cannot load worktree visibility: {error}")); }
        }
        state.worktree_visibility.next_check = Instant::now() + Duration::from_secs(1);
    }
}
