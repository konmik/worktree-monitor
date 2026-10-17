use super::runtime::{mouse_point};
use super::window_fading::{track_hover};
use super::window_resizing::{resize_hit};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{CWP_SKIPINVISIBLE, ChildWindowFromPointEx, GA_ROOT, GetAncestor, GetClientRect, GetDlgCtrlID, HTTRANSPARENT, WM_NCDESTROY, WM_NCHITTEST};
#[derive(Clone, Copy)]
pub(super) enum NativeHit {
    Edge(LRESULT),
    Child,
    Empty,
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

