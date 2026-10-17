use super::*;

pub(super) unsafe fn update_hover_state(window: HWND, state: &mut State) {
    let mut point: POINT = std::mem::zeroed();
    let visible = IsWindowVisible(window) != 0 && IsIconic(window) == 0 && GetCursorPos(&mut point) != 0
        && { let hit = GetAncestor(WindowFromPoint(point), GA_ROOT); hit == window || hit == state.indicator_overlay };
    if visible == state.controls_visible { return; }
    if SetLayeredWindowAttributes(window, 0, if visible { 255 } else { state.opacity.alpha() }, LWA_ALPHA) == 0 {
        state.failure = Some(io::Error::last_os_error());
        PostMessageW(window, WM_CLOSE, 0, 0);
        return;
    }
    state.controls_visible = visible;
    if visible && !state.indicator_overlay.is_null() { ShowWindow(state.indicator_overlay, SW_HIDE); }
    let focus = GetFocus();
    if !visible && (focus == state.show_all_button || state.cards.iter().any(|card| card.action == Some(focus))) {
        SetFocus(if GetForegroundWindow() == window && IsIconic(window) == 0 { state.viewport } else { null_mut() });
    }
    ShowWindow(state.show_all_button, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
    for card in &state.cards {
        if let Some(action) = card.action { ShowWindow(action, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE }); }
    }
    layout(window, state, None, false);
}

pub(super) unsafe fn track_hover(window: HWND, message: u32) {
    if !matches!(message, WM_MOUSEMOVE | WM_NCMOUSEMOVE | WM_MOUSELEAVE | WM_NCMOUSELEAVE) { return; }
    if matches!(message, WM_MOUSEMOVE | WM_NCMOUSEMOVE) {
        let mut tracking = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE | if message == WM_NCMOUSEMOVE { TME_NONCLIENT } else { 0 },
            hwndTrack: window, dwHoverTime: 0 };
        TrackMouseEvent(&mut tracking);
    }
    let root = GetAncestor(window, GA_ROOT);
    let pointer = GetWindowLongPtrW(root, GWLP_USERDATA) as *mut State;
    if !pointer.is_null() { update_hover_state(root, &mut *pointer); }
}

