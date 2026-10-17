use std::io;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::CreateMutexW;

pub(crate) struct InstanceGuard(HANDLE);

pub(crate) fn acquire() -> io::Result<Option<InstanceGuard>> {
    acquire_named("Local\\wtm-ui")
}

fn acquire_named(name: &str) -> io::Result<Option<InstanceGuard>> {
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let guard = InstanceGuard(handle);
    Ok(if exists { None } else { Some(guard) })
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
