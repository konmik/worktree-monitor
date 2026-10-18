#[cfg(windows)]
#[test]
fn executables_have_their_intended_windows_subsystems() {
    for (executable, expected) in [(env!("CARGO_BIN_EXE_wtm"), 3), (env!("CARGO_BIN_EXE_wtm-tui"), 3), (env!("CARGO_BIN_EXE_wtm-gui"), 2)] {
        let image = std::fs::read(executable).unwrap();
        assert_eq!(&image[..2], b"MZ");
        let pe = u32::from_le_bytes(image[0x3c..0x40].try_into().unwrap()) as usize;
        assert_eq!(&image[pe..pe + 4], b"PE\0\0");
        let subsystem = pe + 24 + 68;
        assert_eq!(u16::from_le_bytes(image[subsystem..subsystem + 2].try_into().unwrap()), expected, "{executable}");
    }
}
