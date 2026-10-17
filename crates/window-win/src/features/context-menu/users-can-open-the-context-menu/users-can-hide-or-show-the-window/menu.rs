use super::*;

pub(super) const COMMAND: u32 = 1;

pub(super) unsafe fn append(menu: HMENU, visible: bool) -> io::Result<()> {
    if AppendMenuW(menu, MF_STRING, COMMAND as usize, wide(if visible { "Hide" } else { "Show" }).as_ptr()) == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn action(visible: bool) -> Action {
    if visible { Action::Hide } else { Action::Show }
}

pub(crate) unsafe fn show_window(window: HWND) {
    ShowWindow(window, if IsIconic(window) != 0 { SW_RESTORE } else { SW_SHOWNOACTIVATE });
    SetForegroundWindow(window);
}


#[cfg(test)]
#[path = "menu.test.rs"]
mod tests;
