use super::*;

pub(super) unsafe fn update_action_caption(card: &mut Card, visibility: &Visibility) -> bool {
    let Some(action) = card.action else { return false; };
    let Some(ItemKey::Worktree(repository, worktree)) = &card.key else { return false; };
    let hidden = visibility.contains(repository, worktree);
    if card.action_hidden != Some(hidden) {
        SetWindowTextW(action, wide(if hidden { "Unhide" } else { "Hide" }).as_ptr());
        InvalidateRect(action, null(), 0);
        card.action_hidden = Some(hidden);
        return true;
    }
    false
}

pub(super) unsafe fn position_action(card: &Card) {
    let Some(action) = card.action else { return; };
    let Some(rect) = card.action_rect else { return; };
    SetWindowPos(action, null_mut(), rect.left, rect.top, rect.width, rect.height,
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS);
}

pub(super) unsafe fn button_caption(window: HWND) -> String {
    let mut value = [0u16; 128];
    let length = GetWindowTextW(window, value.as_mut_ptr(), value.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&value[..length.min(value.len())])
}

pub(super) unsafe fn show_all_checked(state: &State) -> bool {
    SendMessageW(state.show_all_button, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT
}

pub(super) unsafe fn paint_button(draw: &DRAWITEMSTRUCT, state: &State) {
    let width = draw.rcItem.right - draw.rcItem.left;
    let height = draw.rcItem.bottom - draw.rcItem.top;
    if width <= 0 || height <= 0 { return; }
    let memory = CreateCompatibleDC(draw.hDC);
    let bitmap = CreateCompatibleBitmap(draw.hDC, width, height);
    if !memory.is_null() && !bitmap.is_null() {
        let old = SelectObject(memory, bitmap);
        let buffered = DRAWITEMSTRUCT { hDC: memory,
            rcItem: RECT { left: 0, top: 0, right: width, bottom: height }, ..*draw };
        draw_button(&buffered, state);
        BitBlt(draw.hDC, draw.rcItem.left, draw.rcItem.top, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old);
    } else { draw_button(draw, state); }
    if !bitmap.is_null() { DeleteObject(bitmap); }
    if !memory.is_null() { DeleteDC(memory); }
}

pub(super) unsafe fn draw_button(draw: &DRAWITEMSTRUCT, state: &State) {
    let Some(resources) = &state.resources else { return; };
    let saved = SaveDC(draw.hDC);
    let rect = draw.rcItem;
    let s = |value| scaled(value, state.dpi);
    let disabled = draw.itemState & (ODS_DISABLED | ODS_GRAYED) != 0;
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let focused = draw.itemState & ODS_FOCUS != 0;
    let hot = draw.itemState & ODS_HOTLIGHT != 0;
    let checked = draw.itemState & ODS_CHECKED != 0;
    fill(draw.hDC, rect, resources.background);
    let padding = s(8);
    let text_top = rect.top + (rect.bottom - rect.top - resources.typography.cell).max(0) / 2;
    let text_rect = RECT { left: rect.left + padding, top: text_top,
        right: rect.right - padding, bottom: (text_top + resources.typography.cell).min(rect.bottom) };
    let color = if disabled { MUTED } else if checked || pressed { GREEN }
        else if focused || hot { TEAL } else { WHITE };
    text(draw.hDC, &button_caption(draw.hwndItem), text_rect, resources.font, color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    RestoreDC(draw.hDC, saved);
}

pub(super) unsafe fn draw_checkbox(dc: HDC, rect: RECT, window: HWND, state: &State) {
    let Some(resources) = &state.resources else { return; };
    let saved = SaveDC(dc);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width > 0 && height > 0 {
        fill(dc, rect, resources.background);
        let checked = SendMessageW(window, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT;
        let focused = GetFocus() == window;
        let disabled = IsWindowEnabled(window) == 0;
        let pressed = SendMessageW(window, BM_GETSTATE, 0, 0) & BST_PUSHED as LRESULT != 0;
        text(dc, if checked { "[x] Show all" } else { "[ ] Show all" }, rect,
            resources.font, if disabled { MUTED } else if pressed { GREEN } else if focused { TEAL }
                else if checked { GREEN } else { WHITE }, DT_SINGLELINE | DT_VCENTER);
    }
    RestoreDC(dc, saved);
}

pub(super) unsafe extern "system" fn show_all_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, data: usize) -> LRESULT {
    track_hover(window, message);
    if message == WM_ERASEBKGND { return 1; }
    if message == WM_NCDESTROY { RemoveWindowSubclass(window, Some(show_all_procedure), 1); }
    if message == WM_NCHITTEST && edge_hit_transparent(window, mouse_point(lparam)) {
        return HTTRANSPARENT as LRESULT;
    }
    if message == WM_PAINT || message == WM_PRINTCLIENT {
        let mut paint: PAINTSTRUCT = std::mem::zeroed();
        let dc = if message == WM_PAINT { BeginPaint(window, &mut paint) } else { wparam as HDC };
        let mut rect: RECT = std::mem::zeroed();
        if !dc.is_null() && GetClientRect(window, &mut rect) != 0 {
            draw_checkbox(dc, rect, window, &*(data as *const State));
        }
        if message == WM_PAINT { EndPaint(window, &paint); }
        return 0;
    }
    let result = DefSubclassProc(window, message, wparam, lparam);
    if matches!(message, WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | BM_SETCHECK | BM_SETSTATE) {
        InvalidateRect(window, null(), 0);
    }
    result
}

