use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use wtm_gui_windows::placement::{self, Monitor, Placement, Rect, ShowState};

fn monitor(identity: &str, full_bounds: Rect, work_area: Rect) -> Monitor {
    Monitor::new(identity, full_bounds, work_area)
}

fn placement(state: ShowState) -> Placement {
    Placement::new(Rect::new(-1800, -80, -1200, 520),
        monitor("\\\\.\\DISPLAY2", Rect::new(-1920, -120, 0, 1080), Rect::new(-1920, -80, 0, 1040)), state)
}

fn path(directory: &TempDir) -> PathBuf { directory.path().join("config.placement.json") }

#[test]
fn literal_bounds_and_negative_monitor_origins_are_preserved() {
    let saved = placement(ShowState::Normal);
    let restored = saved.restore_for(&saved.monitor).unwrap();
    assert_eq!(restored.normal, Rect::new(-1800, -80, -1200, 520));
    assert_eq!(restored.state, ShowState::Normal);
}

#[test]
fn restore_requires_the_same_monitor_and_full_screen_size() {
    let saved = placement(ShowState::Normal);
    let moved = monitor("\\\\.\\DISPLAY2", Rect::new(0, -120, 1920, 1080), Rect::new(0, -80, 1920, 1040));
    let resized = monitor("\\\\.\\DISPLAY2", Rect::new(-1920, -120, 1, 1080), saved.monitor.work_area);
    let missing = monitor("\\\\.\\DISPLAY3", saved.monitor.full_bounds, saved.monitor.work_area);
    assert!(saved.restore_for(&moved).is_none());
    assert!(saved.restore_for(&resized).is_none());
    assert!(saved.restore_for(&missing).is_none());
}

#[test]
fn restore_preserves_windows_partly_outside_each_work_area_edge() {
    let screen = monitor("DISPLAY1", Rect::new(0, 0, 1920, 1080), Rect::new(0, 40, 1920, 1040));
    for normal in [
        Rect::new(-200, 100, 400, 700),
        Rect::new(100, -200, 700, 400),
        Rect::new(1800, 100, 2400, 700),
        Rect::new(100, 900, 700, 1500),
    ] {
        let saved = Placement::new(normal, screen.clone(), ShowState::Normal);
        assert_eq!(saved.restore_for(&screen).unwrap().normal, normal);
    }
}

#[test]
fn restore_rejects_windows_fully_outside_or_only_touching_the_work_area() {
    let screen = monitor("DISPLAY1", Rect::new(0, 0, 1920, 1080), Rect::new(0, 40, 1920, 1040));
    for normal in [
        Rect::new(-700, 100, -100, 700),
        Rect::new(100, -700, 700, -100),
        Rect::new(2000, 100, 2600, 700),
        Rect::new(100, 1100, 700, 1700),
        Rect::new(-600, 100, 0, 700),
        Rect::new(100, -560, 700, 40),
        Rect::new(1920, 100, 2520, 700),
        Rect::new(100, 1040, 700, 1640),
    ] {
        let saved = Placement::new(normal, screen.clone(), ShowState::Normal);
        assert!(saved.restore_for(&screen).is_none(), "{normal:?}");
    }
}

#[test]
fn work_area_changes_preserve_windows_with_visible_overlap() {
    let saved = placement(ShowState::Normal);
    let smaller_work_area = monitor("\\\\.\\DISPLAY2", saved.monitor.full_bounds, Rect::new(-1920, 0, -100, 1040));
    assert_eq!(saved.restore_for(&smaller_work_area).unwrap().normal, Rect::new(-1800, -80, -1200, 520));
}

#[test]
fn work_area_changes_do_not_reset_a_fitting_window() {
    let saved = placement(ShowState::Normal);
    let changed_work_area = monitor("\\\\.\\DISPLAY2", saved.monitor.full_bounds, Rect::new(-1920, -80, -20, 1000));
    assert_eq!(saved.restore_for(&changed_work_area).unwrap().normal, Rect::new(-1800, -80, -1200, 520));
}

#[test]
fn normal_and_maximized_states_round_trip_without_a_minimized_state() {
    let directory = tempfile::tempdir().unwrap();
    for state in [ShowState::Normal, ShowState::Maximized] {
        let saved = placement(state);
        placement::write(&path(&directory), &saved).unwrap();
        assert_eq!(placement::read(&path(&directory)).unwrap(), Some(saved));
    }
    fs::write(path(&directory), br#"{"version":1,"normal":{"left":0,"top":0,"right":100,"bottom":100},"monitor":{"identity":"DISPLAY1","full_bounds":{"left":0,"top":0,"right":100,"bottom":100},"work_area":{"left":0,"top":0,"right":100,"bottom":100}},"state":"minimized"}"#).unwrap();
    assert_eq!(placement::read(&path(&directory)).unwrap(), None);
}

#[test]
fn invalid_oversized_and_unsupported_files_default() {
    let directory = tempfile::tempdir().unwrap();
    let target = path(&directory);
    for bytes in [
        Vec::new(),
        b"broken".to_vec(),
        br#"{"version":2}"#.to_vec(),
        vec![b' '; placement::MAX_BYTES + 1],
    ] {
        fs::write(&target, bytes).unwrap();
        assert_eq!(placement::read(&target).unwrap(), None);
    }
}

#[test]
fn invalid_write_preserves_the_previous_destination() {
    let directory = tempfile::tempdir().unwrap();
    let target = path(&directory);
    let saved = placement(ShowState::Normal);
    placement::write(&target, &saved).unwrap();
    let before = fs::read(&target).unwrap();
    let invalid = Placement { version: 2, ..saved };
    assert!(placement::write(&target, &invalid).is_err());
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[test]
fn a_directory_destination_is_not_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let target = path(&directory);
    fs::create_dir(&target).unwrap();
    assert!(placement::write(&target, &placement(ShowState::Normal)).is_err());
    assert!(target.is_dir());
}

#[cfg(windows)]
#[test]
fn failed_atomic_replacement_preserves_the_saved_placement() {
    let directory = tempfile::tempdir().unwrap();
    let target = path(&directory);
    let saved = placement(ShowState::Normal);
    placement::write(&target, &saved).unwrap();
    let before = fs::read(&target).unwrap();
    let original_permissions = fs::metadata(&target).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&target, permissions).unwrap();
    let result = placement::write(&target, &placement(ShowState::Maximized));
    fs::set_permissions(&target, original_permissions).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(&target).unwrap(), before);
}
