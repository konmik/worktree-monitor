use std::io;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Console::{GetConsoleMode, GetStdHandle, SetConsoleMode, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};

pub(super) struct ConsoleModes {
    input: (HANDLE, u32),
    output: (HANDLE, u32),
}

impl ConsoleModes {
    pub(super) fn capture() -> io::Result<Self> {
        fn mode(kind: u32) -> io::Result<(HANDLE, u32)> {
            let handle = unsafe { GetStdHandle(kind) };
            let mut value = 0;
            if unsafe { GetConsoleMode(handle, &mut value) } == 0 { return Err(io::Error::last_os_error()); }
            Ok((handle, value))
        }
        Ok(Self { input: mode(STD_INPUT_HANDLE)?, output: mode(STD_OUTPUT_HANDLE)? })
    }

    pub(super) fn restore(&self) {
        unsafe {
            SetConsoleMode(self.input.0, self.input.1);
            SetConsoleMode(self.output.0, self.output.1);
        }
    }
}
