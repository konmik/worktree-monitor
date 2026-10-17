use super::*;

pub(super) unsafe fn context_menu(window: HWND, point: POINT, pointer: *mut State) -> Result<(), io::Error> {
    match context_menu::show(window, point, (*pointer).opacity)? {
        Action::Hide => { ShowWindow(window, SW_HIDE); }
        Action::Show => show_from_tray(window),
        Action::Exit => { PostMessageW(window, WM_CLOSE, 0, 0); }
        Action::ResetPosition => {
            ShowWindow(window, SW_SHOWNORMAL);
            reset_window_bounds(window)?;
            SetForegroundWindow(window);
            placement_win::save(window, &(*pointer).placement_path)?;
        }
        Action::Opacity(opacity) => {
            opacity.write(&(*pointer).opacity_path)?;
            (*pointer).opacity = opacity;
            if SetLayeredWindowAttributes(window, 0, if (*pointer).controls_visible { 255 } else { opacity.alpha() }, LWA_ALPHA) == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Action::None => {}
    }
    PostMessageW(window, WM_NULL, 0, 0);
    Ok(())
}
