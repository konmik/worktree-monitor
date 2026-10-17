use super::*;

#[derive(Clone, Copy)]
pub(super) enum NativeHit {
    Edge(LRESULT),
    Child,
    Empty,
}

pub(super) unsafe fn resize_hit(window: HWND, point: POINT) -> Option<LRESULT> {
    if IsZoomed(window) != 0 { return None; }
    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(window, &mut rect) == 0 || point.x < rect.left || point.x >= rect.right
        || point.y < rect.top || point.y >= rect.bottom { return None; }
    let dpi = GetDpiForWindow(window).max(96) as i32;
    let frame_x = (GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi as u32)
        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi as u32)).max(scaled(4, dpi));
    let frame_y = (GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi as u32)
        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi as u32)).max(scaled(4, dpi));
    let left = point.x < rect.left + frame_x;
    let right = point.x >= rect.right - frame_x;
    let top = point.y < rect.top + frame_y;
    let bottom = point.y >= rect.bottom - frame_y;
    Some(match (left, right, top, bottom) {
        (true, _, true, _) => HTTOPLEFT as LRESULT,
        (_, true, true, _) => HTTOPRIGHT as LRESULT,
        (true, _, _, true) => HTBOTTOMLEFT as LRESULT,
        (_, true, _, true) => HTBOTTOMRIGHT as LRESULT,
        (_, _, true, _) => HTTOP as LRESULT,
        (_, _, _, true) => HTBOTTOM as LRESULT,
        (true, _, _, _) => HTLEFT as LRESULT,
        (_, true, _, _) => HTRIGHT as LRESULT,
        _ => return None,
    })
}

pub(super) unsafe fn native_hit_test(root: HWND, viewport: HWND, point: POINT) -> Option<NativeHit> {
    if let Some(hit) = resize_hit(root, point) { return Some(NativeHit::Edge(hit)); }
    let mut client = point;
    if ScreenToClient(root, &mut client) == 0 { return None; }
    let mut bounds: RECT = std::mem::zeroed();
    if GetClientRect(root, &mut bounds) == 0 || client.x < 0 || client.x >= bounds.right
        || client.y < 0 || client.y >= bounds.bottom { return None; }
    let child = ChildWindowFromPointEx(root, client, CWP_SKIPINVISIBLE);
    if child.is_null() { return None; }
    if child == root || GetDlgCtrlID(child) == 1002 { return Some(NativeHit::Empty); }
    if child == viewport {
        let mut viewport_point = point;
        if ScreenToClient(viewport, &mut viewport_point) == 0 { return None; }
        let nested = ChildWindowFromPointEx(viewport, viewport_point, CWP_SKIPINVISIBLE);
        if nested.is_null() || nested == viewport { Some(NativeHit::Empty) } else { Some(NativeHit::Child) }
    } else {
        Some(NativeHit::Child)
    }
}

pub(super) unsafe fn edge_hit_transparent(window: HWND, point: POINT) -> bool {
    let root = GetAncestor(window, GA_ROOT);
    resize_hit(root, point).is_some()
}

pub(super) unsafe extern "system" fn edge_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    track_hover(window, message);
    if message == WM_NCDESTROY { RemoveWindowSubclass(window, Some(edge_procedure), 1); }
    if message == WM_NCHITTEST && (GetDlgCtrlID(window) == 1002 || edge_hit_transparent(window, mouse_point(lparam))) {
        return HTTRANSPARENT as LRESULT;
    }
    DefSubclassProc(window, message, wparam, lparam)
}

