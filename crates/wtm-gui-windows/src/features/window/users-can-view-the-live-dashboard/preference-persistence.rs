use super::runtime::State;
use wtm_ui::order::{Order, OrderMove};
use wtm_ui::visibility::Visibility;
use std::path::Path;
use wtm_ui::Snapshot;

pub(super) fn save_order(state: &mut State, snapshot: &Snapshot, movement: &OrderMove) {
    match Order::persist_move(&state.ordering.path, snapshot, movement) {
        Ok((order, _)) => { state.ordering.value = order; state.ordering.warning = None; state.ordering.write_failed = false; }
        Err(error) => { state.ordering.warning = Some(format!("Order was not saved: {error}")); state.ordering.write_failed = true; }
    }
}


pub(super) fn save_visibility(state: &mut State, repository: &Path, worktree: &Path) {
    match Visibility::persist_toggle(&state.worktree_visibility.path, repository, worktree) {
        Ok(visibility) => {
            state.worktree_visibility.value = visibility;
            state.worktree_visibility.warning = None;
            state.worktree_visibility.write_failed = false;
        }
        Err(error) => {
            state.worktree_visibility.warning = Some(format!("Visibility was not saved: {error}"));
            state.worktree_visibility.write_failed = true;
        }
    }
}
