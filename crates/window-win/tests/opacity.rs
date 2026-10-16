use std::fs;
use tempfile::TempDir;
use window_win::opacity::Opacity;

#[test]
fn opacity_accepts_only_visible_five_percent_steps() {
    for percent in 0..=255 {
        assert_eq!(Opacity::new(percent).is_some(), percent >= 5 && percent <= 100 && percent % 5 == 0);
    }
    assert_eq!(Opacity::default().percent(), 20);
    assert_eq!(Opacity::default().alpha(), 51);
    assert_eq!(Opacity::new(5).unwrap().alpha(), 13);
    assert_eq!(Opacity::new(100).unwrap().alpha(), 255);
}

#[test]
fn selected_opacity_survives_restarts_and_replacement() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("nested/config.opacity.json");
    for percent in [5, 35, 100] {
        let opacity = Opacity::new(percent).unwrap();
        opacity.write(&path).unwrap();
        assert_eq!(Opacity::read(&path).unwrap(), opacity);
    }
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn missing_or_invalid_opacity_uses_the_default() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("config.opacity.json");
    assert_eq!(Opacity::read(&path).unwrap(), Opacity::default());
    for bytes in ["", "null", "{}", "-5", "0", "1", "101", "255", "256", "25.0", "\"25\"", "invalid"] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(Opacity::read(&path).unwrap(), Opacity::default(), "{bytes}");
    }
    fs::write(&path, vec![b' '; 1025]).unwrap();
    assert_eq!(Opacity::read(&path).unwrap(), Opacity::default());
}

#[test]
fn failed_replacement_preserves_the_destination() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("config.opacity.json");
    fs::create_dir(&path).unwrap();
    let marker = path.join("keep");
    fs::write(&marker, b"saved").unwrap();
    assert!(Opacity::new(35).unwrap().write(&path).is_err());
    assert_eq!(fs::read(&marker).unwrap(), b"saved");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
