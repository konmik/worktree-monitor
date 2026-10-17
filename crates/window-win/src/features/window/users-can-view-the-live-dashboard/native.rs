use super::notification_lifecycle::dispatch_notification_icon;
use super::context_menu_actions::{context_menu};
use super::dashboard_rows::{Card, create_card, reconcile, refresh};
use super::font_resources::{Resources, fonts};
use super::idle_indicator::{set_status};
use super::opaque_indicators::{WM_SYNC_INDICATORS, create_indicator_overlay, indicator_procedure, sync_indicators};
use super::refresh_markers::{refresh_indicators};
use super::row_layout::{layout};
use super::row_ordering::{Ordering, cancel_drag, finish_drag, move_drag};
use super::row_scrolling::{Scrolling, handle_scroll, keyboard, scroll_to, scrollbar_procedure, wheel};
use super::saved_preferences::{reload_preferences};
use super::scroll_anchor::{anchor};
use super::visibility_controls::{show_all_procedure};
use super::window_appearance::{BACKGROUND, WHITE, configure_window_appearance, draw_item, paint_background};
use super::window_fading::{track_hover, update_hover_state};
use super::window_movement::{NativeHit, native_hit_test};
use super::worktree_visibility::{WorktreeVisibility, command};
use super::window_placement::restore_window_geometry;
use crate::{notification_icon, placement_win};
use crate::dashboard::{DashboardView, DisplayItem, Health, ItemKey, RefreshTracker};
use crate::notification_icon::{TrayIcon};
use crate::opacity::Opacity;
use std::io;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, HDC, MONITORINFO, MONITOR_DEFAULTTONEAREST, MonitorFromWindow, SetBkColor, SetTextColor};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::SetWindowTheme;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture};
use windows_sys::Win32::UI::Shell::SetWindowSubclass;
use windows_sys::Win32::UI::WindowsAndMessaging::{BS_AUTOCHECKBOX, CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetMessageW, GetParent, GetWindowLongPtrW, HTCAPTION, HTCLIENT, HTTRANSPARENT, IDC_ARROW, IsDialogMessageW, IsWindow, KillTimer, LWA_ALPHA, LoadCursorW, LoadIconW, MINMAXINFO, MSG, PostMessageW, RegisterClassW, SBS_VERT, SW_HIDE, SendMessageW, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage, UnregisterClassW, WM_ACTIVATEAPP, WM_CANCELMODE, WM_CAPTURECHANGED, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCRBUTTONUP, WM_PAINT, WM_QUERYENDSESSION, WM_SETFONT, WM_SHOWWINDOW, WM_SIZE, WM_TIMER, WM_VSCROLL, WM_WINDOWPOSCHANGED, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_CONTROLPARENT, WS_TABSTOP, WS_VISIBLE};
use wtm_core::{Monitor, MonitorUpdate, Service};
pub(super) struct State {
    pub(super) ordering: Ordering,
    pub(super) worktree_visibility: WorktreeVisibility,
    pub(super) scrolling: Scrolling,
    pub(super) monitor: Monitor,
    pub(super) viewport: HWND,
    pub(super) fixed: Vec<Card>,
    pub(super) cards: Vec<Card>,
    pub(super) resources: Option<Resources>,
    pub(super) view: DashboardView,
    pub(super) latest: Option<Arc<MonitorUpdate>>,
    pub(super) refresh_tracker: RefreshTracker,
    pub(super) refreshing: HashSet<ItemKey>,
    pub(super) dpi: i32,
    pub(super) next_card_id: usize,
    pub(super) failure: Option<io::Error>,
    pub(super) placement_path: PathBuf,
    pub(super) opacity_path: PathBuf,
    pub(super) opacity: Opacity,
    pub(super) starting: bool,
    pub(super) show_all_button: HWND,
    pub(super) controls_visible: bool,
    pub(super) next_action_id: usize,
    pub(super) indicator_overlay: HWND,
    pub(super) indicators_dirty: bool,
    pub(super) indicators_queued: bool,
    pub(super) tray: Option<TrayIcon>,
    pub(super) taskbar_created: u32,
}



pub(super) fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }

pub(super) fn scaled(value: i32, dpi: i32) -> i32 { (value * dpi / 96).max(1) }

pub(super) fn mouse_point(lparam: LPARAM) -> POINT {
    POINT { x: (lparam as u16 as i16) as i32, y: ((lparam >> 16) as u16 as i16) as i32 }
}

pub(super) unsafe extern "system" fn viewport_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    callback(window, message, wparam, lparam, true)
}

pub(super) unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    callback(window, message, wparam, lparam, false)
}

pub(super) unsafe fn callback(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, viewport: bool) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut State;
    if pointer.is_null() { return DefWindowProcW(window, message, wparam, lparam); }
    match catch_unwind(AssertUnwindSafe(|| dispatch(window, message, wparam, lparam, pointer, viewport))) {
        Ok(result) => {
            if message != WM_SYNC_INDICATORS && message != WM_NCDESTROY && !(*pointer).indicators_queued
                && !(*pointer).indicator_overlay.is_null()
                && ((*pointer).indicators_dirty || !viewport && matches!(message, WM_WINDOWPOSCHANGED | WM_SHOWWINDOW)) {
                let root = if viewport { GetParent(window) } else { window };
                (*pointer).indicators_queued = PostMessageW(root, WM_SYNC_INDICATORS, 0, 0) != 0;
            }
            result
        }
        Err(_) => {
            (*pointer).failure = Some(io::Error::other("Window callback failed"));
            if message == WM_CREATE { -1 } else {
                PostMessageW(if viewport { GetParent(window) } else { window }, WM_CLOSE, 0, 0);
                0
            }
        }
    }
}

pub(super) unsafe fn dispatch(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, pointer: *mut State, viewport: bool) -> LRESULT {
    track_hover(window, message);
    if !viewport {
        if let Some(result) = dispatch_notification_icon(window, message, wparam, lparam, &mut *pointer) { return result; }
    }
    match message {
        WM_CONTEXTMENU | WM_NCRBUTTONUP if !viewport => {
            let mut point = mouse_point(lparam);
            if point.x == -1 && point.y == -1 { GetCursorPos(&mut point); }
            if let Err(error) = context_menu(window, point, pointer) { (*pointer).failure = Some(error); }
            0
        }
        WM_SYNC_INDICATORS if !viewport => {
            (*pointer).indicators_queued = false;
            if !sync_indicators(window, &mut *pointer) {
                (*pointer).failure = Some(io::Error::last_os_error());
                PostMessageW(window, WM_CLOSE, 0, 0);
            }
            0
        }
        WM_NCCALCSIZE if !viewport => 0,
        WM_NCACTIVATE if !viewport => DefWindowProcW(window, message, wparam, -1),
        WM_GETMINMAXINFO if !viewport => {
            let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let limits = &mut *(lparam as *mut MINMAXINFO);
                limits.ptMaxPosition = POINT { x: info.rcWork.left - info.rcMonitor.left, y: info.rcWork.top - info.rcMonitor.top };
                limits.ptMaxSize = POINT { x: info.rcWork.right - info.rcWork.left, y: info.rcWork.bottom - info.rcWork.top };
                0
            } else { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_NCHITTEST if !viewport => {
            match native_hit_test(window, (*pointer).viewport, mouse_point(lparam)) {
                Some(NativeHit::Edge(hit)) => hit,
                Some(NativeHit::Child) => HTCLIENT as LRESULT,
                Some(NativeHit::Empty) => HTCAPTION as LRESULT,
                None => DefWindowProcW(window, message, wparam, lparam),
            }
        }
        WM_NCHITTEST if viewport => {
            match native_hit_test(GetParent(window), window, mouse_point(lparam)) {
                Some(NativeHit::Empty) | Some(NativeHit::Edge(_)) | Some(NativeHit::Child) => HTTRANSPARENT as LRESULT,
                None => DefWindowProcW(window, message, wparam, lparam),
            }
        }
        WM_NCLBUTTONDOWN if !viewport && wparam == HTCAPTION as WPARAM => {
            ReleaseCapture();
            DefWindowProcW(window, message, wparam, lparam)
        }
        WM_CREATE if !viewport => {
            let state = &mut *pointer;
            if controls(window, state) { 0 } else { state.failure = Some(io::Error::last_os_error()); -1 }
        }
        WM_SIZE if !viewport => { cancel_drag(pointer); let saved = anchor(&*pointer); layout(window, &mut *pointer, saved, true); 0 }
        WM_EXITSIZEMOVE if !viewport => {
            let _ = placement_win::save(window, &(*pointer).placement_path);
            0
        }
        WM_QUERYENDSESSION if !viewport => {
            let _ = placement_win::save(window, &(*pointer).placement_path);
            1
        }
        WM_DPICHANGED if !viewport => super::initial_geometry::update_dpi(window, pointer, lparam),
        WM_TIMER if !viewport => {
            let state = &mut *pointer;
            update_hover_state(window, state);
            if state.scrolling.idle.is_some_and(|deadline| Instant::now() >= deadline)
                && GetCapture() != state.scrolling.control {
                ShowWindow(state.scrolling.control, SW_HIDE);
                state.scrolling.idle = None;
            }
            if let Some(update) = state.monitor.latest() {
                state.latest = Some(update);
            }
            if let Some((point, active)) = state.ordering.drag.as_ref().map(|drag| (drag.position, drag.active)) {
                refresh_indicators(state);
                set_status(state);
                if active && point.x >= 0 && point.x < state.scrolling.width && point.y >= 0 && point.y < state.scrolling.page {
                    let edge = scaled(24, state.dpi);
                    if point.y < edge { scroll_to(state, state.scrolling.position - edge); }
                    else if point.y > state.scrolling.page - edge { scroll_to(state, state.scrolling.position + edge); }
                    move_drag(state, point);
                }
                return 0;
            }
            reload_preferences(state);
            refresh(window, state);
            0
        }
        WM_PAINT => {
            if let Some(resources) = &(*pointer).resources {
                let marker = viewport.then(|| (*pointer).ordering.drag.as_ref().and_then(|drag| drag.marker_rect)).flatten();
                paint_background(window, resources, marker);
                0
            }
            else { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_COMMAND => command(window, &mut *pointer, wparam, lparam, viewport),
        WM_ERASEBKGND => 1,
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            if let Some(resources) = &(*pointer).resources {
                SetBkColor(wparam as HDC, BACKGROUND);
                SetTextColor(wparam as HDC, WHITE);
                resources.palette.background as LRESULT
            } else { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_DRAWITEM => draw_item(&*pointer, lparam),
        WM_MOUSEMOVE if viewport && (*pointer).ordering.drag.is_some() => { move_drag(&mut *pointer, mouse_point(lparam)); 0 }
        WM_LBUTTONUP if viewport && (*pointer).ordering.drag.is_some() => { finish_drag(pointer, mouse_point(lparam)); 0 }
        WM_CAPTURECHANGED | WM_CANCELMODE => { cancel_drag(pointer); 0 }
        WM_ACTIVATEAPP if wparam == 0 => { cancel_drag(pointer); 0 }
        WM_KEYDOWN if wparam == 0x1b => { cancel_drag(pointer); 0 }
        WM_VSCROLL | WM_MOUSEWHEEL => {
            if message == WM_VSCROLL { handle_scroll(&mut *pointer, wparam); } else { wheel(&mut *pointer, wparam); }
            if let Some(point) = (*pointer).ordering.drag.as_ref().map(|drag| drag.position) { move_drag(&mut *pointer, point); }
            0
        }
        WM_KEYDOWN if keyboard(&mut *pointer, wparam) => {
            if let Some(point) = (*pointer).ordering.drag.as_ref().map(|drag| drag.position) { move_drag(&mut *pointer, point); }
            0
        }
        WM_CLOSE if !viewport => super::dashboard_closing::close(window, pointer),
        WM_DESTROY if !viewport => super::dashboard_closing::destroy(window, pointer),
        WM_NCDESTROY => {
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            DefWindowProcW(window, message, wparam, lparam)
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

pub(super) unsafe fn controls(window: HWND, state: &mut State) -> bool {
    if !fonts(window, state) { return false; }
    let resources = state.resources.as_ref().unwrap();
    let displays = [DisplayItem::Status { health: Health::Scanning, polling_time_last_minute: Duration::ZERO }];
    for (index, display) in displays.into_iter().enumerate() {
        let Some(card) = create_card(window, 1002 + index, None, None, display, resources) else { return false; };
        state.fixed.push(card);
    }
    state.show_all_button = CreateWindowExW(0, wide("BUTTON").as_ptr(), wide("Show all").as_ptr(),
        WS_CHILD | WS_TABSTOP | BS_AUTOCHECKBOX as u32, 0, 0, 1, 1, window, 1004 as _, GetModuleHandleW(null()), null());
    if state.show_all_button.is_null() { return false; }
    if SetWindowSubclass(state.show_all_button, Some(show_all_procedure), 1, state as *const State as usize) == 0 { return false; }
    SendMessageW(state.show_all_button, WM_SETFONT, resources.font as WPARAM, 0);
    state.scrolling.control = CreateWindowExW(0, wide("SCROLLBAR").as_ptr(), wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE | SBS_VERT as u32, 0, 0, 1, 1, window, 1003 as _, GetModuleHandleW(null()), null());
    if state.scrolling.control.is_null() { return false; }
    if SetWindowSubclass(state.scrolling.control, Some(scrollbar_procedure), 1, state as *const State as usize) == 0 { return false; }
    SetWindowTheme(state.scrolling.control, wide("DarkMode_Explorer").as_ptr(), null());
    state.viewport = CreateWindowExW(WS_EX_CONTROLPARENT, wide("WtmDashboardViewport").as_ptr(), wide("Repository worktrees").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_TABSTOP,
        0, 0, 1, 1, window, 1001 as _, GetModuleHandleW(null()), (state as *mut State).cast());
    if state.viewport.is_null() || !reconcile(state) { return false; }
    SetWindowTheme(state.viewport, wide("DarkMode_Explorer").as_ptr(), null());
    layout(window, state, None, true);
    SetTimer(window, 1, 200, None) != 0
}

pub fn run(service: Service, launch_directory: PathBuf) -> Result<(), io::Error> {
    let order_path = service.config_path().with_extension("order.json");
    let visibility_path = service.config_path().with_extension("visibility.json");
    let placement_path = service.config_path().with_extension("placement.json");
    let opacity_path = service.config_path().with_extension("opacity.json");
    let opacity = Opacity::read(&opacity_path).unwrap_or_default();
    let ordering = Ordering::load(order_path);
    let worktree_visibility = WorktreeVisibility::load(visibility_path);
    let mut state = Box::new(State { monitor: service.monitor_from(launch_directory).map_err(io::Error::other)?, viewport: null_mut(),
        fixed: Vec::new(), cards: Vec::new(), resources: None, view: DashboardView::scanning(), latest: None,
        refresh_tracker: RefreshTracker::default(), refreshing: HashSet::new(),
        dpi: 96, scrolling: Scrolling { control: null_mut(), position: 0, content: 0, page: 0, idle: None, width: 0, wheel_remainder: 0 }, next_card_id: 2000, failure: None,
        placement_path, opacity_path, opacity, starting: true,
        ordering: Ordering { next_check: Instant::now(), ..ordering },
        worktree_visibility: WorktreeVisibility { next_check: Instant::now(), ..worktree_visibility },
        show_all_button: null_mut(), controls_visible: false, next_action_id: 3000,
        indicator_overlay: null_mut(), indicators_dirty: true, indicators_queued: false, tray: None, taskbar_created: 0 });
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        state.taskbar_created = notification_icon::restart_message()?;
        let instance = GetModuleHandleW(null());
        if instance.is_null() { return Err(io::Error::last_os_error()); }
        let icon = LoadIconW(instance, APP_ICON);
        if icon.is_null() { return Err(io::Error::last_os_error()); }
        let class = wide("WtmStatusWindow");
        let viewport_class = wide("WtmDashboardViewport");
        let overlay_class = wide("WtmIndicatorOverlay");
        let definition = WNDCLASSW { lpfnWndProc: Some(procedure), hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW), hIcon: icon, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        if RegisterClassW(&definition) == 0 { return Err(io::Error::last_os_error()); }
        let viewport_definition = WNDCLASSW { lpfnWndProc: Some(viewport_procedure), lpszClassName: viewport_class.as_ptr(), ..definition };
        if RegisterClassW(&viewport_definition) == 0 {
            let error = io::Error::last_os_error();
            UnregisterClassW(class.as_ptr(), instance);
            return Err(error);
        }
        let overlay_definition = WNDCLASSW { lpfnWndProc: Some(indicator_procedure), lpszClassName: overlay_class.as_ptr(), ..definition };
        if RegisterClassW(&overlay_definition) == 0 {
            let error = io::Error::last_os_error();
            UnregisterClassW(viewport_class.as_ptr(), instance);
            UnregisterClassW(class.as_ptr(), instance);
            return Err(error);
        }
        let window = super::dashboard_opening::create(class.as_ptr(), instance, &mut state);
        if window.is_null() {
            let error = state.failure.take().unwrap_or_else(io::Error::last_os_error);
            UnregisterClassW(viewport_class.as_ptr(), instance);
            UnregisterClassW(class.as_ptr(), instance);
            UnregisterClassW(overlay_class.as_ptr(), instance);
            return Err(error);
        }
        state.indicator_overlay = create_indicator_overlay(window, instance, overlay_class.as_ptr());
        if state.indicator_overlay.is_null() {
            let error = io::Error::last_os_error();
            DestroyWindow(window);
            UnregisterClassW(overlay_class.as_ptr(), instance);
            UnregisterClassW(viewport_class.as_ptr(), instance);
            UnregisterClassW(class.as_ptr(), instance);
            return Err(error);
        }
        configure_window_appearance(window);
        let (restored, positioned) = restore_window_geometry(window, &state.placement_path);
        let tray = TrayIcon::new(window);
        if let Err(error) = tray {
            state.failure = Some(error);
            DestroyWindow(window);
        } else if let Err(error) = positioned {
            state.failure = Some(error);
            DestroyWindow(window);
        } else if SetLayeredWindowAttributes(window, 0, state.opacity.alpha(), LWA_ALPHA) == 0 {
            state.failure = Some(io::Error::last_os_error());
            DestroyWindow(window);
        } else {
            state.tray = tray.ok();
            super::dashboard_opening::show(window, restored);
            update_hover_state(window, &mut state);
            if !sync_indicators(window, &mut state) {
                state.failure = Some(io::Error::last_os_error());
                DestroyWindow(window);
            }
        }
        state.starting = false;
        let mut message: MSG = std::mem::zeroed();
        loop {
            let received = GetMessageW(&mut message, null_mut(), 0, 0);
            if received == -1 {
                state.failure = Some(io::Error::last_os_error());
                DestroyWindow(window);
                break;
            }
            if received == 0 { break; }
            if state.ordering.drag.is_none() && message.message == WM_KEYDOWN && matches!(message.wParam, 0x09 | 0x0d)
                && IsDialogMessageW(window, &message) != 0 { continue; }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if IsWindow(window) != 0 {
            KillTimer(window, 1);
            DestroyWindow(window);
        }
        let stopped = state.stop_monitor();
        UnregisterClassW(viewport_class.as_ptr(), instance);
        UnregisterClassW(overlay_class.as_ptr(), instance);
        UnregisterClassW(class.as_ptr(), instance);
        stopped?;
        if let Some(error) = state.failure.take() { return Err(error); }
    }
    Ok(())
}

pub(super) const APP_ICON: *const u16 = std::ptr::without_provenance(1);
