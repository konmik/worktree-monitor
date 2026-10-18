use super::notification_lifecycle::{NotificationIcon, dispatch_notification_icon};
use super::context_menu_actions::context_menu;
use super::dashboard_rows::{Card, refresh};
use super::font_resources::{Resources, fonts};
use super::idle_indicator::set_status;
use super::opaque_indicators::{WM_SYNC_INDICATORS, create_indicator_overlay, indicator_procedure, sync_indicators};
use super::refresh_markers::refresh_indicators;
use super::row_layout::layout;
use super::row_ordering::{Ordering, begin_drag, cancel_drag, finish_drag, move_drag, reorderable};
use super::row_scrolling::{Scrolling, keyboard, scroll_to, wheel};
use super::saved_preferences::reload_preferences;
use super::scroll_anchor::anchor;
use super::visibility_controls::{self, Control};
use super::window_appearance::{configure_window_appearance, paint_background};
use super::window_fading::{track_hover, update_hover_state};
use super::window_movement::{NativeHit, native_hit_test};
use super::worktree_visibility::WorktreeVisibility;
use super::window_placement::restore_window_geometry;
use crate::{notification_icon, placement_win};
use wtm_ui::dashboard::{DashboardView, ItemKey, RefreshTracker};
use crate::notification_icon::TrayIcon;
use crate::opacity::Opacity;
use wtm_ui::rendering::DashboardFrame;
use std::io;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::Instant;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, InvalidateRect, MONITORINFO, MONITOR_DEFAULTTONEAREST, MonitorFromWindow, ScreenToClient};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, GetKeyState, ReleaseCapture, SetCapture, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use wtm_ui::{Monitor, MonitorUpdate, Service};

pub(super) struct State {
    pub(super) window: HWND,
    pub(super) ordering: Ordering,
    pub(super) worktree_visibility: WorktreeVisibility,
    pub(super) scrolling: Scrolling,
    pub(super) monitor: Monitor,
    pub(super) cards: Vec<Card>,
    pub(super) resources: Option<Resources>,
    pub(super) view: DashboardView,
    pub(super) latest: Option<Arc<MonitorUpdate>>,
    pub(super) refresh_tracker: RefreshTracker,
    pub(super) refreshing: HashSet<ItemKey>,
    pub(super) dpi: i32,
    pub(super) failure: Option<io::Error>,
    pub(super) placement_path: PathBuf,
    pub(super) opacity_path: PathBuf,
    pub(super) opacity: Opacity,
    pub(super) starting: bool,
    pub(super) show_all: bool,
    pub(super) controls_visible: bool,
    pub(super) focused: Option<Control>,
    pub(super) pressed: Option<Control>,
    pub(super) columns: usize,
    pub(super) rows: usize,
    pub(super) frame: Option<DashboardFrame>,
    pub(super) frame_dirty: bool,
    pub(super) indicator_overlay: HWND,
    pub(super) indicators_dirty: bool,
    pub(super) indicators_queued: bool,
    pub(super) notification_icon: NotificationIcon,
}

pub(super) fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }
pub(super) fn scaled(value: i32, dpi: i32) -> i32 { (value * dpi / 96).max(1) }
pub(super) fn mouse_point(lparam: LPARAM) -> POINT {
    POINT { x: (lparam as u16 as i16) as i32, y: ((lparam >> 16) as u16 as i16) as i32 }
}
pub(super) fn grid_point(state: &State, point: POINT) -> POINT {
    let Some(resources) = &state.resources else { return POINT { x: -1, y: -1 }; };
    POINT { x: point.x.div_euclid(resources.space_width), y: point.y.div_euclid(resources.typography.advance) }
}
pub(super) fn body_point(point: POINT) -> POINT { POINT { x: point.x, y: point.y - 2 } }

pub(super) unsafe fn rebuild_frame(state: &mut State) {
    if !state.frame_dirty { return; }
    let frame = super::buffer_presentation::compose_frame(state);
    state.frame_dirty = false;
    if state.frame.as_ref() != Some(&frame) {
        state.frame = Some(frame);
        state.indicators_dirty = true;
        InvalidateRect(state.window, null(), 0);
    }
}

unsafe fn cancel_input(pointer: *mut State) {
    let owns_capture = (*pointer).pressed.is_some() || (*pointer).scrolling.grab.is_some() || (*pointer).ordering.drag.is_some();
    (*pointer).pressed = None;
    (*pointer).scrolling.grab = None;
    cancel_drag(pointer);
    (*pointer).frame_dirty = true;
    if owns_capture && GetCapture() == (*pointer).window { ReleaseCapture(); }
}

pub(super) unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        (*(create.lpCreateParams as *mut State)).window = window;
    }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut State;
    if pointer.is_null() { return DefWindowProcW(window, message, wparam, lparam); }
    match catch_unwind(AssertUnwindSafe(|| {
        let result = dispatch(window, message, wparam, lparam, pointer);
        if message != WM_NCDESTROY {
            rebuild_frame(&mut *pointer);
            if message != WM_SYNC_INDICATORS && !(*pointer).indicators_queued && !(*pointer).indicator_overlay.is_null()
                && ((*pointer).indicators_dirty || matches!(message, WM_WINDOWPOSCHANGED | WM_SHOWWINDOW)) {
                (*pointer).indicators_queued = PostMessageW(window, WM_SYNC_INDICATORS, 0, 0) != 0;
            }
        }
        result
    })) {
        Ok(result) => result,
        Err(_) => {
            (*pointer).failure = Some(io::Error::other("Window callback failed"));
            if message == WM_CREATE { -1 } else { PostMessageW(window, WM_CLOSE, 0, 0); 0 }
        }
    }
}

unsafe fn dispatch(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, pointer: *mut State) -> LRESULT {
    track_hover(window, message);
    if let Some(result) = dispatch_notification_icon(window, message, wparam, lparam, &mut *pointer) { return result; }
    match message {
        WM_CONTEXTMENU | WM_NCRBUTTONUP => {
            let mut point = mouse_point(lparam);
            if point.x == -1 && point.y == -1 { GetCursorPos(&mut point); }
            if let Err(error) = context_menu(window, point, pointer) { (*pointer).failure = Some(error); }
            0
        }
        WM_SYNC_INDICATORS => {
            (*pointer).indicators_queued = false;
            rebuild_frame(&mut *pointer);
            if !sync_indicators(window, &mut *pointer) {
                (*pointer).failure = Some(io::Error::last_os_error());
                PostMessageW(window, WM_CLOSE, 0, 0);
            }
            0
        }
        WM_NCCALCSIZE => 0,
        WM_NCACTIVATE => DefWindowProcW(window, message, wparam, -1),
        WM_GETMINMAXINFO => {
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST), &mut info) == 0 { return DefWindowProcW(window, message, wparam, lparam); }
            let limits = &mut *(lparam as *mut MINMAXINFO);
            limits.ptMaxPosition = POINT { x: info.rcWork.left - info.rcMonitor.left, y: info.rcWork.top - info.rcMonitor.top };
            limits.ptMaxSize = POINT { x: info.rcWork.right - info.rcWork.left, y: info.rcWork.bottom - info.rcWork.top };
            0
        }
        WM_NCHITTEST => {
            if let Some(NativeHit::Edge(hit)) = native_hit_test(window, mouse_point(lparam)) { return hit; }
            let state = &*pointer;
            let mut point = mouse_point(lparam);
            ScreenToClient(window, &mut point);
            let point = grid_point(state, point);
            let body = body_point(point);
            let scrollbar = body.y >= 0 && body.y < state.scrolling.page && body.x == state.scrolling.width
                && state.scrolling.idle.is_some();
            let within = body.x >= 0 && body.x < state.scrolling.width && body.y >= 0 && body.y < state.scrolling.page;
            let row = within && state.cards.iter().find(|card| body.y + state.scrolling.position >= card.top
                && body.y + state.scrolling.position < card.top + card.height)
                .and_then(|card| card.key.as_ref()).is_some_and(|key| reorderable(state, key));
            if visibility_controls::hit_test(state, point).is_some() || scrollbar || row { HTCLIENT as LRESULT }
            else { HTCAPTION as LRESULT }
        }
        WM_NCLBUTTONDOWN if wparam == HTCAPTION as WPARAM => { cancel_input(pointer); DefWindowProcW(window, message, wparam, lparam) }
        WM_CREATE => {
            let state = &mut *pointer;
            if !fonts(window, state) { state.failure = Some(io::Error::last_os_error()); return -1; }
            refresh(window, state);
            layout(window, state, None, true);
            if SetTimer(window, 1, 200, None) == 0 { -1 } else { 0 }
        }
        WM_SIZE => { cancel_input(pointer); let saved = anchor(&*pointer); layout(window, &mut *pointer, saved, true); 0 }
        WM_EXITSIZEMOVE => { let _ = placement_win::save(window, &(*pointer).placement_path); 0 }
        WM_QUERYENDSESSION => { let _ = placement_win::save(window, &(*pointer).placement_path); 1 }
        WM_DPICHANGED => super::initial_geometry::update_dpi(window, pointer, lparam),
        WM_TIMER => {
            let state = &mut *pointer;
            update_hover_state(window, state);
            if state.scrolling.idle.is_some_and(|deadline| Instant::now() >= deadline) && state.scrolling.grab.is_none() {
                state.scrolling.idle = None;
                state.frame_dirty = true;
            }
            if let Some(update) = state.monitor.latest() { state.latest = Some(update); }
            if let Some((point, active)) = state.ordering.drag.as_ref().map(|drag| (drag.position, drag.active)) {
                refresh_indicators(state);
                set_status(state);
                if active && point.x >= 0 && point.x < state.scrolling.width && point.y >= 0 && point.y < state.scrolling.page {
                    if point.y == 0 { scroll_to(state, state.scrolling.position - 1); }
                    else if point.y == state.scrolling.page - 1 { scroll_to(state, state.scrolling.position + 1); }
                    move_drag(state, point);
                }
            } else { reload_preferences(state); refresh(window, state); }
            0
        }
        WM_PAINT => { rebuild_frame(&mut *pointer); paint_background(window, &*pointer); 0 }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            let state = &mut *pointer;
            SetFocus(window);
            let point = grid_point(state, mouse_point(lparam));
            if let Some(control) = visibility_controls::hit_test(state, point) {
                state.focused = Some(control.clone());
                state.pressed = Some(control);
                state.frame_dirty = true;
                SetCapture(window);
            } else if !super::row_scrolling::pointer(state, &body_point(point), message) { begin_drag(state, body_point(point)); }
            0
        }
        WM_MOUSEMOVE => {
            let state = &mut *pointer;
            let point = body_point(grid_point(state, mouse_point(lparam)));
            if !super::row_scrolling::pointer(state, &point, message) && state.ordering.drag.is_some() { move_drag(state, point); }
            0
        }
        WM_LBUTTONUP => {
            let state = &mut *pointer;
            let point = grid_point(state, mouse_point(lparam));
            if let Some(control) = state.pressed.take() {
                let activate = visibility_controls::hit_test(state, point).as_ref() == Some(&control);
                if GetCapture() == window { ReleaseCapture(); }
                if activate { visibility_controls::activate(state, control); }
                state.frame_dirty = true;
            } else if !super::row_scrolling::pointer(state, &body_point(point), message) && state.ordering.drag.is_some() { finish_drag(pointer, body_point(point)); }
            0
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => { cancel_input(pointer); 0 }
        WM_ACTIVATEAPP if wparam == 0 => { cancel_input(pointer); 0 }
        WM_KILLFOCUS => { (*pointer).focused = None; cancel_input(pointer); 0 }
        WM_KEYDOWN if wparam == 0x1b => { cancel_input(pointer); 0 }
        WM_KEYDOWN => {
            let state = &mut *pointer;
            if visibility_controls::keyboard(state, wparam, GetKeyState(0x10) < 0) || keyboard(state, wparam) { 0 }
            else { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_MOUSEWHEEL => { wheel(&mut *pointer, wparam); 0 }
        WM_CLOSE => super::dashboard_closing::close(window, pointer),
        WM_DESTROY => super::dashboard_closing::destroy(window, pointer),
        WM_NCDESTROY => { SetWindowLongPtrW(window, GWLP_USERDATA, 0); DefWindowProcW(window, message, wparam, lparam) }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

pub fn run(service: Service, launch_directory: PathBuf) -> Result<(), io::Error> {
    let order_path = service.config_path().with_extension("order.json");
    let visibility_path = service.config_path().with_extension("visibility.json");
    let placement_path = service.config_path().with_extension("placement.json");
    let opacity_path = service.config_path().with_extension("opacity.json");
    let opacity = Opacity::read(&opacity_path).unwrap_or_default();
    let ordering = Ordering::load(order_path);
    let worktree_visibility = WorktreeVisibility::load(visibility_path);
    let mut state = Box::new(State { monitor: service.monitor_from(launch_directory).map_err(io::Error::other)?, window: null_mut(),
        cards: Vec::new(), resources: None, view: DashboardView::scanning(), latest: None,
        refresh_tracker: RefreshTracker::default(), refreshing: HashSet::new(),
        dpi: 96, scrolling: Scrolling { position: 0, content: 0, page: 0, idle: None, width: 0, wheel_remainder: 0, grab: None }, failure: None,
        placement_path, opacity_path, opacity, starting: true,
        ordering: Ordering { next_check: Instant::now(), ..ordering },
        worktree_visibility: WorktreeVisibility { next_check: Instant::now(), ..worktree_visibility },
        show_all: false, controls_visible: false, focused: None, pressed: None, columns: 0, rows: 0, frame: None, frame_dirty: true,
        indicator_overlay: null_mut(), indicators_dirty: true, indicators_queued: false, notification_icon: NotificationIcon::default() });
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        state.notification_icon.initialize()?;
        let instance = GetModuleHandleW(null());
        if instance.is_null() { return Err(io::Error::last_os_error()); }
        let icon = notification_icon::class_icon(instance)?;
        let class = wide("WtmStatusWindow");
        let overlay_class = wide("WtmIndicatorOverlay");
        let definition = WNDCLASSW { lpfnWndProc: Some(procedure), hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW), hIcon: icon, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        if RegisterClassW(&definition) == 0 { return Err(io::Error::last_os_error()); }
        let overlay_definition = WNDCLASSW { lpfnWndProc: Some(indicator_procedure), lpszClassName: overlay_class.as_ptr(), ..definition };
        if RegisterClassW(&overlay_definition) == 0 {
            let error = io::Error::last_os_error();
            UnregisterClassW(class.as_ptr(), instance);
            return Err(error);
        }
        let window = super::dashboard_opening::create(class.as_ptr(), instance, &mut state);
        if window.is_null() {
            let error = state.failure.take().unwrap_or_else(io::Error::last_os_error);
            UnregisterClassW(class.as_ptr(), instance);
            UnregisterClassW(overlay_class.as_ptr(), instance);
            return Err(error);
        }
        state.indicator_overlay = create_indicator_overlay(window, instance, overlay_class.as_ptr());
        if state.indicator_overlay.is_null() {
            let error = io::Error::last_os_error();
            DestroyWindow(window);
            UnregisterClassW(overlay_class.as_ptr(), instance);
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
            state.notification_icon.install(tray.ok());
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
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if IsWindow(window) != 0 {
            KillTimer(window, 1);
            DestroyWindow(window);
        }
        let stopped = state.stop_monitor();
        UnregisterClassW(overlay_class.as_ptr(), instance);
        UnregisterClassW(class.as_ptr(), instance);
        stopped?;
        if let Some(error) = state.failure.take() { return Err(error); }
    }
    Ok(())
}
