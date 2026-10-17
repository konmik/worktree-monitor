use super::*;

pub(super) unsafe fn fonts(window: HWND, state: &mut State) -> bool {
    state.indicators_dirty = true;
    let dpi = GetDpiForWindow(window).max(96) as i32;
    let Some(resources) = Resources::new(dpi) else { return false; };
    for card in state.fixed.iter().chain(&state.cards) {
        SendMessageW(card.window, WM_SETFONT, resources.font as WPARAM, 0);
        if let Some(action) = card.action {
            SendMessageW(action, WM_SETFONT, resources.font as WPARAM, 0);
        }
    }
    if !state.show_all_button.is_null() {
        SendMessageW(state.show_all_button, WM_SETFONT, resources.font as WPARAM, 0);
    }
    state.resources = Some(resources);
    state.dpi = dpi;
    true
}

pub(super) unsafe fn wrapped_width(dc: HDC, value: &str, font: HFONT) -> i32 {
    let saved = SaveDC(dc);
    SelectObject(dc, font);
    let mut size = std::mem::zeroed();
    let value = wide(value);
    GetTextExtentPoint32W(dc, value.as_ptr(), (value.len() - 1) as i32, &mut size);
    RestoreDC(dc, saved);
    size.cx
}

