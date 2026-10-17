use super::buffer_presentation::TextLayer;
use super::dashboard_rows::render_card;
use super::buffer_presentation;
use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;
use super::runtime::{State};
use super::window_appearance::intersection;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, ClientToScreen, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, HBITMAP, HDC, HGDIOBJ, MapWindowPoints, ReleaseDC, SelectObject, SaveDC, RestoreDC, IntersectClipRect};
use windows_sys::Win32::UI::WindowsAndMessaging::{DefWindowProcW, GA_ROOT, GetAncestor, GetClientRect, GetWindowRect, HTTRANSPARENT, IsIconic, IsWindowVisible, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCHITTEST};
pub(super) struct IndicatorBitmap {
    pub(super) dc: HDC,
    pub(super) bitmap: HBITMAP,
    pub(super) previous: HGDIOBJ,
    pub(super) pixels: *mut u32,
    pub(super) length: usize,
}

impl IndicatorBitmap {
    pub(super) unsafe fn new(screen: HDC, width: i32, height: i32) -> Option<Self> {
        let length = usize::try_from(width).ok()?.checked_mul(usize::try_from(height).ok()?)?;
        let mut bitmap = Self { dc: CreateCompatibleDC(screen), bitmap: null_mut(), previous: null_mut(), pixels: null_mut(), length };
        if bitmap.dc.is_null() { return None; }
        let info = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width, biHeight: -height, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..std::mem::zeroed() },
            ..std::mem::zeroed() };
        bitmap.bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, (&mut bitmap.pixels as *mut *mut u32).cast(), null_mut(), 0);
        if bitmap.bitmap.is_null() || bitmap.pixels.is_null() { return None; }
        bitmap.previous = SelectObject(bitmap.dc, bitmap.bitmap);
        std::ptr::write_bytes(bitmap.pixels, 0, length);
        Some(bitmap)
    }
}

impl Drop for IndicatorBitmap {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() { SelectObject(self.dc, self.previous); }
            if !self.bitmap.is_null() { DeleteObject(self.bitmap); }
            if !self.dc.is_null() { DeleteDC(self.dc); }
        }
    }
}

pub(super) unsafe fn draw_stressed_cells(state: &State, dc: HDC, layer: TextLayer) {
    let Some(resources) = &state.resources else { return; };
    let root = GetAncestor(state.viewport, GA_ROOT);
    let mut viewport: RECT = std::mem::zeroed();
    if GetClientRect(state.viewport, &mut viewport) == 0 { return; }
    MapWindowPoints(state.viewport, root, (&mut viewport as *mut RECT).cast(), 2);
    for card in state.fixed.iter().chain(&state.cards) {
        if IsWindowVisible(card.window) == 0 { continue; }
        let mut rect: RECT = std::mem::zeroed();
        if GetClientRect(card.window, &mut rect) == 0 { continue; }
        let width = rect.right;
        let height = rect.bottom;
        let Some((buffer, left, top)) = render_card(card, state, width) else { continue; };
        MapWindowPoints(card.window, root, (&mut rect as *mut RECT).cast(), 2);
        let clip = if state.cards.iter().any(|row| row.window == card.window) {
            let Some(clip) = intersection(rect, viewport) else { continue; };
            clip
        } else { rect };
        let saved = SaveDC(dc);
        IntersectClipRect(dc, clip.left, clip.top, clip.right, clip.bottom);
        let top = if matches!(card.display, crate::dashboard::DisplayItem::Status { .. }) {
            (height - resources.typography.cell) / 2
        } else { top };
        buffer_presentation::present_at(&buffer, dc, resources, rect.left + left, rect.top + top, layer);
        RestoreDC(dc, saved);
    }
}

pub(super) unsafe fn sync_indicators(window: HWND, state: &mut State) -> bool {
    let overlay = state.indicator_overlay;
    if overlay.is_null() { return true; }
    if state.controls_visible || IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
        if IsWindowVisible(overlay) != 0 { ShowWindow(overlay, SW_HIDE); }
        return true;
    }
    let mut rect: RECT = std::mem::zeroed();
    if GetClientRect(window, &mut rect) == 0 { return false; }
    let width = rect.right;
    let height = rect.bottom;
    if width <= 0 || height <= 0 { ShowWindow(overlay, SW_HIDE); return true; }
    let mut origin = POINT { x: 0, y: 0 };
    if ClientToScreen(window, &mut origin) == 0 { return false; }
    let mut previous: RECT = std::mem::zeroed();
    GetWindowRect(overlay, &mut previous);
    if previous.right - previous.left != width || previous.bottom - previous.top != height { state.indicators_dirty = true; }
    if state.indicators_dirty {
        let screen = GetDC(null_mut());
        let color = IndicatorBitmap::new(screen, width, height);
        let mask = IndicatorBitmap::new(screen, width, height);
        let success = if let (Some(color), Some(mask)) = (color, mask) {
            draw_stressed_cells(state, color.dc, TextLayer::Stressed);
            draw_stressed_cells(state, mask.dc, TextLayer::Mask);
            GdiFlush();
            let colors = std::slice::from_raw_parts_mut(color.pixels, color.length);
            let masks = std::slice::from_raw_parts(mask.pixels, mask.length);
            for (color, mask) in colors.iter_mut().zip(masks) {
                let alpha = (mask & 255).max((mask >> 8) & 255).max((mask >> 16) & 255);
                *color = (*color & 0xffffff) | (alpha << 24);
            }
            let size = windows_sys::Win32::Foundation::SIZE { cx: width, cy: height };
            let source = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            UpdateLayeredWindow(overlay, screen, &origin, &size, color.dc, &source, 0, &blend, ULW_ALPHA) != 0
        } else { false };
        ReleaseDC(null_mut(), screen);
        if !success { return false; }
        state.indicators_dirty = false;
    } else if (previous.left != origin.x || previous.top != origin.y)
        && SetWindowPos(overlay, null_mut(), origin.x, origin.y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) == 0 { return false; }
    if IsWindowVisible(overlay) == 0 { ShowWindow(overlay, SW_SHOWNOACTIVATE); }
    true
}

pub(super) unsafe extern "system" fn indicator_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

pub(super) const WM_SYNC_INDICATORS: u32 = WM_APP + 42;

use windows_sys::Win32::Foundation::HINSTANCE;
use std::ptr::null;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WS_DISABLED, WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_NOACTIVATE, WS_POPUP};
use super::runtime::wide;

pub(super) unsafe fn create_indicator_overlay(window: HWND, instance: HINSTANCE, class: *const u16) -> HWND {
    CreateWindowExW(WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | crate::taskbar_icon::WINDOW_STYLE,
            class, wide("Line-change indicators").as_ptr(), WS_POPUP | WS_DISABLED,
            0, 0, 1, 1, window, null_mut(), instance, null())
}
