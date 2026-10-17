use super::*;

pub(super) unsafe fn apply(window: HWND) {
    PostMessageW(window, WM_CLOSE, 0, 0);
}
