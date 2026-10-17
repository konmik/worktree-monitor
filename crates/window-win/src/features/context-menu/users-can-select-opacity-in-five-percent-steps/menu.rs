use super::*;

pub(super) unsafe fn append(menu: HMENU, opacity: Opacity) -> io::Result<()> {
    let submenu = CreatePopupMenu();
    if submenu.is_null() { return Err(io::Error::last_os_error()); }
    let submenu = Menu(submenu);
    let populated = (5..=100).step_by(5).all(|percent| {
        AppendMenuW(submenu.0, MF_STRING, percent, wide(&format!("{percent}%")).as_ptr()) != 0
    }) && CheckMenuRadioItem(submenu.0, 5, 100, u32::from(opacity.percent()), MF_BYCOMMAND) != 0;
    if !populated || AppendMenuW(menu, MF_POPUP, submenu.0 as usize, wide("Opacity").as_ptr()) == 0 {
        return Err(io::Error::last_os_error());
    }
    std::mem::forget(submenu);
    Ok(())
}

pub(super) fn action(command: i32) -> Action {
    u8::try_from(command).ok().and_then(Opacity::new).map(Action::Opacity).unwrap_or(Action::None)
}

#[cfg(test)]
#[path = "menu.test.rs"]
mod tests;
