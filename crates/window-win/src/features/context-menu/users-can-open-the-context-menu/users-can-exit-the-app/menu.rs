use super::*;

pub(super) const COMMAND: u32 = 2;

pub(super) unsafe fn append(menu: HMENU) -> io::Result<()> {
    if AppendMenuW(menu, MF_STRING, COMMAND as usize, wide("Exit").as_ptr()) == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
#[path = "menu.test.rs"]
mod tests;
