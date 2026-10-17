use super::row_layout::invalidate_frame;
use super::runtime::State;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{POINT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, SetCapture, ReleaseCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::{SB_BOTTOM, SB_LINEDOWN, SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION, SB_THUMBTRACK, SB_TOP, WM_CANCELMODE, WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE};

pub(super) unsafe fn wake_scrollbar(state: &mut State) {
    let idle = (state.scrolling.content > state.scrolling.page).then(|| Instant::now() + Duration::from_secs(3));
    if idle.is_some() != state.scrolling.idle.is_some() { invalidate_frame(state); }
    state.scrolling.idle = idle;
}

pub(super) unsafe fn update_scrollbar(state: &mut State, activity: bool) {
    if state.scrolling.content <= state.scrolling.page { state.scrolling.idle = None; }
    else if activity { wake_scrollbar(state); }
}

pub(super) unsafe fn scroll_to(state: &mut State, position: i32) {
    let position = position.clamp(0, (state.scrolling.content - state.scrolling.page).max(0));
    if position != state.scrolling.position {
        state.scrolling.position = position;
        invalidate_frame(state);
    }
    wake_scrollbar(state);
}

pub(super) unsafe fn handle_scroll(state: &mut State, wparam: WPARAM) {
    let page = (state.scrolling.page - 1).max(1);
    let target = match (wparam & 0xffff) as i32 {
        SB_LINEUP => state.scrolling.position.saturating_sub(1),
        SB_LINEDOWN => state.scrolling.position.saturating_add(1),
        SB_PAGEUP => state.scrolling.position.saturating_sub(page),
        SB_PAGEDOWN => state.scrolling.position.saturating_add(page),
        SB_TOP => 0,
        SB_BOTTOM => state.scrolling.content,
        SB_THUMBTRACK | SB_THUMBPOSITION => ((wparam >> 16) & 0xffff) as i32,
        _ => return,
    };
    scroll_to(state, target);
}

pub(super) unsafe fn wheel(state: &mut State, wparam: WPARAM) {
    state.scrolling.wheel_remainder += ((wparam >> 16) as u16 as i16) as i32;
    let steps = state.scrolling.wheel_remainder / 120;
    state.scrolling.wheel_remainder %= 120;
    scroll_to(state, state.scrolling.position.saturating_sub(steps * 3));
}

pub(super) unsafe fn keyboard(state: &mut State, key: WPARAM) -> bool {
    let code = match key as u32 {
        0x26 => SB_LINEUP, 0x28 => SB_LINEDOWN, 0x21 => SB_PAGEUP, 0x22 => SB_PAGEDOWN,
        0x24 => SB_TOP, 0x23 => SB_BOTTOM, _ => return false,
    };
    handle_scroll(state, code as WPARAM);
    true
}

pub(super) unsafe fn pointer(state: &mut State, point: &POINT, message: u32) -> bool {
    if matches!(message, WM_LBUTTONUP | WM_CAPTURECHANGED | WM_CANCELMODE) {
        if state.scrolling.grab.take().is_some() {
            if message == WM_LBUTTONUP && GetCapture() == state.window { ReleaseCapture(); }
            return true;
        }
        return false;
    }
    let scrollbar = crate::rendering::Scrollbar::new(state.scrolling.content.max(0) as usize,
        state.scrolling.page.max(0) as usize, state.scrolling.position.max(0) as usize);
    if message == WM_MOUSEMOVE {
        if let Some(grab) = state.scrolling.grab {
            scroll_to(state, scrollbar.position_at(point.y, grab) as i32);
            return true;
        }
        return false;
    }
    if message != WM_LBUTTONDOWN || point.x != state.scrolling.width || point.y < 0
        || point.y >= state.scrolling.page || state.scrolling.content <= state.scrolling.page { return false; }
    wake_scrollbar(state);
    if point.y == 0 { handle_scroll(state, SB_LINEUP as WPARAM); }
    else if point.y == state.scrolling.page - 1 { handle_scroll(state, SB_LINEDOWN as WPARAM); }
    else if scrollbar.thumb.contains(&(point.y as usize)) {
        state.scrolling.grab = Some(point.y as usize - scrollbar.thumb.start);
        SetCapture(state.window);
    } else { handle_scroll(state, if point.y < scrollbar.thumb.start as i32 { SB_PAGEUP } else { SB_PAGEDOWN } as WPARAM); }
    true
}

pub(super) struct Scrolling {
    pub(super) position: i32,
    pub(super) content: i32,
    pub(super) page: i32,
    pub(super) idle: Option<Instant>,
    pub(super) width: i32,
    pub(super) wheel_remainder: i32,
    pub(super) grab: Option<usize>,
}
