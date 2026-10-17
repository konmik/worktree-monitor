use super::runtime::State;
use std::ptr::null;
use std::time::Instant;
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;

pub(super) unsafe fn refresh_indicators(state: &mut State) {
    if let Some(update) = &state.latest {
        state.refreshing = state.refresh_tracker.update(&update.scan_activities, Instant::now());
    }
    let mut changed = false;
    for card in &mut state.cards {
        let refreshing = card.key.as_ref().is_some_and(|key| state.refreshing.contains(key));
        if card.refreshing != refreshing {
            card.refreshing = refreshing;
            changed = true;
        }
    }
    if changed {
        state.indicators_dirty = true;
        state.frame_dirty = true;
        InvalidateRect(state.window, null(), 0);
    }
}
