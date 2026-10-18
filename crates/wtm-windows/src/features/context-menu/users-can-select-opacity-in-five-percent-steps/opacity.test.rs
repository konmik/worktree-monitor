use std::fs;
use tempfile::TempDir;
use wtm_windows::opacity::Opacity;

#[test]
fn opacity_accepts_only_visible_five_percent_steps() {
    for percent in 0..=255 {
        assert_eq!(Opacity::new(percent).is_some(), (5..=100).step_by(5).any(|value| value == percent));
    }
    assert_eq!(Opacity::default().percent(), 20);
    assert_eq!(Opacity::default().alpha(), 51);
    assert_eq!(Opacity::new(5).unwrap().alpha(), 13);
    assert_eq!(Opacity::new(100).unwrap().alpha(), 255);
}

#[path = "../selected-opacity-survives-restart/tests.rs"]
mod persistence_tests;
