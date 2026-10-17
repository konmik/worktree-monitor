use super::window_resizing::resize_hit;
use windows_sys::Win32::Foundation::{HWND, LRESULT, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

#[derive(Clone, Copy)]
pub(super) enum NativeHit {
    Edge(LRESULT),
    Empty,
}

pub(super) unsafe fn native_hit_test(root: HWND, point: POINT) -> Option<NativeHit> {
    if let Some(hit) = resize_hit(root, point) { return Some(NativeHit::Edge(hit)); }
    let mut client = point;
    if ScreenToClient(root, &mut client) == 0 { return None; }
    let mut bounds: RECT = std::mem::zeroed();
    if GetClientRect(root, &mut bounds) == 0 || client.x < 0 || client.x >= bounds.right
        || client.y < 0 || client.y >= bounds.bottom { return None; }
    Some(NativeHit::Empty)
}
