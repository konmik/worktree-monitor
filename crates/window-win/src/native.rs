use crate::dashboard::{ellipsize_start, DashboardView, DisplayItem, Health, ItemKey};
use crate::order::{Order, OrderMove};
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
use windows_sys::Win32::UI::Controls::{SetScrollInfo, SetWindowTheme, DRAWITEMSTRUCT, ODT_STATIC};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture, SetFocus};
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

const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF { red | (green << 8) | (blue << 16) }

struct Resources {
    fonts: [HFONT; 6],
    background: HBRUSH,
    surface: HBRUSH,
    border: HPEN,
    green: HBRUSH,
    red: HBRUSH,
    teal: HBRUSH,
    muted: HBRUSH,
}

impl Resources {
    unsafe fn new(dpi: i32) -> Option<Self> {
        let mut resources = Self { fonts: [null_mut(); 6], background: CreateSolidBrush(BACKGROUND),
            surface: CreateSolidBrush(SURFACE), border: CreatePen(PS_SOLID, 1, BORDER),
            green: CreateSolidBrush(GREEN), red: CreateSolidBrush(RED), teal: CreateSolidBrush(TEAL), muted: CreateSolidBrush(MUTED) };
        for (index, (size, weight)) in [(11, 400), (14, 400), (16, 700), (28, 700), (36, 700), (20, 700)].into_iter().enumerate() {
            resources.fonts[index] = CreateFontW(-(size * dpi / 96), 0, 0, 0, weight, 0, 0, 0,
                DEFAULT_CHARSET as u32, 0, 0, CLEARTYPE_QUALITY as u32, (FIXED_PITCH | FF_MODERN) as u32,
                wide("Consolas").as_ptr());
        }
        if resources.fonts.iter().any(|font| font.is_null()) || resources.background.is_null() || resources.surface.is_null()
            || resources.border.is_null() || resources.green.is_null() || resources.red.is_null() || resources.teal.is_null() || resources.muted.is_null() {
            None
        } else { Some(resources) }
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts { if !font.is_null() { DeleteObject(font); } }
            for object in [self.background, self.surface, self.border, self.green, self.red, self.teal, self.muted] {
                if !object.is_null() { DeleteObject(object); }
            }
        }
    }
}

struct Card {
    window: HWND,
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
    order: Order,
    order_warning: Option<String>,
    order_write_failed: bool,
    order_check: Instant,
    drag: Option<Drag>,
}

struct Drag {
    source: ItemKey,
    origin: POINT,
    position: POINT,
    active: bool,
    target: Option<DropTarget>,
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

unsafe fn create_card(parent: HWND, id: usize, key: Option<ItemKey>, display: DisplayItem, resources: &Resources) -> Option<Card> {
    let window = CreateWindowExW(0, wide("STATIC").as_ptr(), wide(&display.caption()).as_ptr(),
        WS_CHILD | WS_VISIBLE | STATIC_OWNERDRAW | STATIC_NOTIFY, 0, 0, 1, 1, parent, id as _, GetModuleHandleW(null()), null());
    if window.is_null() { return None; }
    if key.as_ref().is_some_and(draggable) && SetWindowSubclass(window, Some(card_procedure), 1, 0) == 0 {
        DestroyWindow(window);
        return None;
    }
    SendMessageW(window, WM_SETFONT, resources.fonts[1] as WPARAM, 0);
    Some(Card { window, key, display, top: 0, height: 0 })
}

unsafe fn reconcile(state: &mut State) -> bool {
    let Some(resources) = &state.resources else { return false; };
    let mut previous: HashMap<_, _> = std::mem::take(&mut state.cards).into_iter()
        .map(|card| (card.key.clone().unwrap(), card)).collect();
    let mut succeeded = true;
    for item in &state.view.items {
        if let Some(mut card) = previous.remove(&item.key) {
            set_item(&mut card, item.display.clone());
            state.cards.push(card);
        } else if let Some(card) = create_card(state.viewport, state.next_card_id, Some(item.key.clone()), item.display.clone(), resources) {
            state.next_card_id += 1;
            state.cards.push(card);
        } else { succeeded = false; break; }
    }
    for card in previous.into_values() { DestroyWindow(card.window); }
    succeeded
}

unsafe fn fonts(window: HWND, state: &mut State) -> bool {
    let dpi = GetDpiForWindow(window).max(96) as i32;
    let Some(resources) = Resources::new(dpi) else { return false; };
    for card in state.fixed.iter().chain(&state.cards) {
        SendMessageW(card.window, WM_SETFONT, resources.fonts[1] as WPARAM, 0);
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
    for card in &state.cards {
        SetWindowPos(card.window, null_mut(), 0, card.top - state.scroll, state.width, card.height,
            SWP_NOZORDER | SWP_NOACTIVATE);
    }
    InvalidateRect(state.viewport, null(), 0);
}

unsafe fn scroll_to(state: &mut State, position: i32) {
    let position = position.clamp(0, (state.content - state.page).max(0));
    if position != state.scroll {
        state.scroll = position;
        let info = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_POS, nPos: position, ..std::mem::zeroed() };
        SetScrollInfo(state.viewport, SB_VERT, &info, 1);
        position_cards(state);
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
    let header = s(if tiny { 24 } else if short { 38 } else { 48 });
    let width = (client.right - margin * 2).max(1);
    let brand_width = (width * 52 / 100).max(1);
    MoveWindow(state.fixed[0].window, margin, margin, brand_width, header, 0);
    MoveWindow(state.fixed[1].window, margin + brand_width, margin, (width - brand_width).max(1), header, 0);
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
            text(dc, &caption, bounds(s(17), y, width, y + s(20)), resources.fonts[0], color, line | DT_RIGHT);
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
            let path = path.display().to_string();
            match result {
                Ok(stats) => {
                    let badge_width = wrapped_width(dc, lines[0], resources.fonts[0]).saturating_add(s(18))
                        .min(text_width).max(1);
                    let badge_left = inset;
                    let badge_padding = s(8).min(badge_width / 2);
                    SelectObject(dc, resources.background);
                    SelectObject(dc, resources.border);
                    RoundRect(dc, badge_left, s(10), badge_left + badge_width, s(31), s(6), s(6));
                    text(dc, lines[0], bounds(badge_left + badge_padding, s(13), badge_left + badge_width - badge_padding, s(30)), resources.fonts[0], TEAL, line);
                    let fitted_path = ellipsize_start(&path, text_width, |value| wrapped_width(dc, value, resources.fonts[0]));
                    text(dc, &fitted_path, bounds(inset, s(37), right, s(57)), resources.fonts[0], MUTED, DT_SINGLELINE);
                    let y = s(60);
                    let addition = format!("+{}", stats.counts.additions);
                    let deletion = format!("-{}", stats.counts.deletions);
                    let addition_width = wrapped_width(dc, &addition, resources.fonts[1]).min(text_width / 3);
                    let deletion_width = wrapped_width(dc, &deletion, resources.fonts[1]).min(text_width / 3);
                    let mut x = inset;
                    text(dc, &addition, bounds(x, y, (x + addition_width).min(right), y + s(20)), resources.fonts[1], GREEN, line);
                    x = (x + addition_width + s(12)).min(right);
                    text(dc, &deletion, bounds(x, y, (x + deletion_width).min(right), y + s(20)), resources.fonts[1], RED, line);
                    if stats.counts.binaries > 0 {
                        let binaries = format!("binaries: {}", stats.counts.binaries);
                        let binary_width = wrapped_width(dc, &binaries, resources.fonts[1]).min(text_width);
                        x = (x + deletion_width + s(16)).min(right);
                        text(dc, &binaries, bounds(x, y, (x + binary_width).min(right), y + s(20)), resources.fonts[1], MUTED, line);
                    }
                }
                Err(_) => {
                    let fitted_path = ellipsize_start(&path, text_width, |value| wrapped_width(dc, value, resources.fonts[0]));
                    text(dc, &fitted_path, bounds(inset, s(12), right, s(32)), resources.fonts[0], MUTED, DT_SINGLELINE);
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
        let active = state.drag.as_ref().is_some_and(|drag| Some(&drag.source) == card.key.as_ref());
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

unsafe fn paint_background(window: HWND, resources: &Resources) {
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(window, &mut paint);
    let rect = paint.rcPaint;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width > 0 && height > 0 && !dc.is_null() {
        let memory = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, width, height);
        if !memory.is_null() && !bitmap.is_null() {
            let old = SelectObject(memory, bitmap);
            fill(memory, RECT { left: 0, top: 0, right: width, bottom: height }, resources.background);
            BitBlt(dc, rect.left, rect.top, width, height, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
        } else { fill(dc, rect, resources.background); }
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

unsafe fn invalidate_drag(state: &State) {
    InvalidateRect(state.viewport, null(), 0);
    for card in &state.cards { InvalidateRect(card.window, null(), 0); }
}

unsafe fn cancel_drag(pointer: *mut State) {
    if (*pointer).drag.take().is_some() {
        let viewport = (*pointer).viewport;
        invalidate_drag(&*pointer);
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
    let drag = state.drag.as_mut().unwrap();
    drag.position = point;
    drag.active = active;
    drag.target = target;
    invalidate_drag(state);
}

unsafe fn refresh(window: HWND, state: &mut State) {
    let mut view = if let Some(update) = &state.latest {
        let mut snapshot = (*update.snapshot).clone();
        state.order.apply(&mut snapshot);
        let mut view = DashboardView::from_snapshot(&snapshot);
        if let Some(error) = &update.launch_error { view.launch_error(error); }
        view
    } else { DashboardView::scanning() };
    if let Some(warning) = &state.order_warning { view.presentation_warning(warning); }
    if view != state.view {
        let warning_changed = view.items.first().filter(|item| item.key == ItemKey::Presentation)
            .is_some_and(|item| state.view.items.first() != Some(item));
        let saved = if warning_changed { state.scroll = 0; None } else { anchor(state) };
        state.view = view;
        if !reconcile(state) {
            state.failure = Some(io::Error::last_os_error());
            PostMessageW(window, WM_CLOSE, 0, 0);
            return;
        }
        layout(window, state, saved);
    }
    if let Some(card) = state.fixed.get_mut(1) {
        set_item(card, DisplayItem::Status { health: state.view.health,
            interval: state.latest.as_ref().map_or(5, |update| update.interval_seconds) });
    }
}

unsafe fn finish_drag(pointer: *mut State, point: POINT) {
    move_drag(&mut *pointer, point);
    let movement = (*pointer).drag.take().and_then(|drag| drag.target.map(|target| target.movement));
    invalidate_drag(&*pointer);
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
                        (*pointer).drag = Some(Drag { source, origin: point, position: point, active: false, target: None });
                        invalidate_drag(&*pointer);
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
        WM_DPICHANGED if !viewport => {
            cancel_drag(pointer);
            let saved = anchor(&*pointer).map(|(key, offset)| (key,
                (i64::from(offset) * i64::from(GetDpiForWindow(window).max(96)) / i64::from((*pointer).dpi)) as i32));
            if !fonts(window, &mut *pointer) { (*pointer).failure = Some(io::Error::last_os_error()); }
            let suggested = &*(lparam as *const RECT);
            SetWindowPos(window, null_mut(), suggested.left, suggested.top, suggested.right - suggested.left,
                suggested.bottom - suggested.top, SWP_NOZORDER | SWP_NOACTIVATE);
            layout(window, &mut *pointer, saved);
            0
        }
        WM_TIMER if !viewport => {
            let state = &mut *pointer;
            if let Some(drag) = &state.drag {
                let point = drag.position;
                if drag.active && point.x >= 0 && point.x < state.width && point.y >= 0 && point.y < state.page {
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
                paint_background(window, resources);
                if viewport {
                    if let Some(target) = (*pointer).drag.as_ref().and_then(|drag| drag.target.as_ref()) {
                        let dc = GetDC(window);
                        let y = target.marker - (*pointer).scroll;
                        fill(dc, RECT { left: 0, top: y - scaled(2, (*pointer).dpi), right: (*pointer).width, bottom: y }, resources.teal);
                        ReleaseDC(window, dc);
                    }
                }
                0
            }
            else { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_ERASEBKGND => 1,
        WM_CTLCOLORSTATIC => {
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
    let displays = [DisplayItem::Brand, DisplayItem::Status { health: Health::Scanning, interval: 5 }];
    for (index, display) in displays.into_iter().enumerate() {
        let Some(card) = create_card(window, 1002 + index, None, display, resources) else { return false; };
        state.fixed.push(card);
    }
    state.viewport = CreateWindowExW(0, wide("WtsDashboardViewport").as_ptr(), wide("Repository worktrees").as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_TABSTOP,
        0, 0, 1, 1, window, 1001 as _, GetModuleHandleW(null()), (state as *mut State).cast());
    if state.viewport.is_null() || !reconcile(state) { return false; }
    SetWindowTheme(state.viewport, wide("DarkMode_Explorer").as_ptr(), null());
    layout(window, state, None);
    SetTimer(window, 1, 200, None) != 0
}

pub fn run(service: Service, launch_directory: PathBuf) -> Result<(), io::Error> {
    let order_path = service.config_path().with_extension("order.json");
    let (order, order_warning) = match Order::read(&order_path) {
        Ok(order) => (order, None),
        Err(error) => (Order::default(), Some(format!("Cannot load presentation order: {error}"))),
    };
    let mut state = Box::new(State { monitor: service.monitor_from(launch_directory).map_err(io::Error::other)?, viewport: null_mut(),
        fixed: Vec::new(), cards: Vec::new(), resources: None, view: DashboardView::scanning(), latest: None,
        dpi: 96, scroll: 0, content: 0, page: 0, width: 0, wheel_remainder: 0, next_card_id: 2000, failure: None,
        order_path, order, order_warning, order_write_failed: false, order_check: Instant::now(), drag: None });
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
        let dpi = GetDpiForWindow(window).max(96) as i32;
        if SetWindowPos(window, HWND_TOPMOST, 0, 0, 640 * dpi / 96, 540 * dpi / 96, SWP_NOMOVE | SWP_NOACTIVATE) == 0 {
            state.failure = Some(io::Error::last_os_error());
            DestroyWindow(window);
        } else { ShowWindow(window, SW_SHOWNOACTIVATE); }
        let mut message: MSG = std::mem::zeroed();
        loop {
            let received = GetMessageW(&mut message, null_mut(), 0, 0);
            if received == -1 {
                state.failure = Some(io::Error::last_os_error());
                DestroyWindow(window);
                break;
            }
            if received == 0 { break; }
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
