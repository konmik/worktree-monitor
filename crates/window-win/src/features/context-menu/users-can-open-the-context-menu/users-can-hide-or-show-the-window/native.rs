use super::*;

pub(super) unsafe fn hide(window: HWND) {
    ShowWindow(window, SW_HIDE);
}

pub(super) unsafe fn show(window: HWND) {
    context_menu::show_window(window);
}
