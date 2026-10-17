
use crate::opacity::Opacity;
use std::io;
use super::super::runtime::State;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{LWA_ALPHA, SetLayeredWindowAttributes};
pub(super) unsafe fn apply(window: HWND, state: &mut State, opacity: Opacity) -> io::Result<()> {
    opacity.write(&state.opacity_path)?;
    state.opacity = opacity;
    if SetLayeredWindowAttributes(window, 0, if state.controls_visible { 255 } else { opacity.alpha() }, LWA_ALPHA) == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
