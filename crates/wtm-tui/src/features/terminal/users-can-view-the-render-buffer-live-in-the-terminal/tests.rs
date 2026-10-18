use super::*;
use wtm_ui::render_buffer::CellFlags;
use wtm_ui::rendering::{MUTED, RED, WHITE};

#[test]
fn terminal_output_preserves_graphemes_colors_and_attributes() {
    let mut buffer = RenderBuffer::new(6, 1, MUTED);
    buffer.write_styled(0, 0, "界e\u{301}", RED, CellFlags { bold: true, underline: true, stressed: true });
    buffer.write(3, 0, "x", WHITE);
    let mut output = Vec::new();
    present(&mut output, &buffer).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert_eq!(text.matches('界').count(), 1);
    assert_eq!(text.matches("e\u{301}").count(), 1);
    assert!(text.contains("\x1b[38;2;255;131;137m"));
    assert!(text.contains("\x1b[1m"));
    assert!(text.contains("\x1b[4m"));
    assert!(text.contains("\x1b[0m\x1b[38;2;232;239;235mx"));
    assert!(text.ends_with("\x1b[0m\x1b[0m"));
    assert!(!text.contains(['\n', '\r']));
}

#[test]
fn terminal_output_positions_rows_without_wrapping() {
    let mut buffer = RenderBuffer::new(2, 2, MUTED);
    buffer.write(0, 0, "ab", WHITE);
    buffer.write(0, 1, "界", WHITE);
    let mut output = Vec::new();
    present(&mut output, &buffer).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("ab\x1b[2;1H界"));
    assert!(!text.contains(['\n', '\r']));
}

#[test]
fn scrolling_and_resizing_keep_the_frame_within_the_terminal() {
    let update = MonitorUpdate {
        snapshot: Arc::new(wtm_ui::Snapshot::default()), scan_targets: Vec::new(), scan_activities: Vec::new(),
        launch_error: Some(wtm_ui::Error { kind: wtm_ui::ErrorKind::Io, message: "long failure ".repeat(20) }),
        polling_time_last_minute: Duration::ZERO,
    };
    let mut view = View { latest: Some(Arc::new(update)), ..View::default() };
    let first = view.frame(20, 6);
    assert!(first.content_rows > 4);
    assert_eq!(first.scroll_row, 0);
    assert!(!view.key(event::KeyEvent::new(KeyCode::End, KeyModifiers::NONE), 6));
    let last = view.frame(20, 6);
    assert_eq!(last.scroll_row, last.content_rows - 4);
    assert_ne!(last.buffer, first.buffer);
    let resized = view.frame(80, 40);
    assert_eq!(resized.scroll_row, 0);
    assert_eq!((resized.buffer.columns(), resized.buffer.rows()), (80, 40));
    assert!(!view.key(event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 40));
    assert_eq!(view.frame(0, 0).buffer.cells(), &[]);
}

#[test]
fn exit_keys_and_scroll_keys_are_handled_without_waiting_for_a_scan() {
    let mut view = View::default();
    for code in [KeyCode::Char('q'), KeyCode::Esc] {
        assert!(view.key(event::KeyEvent::new(code, KeyModifiers::NONE), 10));
    }
    assert!(view.key(event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL), 10));
    assert!(!view.key(event::KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), 10));
    assert_eq!(view.scroll, 8);
    assert!(!view.key(event::KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), 10));
    assert_eq!(view.scroll, 7);
    assert!(!view.key(event::KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), 10));
    assert_eq!(view.scroll, 0);
}
