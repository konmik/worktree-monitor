use crate::dashboard::{ellipsize_start, DashboardView, DisplayItem, Health, ItemKey, RefreshTracker};
use crate::order::{Order, OrderMove};
use crate::placement_win;
use crate::layout::{self, Rect, RowSize};
use crate::visibility::Visibility;
use crate::typography::{self, RowSpacing, Typography, FONT_SIZE};
use std::collections::{HashMap, HashSet};
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{SetScrollInfo, SetWindowTheme, DRAWITEMSTRUCT,
    ODT_BUTTON, ODT_STATIC, ODS_CHECKED, ODS_DISABLED, ODS_FOCUS, ODS_GRAYED,
    ODS_HOTLIGHT, ODS_SELECTED, BST_CHECKED, WM_MOUSELEAVE};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetCapture, GetFocus, IsWindowEnabled, ReleaseCapture, SetCapture, SetFocus};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use wts_core::{Monitor, MonitorUpdate, Service};

const BACKGROUND: COLORREF = rgb(11, 16, 18);
const GREEN: COLORREF = rgb(194, 249, 112);
const TEAL: COLORREF = rgb(84, 213, 197);
const RED: COLORREF = rgb(255, 131, 137);
const WHITE: COLORREF = rgb(232, 239, 235);
const MUTED: COLORREF = rgb(132, 150, 156);
const STATIC_OWNERDRAW: u32 = 13;
const STATIC_NOTIFY: u32 = 256;

const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF { red | (green << 8) | (blue << 16) }

struct Resources {
    font: HFONT,
    typography: Typography,
    background: HBRUSH,
    teal: HBRUSH,
    muted: HBRUSH,
}

impl Resources {
    unsafe fn new(dpi: i32) -> Option<Self> {
        let mut resources = Self { font: null_mut(), typography: Typography::new(dpi, 1), background: CreateSolidBrush(BACKGROUND),
            teal: CreateSolidBrush(TEAL), muted: CreateSolidBrush(MUTED) };
        let dc = CreateCompatibleDC(null_mut());
        if dc.is_null() { return None; }
        for face in ["Berkeley Mono", "Berkeley Mono Variable", "Consolas", "Cascadia Mono", "Courier New"] {
            let font = CreateFontW(-(FONT_SIZE * dpi / 96), 0, 0, 0, 400, 0, 0, 0,
                DEFAULT_CHARSET as u32, 0, 0, CLEARTYPE_QUALITY as u32, (FIXED_PITCH | FF_MODERN) as u32,
                wide(face).as_ptr());
            if font.is_null() { continue; }
            let old = SelectObject(dc, font);
            let mut actual = [0u16; 64];
            let length = GetTextFaceW(dc, actual.len() as i32, actual.as_mut_ptr()).max(0) as usize;
            let mut metrics: TEXTMETRICW = std::mem::zeroed();
            let monospaced = GetTextMetricsW(dc, &mut metrics) != 0 && metrics.tmPitchAndFamily & TMPF_FIXED_PITCH == 0;
            SelectObject(dc, old);
            let actual = String::from_utf16_lossy(&actual[..length.min(actual.len())]);
            if actual.trim_end_matches('\0').eq_ignore_ascii_case(face) && monospaced {
                resources.font = font;
                resources.typography = Typography::new(dpi, metrics.tmHeight);
                break;
            }
            DeleteObject(font);
        }
        DeleteDC(dc);
        if resources.font.is_null() || resources.background.is_null() || resources.teal.is_null() || resources.muted.is_null() {
            None
        } else { Some(resources) }
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            if !self.font.is_null() { DeleteObject(self.font); }
            for object in [self.background, self.teal, self.muted] {
                if !object.is_null() { DeleteObject(object); }
            }
        }
    }
}

struct Card {
    window: HWND,
    action: Option<HWND>,
    action_hidden: Option<bool>,
    key: Option<ItemKey>,
    display: DisplayItem,
    refreshing: bool,
    top: i32,
    height: i32,
    action_rect: Option<Rect>,
    spacing: RowSpacing,
}

struct State {
    monitor: Monitor,
    viewport: HWND,
    scrollbar: HWND,
    fixed: Vec<Card>,
    cards: Vec<Card>,
    resources: Option<Resources>,
    view: DashboardView,
    latest: Option<Arc<MonitorUpdate>>,
    refresh_tracker: RefreshTracker,
    refreshing: HashSet<ItemKey>,
    dpi: i32,
    scroll: i32,
    content: i32,
    page: i32,
    scrollbar_idle: Option<Instant>,
    width: i32,
    wheel_remainder: i32,
    next_card_id: usize,
    failure: Option<io::Error>,
    order_path: PathBuf,
    placement_path: PathBuf,
    starting: bool,
    order: Order,
    order_warning: Option<String>,
    order_write_failed: bool,
    order_check: Instant,
    visibility_path: PathBuf,
    visibility: Visibility,
    visibility_warning: Option<String>,
    visibility_write_failed: bool,
    visibility_check: Instant,
    show_all_button: HWND,
    next_action_id: usize,
    drag: Option<Drag>,
}

struct Drag {
    source: ItemKey,
    source_handle: HWND,
    origin: POINT,
    position: POINT,
    active: bool,
    target: Option<DropTarget>,
    marker_rect: Option<RECT>,
}

struct DropTarget {
    movement: OrderMove,
    marker: i32,
}

impl Drop for State {
    fn drop(&mut self) { let _ = self.monitor.stop(); }
}

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }
fn scaled(value: i32, dpi: i32) -> i32 { (value * dpi / 96).max(1) }
fn worktree_inset(width: i32, dpi: i32) -> i32 { scaled(14, dpi).min(width.saturating_sub(1) / 2) }
fn worktree_text_width(width: i32, dpi: i32) -> i32 {
    (width - worktree_inset(width, dpi) * 2).max(1)
}

unsafe fn set_item(card: &mut Card, display: DisplayItem) {
    if card.display != display {
        if matches!(card.display, DisplayItem::PendingWorktree { .. }) != matches!(display, DisplayItem::PendingWorktree { .. }) {
            if let Some(action) = card.action { EnableWindow(action, i32::from(!matches!(display, DisplayItem::PendingWorktree { .. }))); }
        }
        SetWindowTextW(card.window, wide(&display.caption_with_refresh(card.refreshing)).as_ptr());
        card.display = display;
        InvalidateRect(card.window, null(), 0);
    }
}

unsafe fn create_card(parent: HWND, id: usize, action_id: Option<usize>, key: Option<ItemKey>, display: DisplayItem, resources: &Resources) -> Option<Card> {
    let window = CreateWindowExW(WS_EX_CONTROLPARENT, wide("STATIC").as_ptr(), wide(&display.caption()).as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | STATIC_OWNERDRAW | STATIC_NOTIFY, 0, 0, 1, 1, parent, id as _, GetModuleHandleW(null()), null());
    if window.is_null() { return None; }
    let subclassed = if key.is_some() {
        SetWindowSubclass(window, Some(card_procedure), 1, 0)
    } else {
        SetWindowSubclass(window, Some(edge_procedure), 1, 0)
    };
    if subclassed == 0 {
        DestroyWindow(window);
        return None;
    }
    SendMessageW(window, WM_SETFONT, resources.font as WPARAM, 0);
    let action = if let Some(action_id) = action_id {
        let action = CreateWindowExW(0, wide("BUTTON").as_ptr(), wide("Hide").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32, 0, 0, 1, 1, window,
            action_id as _, GetModuleHandleW(null()), null());
        if action.is_null() {
            DestroyWindow(window);
            return None;
        }
        if SetWindowSubclass(action, Some(edge_procedure), 1, 0) == 0 {
            DestroyWindow(action);
            DestroyWindow(window);
            return None;
        }
        SendMessageW(action, WM_SETFONT, resources.font as WPARAM, 0);
        if matches!(display, DisplayItem::PendingWorktree { .. }) { EnableWindow(action, 0); }
        Some(action)
    } else { None };
    Some(Card { window, action, action_hidden: None, key, display, refreshing: false, top: 0, height: 0, action_rect: None,
        spacing: RowSpacing::new(resources.typography, false, false) })
}

unsafe fn destroy_card(card: Card) {
    DestroyWindow(card.window);
}

unsafe fn set_action(card: &mut Card, visibility: &Visibility) {
    let Some(action) = card.action else { return; };
    let Some(ItemKey::Worktree(repository, worktree)) = &card.key else { return; };
    let hidden = visibility.contains(repository, worktree);
    if card.action_hidden != Some(hidden) {
        SetWindowTextW(action, wide(if hidden { "Unhide" } else { "Hide" }).as_ptr());
        InvalidateRect(action, null(), 0);
        card.action_hidden = Some(hidden);
    }
}

unsafe fn reconcile(state: &mut State) -> bool {
    let Some(resources) = &state.resources else { return false; };
    let mut previous: HashMap<_, _> = std::mem::take(&mut state.cards).into_iter()
        .map(|card| (card.key.clone().unwrap(), card)).collect();
    let mut succeeded = true;
    for item in &state.view.items {
        if let Some(mut card) = previous.remove(&item.key) {
            set_item(&mut card, item.display.clone());
            set_action(&mut card, &state.visibility);
            state.cards.push(card);
        } else {
            let action_id = matches!(item.key, ItemKey::Worktree(_, _)).then(|| {
                let id = state.next_action_id;
                state.next_action_id += 1;
                id
            });
            if let Some(mut card) = create_card(state.viewport, state.next_card_id, action_id, Some(item.key.clone()), item.display.clone(), resources) {
                state.next_card_id += 1;
                set_action(&mut card, &state.visibility);
                state.cards.push(card);
            } else { succeeded = false; break; }
        }
    }
    for card in previous.into_values() { destroy_card(card); }
    succeeded
}

unsafe fn fonts(window: HWND, state: &mut State) -> bool {
    let dpi = GetDpiForWindow(window).max(96) as i32;
    let Some(resources) = Resources::new(dpi) else { return false; };
    for card in state.fixed.iter().chain(&state.cards) {
        SendMessageW(card.window, WM_SETFONT, resources.font as WPARAM, 0);
        if let Some(action) = card.action {
            SendMessageW(action, WM_SETFONT, resources.font as WPARAM, 0);
        }
    }
    if !state.show_all_button.is_null() {
        SendMessageW(state.show_all_button, WM_SETFONT, resources.font as WPARAM, 0);
    }
    state.resources = Some(resources);
    state.dpi = dpi;
    true
}

unsafe fn wrapped_lines(dc: HDC, text: &str, width: i32, font: HFONT) -> Vec<String> {
    typography::wrap(text, width, |value| wrapped_width(dc, value, font))
}

unsafe fn item_height(card: &Card, dc: HDC, width: i32, dpi: i32, resources: &Resources) -> i32 {
    let s = |value| scaled(value, dpi);
    let lines = match &card.display {
        DisplayItem::Repository { bare, .. } => 1 + usize::from(*bare),
        DisplayItem::PendingWorktree { .. } => 1,
        DisplayItem::Worktree { result, .. } => {
            let text_width = worktree_text_width(width, dpi);
            match result {
                Ok(_) => 2,
                Err(error) => 2 + wrapped_lines(dc, &format!("ERROR ({:?}): {}", error.kind, error.message), text_width, resources.font).len(),
            }
        }
        DisplayItem::Notice { message, .. } => 1 + if message.is_empty() { 0 } else {
            let text_width = (width - s(28)).max(1);
            wrapped_lines(dc, message, text_width, resources.font).len()
        },
        _ => 1,
    };
    card.spacing.height(resources.typography, lines)
}

fn attached_rows(previous: &DisplayItem, next: &DisplayItem) -> bool {
    matches!(previous, DisplayItem::Repository { .. })
        && matches!(next, DisplayItem::Worktree { .. } | DisplayItem::PendingWorktree { .. })
}

#[cfg(test)]
mod row_tests {
    use super::*;

    #[test]
    fn refresh_marker_pixels_follow_displayed_name_by_one_physical_pixel() {
        unsafe {
            let screen = GetDC(null_mut());
            for dpi in [96, 120, 144, 192] {
                let resources = Resources::new(dpi).unwrap();
                for (name, suffix, displayed, narrow) in [
                    ("repository", false, "repository", false),
                    ("master", false, "master", false),
                    ("repository-name", false, "repo…", true),
                    ("C:\\work\\branch", true, "…branch", true),
                ] {
                    let actual = CreateCompatibleDC(screen);
                    let expected = CreateCompatibleDC(screen);
                    SelectObject(expected, resources.font);
                    let mut name_size = std::mem::zeroed();
                    let name_wide = wide(displayed);
                    assert_ne!(GetTextExtentPoint32W(expected, name_wide.as_ptr(), (name_wide.len() - 1) as i32, &mut name_size), 0);
                    let mut marker_size = std::mem::zeroed();
                    assert_ne!(GetTextExtentPoint32W(expected, wide("*").as_ptr(), 1, &mut marker_size), 0);
                    let width = if narrow { name_size.cx + 1 + marker_size.cx } else { scaled(500, dpi) };
                    let height = resources.typography.cell;
                    let actual_bitmap = CreateCompatibleBitmap(screen, width, height);
                    let expected_bitmap = CreateCompatibleBitmap(screen, width, height);
                    let actual_old = SelectObject(actual, actual_bitmap);
                    let expected_old = SelectObject(expected, expected_bitmap);
                    let rect = RECT { left: 0, top: 0, right: width, bottom: height };
                    fill(actual, rect, resources.background);
                    fill(expected, rect, resources.background);
                    draw_name(actual, name, rect, resources.font, WHITE, suffix, true);
                    text(expected, displayed, RECT { right: name_size.cx, ..rect }, resources.font, WHITE, DT_SINGLELINE);
                    text(expected, "*", RECT { left: name_size.cx + 1, right: name_size.cx + 1 + marker_size.cx, ..rect }, resources.font, TEAL, DT_SINGLELINE);
                    let mut marker_pixels = 0;
                    for y in 0..height {
                        for x in 0..width {
                            let pixel = GetPixel(actual, x, y);
                            assert_eq!(pixel, GetPixel(expected, x, y), "DPI {dpi}, {name}, pixel {x},{y}");
                            if x > name_size.cx && pixel != BACKGROUND { marker_pixels += 1; }
                        }
                        assert_eq!(GetPixel(actual, name_size.cx, y), BACKGROUND);
                    }
                    assert!(marker_pixels > 0);
                    SelectObject(actual, actual_old);
                    SelectObject(expected, expected_old);
                    DeleteObject(actual_bitmap);
                    DeleteObject(expected_bitmap);
                    DeleteDC(actual);
                    DeleteDC(expected);
                }
            }
            ReleaseDC(null_mut(), screen);
        }
    }

    #[test]
    fn narrowing_only_adds_height_for_wrapped_text() {
        use wts_core::{Counts, Error, ErrorKind, Head, WorktreeStats};
        let items = [
            DisplayItem::Repository { name: "repository".into(), bare: false },
            DisplayItem::Repository { name: "bare".into(), bare: true },
            DisplayItem::PendingWorktree { path: "branch".into() },
            DisplayItem::Worktree { path: "branch".into(), result: Ok(WorktreeStats {
                head: Head::Branch("master".into()), counts: Counts::default(),
            }) },
            DisplayItem::Worktree { path: "branch".into(), result: Err(Error {
                kind: ErrorKind::Git, message: "failure".into(),
            }) },
        ];
        for dpi in [96, 120, 144, 192] {
            let resources = unsafe { Resources::new(dpi) }.unwrap();
            let dc = unsafe { GetDC(null_mut()) };
            for display in &items {
                for attached_before in [false, true] {
                    let card = Card { window: null_mut(), action: None, action_hidden: None, key: None,
                        display: display.clone(), refreshing: false, top: 0, height: 0, action_rect: None,
                        spacing: RowSpacing::new(resources.typography, attached_before, false) };
                    let height = unsafe { item_height(&card, dc, scaled(800, dpi), dpi, &resources) };
                    if matches!(display, DisplayItem::PendingWorktree { .. }) {
                        assert_eq!(height, card.spacing.height(resources.typography, 1));
                    }
                    for width in [359, 360, 361] {
                        assert_eq!(unsafe { item_height(&card, dc, scaled(width, dpi), dpi, &resources) }, height);
                    }
                    if matches!(display, DisplayItem::Worktree { result: Err(_), .. }) {
                        assert!(unsafe { item_height(&card, dc, scaled(100, dpi), dpi, &resources) } > height);
                    }
                }
            }
            unsafe { ReleaseDC(null_mut(), dc); }
        }
    }

    #[test]
    fn attached_narrow_heading_keeps_refresh_inline() {
        let repository = DisplayItem::Repository { name: "repository".into(), bare: false };
        let branch = DisplayItem::PendingWorktree { path: PathBuf::from("branch") };
        let notice = DisplayItem::Notice { title: "error".into(), message: "failure".into(), error: true };
        assert!(attached_rows(&repository, &branch));
        assert!(!attached_rows(&repository, &notice));
        assert!(!attached_rows(&branch, &branch));
        for dpi in [96, 120, 144, 192] {
            let resources = unsafe { Resources::new(dpi) }.unwrap();
            let typography = resources.typography;
            let spacing = RowSpacing::new(typography, false, true);
            let card = Card { window: null_mut(), action: None, action_hidden: None, key: None,
                display: repository.clone(), refreshing: false, top: 0, height: 0, action_rect: None, spacing };
            assert_eq!(unsafe { item_height(&card, null_mut(), scaled(180, dpi), dpi, &resources) },
                typography.padding + typography.cell.max(typography.advance));
        }
    }
}

fn anchor(state: &State) -> Option<(ItemKey, i32)> {
    state.cards.iter().find(|card| card.top + card.height > state.scroll)
        .and_then(|card| card.key.clone().map(|key| (key, state.scroll - card.top)))
}

unsafe fn position_cards(state: &State) {
    let mut placements = Vec::with_capacity(state.cards.len());
    for card in &state.cards {
        let top = card.top - state.scroll;
        placements.push((card.window, RECT { left: 0, top, right: state.width, bottom: top + card.height }));
    }
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS;
    let mut batch = BeginDeferWindowPos(placements.len() as i32);
    for (window, rect) in &placements {
        if batch.is_null() { break; }
        batch = DeferWindowPos(batch, *window, null_mut(), rect.left, rect.top,
            rect.right - rect.left, rect.bottom - rect.top, flags);
    }
    if batch.is_null() || EndDeferWindowPos(batch) == 0 {
        for (window, rect) in placements {
            SetWindowPos(window, null_mut(), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, flags);
        }
    }
    for card in &state.cards { position_action(card); }
    InvalidateRect(state.viewport, null(), 0);
}

unsafe fn position_action(card: &Card) {
    let Some(action) = card.action else { return; };
    let Some(rect) = card.action_rect else { return; };
    SetWindowPos(action, null_mut(), rect.left, rect.top, rect.width, rect.height,
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS);
}

unsafe fn wake_scrollbar(state: &mut State) {
    if state.scrollbar.is_null() { return; }
    if state.content > state.page {
        ShowWindow(state.scrollbar, SW_SHOWNOACTIVATE);
        state.scrollbar_idle = Some(Instant::now() + Duration::from_secs(3));
    } else {
        ShowWindow(state.scrollbar, SW_HIDE);
        state.scrollbar_idle = None;
    }
}

unsafe fn update_scrollbar(state: &mut State, activity: bool) {
    if state.scrollbar.is_null() { return; }
    if state.content <= state.page {
        ShowWindow(state.scrollbar, SW_HIDE);
        state.scrollbar_idle = None;
    } else if activity {
        wake_scrollbar(state);
    }
}

unsafe fn set_scroll_info(state: &State) {
    let info = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0, nMax: (state.content - 1).max(0), nPage: state.page.max(0) as u32, nPos: state.scroll, ..std::mem::zeroed() };
    if !state.scrollbar.is_null() {
        SetScrollInfo(state.scrollbar, SB_CTL, &info, 1);
        InvalidateRect(state.scrollbar, null(), 0);
    }
}

unsafe fn scroll_to(state: &mut State, position: i32) {
    let position = position.clamp(0, (state.content - state.page).max(0));
    if position != state.scroll {
        state.scroll = position;
        set_scroll_info(state);
        position_cards(state);
        let marker = state.drag.as_ref().and_then(|drag| drag.target.as_ref().map(|target| target.marker));
        let marker_rect = marker.and_then(|marker| project_marker(state, marker));
        if let Some(drag) = state.drag.as_mut() { drag.marker_rect = marker_rect; }
    }
    wake_scrollbar(state);
}

unsafe fn layout(window: HWND, state: &mut State, restore: Option<(ItemKey, i32)>, scrollbar_activity: bool) {
    if state.resources.is_none() { return; }
    if state.fixed.len() != 1 || state.viewport.is_null() { return; }
    let was_overflowing = state.content > state.page;
    let mut client: RECT = std::mem::zeroed();
    if GetClientRect(window, &mut client) == 0 || client.right <= 0 || client.bottom <= 0 { return; }
    let dashboard = match layout::dashboard(client.right, client.bottom, state.dpi, state.resources.as_ref().unwrap().typography) {
        Ok(layout) => layout,
        Err(error) => { state.failure = Some(error); PostMessageW(window, WM_CLOSE, 0, 0); return; }
    };
    for (control, rect) in [(state.fixed[0].window, dashboard.status), (state.show_all_button, dashboard.toggle),
        (state.viewport, dashboard.viewport), (state.scrollbar, dashboard.scrollbar)] {
        if !control.is_null() { MoveWindow(control, rect.left, rect.top, rect.width.max(1), rect.height.max(1), 0); }
    }
    let mut viewport: RECT = std::mem::zeroed();
    GetClientRect(state.viewport, &mut viewport);
    state.width = viewport.right.max(1);
    state.page = viewport.bottom.max(1);
    let resources = state.resources.as_ref().unwrap();
    let attachments: Vec<_> = state.cards.windows(2).map(|cards| attached_rows(&cards[0].display, &cards[1].display)).collect();
    for (index, card) in state.cards.iter_mut().enumerate() {
        card.spacing = RowSpacing::new(resources.typography,
            index > 0 && attachments[index - 1], attachments.get(index).copied().unwrap_or(false));
    }
    let dc = GetDC(window);
    let sizes: Vec<_> = state.cards.iter().map(|card| RowSize {
        height: item_height(card, dc, state.width, state.dpi, resources), action: card.action.is_some(), spacing: card.spacing,
    }).collect();
    ReleaseDC(window, dc);
    let content = match layout::content(state.width, state.dpi, resources.typography, &sizes) {
        Ok(layout) => layout,
        Err(error) => { state.failure = Some(error); PostMessageW(window, WM_CLOSE, 0, 0); return; }
    };
    for (card, row) in state.cards.iter_mut().zip(content.rows) {
        card.top = row.rect.top;
        card.height = row.rect.height;
        card.action_rect = row.action;
    }
    state.content = content.height;
    if let Some((key, offset)) = &restore {
        if let Some(card) = state.cards.iter().find(|card| card.key.as_ref() == Some(key)) {
            state.scroll = card.top.saturating_add(*offset);
        }
    }
    state.scroll = state.scroll.clamp(0, (state.content - state.page).max(0));
    set_scroll_info(state);
    update_scrollbar(state, scrollbar_activity || !was_overflowing && state.content > state.page);
    position_cards(state);
    RedrawWindow(window, null(), null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_FRAME);
}

unsafe fn text(dc: HDC, value: &str, rect: RECT, font: HFONT, color: COLORREF, flags: u32) {
    if rect.right <= rect.left || rect.bottom <= rect.top { return; }
    SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT as i32);
    let mut rect = rect;
    let mut value = wide(value);
    DrawTextW(dc, value.as_mut_ptr(), (value.len() - 1) as i32, &mut rect, flags | DT_NOPREFIX);
}

unsafe fn fill(dc: HDC, rect: RECT, brush: HBRUSH) { FillRect(dc, &rect, brush); }

unsafe fn wrapped_text(dc: HDC, value: &str, rect: RECT, resources: &Resources, color: COLORREF) {
    for (index, line) in wrapped_lines(dc, value, rect.right - rect.left, resources.font).iter().enumerate() {
        let top = rect.top + index as i32 * resources.typography.advance;
        text(dc, line, RECT { top, bottom: top + resources.typography.cell, ..rect }, resources.font, color, DT_SINGLELINE);
    }
}

unsafe fn paint_card(draw: &DRAWITEMSTRUCT, card: &Card, state: &State) {
    let width = draw.rcItem.right - draw.rcItem.left;
    let height = draw.rcItem.bottom - draw.rcItem.top;
    if width <= 0 || height <= 0 { return; }
    let memory = CreateCompatibleDC(draw.hDC);
    let bitmap = CreateCompatibleBitmap(draw.hDC, width, height);
    if !memory.is_null() && !bitmap.is_null() {
        let old = SelectObject(memory, bitmap);
        let buffered = DRAWITEMSTRUCT { hDC: memory,
            rcItem: RECT { left: 0, top: 0, right: width, bottom: height }, ..*draw };
        draw_card(&buffered, card, state);
        BitBlt(draw.hDC, draw.rcItem.left, draw.rcItem.top, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old);
    } else { draw_card(draw, card, state); }
    if !bitmap.is_null() { DeleteObject(bitmap); }
    if !memory.is_null() { DeleteDC(memory); }
}

unsafe fn draw_card(draw: &DRAWITEMSTRUCT, card: &Card, state: &State) {
    let Some(resources) = &state.resources else { return; };
    let dc = draw.hDC;
    let saved = SaveDC(dc);
    let rect = draw.rcItem;
    let s = |value| scaled(value, state.dpi);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let line = DT_SINGLELINE | DT_END_ELLIPSIS;
    let caption = card.display.caption();
    let lines: Vec<_> = caption.lines().collect();
    let bounds = |left, top, right, bottom| RECT { left: rect.left + left, top: rect.top + top,
        right: rect.left + right, bottom: rect.top + bottom };
    let typography = resources.typography;
    let line_bounds = |left, index, right| {
        let top = card.spacing.top(typography, index);
        bounds(left, top, right, top + typography.cell)
    };
    let heading_right = if matches!(card.display, DisplayItem::Repository { .. }) { width - s(12) }
        else { card.action_rect.map_or(width - s(12), |rect| rect.left - s(8)) };
    let dragging = state.drag.as_ref().is_some_and(|drag| drag.source_handle == card.window);
    fill(dc, rect, resources.background);
    match &card.display {
        DisplayItem::Status { health, .. } => {
            let color = match health { Health::Live => GREEN, Health::Errors => RED, Health::Scanning => MUTED };
            let y = (height - typography.cell) / 2;
            text(dc, &format!("> {caption}"), bounds(0, y, width, y + typography.cell), resources.font, color, line);
        }
        DisplayItem::Repository { bare, .. } => {
            draw_name(dc, lines[0], line_bounds(s(14), 0, heading_right), resources.font,
                if dragging { GREEN } else { TEAL }, false, card.refreshing);
            if *bare { text(dc, lines[1], line_bounds(s(14), 1, width - s(12)), resources.font, MUTED, line); }
        }
        DisplayItem::Worktree { path, result } => {
            let inset = worktree_inset(width, state.dpi);
            let text_width = worktree_text_width(width, state.dpi);
            let right = inset + text_width;
            let heading_width = (heading_right - inset).max(0).min(text_width);
            let path = path.display().to_string();
            match result {
                Ok(stats) => {
                    draw_name(dc, lines[0], line_bounds(inset, 0, inset + heading_width), resources.font,
                        if dragging { GREEN } else { WHITE }, false, card.refreshing);
                    let y = card.spacing.top(typography, 1);
                    let mut parts = vec![
                        ("S ".to_owned(), MUTED),
                        (format!("+{}", stats.counts.source.additions), GREEN),
                        ("/".to_owned(), MUTED),
                        (format!("-{}", stats.counts.source.deletions), RED),
                        (format!(" · T +{}/-{} · X +{}/-{}", stats.counts.test.additions,
                            stats.counts.test.deletions, stats.counts.extra.additions, stats.counts.extra.deletions), MUTED),
                    ];
                    if stats.counts.binaries > 0 {
                        parts.push((format!(" · binaries: {}", stats.counts.binaries), MUTED));
                    }
                    let mut x = inset;
                    for (part, color) in parts {
                        let part_width = wrapped_width(dc, &part, resources.font).min(right.saturating_sub(x));
                        if part_width > 0 {
                            text(dc, &part, bounds(x, y, (x + part_width).min(right), y + typography.cell), resources.font, if dragging { GREEN } else { color }, line);
                        }
                        x = (x + part_width).min(right);
                    }
                }
                Err(error) => {
                    draw_name(dc, &path, line_bounds(inset, 0, inset + heading_width), resources.font,
                        if dragging { GREEN } else { MUTED }, true, card.refreshing);
                    text(dc, lines[1], line_bounds(inset, 1, right), resources.font, MUTED, line);
                    wrapped_text(dc, &format!("ERROR ({:?}): {}", error.kind, error.message), line_bounds(inset, 2, right), resources, RED);
                }
            }
        }
        DisplayItem::PendingWorktree { path } => {
            let inset = worktree_inset(width, state.dpi);
            let heading_width = (heading_right - inset).max(0);
            draw_name(dc, &path.display().to_string(), line_bounds(inset, 0, inset + heading_width), resources.font,
                if dragging { GREEN } else { MUTED }, true, card.refreshing);
        }
        DisplayItem::Notice { title, message, error } => {
            text(dc, title, line_bounds(s(14), 0, width - s(14)), resources.font, if *error { RED } else { WHITE }, line);
            wrapped_text(dc, message, line_bounds(s(14), 1, width - s(14)), resources, if *error { RED } else { MUTED });
        }
    }
    if let Some(target) = state.drag.as_ref().and_then(|drag| drag.target.as_ref()) {
        let y = if target.marker == card.top { Some(0) }
            else if target.marker == card.top + card.height { Some(height - s(2)) } else { None };
        if let Some(y) = y { fill(dc, bounds(0, y, width, y + s(2)), resources.teal); }
    }
    RestoreDC(dc, saved);
}

unsafe fn wrapped_width(dc: HDC, value: &str, font: HFONT) -> i32 {
    let saved = SaveDC(dc);
    SelectObject(dc, font);
    let mut size = std::mem::zeroed();
    let value = wide(value);
    GetTextExtentPoint32W(dc, value.as_ptr(), (value.len() - 1) as i32, &mut size);
    RestoreDC(dc, saved);
    size.cx
}

unsafe fn draw_name(dc: HDC, value: &str, rect: RECT, font: HFONT, color: COLORREF, suffix: bool, refreshing: bool) {
    let marker_width = wrapped_width(dc, "*", font);
    let available = (rect.right - rect.left - marker_width - 1).max(0);
    let measure = |value: &str| wrapped_width(dc, value, font);
    let fitted = if suffix { ellipsize_start(value, available, measure) }
        else if measure(value) <= available { value.to_owned() }
        else if measure("…") > available { String::new() }
        else {
            let mut fitted = "…".to_owned();
            for (index, character) in value.char_indices() {
                let candidate = format!("{}…", &value[..index + character.len_utf8()]);
                if measure(&candidate) > available { break; }
                fitted = candidate;
            }
            fitted
        };
    let name_right = rect.left + measure(&fitted);
    text(dc, &fitted, RECT { right: name_right, ..rect }, font, color, DT_SINGLELINE);
    if refreshing && !fitted.is_empty() {
        text(dc, "*", RECT { left: name_right + 1, right: name_right + 1 + marker_width, ..rect }, font, TEAL, DT_SINGLELINE);
    }
}

unsafe fn button_caption(window: HWND) -> String {
    let mut value = [0u16; 128];
    let length = GetWindowTextW(window, value.as_mut_ptr(), value.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&value[..length.min(value.len())])
}

unsafe fn show_all_checked(state: &State) -> bool {
    SendMessageW(state.show_all_button, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT
}

unsafe fn paint_button(draw: &DRAWITEMSTRUCT, state: &State) {
    let width = draw.rcItem.right - draw.rcItem.left;
    let height = draw.rcItem.bottom - draw.rcItem.top;
    if width <= 0 || height <= 0 { return; }
    let memory = CreateCompatibleDC(draw.hDC);
    let bitmap = CreateCompatibleBitmap(draw.hDC, width, height);
    if !memory.is_null() && !bitmap.is_null() {
        let old = SelectObject(memory, bitmap);
        let buffered = DRAWITEMSTRUCT { hDC: memory,
            rcItem: RECT { left: 0, top: 0, right: width, bottom: height }, ..*draw };
        draw_button(&buffered, state);
        BitBlt(draw.hDC, draw.rcItem.left, draw.rcItem.top, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old);
    } else { draw_button(draw, state); }
    if !bitmap.is_null() { DeleteObject(bitmap); }
    if !memory.is_null() { DeleteDC(memory); }
}

unsafe fn draw_button(draw: &DRAWITEMSTRUCT, state: &State) {
    let Some(resources) = &state.resources else { return; };
    let saved = SaveDC(draw.hDC);
    let rect = draw.rcItem;
    let s = |value| scaled(value, state.dpi);
    let disabled = draw.itemState & (ODS_DISABLED | ODS_GRAYED) != 0;
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let focused = draw.itemState & ODS_FOCUS != 0;
    let hot = draw.itemState & ODS_HOTLIGHT != 0;
    let checked = draw.itemState & ODS_CHECKED != 0;
    fill(draw.hDC, rect, resources.background);
    let padding = s(8);
    let text_top = rect.top + (rect.bottom - rect.top - resources.typography.cell).max(0) / 2;
    let text_rect = RECT { left: rect.left + padding, top: text_top,
        right: rect.right - padding, bottom: (text_top + resources.typography.cell).min(rect.bottom) };
    let color = if disabled { MUTED } else if checked || pressed { GREEN }
        else if focused || hot { TEAL } else { WHITE };
    text(draw.hDC, &button_caption(draw.hwndItem), text_rect, resources.font, color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    RestoreDC(draw.hDC, saved);
}

unsafe fn draw_checkbox(dc: HDC, rect: RECT, window: HWND, state: &State) {
    let Some(resources) = &state.resources else { return; };
    let saved = SaveDC(dc);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width > 0 && height > 0 {
        fill(dc, rect, resources.background);
        let checked = SendMessageW(window, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT;
        let focused = GetFocus() == window;
        let disabled = IsWindowEnabled(window) == 0;
        let pressed = SendMessageW(window, BM_GETSTATE, 0, 0) & BST_PUSHED as LRESULT != 0;
        text(dc, if checked { "[x] Show all" } else { "[ ] Show all" }, rect,
            resources.font, if disabled { MUTED } else if pressed { GREEN } else if focused { TEAL }
                else if checked { GREEN } else { WHITE }, DT_SINGLELINE | DT_VCENTER);
    }
    RestoreDC(dc, saved);
}

fn intersection(left: RECT, right: RECT) -> Option<RECT> {
    let rect = RECT { left: left.left.max(right.left), top: left.top.max(right.top),
        right: left.right.min(right.right), bottom: left.bottom.min(right.bottom) };
    (rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

unsafe fn paint_background(window: HWND, resources: &Resources, marker: Option<RECT>) {
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(window, &mut paint);
    let rect = paint.rcPaint;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let marker = marker.and_then(|marker| intersection(marker, rect)).map(|mut marker| {
        marker.left -= rect.left;
        marker.top -= rect.top;
        marker.right -= rect.left;
        marker.bottom -= rect.top;
        marker
    });
    if width > 0 && height > 0 && !dc.is_null() {
        let memory = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, width, height);
        if !memory.is_null() && !bitmap.is_null() {
            let old = SelectObject(memory, bitmap);
            fill(memory, RECT { left: 0, top: 0, right: width, bottom: height }, resources.background);
            if let Some(marker) = marker.as_ref() { fill(memory, *marker, resources.teal); }
            BitBlt(dc, rect.left, rect.top, width, height, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
        } else {
            fill(dc, rect, resources.background);
            if let Some(marker) = marker.as_ref() {
                fill(dc, RECT { left: marker.left + rect.left, top: marker.top + rect.top,
                    right: marker.right + rect.left, bottom: marker.bottom + rect.top }, resources.teal);
            }
        }
        if !bitmap.is_null() { DeleteObject(bitmap); }
        if !memory.is_null() { DeleteDC(memory); }
    }
    EndPaint(window, &paint);
}

unsafe fn draw_item(state: &State, lparam: LPARAM) -> LRESULT {
    let draw = &*(lparam as *const DRAWITEMSTRUCT);
    if draw.CtlType == ODT_STATIC {
        if let Some(card) = state.fixed.iter().chain(&state.cards).find(|card| card.window == draw.hwndItem) {
            paint_card(draw, card, state);
            return 1;
        }
    }
    if draw.CtlType == ODT_BUTTON {
        paint_button(draw, state);
        return 1;
    }
    0
}

unsafe fn handle_scroll(state: &mut State, wparam: WPARAM) {
    wake_scrollbar(state);
    let code = (wparam & 0xffff) as i32;
    let line = scaled(32, state.dpi);
    let page = (state.page - line).max(line);
    let target = match code {
        SB_LINEUP => state.scroll.saturating_sub(line),
        SB_LINEDOWN => state.scroll.saturating_add(line),
        SB_PAGEUP => state.scroll.saturating_sub(page),
        SB_PAGEDOWN => state.scroll.saturating_add(page),
        SB_TOP => 0,
        SB_BOTTOM => state.content,
        SB_THUMBTRACK | SB_THUMBPOSITION => {
            let mut info = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_TRACKPOS, ..std::mem::zeroed() };
            if state.scrollbar.is_null() || GetScrollInfo(state.scrollbar, SB_CTL, &mut info) == 0 { return; }
            info.nTrackPos
        }
        _ => return,
    };
    scroll_to(state, target);
}

unsafe fn wheel(state: &mut State, wparam: WPARAM) {
    state.wheel_remainder += ((wparam >> 16) as u16 as i16) as i32;
    let steps = state.wheel_remainder / 120;
    state.wheel_remainder %= 120;
    scroll_to(state, state.scroll.saturating_sub(steps * scaled(72, state.dpi)));
}

unsafe fn keyboard(state: &mut State, key: WPARAM) -> bool {
    let code = match key as u32 {
        0x26 => SB_LINEUP, 0x28 => SB_LINEDOWN, 0x21 => SB_PAGEUP, 0x22 => SB_PAGEDOWN,
        0x24 => SB_TOP, 0x23 => SB_BOTTOM, _ => return false,
    };
    handle_scroll(state, code as WPARAM);
    true
}

fn reorderable(state: &State, key: &ItemKey) -> bool {
    let Some(update) = &state.latest else { return false; };
    match key {
        ItemKey::Repository(repository) => update.snapshot.repos.iter()
            .any(|repo| crate::order::identity(&repo.common_dir) == *repository),
        ItemKey::Worktree(repository, path) => update.snapshot.repos.iter()
            .find(|repo| crate::order::identity(&repo.common_dir) == *repository)
            .and_then(|repo| repo.worktrees.as_ref().ok())
            .is_some_and(|trees| trees.iter().any(|tree| crate::order::identity(&tree.path) == *path)),
        _ => false,
    }
}

fn mouse_point(lparam: LPARAM) -> POINT {
    POINT { x: (lparam as u16 as i16) as i32, y: ((lparam >> 16) as u16 as i16) as i32 }
}

#[derive(Clone, Copy)]
enum NativeHit {
    Edge(LRESULT),
    Child,
    Empty,
}

unsafe fn resize_hit(window: HWND, point: POINT) -> Option<LRESULT> {
    if IsZoomed(window) != 0 { return None; }
    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(window, &mut rect) == 0 || point.x < rect.left || point.x >= rect.right
        || point.y < rect.top || point.y >= rect.bottom { return None; }
    let dpi = GetDpiForWindow(window).max(96) as i32;
    let frame_x = (GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi as u32)
        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi as u32)).max(scaled(4, dpi));
    let frame_y = (GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi as u32)
        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi as u32)).max(scaled(4, dpi));
    let left = point.x < rect.left + frame_x;
    let right = point.x >= rect.right - frame_x;
    let top = point.y < rect.top + frame_y;
    let bottom = point.y >= rect.bottom - frame_y;
    Some(match (left, right, top, bottom) {
        (true, _, true, _) => HTTOPLEFT as LRESULT,
        (_, true, true, _) => HTTOPRIGHT as LRESULT,
        (true, _, _, true) => HTBOTTOMLEFT as LRESULT,
        (_, true, _, true) => HTBOTTOMRIGHT as LRESULT,
        (_, _, true, _) => HTTOP as LRESULT,
        (_, _, _, true) => HTBOTTOM as LRESULT,
        (true, _, _, _) => HTLEFT as LRESULT,
        (_, true, _, _) => HTRIGHT as LRESULT,
        _ => return None,
    })
}

unsafe fn native_hit_test(root: HWND, viewport: HWND, point: POINT) -> Option<NativeHit> {
    if let Some(hit) = resize_hit(root, point) { return Some(NativeHit::Edge(hit)); }
    let mut client = point;
    if ScreenToClient(root, &mut client) == 0 { return None; }
    let mut bounds: RECT = std::mem::zeroed();
    if GetClientRect(root, &mut bounds) == 0 || client.x < 0 || client.x >= bounds.right
        || client.y < 0 || client.y >= bounds.bottom { return None; }
    let child = ChildWindowFromPointEx(root, client, CWP_SKIPINVISIBLE);
    if child.is_null() { return None; }
    if child == root { return Some(NativeHit::Empty); }
    if child == viewport {
        let mut viewport_point = point;
        if ScreenToClient(viewport, &mut viewport_point) == 0 { return None; }
        let nested = ChildWindowFromPointEx(viewport, viewport_point, CWP_SKIPINVISIBLE);
        if nested.is_null() || nested == viewport { Some(NativeHit::Empty) } else { Some(NativeHit::Child) }
    } else {
        Some(NativeHit::Child)
    }
}

unsafe fn edge_hit_transparent(window: HWND, point: POINT) -> bool {
    let root = GetAncestor(window, GA_ROOT);
    resize_hit(root, point).is_some()
}

unsafe extern "system" fn edge_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if message == WM_NCDESTROY { RemoveWindowSubclass(window, Some(edge_procedure), 1); }
    if message == WM_NCHITTEST && edge_hit_transparent(window, mouse_point(lparam)) {
        return HTTRANSPARENT as LRESULT;
    }
    DefSubclassProc(window, message, wparam, lparam)
}

unsafe fn draw_scrollbar_chevron(dc: HDC, rect: RECT, up: bool) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 3 || height < 3 { return; }
    let half_width = ((width - 2).min(6) / 2).max(1);
    let half_height = ((height - 2).min(6) / 2).max(1);
    let center_x = (rect.left + rect.right) / 2;
    let center_y = (rect.top + rect.bottom) / 2;
    if up {
        MoveToEx(dc, center_x - half_width, center_y + half_height, null_mut());
        LineTo(dc, center_x, center_y - half_height);
        LineTo(dc, center_x + half_width, center_y + half_height);
    } else {
        MoveToEx(dc, center_x - half_width, center_y - half_height, null_mut());
        LineTo(dc, center_x, center_y + half_height);
        LineTo(dc, center_x + half_width, center_y - half_height);
    }
}

unsafe fn paint_scrollbar(window: HWND, state: &State, message: u32, wparam: WPARAM) {
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let dc = if message == WM_PAINT { BeginPaint(window, &mut paint) } else { wparam as HDC };
    if !dc.is_null() {
        if let Some(resources) = &state.resources {
            let mut client: RECT = std::mem::zeroed();
            if GetClientRect(window, &mut client) != 0 {
                fill(dc, client, resources.background);
                let mut info = SCROLLBARINFO { cbSize: std::mem::size_of::<SCROLLBARINFO>() as u32, ..std::mem::zeroed() };
                if GetScrollBarInfo(window, OBJID_CLIENT, &mut info) != 0 {
                    let mut origin = POINT { x: info.rcScrollBar.left, y: info.rcScrollBar.top };
                    let mut end = POINT { x: info.rcScrollBar.right, y: info.rcScrollBar.bottom };
                    if ScreenToClient(window, &mut origin) != 0 && ScreenToClient(window, &mut end) != 0 {
                        let bar = RECT { left: origin.x, top: origin.y, right: end.x, bottom: end.y };
                        let arrow = info.dxyLineButton.max(0).min((bar.bottom - bar.top).max(0) / 2);
                        let top_arrow = RECT { left: bar.left, top: bar.top, right: bar.right, bottom: bar.top + arrow };
                        let bottom_arrow = RECT { left: bar.left, top: bar.bottom - arrow, right: bar.right, bottom: bar.bottom };
                        let pen = CreatePen(PS_SOLID, 1, MUTED);
                        if !pen.is_null() {
                            let old = SelectObject(dc, pen);
                            if let Some(rect) = intersection(top_arrow, client) {
                                draw_scrollbar_chevron(dc, rect, true);
                            }
                            if let Some(rect) = intersection(bottom_arrow, client) {
                                draw_scrollbar_chevron(dc, rect, false);
                            }
                            SelectObject(dc, old);
                            DeleteObject(pen);
                        }
                        if info.xyThumbBottom > info.xyThumbTop {
                            let top = (bar.top + info.xyThumbTop).clamp(client.top, client.bottom);
                            let bottom = (bar.top + info.xyThumbBottom).clamp(top, client.bottom);
                            let width = (bar.right - bar.left).max(1);
                            let inset = scaled(1, state.dpi).min(width.saturating_sub(1) / 2);
                            let thumb = RECT { left: (bar.left + inset).max(client.left), top,
                                right: (bar.right - inset).min(client.right), bottom };
                            if thumb.left < thumb.right && thumb.top < thumb.bottom {
                                fill(dc, thumb, if GetCapture() == window { resources.teal } else { resources.muted });
                            }
                        }
                    }
                }
            }
        }
    }
    if message == WM_PAINT { EndPaint(window, &paint); }
}

unsafe extern "system" fn scrollbar_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, data: usize) -> LRESULT {
    if message == WM_NCDESTROY { RemoveWindowSubclass(window, Some(scrollbar_procedure), 1); }
    if message == WM_NCHITTEST {
        let point = mouse_point(lparam);
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(window, &mut rect) != 0 && point.x >= rect.left && point.x < rect.right
            && point.y >= rect.top && point.y < rect.bottom { return HTCLIENT as LRESULT; }
        if edge_hit_transparent(window, point) { return HTTRANSPARENT as LRESULT; }
    }
    if matches!(message, WM_PAINT | WM_PRINTCLIENT) {
        paint_scrollbar(window, &*(data as *const State), message, wparam);
        return 0;
    }
    if message == WM_ERASEBKGND { return 1; }
    let result = DefSubclassProc(window, message, wparam, lparam);
    if matches!(message, WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEMOVE | WM_MOUSELEAVE | 0x0118
        | WM_CAPTURECHANGED | WM_CANCELMODE | WM_SETFOCUS | WM_KILLFOCUS) {
        InvalidateRect(window, null(), 0);
        UpdateWindow(window);
    }
    result
}

fn project_marker(state: &State, marker: i32) -> Option<RECT> {
    let thickness = scaled(2, state.dpi);
    let y = marker.saturating_sub(state.scroll);
    let rect = RECT { left: 0, top: y.saturating_sub(thickness).max(0), right: state.width, bottom: y.min(state.page) };
    (state.width > 0 && state.page > 0 && rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

fn same_rect(left: Option<&RECT>, right: Option<&RECT>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.left == right.left && left.top == right.top
            && left.right == right.right && left.bottom == right.bottom,
        _ => false,
    }
}

unsafe fn invalidate_drag(state: &State, source_handle: Option<HWND>, old_marker: Option<RECT>, new_marker: Option<RECT>) {
    if let Some(source_handle) = source_handle {
        InvalidateRect(source_handle, null(), 0);
    }
    if !same_rect(old_marker.as_ref(), new_marker.as_ref()) {
        for marker in old_marker.as_ref().into_iter().chain(new_marker.as_ref()) {
            RedrawWindow(state.viewport, marker, null_mut(), RDW_INVALIDATE | RDW_NOERASE | RDW_ALLCHILDREN);
        }
    }
}

unsafe fn cancel_drag(pointer: *mut State) {
    if let Some(drag) = (*pointer).drag.take() {
        let viewport = (*pointer).viewport;
        invalidate_drag(&*pointer, Some(drag.source_handle), drag.marker_rect, None);
        if GetCapture() == viewport { ReleaseCapture(); }
    }
}

fn drop_target(state: &State, source: &ItemKey, point: POINT) -> Option<DropTarget> {
    if point.x < 0 || point.x >= state.width || point.y < 0 || point.y >= state.page { return None; }
    let y = point.y + state.scroll;
    match source {
        ItemKey::Repository(repository) => {
            let groups: Vec<_> = state.cards.iter().filter(|card| matches!(card.key, Some(ItemKey::Repository(_)))
                && card.key.as_ref().is_some_and(|key| reorderable(state, key))).collect();
            for (index, card) in groups.iter().enumerate() {
                let next = groups.get(index + 1);
                let end = next.map_or(state.content, |card| card.top);
                if y < card.top || y >= end { continue; }
                let Some(ItemKey::Repository(target)) = &card.key else { return None; };
                let after = y >= card.top + card.height / 2;
                let before = if after { next.and_then(|card| match &card.key {
                    Some(ItemKey::Repository(path)) => Some(path.clone()), _ => None,
                }) } else { Some(target.clone()) };
                let marker = if after { next.map_or_else(|| state.cards.last().map_or(end, |card| card.top + card.height), |card| card.top) } else { card.top };
                return Some(DropTarget { movement: OrderMove::Repository { repository: repository.clone(), before }, marker });
            }
            None
        }
        ItemKey::Worktree(repository, worktree) => {
            let siblings: Vec<_> = state.cards.iter().filter(|card| matches!(&card.key,
                Some(ItemKey::Worktree(repo, _)) if repo == repository)
                && card.key.as_ref().is_some_and(|key| reorderable(state, key))).collect();
            for (index, card) in siblings.iter().enumerate() {
                if y < card.top || y >= card.top + card.height { continue; }
                let Some(ItemKey::Worktree(_, target)) = &card.key else { return None; };
                let next = siblings.get(index + 1);
                let after = y >= card.top + card.height / 2;
                let before = if after { next.and_then(|card| match &card.key {
                    Some(ItemKey::Worktree(_, path)) => Some(path.clone()), _ => None,
                }) } else { Some(target.clone()) };
                let marker = if after { next.map_or(card.top + card.height, |card| card.top) } else { card.top };
                return Some(DropTarget { movement: OrderMove::Worktree { repository: repository.clone(), worktree: worktree.clone(), before }, marker });
            }
            None
        }
        _ => None,
    }
}

unsafe fn move_drag(state: &mut State, point: POINT) {
    let Some(drag) = &state.drag else { return; };
    let active = drag.active || (point.x - drag.origin.x).abs() >= GetSystemMetrics(SM_CXDRAG).max(1)
        || (point.y - drag.origin.y).abs() >= GetSystemMetrics(SM_CYDRAG).max(1);
    let target = if active { drop_target(state, &drag.source, point) } else { None };
    let old_marker = drag.marker_rect;
    let new_marker = target.as_ref().and_then(|target| project_marker(state, target.marker));
    let drag = state.drag.as_mut().unwrap();
    drag.position = point;
    drag.active = active;
    drag.target = target;
    drag.marker_rect = new_marker;
    if !same_rect(old_marker.as_ref(), new_marker.as_ref()) {
        invalidate_drag(state, None, old_marker, new_marker);
    }
}

unsafe fn set_status(state: &mut State) {
    if let Some(card) = state.fixed.get_mut(0) {
        set_item(card, DisplayItem::Status { health: state.view.health,
            polling_time_last_minute: state.latest.as_ref().map_or(Duration::ZERO, |update| update.polling_time_last_minute) });
    }
}

unsafe fn refresh_indicators(state: &mut State) {
    if let Some(update) = &state.latest {
        state.refreshing = state.refresh_tracker.update(&update.scan_activities, Instant::now());
    }
    for card in &mut state.cards {
        let refreshing = card.key.as_ref().is_some_and(|key| state.refreshing.contains(key));
        if card.refreshing != refreshing {
            card.refreshing = refreshing;
            SetWindowTextW(card.window, wide(&card.display.caption_with_refresh(refreshing)).as_ptr());
            InvalidateRect(card.window, null(), 0);
        }
    }
}

unsafe fn refresh(window: HWND, state: &mut State) {
    let mut view = if let Some(update) = &state.latest {
        let mut targets = update.scan_targets.clone();
        state.order.apply_targets(&mut targets);
        let mut view = DashboardView::from_scan(&update.snapshot, &targets, &state.visibility, show_all_checked(state));
        if let Some(error) = &update.launch_error { view.launch_error(error); }
        view
    } else { DashboardView::scanning() };
    if let Some(warning) = &state.order_warning { view.presentation_warning(warning); }
    if let Some(warning) = &state.visibility_warning { view.visibility_warning(warning); }
    if view != state.view {
        let warning_changed = view.items.first().is_some_and(|item| matches!(item.key, ItemKey::Presentation | ItemKey::Visibility))
            && state.view.items.first() != view.items.first();
        let saved = if warning_changed { state.scroll = 0; None } else { anchor(state) };
        state.view = view;
        if !reconcile(state) {
            state.failure = Some(io::Error::last_os_error());
            PostMessageW(window, WM_CLOSE, 0, 0);
            return;
        }
        layout(window, state, saved, false);
    }
    for card in &mut state.cards { set_action(card, &state.visibility); }
    refresh_indicators(state);
    set_status(state);
}

unsafe fn toggle_visibility(window: HWND, state: &mut State, repository: PathBuf, worktree: PathBuf) {
    match Visibility::persist_toggle(&state.visibility_path, &repository, &worktree) {
        Ok(visibility) => {
            state.visibility = visibility;
            state.visibility_warning = None;
            state.visibility_write_failed = false;
        }
        Err(error) => {
            state.visibility_warning = Some(format!("Visibility was not saved: {error}"));
            state.visibility_write_failed = true;
        }
    }
    refresh(window, state);
}

unsafe fn command(window: HWND, state: &mut State, wparam: WPARAM, lparam: LPARAM, viewport: bool) -> LRESULT {
    if ((wparam >> 16) & 0xffff) as u32 != BN_CLICKED {
        return DefWindowProcW(window, WM_COMMAND, wparam, lparam);
    }
    let control = lparam as HWND;
    if !viewport && control == state.show_all_button {
        refresh(window, state);
        return 0;
    }
    if viewport {
        let target = state.cards.iter().find(|card| card.action == Some(control)
            && !matches!(card.display, DisplayItem::PendingWorktree { .. })).and_then(|card| match &card.key {
            Some(ItemKey::Worktree(repository, worktree)) => Some((repository.clone(), worktree.clone())),
            _ => None,
        });
        if let Some((repository, worktree)) = target {
            let focused = GetFocus() == control;
            toggle_visibility(GetParent(window), state, repository, worktree);
            if focused && GetFocus() != control { SetFocus(state.show_all_button); }
            return 0;
        }
    }
    DefWindowProcW(window, WM_COMMAND, wparam, lparam)
}

unsafe fn finish_drag(pointer: *mut State, point: POINT) {
    move_drag(&mut *pointer, point);
    let drag = (*pointer).drag.take();
    let movement = drag.as_ref().and_then(|drag| drag.target.as_ref().map(|target| target.movement.clone()));
    let source_handle = drag.as_ref().map(|drag| drag.source_handle);
    let marker_rect = drag.as_ref().and_then(|drag| drag.marker_rect);
    invalidate_drag(&*pointer, source_handle, marker_rect, None);
    let viewport = (*pointer).viewport;
    if GetCapture() == viewport { ReleaseCapture(); }
    if let (Some(movement), Some(update)) = (movement, (*pointer).latest.clone()) {
        let state = &mut *pointer;
        if matches!(state.order.moved(&update.snapshot, &movement), Ok(None)) { return; }
        match Order::persist_move(&state.order_path, &update.snapshot, &movement) {
            Ok((order, _)) => { state.order = order; state.order_warning = None; state.order_write_failed = false; }
            Err(error) => { state.order_warning = Some(format!("Order was not saved: {error}")); state.order_write_failed = true; }
        }
        refresh(GetParent(viewport), state);
    }
}

unsafe extern "system" fn card_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if matches!(message, WM_COMMAND | WM_DRAWITEM | WM_CTLCOLORBTN) {
        let viewport = GetParent(window);
        if !viewport.is_null() && !(GetWindowLongPtrW(viewport, GWLP_USERDATA) as *mut State).is_null() {
            return callback(viewport, message, wparam, lparam, true);
        }
    }
    if message == WM_ERASEBKGND { return 1; }
    if message == WM_NCDESTROY {
        let viewport = GetParent(window);
        if GetCapture() == viewport { SendMessageW(viewport, WM_CANCELMODE, 0, 0); }
        RemoveWindowSubclass(window, Some(card_procedure), 1);
    }
    if message == WM_NCHITTEST {
        if edge_hit_transparent(window, mouse_point(lparam)) {
            return HTTRANSPARENT as LRESULT;
        }
        let mut point = mouse_point(lparam);
        ScreenToClient(window, &mut point);
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(window, &mut rect);
        if point.x >= 0 && point.x < rect.right
            && point.y >= 0 && point.y < rect.bottom { return HTCLIENT as LRESULT; }
    }
    if message == WM_LBUTTONDOWN {
        let viewport = GetParent(window);
        let pointer = GetWindowLongPtrW(viewport, GWLP_USERDATA) as *mut State;
        if !pointer.is_null() {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut point = mouse_point(lparam);
                let mut rect: RECT = std::mem::zeroed();
                GetClientRect(window, &mut rect);
                if point.x < 0 || point.x >= rect.right || point.y < 0 || point.y >= rect.bottom { return false; }
                let source = (*pointer).cards.iter().find(|card| card.window == window).and_then(|card| card.key.clone());
                if let Some(source) = source.filter(|key| reorderable(&*pointer, key)) {
                    cancel_drag(pointer);
                    MapWindowPoints(window, viewport, &mut point, 1);
                    SetFocus(viewport);
                    SetCapture(viewport);
                    if GetCapture() == viewport {
                        (*pointer).drag = Some(Drag { source, source_handle: window, origin: point, position: point,
                            active: false, target: None, marker_rect: None });
                        invalidate_drag(&*pointer, Some(window), None, None);
                    }
                    return true;
                }
                false
            }));
            match result {
                Ok(true) => return 0,
                Ok(false) => {},
                Err(_) => { cancel_drag(pointer); PostMessageW(GetParent(viewport), WM_CLOSE, 0, 0); return 0; }
            }
        }
    }
    DefSubclassProc(window, message, wparam, lparam)
}

unsafe extern "system" fn show_all_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, data: usize) -> LRESULT {
    if message == WM_ERASEBKGND { return 1; }
    if message == WM_NCDESTROY { RemoveWindowSubclass(window, Some(show_all_procedure), 1); }
    if message == WM_NCHITTEST && edge_hit_transparent(window, mouse_point(lparam)) {
        return HTTRANSPARENT as LRESULT;
    }
    if message == WM_PAINT || message == WM_PRINTCLIENT {
        let mut paint: PAINTSTRUCT = std::mem::zeroed();
        let dc = if message == WM_PAINT { BeginPaint(window, &mut paint) } else { wparam as HDC };
        let mut rect: RECT = std::mem::zeroed();
        if !dc.is_null() && GetClientRect(window, &mut rect) != 0 {
            draw_checkbox(dc, rect, window, &*(data as *const State));
        }
        if message == WM_PAINT { EndPaint(window, &paint); }
        return 0;
    }
    let result = DefSubclassProc(window, message, wparam, lparam);
    if matches!(message, WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | BM_SETCHECK | BM_SETSTATE) {
        InvalidateRect(window, null(), 0);
    }
    result
}

unsafe extern "system" fn viewport_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    callback(window, message, wparam, lparam, true)
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    callback(window, message, wparam, lparam, false)
}

unsafe fn callback(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, viewport: bool) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut State;
    if pointer.is_null() { return DefWindowProcW(window, message, wparam, lparam); }
    match catch_unwind(AssertUnwindSafe(|| dispatch(window, message, wparam, lparam, pointer, viewport))) {
        Ok(result) => result,
        Err(_) => {
            (*pointer).failure = Some(io::Error::other("Window callback failed"));
            if message == WM_CREATE { -1 } else {
                PostMessageW(if viewport { GetParent(window) } else { window }, WM_CLOSE, 0, 0);
                0
            }
        }
    }
}

unsafe fn dispatch(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, pointer: *mut State, viewport: bool) -> LRESULT {
    match message {
        WM_NCCALCSIZE if !viewport => 0,
        WM_NCACTIVATE if !viewport => DefWindowProcW(window, message, wparam, -1),
        WM_GETMINMAXINFO if !viewport => {
            let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let limits = &mut *(lparam as *mut MINMAXINFO);
                limits.ptMaxPosition = POINT { x: info.rcWork.left - info.rcMonitor.left, y: info.rcWork.top - info.rcMonitor.top };
                limits.ptMaxSize = POINT { x: info.rcWork.right - info.rcWork.left, y: info.rcWork.bottom - info.rcWork.top };
                0
            } else { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_NCHITTEST if !viewport => {
            match native_hit_test(window, (*pointer).viewport, mouse_point(lparam)) {
                Some(NativeHit::Edge(hit)) => hit,
                Some(NativeHit::Child) => HTCLIENT as LRESULT,
                Some(NativeHit::Empty) => HTCAPTION as LRESULT,
                None => DefWindowProcW(window, message, wparam, lparam),
            }
        }
        WM_NCHITTEST if viewport => {
            match native_hit_test(GetParent(window), window, mouse_point(lparam)) {
                Some(NativeHit::Empty) | Some(NativeHit::Edge(_)) | Some(NativeHit::Child) => HTTRANSPARENT as LRESULT,
                None => DefWindowProcW(window, message, wparam, lparam),
            }
        }
        WM_NCLBUTTONDOWN if !viewport && wparam == HTCAPTION as WPARAM => {
            ReleaseCapture();
            DefWindowProcW(window, message, wparam, lparam)
        }
        WM_CREATE if !viewport => {
            let state = &mut *pointer;
            if controls(window, state) { 0 } else { state.failure = Some(io::Error::last_os_error()); -1 }
        }
        WM_SIZE if !viewport => { cancel_drag(pointer); let saved = anchor(&*pointer); layout(window, &mut *pointer, saved, true); 0 }
        WM_EXITSIZEMOVE if !viewport => {
            let _ = placement_win::save(window, &(*pointer).placement_path);
            0
        }
        WM_QUERYENDSESSION if !viewport => {
            let _ = placement_win::save(window, &(*pointer).placement_path);
            1
        }
        WM_DPICHANGED if !viewport => {
            cancel_drag(pointer);
            let saved = anchor(&*pointer).map(|(key, offset)| (key,
                (i64::from(offset) * i64::from(GetDpiForWindow(window).max(96)) / i64::from((*pointer).dpi)) as i32));
            if !fonts(window, &mut *pointer) { (*pointer).failure = Some(io::Error::last_os_error()); }
            let suggested = &*(lparam as *const RECT);
            if !(*pointer).starting {
                SetWindowPos(window, null_mut(), suggested.left, suggested.top, suggested.right - suggested.left,
                    suggested.bottom - suggested.top, SWP_NOZORDER | SWP_NOACTIVATE);
            }
            layout(window, &mut *pointer, saved, true);
            0
        }
        WM_TIMER if !viewport => {
            let state = &mut *pointer;
            if state.scrollbar_idle.is_some_and(|deadline| Instant::now() >= deadline)
                && GetCapture() != state.scrollbar {
                ShowWindow(state.scrollbar, SW_HIDE);
                state.scrollbar_idle = None;
            }
            if let Some(update) = state.monitor.latest() {
                state.latest = Some(update);
            }
            if let Some((point, active)) = state.drag.as_ref().map(|drag| (drag.position, drag.active)) {
                refresh_indicators(state);
                set_status(state);
                if active && point.x >= 0 && point.x < state.width && point.y >= 0 && point.y < state.page {
                    let edge = scaled(24, state.dpi);
                    if point.y < edge { scroll_to(state, state.scroll - edge); }
                    else if point.y > state.page - edge { scroll_to(state, state.scroll + edge); }
                    move_drag(state, point);
                }
                return 0;
            }
            if Instant::now() >= state.order_check {
                match Order::read(&state.order_path) {
                    Ok(order) => {
                        if !state.order_write_failed || order != state.order { state.order_warning = None; state.order_write_failed = false; }
                        state.order = order;
                    }
                    Err(error) => { state.order_warning = Some(format!("Cannot load presentation order: {error}")); }
                }
                state.order_check = Instant::now() + Duration::from_secs(1);
            }
            if Instant::now() >= state.visibility_check {
                match Visibility::read(&state.visibility_path) {
                    Ok(visibility) => {
                        if !state.visibility_write_failed || visibility != state.visibility {
                            state.visibility_warning = None;
                            state.visibility_write_failed = false;
                        }
                        state.visibility = visibility;
                    }
                    Err(error) => { state.visibility_warning = Some(format!("Cannot load worktree visibility: {error}")); }
                }
                state.visibility_check = Instant::now() + Duration::from_secs(1);
            }
            refresh(window, state);
            0
        }
        WM_PAINT => {
            if let Some(resources) = &(*pointer).resources {
                let marker = viewport.then(|| (*pointer).drag.as_ref().and_then(|drag| drag.marker_rect)).flatten();
                paint_background(window, resources, marker);
                0
            }
            else { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_COMMAND => command(window, &mut *pointer, wparam, lparam, viewport),
        WM_ERASEBKGND => 1,
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            if let Some(resources) = &(*pointer).resources {
                SetBkColor(wparam as HDC, BACKGROUND);
                SetTextColor(wparam as HDC, WHITE);
                resources.background as LRESULT
            } else { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_DRAWITEM => draw_item(&*pointer, lparam),
        WM_MOUSEMOVE if viewport && (*pointer).drag.is_some() => { move_drag(&mut *pointer, mouse_point(lparam)); 0 }
        WM_LBUTTONUP if viewport && (*pointer).drag.is_some() => { finish_drag(pointer, mouse_point(lparam)); 0 }
        WM_CAPTURECHANGED | WM_CANCELMODE => { cancel_drag(pointer); 0 }
        WM_ACTIVATEAPP if wparam == 0 => { cancel_drag(pointer); 0 }
        WM_KEYDOWN if wparam == 0x1b => { cancel_drag(pointer); 0 }
        WM_VSCROLL | WM_MOUSEWHEEL => {
            if message == WM_VSCROLL { handle_scroll(&mut *pointer, wparam); } else { wheel(&mut *pointer, wparam); }
            if let Some(point) = (*pointer).drag.as_ref().map(|drag| drag.position) { move_drag(&mut *pointer, point); }
            0
        }
        WM_KEYDOWN if keyboard(&mut *pointer, wparam) => {
            if let Some(point) = (*pointer).drag.as_ref().map(|drag| drag.position) { move_drag(&mut *pointer, point); }
            0
        }
        WM_CLOSE if !viewport => {
            cancel_drag(pointer);
            let _ = placement_win::save(window, &(*pointer).placement_path);
            KillTimer(window, 1);
            if let Err(error) = (*pointer).monitor.stop() { (*pointer).failure = Some(io::Error::other(error)); }
            DestroyWindow(window);
            0
        }
        WM_DESTROY if !viewport => { cancel_drag(pointer); KillTimer(window, 1); PostQuitMessage(0); 0 }
        WM_NCDESTROY => {
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            DefWindowProcW(window, message, wparam, lparam)
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

unsafe fn controls(window: HWND, state: &mut State) -> bool {
    if !fonts(window, state) { return false; }
    let resources = state.resources.as_ref().unwrap();
    let displays = [DisplayItem::Status { health: Health::Scanning, polling_time_last_minute: Duration::ZERO }];
    for (index, display) in displays.into_iter().enumerate() {
        let Some(card) = create_card(window, 1002 + index, None, None, display, resources) else { return false; };
        state.fixed.push(card);
    }
    state.show_all_button = CreateWindowExW(0, wide("BUTTON").as_ptr(), wide("Show all").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32, 0, 0, 1, 1, window, 1004 as _, GetModuleHandleW(null()), null());
    if state.show_all_button.is_null() { return false; }
    if SetWindowSubclass(state.show_all_button, Some(show_all_procedure), 1, state as *const State as usize) == 0 { return false; }
    SendMessageW(state.show_all_button, WM_SETFONT, resources.font as WPARAM, 0);
    state.scrollbar = CreateWindowExW(0, wide("SCROLLBAR").as_ptr(), wide("").as_ptr(),
        WS_CHILD | WS_VISIBLE | SBS_VERT as u32, 0, 0, 1, 1, window, 1003 as _, GetModuleHandleW(null()), null());
    if state.scrollbar.is_null() { return false; }
    if SetWindowSubclass(state.scrollbar, Some(scrollbar_procedure), 1, state as *const State as usize) == 0 { return false; }
    SetWindowTheme(state.scrollbar, wide("DarkMode_Explorer").as_ptr(), null());
    state.viewport = CreateWindowExW(WS_EX_CONTROLPARENT, wide("WtsDashboardViewport").as_ptr(), wide("Repository worktrees").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_TABSTOP,
        0, 0, 1, 1, window, 1001 as _, GetModuleHandleW(null()), (state as *mut State).cast());
    if state.viewport.is_null() || !reconcile(state) { return false; }
    SetWindowTheme(state.viewport, wide("DarkMode_Explorer").as_ptr(), null());
    layout(window, state, None, true);
    SetTimer(window, 1, 200, None) != 0
}

pub fn run(service: Service, launch_directory: PathBuf) -> Result<(), io::Error> {
    let order_path = service.config_path().with_extension("order.json");
    let visibility_path = service.config_path().with_extension("visibility.json");
    let placement_path = service.config_path().with_extension("placement.json");
    let (order, order_warning) = match Order::read(&order_path) {
        Ok(order) => (order, None),
        Err(error) => (Order::default(), Some(format!("Cannot load presentation order: {error}"))),
    };
    let (visibility, visibility_warning) = match Visibility::read(&visibility_path) {
        Ok(visibility) => (visibility, None),
        Err(error) => (Visibility::default(), Some(format!("Cannot load worktree visibility: {error}"))),
    };
    let mut state = Box::new(State { monitor: service.monitor_from(launch_directory).map_err(io::Error::other)?, viewport: null_mut(), scrollbar: null_mut(),
        fixed: Vec::new(), cards: Vec::new(), resources: None, view: DashboardView::scanning(), latest: None,
        refresh_tracker: RefreshTracker::default(), refreshing: HashSet::new(),
        dpi: 96, scroll: 0, content: 0, page: 0, scrollbar_idle: None, width: 0, wheel_remainder: 0, next_card_id: 2000, failure: None,
        order_path, placement_path, starting: true, order, order_warning, order_write_failed: false, order_check: Instant::now(),
        visibility_path, visibility, visibility_warning, visibility_write_failed: false, visibility_check: Instant::now(),
        show_all_button: null_mut(), next_action_id: 3000, drag: None });
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(null());
        if instance.is_null() { return Err(io::Error::last_os_error()); }
        let class = wide("WtsStatusWindow");
        let viewport_class = wide("WtsDashboardViewport");
        let definition = WNDCLASSW { lpfnWndProc: Some(procedure), hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW), lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        if RegisterClassW(&definition) == 0 { return Err(io::Error::last_os_error()); }
        let viewport_definition = WNDCLASSW { lpfnWndProc: Some(viewport_procedure), lpszClassName: viewport_class.as_ptr(), ..definition };
        if RegisterClassW(&viewport_definition) == 0 {
            let error = io::Error::last_os_error();
            UnregisterClassW(class.as_ptr(), instance);
            return Err(error);
        }
        let window = CreateWindowExW(WS_EX_TOPMOST, class.as_ptr(), wide("WTS Worktree status").as_ptr(),
            WS_POPUP | WS_SYSMENU | WS_THICKFRAME | WS_CLIPCHILDREN | WS_CLIPSIBLINGS, CW_USEDEFAULT, CW_USEDEFAULT, 640, 540, null_mut(), null_mut(), instance,
            (&mut *state as *mut State).cast());
        if window.is_null() {
            let error = state.failure.take().unwrap_or_else(io::Error::last_os_error);
            UnregisterClassW(viewport_class.as_ptr(), instance);
            UnregisterClassW(class.as_ptr(), instance);
            return Err(error);
        }
        let dark = 1i32;
        DwmSetWindowAttribute(window, 20, (&dark as *const i32).cast(), std::mem::size_of::<i32>() as u32);
        let disabled = 1u32;
        DwmSetWindowAttribute(window, 2, (&disabled as *const u32).cast(), std::mem::size_of::<u32>() as u32);
        DwmSetWindowAttribute(window, 33, (&disabled as *const u32).cast(), std::mem::size_of::<u32>() as u32);
        let no_border = 0xfffffffeu32;
        DwmSetWindowAttribute(window, 34, (&no_border as *const u32).cast(), std::mem::size_of::<u32>() as u32);
        let restored = placement_win::restore(window, &state.placement_path).unwrap_or(false);
        let positioned = if restored {
            SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
        } else {
            let dpi = GetDpiForWindow(window).max(96) as i32;
            let mut rect: RECT = std::mem::zeroed();
            GetWindowRect(window, &mut rect);
            let mut monitor: MONITORINFO = std::mem::zeroed();
            monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            let (x, y, width, height) = if GetMonitorInfoW(MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST), &mut monitor) != 0 {
                let work = monitor.rcWork;
                let width = (640 * dpi / 96).min(work.right - work.left);
                let height = (540 * dpi / 96).min(work.bottom - work.top);
                (rect.left.clamp(work.left, work.right - width), rect.top.clamp(work.top, work.bottom - height), width, height)
            } else { (rect.left, rect.top, 640 * dpi / 96, 540 * dpi / 96) };
            SetWindowPos(window, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE)
        };
        state.starting = false;
        if positioned == 0 {
            state.failure = Some(io::Error::last_os_error());
            DestroyWindow(window);
        } else { ShowWindow(window, if restored { SW_SHOWNA } else { SW_SHOWNOACTIVATE }); }
        let mut message: MSG = std::mem::zeroed();
        loop {
            let received = GetMessageW(&mut message, null_mut(), 0, 0);
            if received == -1 {
                state.failure = Some(io::Error::last_os_error());
                DestroyWindow(window);
                break;
            }
            if received == 0 { break; }
            if state.drag.is_none() && message.message == WM_KEYDOWN && matches!(message.wParam, 0x09 | 0x0d)
                && IsDialogMessageW(window, &message) != 0 { continue; }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if IsWindow(window) != 0 {
            KillTimer(window, 1);
            DestroyWindow(window);
        }
        let stopped = state.monitor.stop().map_err(io::Error::other);
        UnregisterClassW(viewport_class.as_ptr(), instance);
        UnregisterClassW(class.as_ptr(), instance);
        stopped?;
        if let Some(error) = state.failure.take() { return Err(error); }
    }
    Ok(())
}
