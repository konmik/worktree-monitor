use super::*;

#[test]
fn restoring_geometry_keeps_the_window_hidden_until_its_owner_shows_it() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, IsWindowVisible, IsZoomed, WS_POPUP};
    unsafe {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let window = CreateWindowExW(0, class.as_ptr(), class.as_ptr(), WS_POPUP, 100, 100, 640, 540,
            null_mut(), null_mut(), null_mut(), null());
        assert!(!window.is_null());
        let monitor = monitor_from_window(window).unwrap();
        let normal = Rect::new(monitor.work_area.left + 20, monitor.work_area.top + 20,
            monitor.work_area.left + 400, monitor.work_area.top + 320);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("placement.json");
        for show_state in [ShowState::Normal, ShowState::Maximized] {
            placement::write(&path, &Placement::new(normal, monitor.clone(), show_state)).unwrap();
            assert_eq!(restore_hidden_geometry(window, &path).unwrap(), Some(show_state));
            assert_eq!(IsWindowVisible(window), 0);
            assert_eq!(IsZoomed(window), 0);
            let mut actual: RECT = std::mem::zeroed();
            assert_ne!(GetWindowRect(window, &mut actual), 0);
            assert_eq!(model_rect(actual).unwrap(), normal);
        }
        DestroyWindow(window);
    }
}

#[test]
fn workspace_conversion_uses_taskbar_offsets_not_monitor_origins() {
    let monitor = Monitor::new("secondary", Rect::new(-1920, -120, 0, 1080), Rect::new(-1880, -80, 0, 1080));
    let normal = Rect::new(-1800, -60, -1200, 540);
    let workspace = RECT { left: -1840, top: -100, right: -1240, bottom: 500 };
    assert_eq!(model_rect(workspace_to_screen(workspace, &monitor, false).unwrap()).unwrap(), normal);
    assert_eq!(model_rect(workspace_to_screen(workspace, &monitor, true).unwrap()).unwrap(), Rect::new(-1840, -100, -1240, 500));
}
