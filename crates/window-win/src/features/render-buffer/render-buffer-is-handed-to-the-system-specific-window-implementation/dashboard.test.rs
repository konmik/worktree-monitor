use super::super::opaque_indicators::IndicatorBitmap;
use super::super::runtime::State;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{BitBlt, ClientToScreen, GdiFlush, GetDC, RDW_ALLCHILDREN, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow, ReleaseDC, SRCCOPY};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static PROBE: Mutex<Option<mpsc::Sender<Snapshot>>> = Mutex::new(None);
const PROBE_TIMER: usize = 0x574d;

struct Snapshot {
    advance: i32,
    cell_width: i32,
    scroll: i32,
    content: i32,
    page: i32,
    cards: usize,
    loaded: usize,
    thumb: Vec<usize>,
    children: usize,
    native_scrollbar: bool,
    expected_pixels: Vec<u32>,
    width: i32,
    height: i32,
    pixels: Vec<u32>,
    frame_rows: usize,
    frame_columns: usize,
    header: String,
    show_all: bool,
    controls_visible: bool,
    actions: Vec<Action>,
    worktrees: Vec<String>,
    buffer: crate::render_buffer::RenderBuffer,
    dragging: bool,
    captured: bool,
}

struct Action {
    key: String,
    caption: &'static str,
    column: i32,
    row: i32,
}

unsafe extern "system" fn inspect(window: HWND, _: u32, _: usize, _: u32) {
    KillTimer(window, PROBE_TIMER);
    let mut children = 0usize;
    EnumChildWindows(window, Some(count_child), (&mut children as *mut usize) as LPARAM);
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(window, &mut rect);
    RedrawWindow(window, null(), null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW);
    let state = &*(GetWindowLongPtrW(window, GWLP_USERDATA) as *const State);
    let Some(resources) = &state.resources else { return; };
    let Some(frame) = &state.frame else { return; };
    let dc = GetDC(window);
    let Some(bitmap) = IndicatorBitmap::new(dc, rect.right, rect.bottom) else { ReleaseDC(window, dc); return; };
    BitBlt(bitmap.dc, 0, 0, rect.right, rect.bottom, dc, 0, 0, SRCCOPY);
    GdiFlush();
    let pixels = std::slice::from_raw_parts(bitmap.pixels, bitmap.length).to_vec();
    let Some(expected) = IndicatorBitmap::new(dc, rect.right, rect.bottom) else { ReleaseDC(window, dc); return; };
    super::paint_at(&frame.buffer, expected.dc, rect, resources, 0, 0,
        if state.controls_visible { super::TextLayer::All } else { super::TextLayer::Faded });
    GdiFlush();
    let expected_pixels = std::slice::from_raw_parts(expected.pixels, expected.length).to_vec();
    ReleaseDC(window, dc);
    let snapshot = Snapshot {
        advance: resources.typography.advance, cell_width: resources.space_width,
        scroll: state.scrolling.position, content: state.scrolling.content, page: state.scrolling.page,
        cards: state.cards.len(),
        loaded: state.cards.iter().filter(|card| matches!(card.display, crate::dashboard::DisplayItem::Worktree { result: Ok(_), .. })).count(),
        thumb: (2..frame.buffer.rows()).filter(|row| frame.buffer.row(*row).unwrap().last().is_some_and(|cell| cell.text() == "|")).map(|row| row - 2).collect(),
        children, native_scrollbar: GetWindowLongPtrW(window, GWL_STYLE) & (WS_VSCROLL | WS_HSCROLL) as isize != 0,
        expected_pixels, width: rect.right, height: rect.bottom, pixels,
        frame_rows: frame.buffer.rows(), frame_columns: frame.buffer.columns(),
        header: frame.buffer.row(0).unwrap_or_default().iter().map(|cell| cell.text()).collect(),
        show_all: state.show_all, controls_visible: state.controls_visible,
        actions: state.cards.iter().filter_map(|card| {
            let rect = card.action_rect?;
            let key = card.key.as_ref()?;
            Some(Action { key: format!("{key:?}"), caption: super::super::visibility_controls::action_caption(card),
                column: rect.left + rect.width / 2, row: card.top + rect.top - state.scrolling.position + 2 })
        }).collect(),
        worktrees: state.cards.iter().filter(|card| matches!(card.display, crate::dashboard::DisplayItem::Worktree { .. }))
            .filter_map(|card| card.key.as_ref().map(|key| format!("{key:?}"))).collect(),
        buffer: frame.buffer.clone(), dragging: state.ordering.drag.is_some(),
        captured: windows_sys::Win32::UI::Input::KeyboardAndMouse::GetCapture() == window,
    };
    if let Some(sender) = PROBE.lock().unwrap().as_ref() { let _ = sender.send(snapshot); }
}

unsafe extern "system" fn count_child(_: HWND, result: LPARAM) -> i32 {
    *(result as *mut usize) += 1;
    1
}

unsafe extern "system" fn find_window(window: HWND, result: LPARAM) -> i32 {
    let mut process = 0;
    GetWindowThreadProcessId(window, &mut process);
    let mut class = [0u16; 80];
    let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
    if process == std::process::id() && String::from_utf16_lossy(&class[..length.max(0) as usize]) == "WtmStatusWindow" {
        *(result as *mut HWND) = window;
        return 0;
    }
    1
}

fn snapshot(window: HWND, receiver: &mpsc::Receiver<Snapshot>) -> Snapshot {
    unsafe { assert_ne!(SetTimer(window, PROBE_TIMER, 10, Some(inspect)), 0); }
    receiver.recv_timeout(Duration::from_secs(5)).expect("dashboard UI did not answer")
}

fn check(snapshot: &Snapshot) {
    assert_eq!(snapshot.children, 0, "dashboard contains native child controls");
    assert!(!snapshot.native_scrollbar, "dashboard contains a native scrollbar");
    assert_eq!(snapshot.page, snapshot.frame_rows.saturating_sub(2) as i32, "scroll page is not measured in complete buffer rows");
    assert!(snapshot.scroll >= 0 && snapshot.scroll <= (snapshot.content - snapshot.page).max(0));
    assert_eq!(snapshot.frame_rows, (snapshot.height / snapshot.advance) as usize);
    assert_eq!(snapshot.frame_columns, (snapshot.width / snapshot.cell_width) as usize);
    assert!(snapshot.header.contains('['), "header is missing from the render buffer");
    let mismatch = snapshot.pixels.iter().zip(&snapshot.expected_pixels).position(|(actual, expected)| actual & 0xffffff != expected & 0xffffff);
    assert!(mismatch.is_none(), "window pixels differ from the single buffer at {:?}", mismatch.map(|index| (index % snapshot.width as usize, index / snapshot.width as usize)));
}

fn save(snapshot: &Snapshot, path: &Path) {
    let size = 54 + snapshot.pixels.len() * 4;
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(size as u32).to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&snapshot.width.to_le_bytes());
    bytes.extend_from_slice(&(-snapshot.height).to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 24]);
    for pixel in &snapshot.pixels { bytes.extend_from_slice(&pixel.to_le_bytes()); }
    std::fs::write(path, bytes).unwrap();
}

fn git(path: &Path, arguments: &[&str]) {
    let output = std::process::Command::new("git").current_dir(path).args(arguments).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
#[ignore = "requires the wtm icon resource linked and opens an isolated native dashboard"]
fn native_dashboard_keeps_whole_cells_while_scrolling_and_resizing() {
    let fixture = tempfile::tempdir().unwrap();
    let service = wtm_core::Service::at(fixture.path().join("config.json"));
    for index in 0..8 {
        let repo = fixture.path().join(format!("repository-{index:02}-with-a-long-name"));
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "--initial-branch=main"]);
        git(&repo, &["-c", "user.name=Native Test", "-c", "user.email=test@example.invalid", "commit", "--allow-empty", "-m", "fixture"]);
        service.add(&repo).unwrap();
    }
    let evidence = std::env::var_os("WTM_NATIVE_EVIDENCE").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("wtm-native-buffer-proof"));
    std::fs::create_dir_all(&evidence).unwrap();
    let (sender, receiver) = mpsc::channel();
    *PROBE.lock().unwrap() = Some(sender);
    let driver = std::thread::spawn(move || unsafe {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut window: HWND = null_mut();
        while window.is_null() && Instant::now() < deadline {
            EnumWindows(Some(find_window), (&mut window as *mut HWND) as LPARAM);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!window.is_null(), "native dashboard did not open");
        let mut cursor: POINT = std::mem::zeroed();
        assert_ne!(GetCursorPos(&mut cursor), 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            SetWindowPos(window, null_mut(), 40, 40, 343, 407, SWP_NOACTIVATE | SWP_NOZORDER);
            let mut top = snapshot(window, &receiver);
            while top.loaded < 8 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(100));
                top = snapshot(window, &receiver);
            }
            assert!(top.cards >= 8, "git fixtures did not reach dashboard");
            assert_eq!(top.loaded, 8, "git fixtures did not finish loading");
            assert!(top.content > top.page);
            check(&top);
            save(&top, &evidence.join("top.bmp"));
            let mut hover = POINT { x: top.cell_width * 2, y: top.advance * 3 };
            assert_ne!(ClientToScreen(window, &mut hover), 0);
            assert_ne!(SetCursorPos(hover.x, hover.y), 0);
            SendMessageW(window, WM_MOUSEMOVE, 0, ((top.advance * 3) << 16 | top.cell_width * 2) as LPARAM);
            SendMessageW(window, WM_TIMER, 1, 0);
            let controls = snapshot(window, &receiver);
            check(&controls);
            assert!(controls.controls_visible, "root hover did not reveal buffer controls");
            assert!(controls.header.contains("[ ] Show all"));
            SendMessageW(window, WM_KEYDOWN, 0x09, 0);
            SendMessageW(window, WM_KEYDOWN, 0x0d, 0);
            let keyboard_toggle = snapshot(window, &receiver);
            check(&keyboard_toggle);
            assert!(keyboard_toggle.show_all, "Tab and Enter did not activate Show all");
            assert!(keyboard_toggle.header.contains("[x] Show all"));
            let point = |column: i32, row: i32| ((row * top.advance + top.advance / 2) << 16
                | (column * top.cell_width + top.cell_width / 2)) as LPARAM;
            let click = |column: i32, row: i32| {
                SendMessageW(window, WM_LBUTTONDOWN, 1, point(column, row));
                SendMessageW(window, WM_LBUTTONUP, 0, point(column, row));
            };
            click(controls.frame_columns as i32 - 6, 0);
            let toggled = snapshot(window, &receiver);
            check(&toggled);
            assert!(!toggled.show_all, "click did not turn Show all off");
            assert!(toggled.header.contains("[ ] Show all"));
            let action = toggled.actions.iter().find(|action| action.row >= 2 && action.row < toggled.frame_rows as i32)
                .expect("visible worktree has no buffer action");
            let hidden_key = action.key.clone();
            assert_eq!(action.caption, "Hide");
            click(action.column, action.row);
            let hidden = snapshot(window, &receiver);
            check(&hidden);
            assert!(!hidden.worktrees.contains(&hidden_key), "Hide left the worktree visible");
            click(controls.frame_columns as i32 - 6, 0);
            let revealed = snapshot(window, &receiver);
            check(&revealed);
            assert!(revealed.show_all);
            assert!(revealed.worktrees.contains(&hidden_key), "Show all did not reveal the hidden worktree");
            let action = revealed.actions.iter().find(|action| action.key == hidden_key).expect("hidden worktree has no buffer action");
            assert_eq!(action.caption, "Unhide");
            let caption: String = revealed.buffer.row(action.row as usize).unwrap().iter().map(|cell| cell.text()).collect();
            assert!(caption.contains("Unhide"), "hidden worktree does not paint Unhide");
            click(action.column, action.row);
            click(controls.frame_columns as i32 - 6, 0);
            let restored = snapshot(window, &receiver);
            check(&restored);
            assert!(!restored.show_all);
            assert!(restored.worktrees.contains(&hidden_key), "Unhide did not restore the worktree");
            let action = restored.actions.iter().find(|action| action.key == hidden_key).unwrap();
            assert_eq!(action.caption, "Hide");
            let source_row = action.row as usize;
            let source_color = restored.buffer.row(source_row).unwrap()[0].color;
            SendMessageW(window, WM_LBUTTONDOWN, 1, point(1, action.row));
            SendMessageW(window, WM_MOUSEMOVE, 1, point(2, action.row));
            let dragging = snapshot(window, &receiver);
            check(&dragging);
            assert!(dragging.dragging, "root row pointer input did not start a drag");
            assert_ne!(dragging.buffer.row(source_row).unwrap()[0].color, source_color, "drag source styling is absent from the buffer");
            SendMessageW(window, WM_KEYDOWN, 0x1b, 0);
            let cancelled = snapshot(window, &receiver);
            check(&cancelled);
            assert!(!cancelled.dragging, "Escape did not cancel the root row drag");
            assert_eq!(cancelled.buffer.row(source_row).unwrap()[0].color, source_color, "cancelled drag left source styling in the buffer");
            SendMessageW(window, WM_LBUTTONDOWN, 1, point(controls.frame_columns as i32 - 6, 0));
            assert!(snapshot(window, &receiver).captured, "pressed root control did not capture the pointer");
            assert_ne!(SetCursorPos(hover.x + top.width + 50, hover.y), 0);
            SendMessageW(window, WM_TIMER, 1, 0);
            let abandoned = snapshot(window, &receiver);
            check(&abandoned);
            assert!(!abandoned.controls_visible, "leaving the root did not hide buffer controls");
            assert!(!abandoned.captured, "hidden pressed control retained pointer capture");
            SendMessageW(window, WM_LBUTTONUP, 0, point(controls.frame_columns as i32 - 6, 0));
            assert!(!snapshot(window, &receiver).show_all, "abandoned control click changed Show all");
            SendMessageW(window, WM_KEYDOWN, 0x28, 0);
            let line = snapshot(window, &receiver);
            check(&line);
            assert_eq!(line.scroll, top.scroll + 1, "line down did not move complete buffer rows");
            SendMessageW(window, WM_KEYDOWN, 0x26, 0);
            assert_eq!(snapshot(window, &receiver).scroll, top.scroll, "line up does not reverse line down");
            SendMessageW(window, WM_MOUSEWHEEL, (-60i16 as u16 as usize) << 16, 0);
            assert_eq!(snapshot(window, &receiver).scroll, top.scroll, "partial wheel tick moved content");
            SendMessageW(window, WM_MOUSEWHEEL, (-60i16 as u16 as usize) << 16, 0);
            let wheel = snapshot(window, &receiver);
            check(&wheel);
            assert_eq!(wheel.scroll, top.scroll + 3, "wheel did not move complete buffer rows");
            for key in [0x28, 0x22, 0x26, 0x21, 0x23] {
                SendMessageW(window, WM_KEYDOWN, key, 0);
                check(&snapshot(window, &receiver));
            }
            let bottom = snapshot(window, &receiver);
            check(&bottom);
            assert!(bottom.scroll > 0);
            assert_eq!(bottom.scroll, bottom.content - bottom.page, "last text row is unreachable");
            save(&bottom, &evidence.join("bottom.bmp"));
            SendMessageW(window, WM_KEYDOWN, 0x24, 0);
            let start = snapshot(window, &receiver);
            let point = |row: usize| (((row as i32 + 2) * start.advance + start.advance / 2) as usize) << 16
                | ((start.frame_columns as i32 - 1) * start.cell_width + start.cell_width / 2) as usize;
            let thumb = *start.thumb.first().expect("painted scrollbar thumb is absent");
            SendMessageW(window, WM_LBUTTONDOWN, 1, point(thumb) as LPARAM);
            SendMessageW(window, WM_MOUSEMOVE, 1, point(start.page as usize - 2) as LPARAM);
            SendMessageW(window, WM_LBUTTONUP, 0, point(start.page as usize - 2) as LPARAM);
            let dragged = snapshot(window, &receiver);
            check(&dragged);
            assert_eq!(dragged.scroll, dragged.content - dragged.page, "painted thumb cannot reach the last row");
            let thumb = *dragged.thumb.first().unwrap();
            SendMessageW(window, WM_LBUTTONDOWN, 1, point(thumb) as LPARAM);
            SendMessageW(window, WM_MOUSEMOVE, 1, point(1) as LPARAM);
            SendMessageW(window, WM_LBUTTONUP, 0, point(1) as LPARAM);
            assert_eq!(snapshot(window, &receiver).scroll, 0, "painted thumb cannot return to the first row");
            for delta in 0..=top.advance {
                SetWindowPos(window, null_mut(), 0, 0, 343 + delta, 407 + delta, SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER);
                let current = snapshot(window, &receiver);
                check(&current);
                if delta == top.advance { save(&current, &evidence.join("resized.bmp")); }
            }
            println!("native dashboard checked root hover, Show all click and keyboard, Hide, Unhide, drag cancellation, wheel, line, page, thumb, bottom, and {} single-pixel resizes; evidence {}", top.advance + 1, evidence.display());
        }));
        SetCursorPos(cursor.x, cursor.y);
        PostMessageW(window, WM_CLOSE, 0, 0);
        result
    });
    let result = crate::window::run(service, fixture.path().to_path_buf());
    if let Err(error) = &result { eprintln!("native dashboard failed to start {error:?}"); }
    let driver = driver.join().unwrap();
    *PROBE.lock().unwrap() = None;
    result.unwrap();
    if let Err(panic) = driver { std::panic::resume_unwind(panic); }
}
