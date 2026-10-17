use super::runtime::State;

pub(super) fn set_status(state: &mut State) {
    state.frame_dirty = true;
}
