use crate::dashboard::{ellipsize_start, DashboardView, DisplayItem, Health, ItemKey};
use crate::order::{Order, OrderMove};
use crate::placement_win;
use crate::visibility::Visibility;
use std::collections::HashMap;
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
    ODS_HOTLIGHT, ODS_SELECTED, BST_CHECKED};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, GetFocus, IsWindowEnabled, ReleaseCapture, SetCapture, SetFocus};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use wts_core::{Monitor, MonitorUpdate, Service};

const BACKGROUND: COLORREF = rgb(11, 16, 18);
const SURFACE: COLORREF = rgb(23, 30, 34);
const BORDER: COLORREF = rgb(40, 52, 58);
const GREEN: COLORREF = rgb(194, 249, 112);
const TEAL: COLORREF = rgb(84, 213, 197);
const RED: COLORREF = rgb(255, 131, 137);
const WHITE: COLORREF = rgb(232, 239, 235);
const MUTED: COLORREF = rgb(132, 150, 156);
const STATIC_OWNERDRAW: u32 = 13;
const STATIC_NOTIFY: u32 = 256;
const WORKTREE_SUCCESS_HEIGHT: i32 = 84;
const WORKTREE_ERROR_HEIGHT: i32 = 76;
const WORKTREE_ACTION_WIDTH: i32 = 78;

const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF { red | (green << 8) | (blue << 16) }

struct Resources {
    fonts: [HFONT; 6],
    background: HBRUSH,
    surface: HBRUSH,
    border: HPEN,
    accent: HPEN,
    checked: HPEN,
    green: HBRUSH,
    red: HBRUSH,
    teal: HBRUSH,
    muted: HBRUSH,
}

impl Resources {
    unsafe fn new(dpi: i32) -> Option<Self> {
        let mut resources = Self { fonts: [null_mut(); 6], background: CreateSolidBrush(BACKGROUND),
            surface: CreateSolidBrush(SURFACE), border: CreatePen(PS_SOLID, 1, BORDER),
            accent: CreatePen(PS_SOLID, 1, TEAL), checked: CreatePen(PS_SOLID, 1, GREEN),
            green: CreateSolidBrush(GREEN), red: CreateSolidBrush(RED), teal: CreateSolidBrush(TEAL), muted: CreateSolidBrush(MUTED) };
        for (index, (size, weight)) in [(11, 400), (14, 400), (16, 700), (28, 700), (36, 700), (20, 700)].into_iter().enumerate() {
            resources.fonts[index] = CreateFontW(-(size * dpi / 96), 0, 0, 0, weight, 0, 0, 0,
                DEFAULT_CHARSET as u32, 0, 0, CLEARTYPE_QUALITY as u32, (FIXED_PITCH | FF_MODERN) as u32,
                wide("Consolas").as_ptr());
        }
        if resources.fonts.iter().any(|font| font.is_null()) || resources.background.is_null() || resources.surface.is_null()
            || resources.border.is_null() || resources.accent.is_null() || resources.checked.is_null()
            || resources.green.is_null() || resources.red.is_null() || resources.teal.is_null() || resources.muted.is_null() {
            None
        } else { Some(resources) }
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts { if !font.is_null() { DeleteObject(font); } }
            for object in [self.background, self.surface, self.border, self.accent, self.checked, self.green, self.red, self.teal, self.muted] {
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
    top: i32,
    height: i32,
}

struct State {
    monitor: Monitor,
    viewport: HWND,
    fixed: Vec<Card>,
    cards: Vec<Card>,
    resources: Option<Resources>,
    view: DashboardView,
    latest: Option<Arc<MonitorUpdate>>,
    dpi: i32,
    scroll: i32,
    content: i32,
    page: i32,
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
fn worktree_inset(width: i32, dpi: i32) -> i32 { scaled(30, dpi).min(width.saturating_sub(1) / 2) }
fn worktree_text_width(width: i32, dpi: i32) -> i32 {
    (width - worktree_inset(width, dpi) * 2).max(1)
}

unsafe fn set_item(card: &mut Card, display: DisplayItem) {
    if card.display != display {
        SetWindowTextW(card.window, wide(&display.caption()).as_ptr());
        card.display = display;
        InvalidateRect(card.window, null(), 0);
    }
}

unsafe fn create_card(parent: HWND, id: usize, action_id: Option<usize>, key: Option<ItemKey>, display: DisplayItem, resources: &Resources) -> Option<Card> {
    let window = CreateWindowExW(WS_EX_CONTROLPARENT, wide("STATIC").as_ptr(), wide(&display.caption()).as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | STATIC_OWNERDRAW | STATIC_NOTIFY, 0, 0, 1, 1, parent, id as _, GetModuleHandleW(null()), null());
    if window.is_null() { return None; }
    if key.as_ref().is_some_and(draggable) && SetWindowSubclass(window, Some(card_procedure), 1, 0) == 0 {
        DestroyWindow(window);
        return None;
    }
    SendMessageW(window, WM_SETFONT, resources.fonts[1] as WPARAM, 0);
    let action = if let Some(action_id) = action_id {
        let action = CreateWindowExW(0, wide("BUTTON").as_ptr(), wide("Hide").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32, 0, 0, 1, 1, window,
            action_id as _, GetModuleHandleW(null()), null());
        if action.is_null() {
            DestroyWindow(window);
            return None;
        }
        SendMessageW(action, WM_SETFONT, resources.fonts[1] as WPARAM, 0);
        Some(action)
    } else { None };
    Some(Card { window, action, action_hidden: None, key, display, top: 0, height: 0 })
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
        SendMessageW(card.window, WM_SETFONT, resources.fonts[1] as WPARAM, 0);
        if let Some(action) = card.action {
            SendMessageW(action, WM_SETFONT, resources.fonts[1] as WPARAM, 0);
        }
    }
    if !state.show_all_button.is_null() {
        SendMessageW(state.show_all_button, WM_SETFONT, resources.fonts[1] as WPARAM, 0);
    }
    state.resources = Some(resources);
    state.dpi = dpi;
    true
}

unsafe fn wrapped_height(dc: HDC, text: &str, width: i32, font: HFONT) -> i32 {
    let saved = SaveDC(dc);
    SelectObject(dc, font);
    let mut rect = RECT { left: 0, top: 0, right: width.max(1), bottom: 0 };
    let mut text = wide(text);
    DrawTextW(dc, text.as_mut_ptr(), (text.len() - 1) as i32, &mut rect, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
    RestoreDC(dc, saved);
    rect.bottom.max(1)
}

unsafe fn item_height(card: &Card, dc: HDC, width: i32, dpi: i32, resources: &Resources) -> i32 {
    let s = |value| scaled(value, dpi);
    match &card.display {
        DisplayItem::Repository { bare, .. } => s(if *bare { 66 } else { 40 }),
        DisplayItem::Worktree { result, .. } => {
            let text_width = worktree_text_width(width, dpi);
            match result {
                Ok(_) => s(WORKTREE_SUCCESS_HEIGHT),
                Err(error) => s(WORKTREE_ERROR_HEIGHT) + wrapped_height(dc, &format!("ERROR ({:?}): {}", error.kind, error.message), text_width, resources.fonts[1]),
            }
        }
        DisplayItem::Notice { message, .. } => s(42) + if message.is_empty() { 0 } else {
            let text_width = (width - s(28)).max(1);
            wrapped_height(dc, message, text_width, resources.fonts[1]) + s(12)
        },
        _ => s(80),
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
    for card in &state.cards { position_action(card, state.width, state.dpi); }
    InvalidateRect(state.viewport, null(), 0);
}

unsafe fn position_action(card: &Card, width: i32, dpi: i32) {
    let Some(action) = card.action else { return; };
    let action_width = scaled(WORKTREE_ACTION_WIDTH, dpi).min(width.max(1));
    let action_height = scaled(24, dpi).min(card.height.max(1));
    let action_left = width.saturating_sub(action_width).saturating_sub(scaled(10, dpi));
    SetWindowPos(action, null_mut(), action_left, scaled(7, dpi), action_width, action_height,
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS);
}

unsafe fn scroll_to(state: &mut State, position: i32) {
    let position = position.clamp(0, (state.content - state.page).max(0));
    if position != state.scroll {
        state.scroll = position;
        let info = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_POS, nPos: position, ..std::mem::zeroed() };
        SetScrollInfo(state.viewport, SB_VERT, &info, 1);
        position_cards(state);
        let marker = state.drag.as_ref().and_then(|drag| drag.target.as_ref().map(|target| target.marker));
        let marker_rect = marker.and_then(|marker| project_marker(state, marker));
        if let Some(drag) = state.drag.as_mut() { drag.marker_rect = marker_rect; }
    }
}

unsafe fn layout(window: HWND, state: &mut State, restore: Option<(ItemKey, i32)>) {
    let Some(resources) = &state.resources else { return; };
    if state.fixed.len() != 2 || state.viewport.is_null() { return; }
    let mut client: RECT = std::mem::zeroed();
    if GetClientRect(window, &mut client) == 0 || client.right <= 0 || client.bottom <= 0 { return; }
    let s = |value| scaled(value, state.dpi);
    let short = client.bottom < s(290);
    let tiny = client.bottom < s(150);
    let margin = s(if tiny { 6 } else if short { 10 } else { 18 });
    let gap = s(if tiny { 5 } else if short { 8 } else { 12 });
    let brand_height = s(if tiny { 24 } else if short { 38 } else { 48 });
    let status_height = s(20);
    let header = brand_height + status_height;
    let width = (client.right - margin * 2).max(1);
    let toggle_width = scaled(104, state.dpi).min(width);
    let toggle_height = scaled(24, state.dpi).min(brand_height);
    MoveWindow(state.fixed[0].window, margin, margin, (width - toggle_width).max(1), brand_height, 0);
    MoveWindow(state.fixed[1].window, margin, margin + brand_height, width, status_height, 0);
    MoveWindow(state.show_all_button, margin + width - toggle_width, margin + (brand_height - toggle_height) / 2,
        toggle_width, toggle_height, 0);
    let top = margin + header + gap;
    let page = (client.bottom - top - margin).max(1);
    MoveWindow(state.viewport, margin, top, width, page, 0);
    let mut viewport: RECT = std::mem::zeroed();
    GetClientRect(state.viewport, &mut viewport);
    state.width = viewport.right.max(1);
    state.page = viewport.bottom.max(1);
    let dc = GetDC(window);
    let mut y: i32 = 0;
    for card in &mut state.cards {
        card.top = y;
        card.height = item_height(card, dc, state.width, state.dpi, resources);
        y = y.saturating_add(card.height).saturating_add(s(8));
    }
    ReleaseDC(window, dc);
    state.content = y.saturating_sub(s(8)).max(0);
    if let Some((key, offset)) = &restore {
        if let Some(card) = state.cards.iter().find(|card| card.key.as_ref() == Some(key)) {
            state.scroll = card.top.saturating_add(*offset);
        }
    }
    state.scroll = state.scroll.clamp(0, (state.content - state.page).max(0));
    let info = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0, nMax: (state.content - 1).max(0), nPage: state.page as u32, nPos: state.scroll, ..std::mem::zeroed() };
    SetScrollInfo(state.viewport, SB_VERT, &info, 1);
    GetClientRect(state.viewport, &mut viewport);
    if state.width != viewport.right.max(1) {
        state.width = viewport.right.max(1);
        let dc = GetDC(window);
        let mut y: i32 = 0;
        for card in &mut state.cards {
            card.top = y;
            card.height = item_height(card, dc, state.width, state.dpi, resources);
            y = y.saturating_add(card.height).saturating_add(s(8));
        }
        ReleaseDC(window, dc);
        state.content = y.saturating_sub(s(8)).max(0);
        if let Some((key, offset)) = &restore {
            if let Some(card) = state.cards.iter().find(|card| card.key.as_ref() == Some(key)) {
                state.scroll = card.top.saturating_add(*offset);
            }
        }
        state.scroll = state.scroll.clamp(0, (state.content - state.page).max(0));
        let info = SCROLLINFO { nMax: (state.content - 1).max(0), nPos: state.scroll, ..info };
        SetScrollInfo(state.viewport, SB_VERT, &info, 1);
    }
    position_cards(state);
    RedrawWindow(window, null(), null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_FRAME);
}

unsafe fn text(dc: HDC, value: &str, rect: RECT, font: HFONT, color: COLORREF, flags: u32) {
    SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT as i32);
    let mut rect = rect;
    let mut value = wide(value);
    DrawTextW(dc, value.as_mut_ptr(), (value.len() - 1) as i32, &mut rect, flags | DT_NOPREFIX);
}

unsafe fn fill(dc: HDC, rect: RECT, brush: HBRUSH) { FillRect(dc, &rect, brush); }

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
    fill(dc, rect, resources.background);
    match &card.display {
        DisplayItem::Brand => {
            let compact = height < s(45);
            let tiny = height < s(30);
            text(dc, "WTS", bounds(0, if tiny { 0 } else { -s(3) }, width, height),
                resources.fonts[if tiny { 5 } else if compact { 3 } else { 4 }], GREEN, line);
            let label_x = if tiny { s(45) } else { s(if compact { 64 } else { 87 }) };
            text(dc, "WORKTREE MONITOR", bounds(label_x, if tiny { s(7) } else { s(if compact { 12 } else { 18 }) }, width, height),
                resources.fonts[0], MUTED, line);
        }
        DisplayItem::Status { health, .. } => {
            let color = match health { Health::Live => GREEN, Health::Errors => RED, Health::Scanning => MUTED };
            let y = (height - s(15)) / 2;
            SelectObject(dc, match health { Health::Live => resources.green, Health::Errors => resources.red, Health::Scanning => resources.muted });
            SelectObject(dc, GetStockObject(NULL_PEN));
            Ellipse(dc, s(3), y + s(3), s(9), y + s(9));
            text(dc, &caption, bounds(s(17), y, width, y + s(20)), resources.fonts[0], color, line);
        }
        DisplayItem::Repository { bare, .. } => {
            text(dc, lines[0], bounds(s(30), s(8), width - s(12), s(32)), resources.fonts[2], TEAL, line);
            if *bare { text(dc, lines[1], bounds(s(30), s(36), width - s(12), height), resources.fonts[0], MUTED, line); }
        }
        DisplayItem::Worktree { path, result } => {
            SelectObject(dc, resources.surface);
            SelectObject(dc, resources.border);
            RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, s(10), s(10));
            let inset = worktree_inset(width, state.dpi);
            let text_width = worktree_text_width(width, state.dpi);
            let right = inset + text_width;
            let heading_width = (width - s(WORKTREE_ACTION_WIDTH + 18) - inset).max(1).min(text_width);
            let path = path.display().to_string();
            match result {
                Ok(stats) => {
                    let badge_width = wrapped_width(dc, lines[0], resources.fonts[0]).saturating_add(s(18))
                        .min(heading_width).max(1);
                    let badge_left = inset;
                    let badge_padding = s(8).min(badge_width / 2);
                    SelectObject(dc, resources.background);
                    SelectObject(dc, resources.border);
                    RoundRect(dc, badge_left, s(10), badge_left + badge_width, s(31), s(6), s(6));
                    text(dc, lines[0], bounds(badge_left + badge_padding, s(13), badge_left + badge_width - badge_padding, s(30)), resources.fonts[0], TEAL, line);
                    let fitted_path = ellipsize_start(&path, text_width, |value| wrapped_width(dc, value, resources.fonts[0]));
                    text(dc, &fitted_path, bounds(inset, s(37), right, s(57)), resources.fonts[0], MUTED, DT_SINGLELINE);
                    let y = s(60);
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
                        let part_width = wrapped_width(dc, &part, resources.fonts[1]).min(right.saturating_sub(x));
                        if part_width > 0 {
                            text(dc, &part, bounds(x, y, (x + part_width).min(right), y + s(20)), resources.fonts[1], color, line);
                        }
                        x = (x + part_width).min(right);
                    }
                }
                Err(_) => {
                    let fitted_path = ellipsize_start(&path, heading_width, |value| wrapped_width(dc, value, resources.fonts[0]));
                    text(dc, &fitted_path, bounds(inset, s(12), inset + heading_width, s(32)), resources.fonts[0], MUTED, DT_SINGLELINE);
                    text(dc, lines[1], bounds(inset, s(36), right, s(57)), resources.fonts[0], MUTED, line);
                    text(dc, &lines[2..].join("\n"), bounds(inset, s(60), right, height - s(8)), resources.fonts[1], RED, DT_WORDBREAK);
                }
            }
        }
        DisplayItem::Notice { title, message, error } => {
            SelectObject(dc, resources.surface);
            SelectObject(dc, resources.border);
            RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, s(8), s(8));
            text(dc, title, bounds(s(14), s(12), width - s(14), s(37)), resources.fonts[2], if *error { RED } else { WHITE }, line);
            text(dc, message, bounds(s(14), s(42), width - s(14), height - s(10)), resources.fonts[1], if *error { RED } else { MUTED }, DT_WORDBREAK);
        }
    }
    if card.key.as_ref().is_some_and(draggable) {
        let active = state.drag.as_ref().is_some_and(|drag| drag.source_handle == card.window);
        let brush = if active { resources.teal } else { resources.muted };
        for y in [14, 19, 24] {
            for x in [8, 13] { fill(dc, bounds(s(x), s(y), s(x + 2), s(y + 2)), brush); }
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
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let s = |value| scaled(value, state.dpi);
    let disabled = draw.itemState & (ODS_DISABLED | ODS_GRAYED) != 0;
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let focused = draw.itemState & ODS_FOCUS != 0;
    let hot = draw.itemState & ODS_HOTLIGHT != 0;
    let checked = draw.itemState & ODS_CHECKED != 0;
    let inset = s(1).min(width.saturating_sub(1) / 2).min(height.saturating_sub(1) / 2);
    let body = RECT { left: rect.left + inset, top: rect.top + inset,
        right: rect.right - inset, bottom: rect.bottom - inset };
    let radius = s(7).min((body.right - body.left).max(1) / 2).min((body.bottom - body.top).max(1) / 2);
    let fill_brush = if disabled || pressed { resources.background } else { resources.surface };
    let border_pen = if disabled { resources.border }
        else if checked { resources.checked } else if focused || pressed || hot { resources.accent } else { resources.border };
    fill(draw.hDC, rect, resources.background);
    SelectObject(draw.hDC, fill_brush);
    SelectObject(draw.hDC, border_pen);
    RoundRect(draw.hDC, body.left, body.top, body.right, body.bottom, radius, radius);
    let offset = if pressed { s(1) } else { 0 };
    let padding = s(8);
    let text_rect = RECT { left: rect.left + padding, top: rect.top + offset,
        right: rect.right - padding, bottom: rect.bottom + offset };
    let color = if disabled { MUTED } else if checked { GREEN } else { WHITE };
    text(draw.hDC, &button_caption(draw.hwndItem), text_rect, resources.fonts[1], color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    RestoreDC(draw.hDC, saved);
}

unsafe fn draw_checkbox(dc: HDC, rect: RECT, window: HWND, state: &State) {
    let Some(resources) = &state.resources else { return; };
    let saved = SaveDC(dc);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let s = |value| scaled(value, state.dpi);
    if width > 0 && height > 0 {
        fill(dc, rect, resources.background);
        let size = s(16).min(width).min(height);
        let top = rect.top + (height - size) / 2;
        let checkbox = RECT { left: rect.left, top, right: rect.left + size, bottom: top + size };
        let checked = SendMessageW(window, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT;
        let focused = GetFocus() == window;
        let disabled = IsWindowEnabled(window) == 0;
        SelectObject(dc, if disabled { resources.background } else { resources.surface });
        SelectObject(dc, if disabled { resources.border }
            else if focused { resources.accent } else if checked { resources.checked } else { resources.border });
        Rectangle(dc, checkbox.left, checkbox.top, checkbox.right, checkbox.bottom);
        if checked && !disabled {
            SelectObject(dc, resources.checked);
            MoveToEx(dc, checkbox.left + size / 4, checkbox.top + size / 2, null_mut());
            LineTo(dc, checkbox.left + size / 2, checkbox.top + size * 3 / 4);
            LineTo(dc, checkbox.left + size * 3 / 4, checkbox.top + size / 4);
        }
        text(dc, "Show all", RECT { left: checkbox.right + s(8), top: rect.top, right: rect.right, bottom: rect.bottom },
            resources.fonts[1], if disabled { MUTED } else if checked { GREEN } else { WHITE }, DT_SINGLELINE | DT_VCENTER);
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
            if GetScrollInfo(state.viewport, SB_VERT, &mut info) == 0 { return; }
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

fn draggable(key: &ItemKey) -> bool { matches!(key, ItemKey::Repository(_) | ItemKey::Worktree(_, _)) }

fn mouse_point(lparam: LPARAM) -> POINT {
    POINT { x: (lparam as u16 as i16) as i32, y: ((lparam >> 16) as u16 as i16) as i32 }
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
        let handle = RECT { left: 0, top: 0, right: scaled(24, state.dpi), bottom: scaled(30, state.dpi) };
        InvalidateRect(source_handle, &handle, 0);
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
            let groups: Vec<_> = state.cards.iter().filter(|card| matches!(card.key, Some(ItemKey::Repository(_)))).collect();
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
                Some(ItemKey::Worktree(repo, _)) if repo == repository)).collect();
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
    if let Some(card) = state.fixed.get_mut(1) {
        set_item(card, DisplayItem::Status { health: state.view.health,
            polling_time_last_minute: state.latest.as_ref().map_or(Duration::ZERO, |update| update.polling_time_last_minute) });
    }
}

unsafe fn refresh(window: HWND, state: &mut State) {
    let mut view = if let Some(update) = &state.latest {
        let mut snapshot = (*update.snapshot).clone();
        state.order.apply(&mut snapshot);
        let mut view = DashboardView::from_snapshot_with_visibility(&snapshot, &state.visibility, show_all_checked(state));
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
        layout(window, state, saved);
    }
    for card in &mut state.cards { set_action(card, &state.visibility); }
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
        let target = state.cards.iter().find(|card| card.action == Some(control)).and_then(|card| match &card.key {
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
        let mut point = mouse_point(lparam);
        ScreenToClient(window, &mut point);
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(window, &mut rect);
        if point.x >= 0 && point.x < scaled(24, GetDpiForWindow(window).max(96) as i32).min(rect.right)
            && point.y >= 0 && point.y < rect.bottom { return HTCLIENT as LRESULT; }
    }
    if message == WM_LBUTTONDOWN {
        let viewport = GetParent(window);
        let pointer = GetWindowLongPtrW(viewport, GWLP_USERDATA) as *mut State;
        if !pointer.is_null() {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let mut point = mouse_point(lparam);
                if point.x < 0 || point.x >= scaled(24, (*pointer).dpi) { return false; }
                let source = (*pointer).cards.iter().find(|card| card.window == window).and_then(|card| card.key.clone());
                if let Some(source) = source.filter(draggable) {
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
        WM_CREATE if !viewport => {
            let state = &mut *pointer;
            if controls(window, state) { 0 } else { state.failure = Some(io::Error::last_os_error()); -1 }
        }
        WM_SIZE if !viewport => { cancel_drag(pointer); let saved = anchor(&*pointer); layout(window, &mut *pointer, saved); 0 }
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
            layout(window, &mut *pointer, saved);
            0
        }
        WM_TIMER if !viewport => {
            let state = &mut *pointer;
            if let Some((point, active)) = state.drag.as_ref().map(|drag| (drag.position, drag.active)) {
                if let Some(update) = state.monitor.latest() {
                    let metric_only = state.latest.as_ref().is_some_and(|old| Arc::ptr_eq(&old.snapshot, &update.snapshot)
                        && old.launch_error == update.launch_error);
                    if metric_only && state.latest.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &update)) {
                        state.latest = Some(update);
                        set_status(state);
                    }
                }
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
            if let Some(update) = state.monitor.latest() {
                if state.latest.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &update)) {
                    state.latest = Some(update);
                }
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
                let (color, brush) = if message == WM_CTLCOLORBTN { (SURFACE, resources.surface) }
                    else { (BACKGROUND, resources.background) };
                SetBkColor(wparam as HDC, color);
                SetTextColor(wparam as HDC, WHITE);
                brush as LRESULT
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
    let displays = [DisplayItem::Brand, DisplayItem::Status { health: Health::Scanning, polling_time_last_minute: Duration::ZERO }];
    for (index, display) in displays.into_iter().enumerate() {
        let Some(card) = create_card(window, 1002 + index, None, None, display, resources) else { return false; };
        state.fixed.push(card);
    }
    state.show_all_button = CreateWindowExW(0, wide("BUTTON").as_ptr(), wide("Show all").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32, 0, 0, 1, 1, window, 1004 as _, GetModuleHandleW(null()), null());
    if state.show_all_button.is_null() { return false; }
    if SetWindowSubclass(state.show_all_button, Some(show_all_procedure), 1, state as *const State as usize) == 0 { return false; }
    SendMessageW(state.show_all_button, WM_SETFONT, resources.fonts[1] as WPARAM, 0);
    state.viewport = CreateWindowExW(WS_EX_CONTROLPARENT, wide("WtsDashboardViewport").as_ptr(), wide("Repository worktrees").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_TABSTOP,
        0, 0, 1, 1, window, 1001 as _, GetModuleHandleW(null()), (state as *mut State).cast());
    if state.viewport.is_null() || !reconcile(state) { return false; }
    SetWindowTheme(state.viewport, wide("DarkMode_Explorer").as_ptr(), null());
    layout(window, state, None);
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
    let mut state = Box::new(State { monitor: service.monitor_from(launch_directory).map_err(io::Error::other)?, viewport: null_mut(),
        fixed: Vec::new(), cards: Vec::new(), resources: None, view: DashboardView::scanning(), latest: None,
        dpi: 96, scroll: 0, content: 0, page: 0, width: 0, wheel_remainder: 0, next_card_id: 2000, failure: None,
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
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN, CW_USEDEFAULT, CW_USEDEFAULT, 640, 540, null_mut(), null_mut(), instance,
            (&mut *state as *mut State).cast());
        if window.is_null() {
            let error = state.failure.take().unwrap_or_else(io::Error::last_os_error);
            UnregisterClassW(viewport_class.as_ptr(), instance);
            UnregisterClassW(class.as_ptr(), instance);
            return Err(error);
        }
        let dark = 1i32;
        DwmSetWindowAttribute(window, 20, (&dark as *const i32).cast(), std::mem::size_of::<i32>() as u32);
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
