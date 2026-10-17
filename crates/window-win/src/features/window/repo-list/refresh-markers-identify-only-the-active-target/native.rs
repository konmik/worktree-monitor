use super::runtime::{State, wide};
use std::ptr::null;
use std::time::Instant;
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;
pub(super) unsafe fn refresh_indicators(state: &mut State) {
    if let Some(update) = &state.latest {
        state.refreshing = state.refresh_tracker.update(&update.scan_activities, Instant::now());
    }
    for card in &mut state.cards {
        let refreshing = card.key.as_ref().is_some_and(|key| state.refreshing.contains(key));
        if card.refreshing != refreshing {
            card.refreshing = refreshing;
            SetWindowTextW(card.window, wide(&card.display.caption_with_refresh(refreshing)).as_ptr());
            InvalidateRect(card.window, null(), 0);
        }
    }
}

