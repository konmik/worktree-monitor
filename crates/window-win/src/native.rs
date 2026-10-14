use crate::dashboard::{DashboardView, DisplayItem, Health, ItemKey};
use std::collections::HashMap;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{SetScrollInfo, SetWindowTheme, DRAWITEMSTRUCT, ODT_STATIC};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
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

const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF { red | (green << 8) | (blue << 16) }

struct Resources {
    fonts: [HFONT; 7],
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
        let mut resources = Self { fonts: [null_mut(); 7], background: CreateSolidBrush(BACKGROUND),
            surface: CreateSolidBrush(SURFACE), border: CreatePen(PS_SOLID, 1, BORDER),
            green: CreateSolidBrush(GREEN), red: CreateSolidBrush(RED), teal: CreateSolidBrush(TEAL), muted: CreateSolidBrush(MUTED) };
        for (index, (size, weight)) in [(11, 400), (14, 400), (16, 700), (28, 700), (36, 700), (20, 700), (9, 400)].into_iter().enumerate() {
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
}

impl Drop for State {
    fn drop(&mut self) { let _ = self.monitor.stop(); }
}

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }
fn scaled(value: i32, dpi: i32) -> i32 { (value * dpi / 96).max(1) }

unsafe fn set_item(card: &mut Card, display: DisplayItem) {
    if card.display != display {
        SetWindowTextW(card.window, wide(&display.caption()).as_ptr());
        card.display = display;
        InvalidateRect(card.window, null(), 0);
    }
}

unsafe fn create_card(parent: HWND, id: usize, key: Option<ItemKey>, display: DisplayItem, resources: &Resources) -> Option<Card> {
    let window = CreateWindowExW(0, wide("STATIC").as_ptr(), wide(&display.caption()).as_ptr(),
        WS_CHILD | WS_VISIBLE | STATIC_OWNERDRAW, 0, 0, 1, 1, parent, id as _, GetModuleHandleW(null()), null());
    if window.is_null() { return None; }
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
    let text_width = (width - s(32)).max(1);
    match &card.display {
        DisplayItem::Repository { bare, .. } => s(if *bare { 66 } else { 40 }),
        DisplayItem::Worktree { path, result, .. } => {
            let path_height = wrapped_height(dc, &path.display().to_string(), text_width, resources.fonts[0]).min(s(44));
            match result {
                Ok(_) => s(80) + path_height,
                Err(error) => s(76) + path_height + wrapped_height(dc, &format!("ERROR ({:?}): {}", error.kind, error.message), text_width, resources.fonts[1]),
            }
        }
        DisplayItem::Notice { message, .. } => s(42) + if message.is_empty() { 0 } else {
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
    if state.fixed.len() != 5 || state.viewport.is_null() { return; }
    let mut client: RECT = std::mem::zeroed();
    if GetClientRect(window, &mut client) == 0 || client.right <= 0 || client.bottom <= 0 { return; }
    let s = |value| scaled(value, state.dpi);
    let short = client.bottom < s(290);
    let tiny = client.bottom < s(150);
    let margin = s(if tiny { 6 } else if short { 10 } else { 18 });
    let gap = s(if tiny { 5 } else if short { 8 } else { 12 });
    let header = s(if tiny { 24 } else if short { 38 } else { 48 });
    let tiles = s(if tiny { 38 } else if short { 60 } else { 82 });
    let width = (client.right - margin * 2).max(1);
    let brand_width = (width * 52 / 100).max(1);
    MoveWindow(state.fixed[0].window, margin, margin, brand_width, header, 0);
    MoveWindow(state.fixed[1].window, margin + brand_width, margin, (width - brand_width).max(1), header, 0);
    let tile_top = margin + header + gap;
    let usable = (width - gap * 2).max(3);
    let one = usable * 27 / 100;
    let two = usable * 27 / 100;
    let three = (usable - one - two).max(1);
    let mut x = margin;
    for (card, tile_width) in state.fixed[2..].iter().zip([one.max(1), two.max(1), three]) {
        MoveWindow(card.window, x, tile_top, tile_width, tiles, 0);
        x += tile_width + gap;
    }
    let top = tile_top + tiles + gap;
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
    for card in &state.fixed { InvalidateRect(card.window, null(), 0); }
    InvalidateRect(window, null(), 0);
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
        DisplayItem::Metric { kind, primary, secondary } => {
            SelectObject(dc, resources.surface);
            SelectObject(dc, resources.border);
            RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, s(8), s(8));
            let tiny = height < s(45);
            let compact = height < s(70);
            let inset = s(if tiny { 7 } else { 12 });
            let label_y = s(if tiny { 2 } else { 8 });
            let value_y = s(if tiny { 12 } else if compact { 22 } else { 28 });
            let detail_y = height - s(if tiny { 11 } else { 19 });
            let label_font = resources.fonts[if tiny { 6 } else { 0 }];
            text(dc, kind.label(), bounds(inset, label_y, width - inset, value_y), label_font, MUTED, line);
            let value_font = resources.fonts[if tiny { 1 } else if compact || width < s(145) { 5 } else { 3 }];
            if let Some(second) = secondary {
                let addition = format!("+{}", primary.text());
                let deletion = format!("-{}", second.text());
                let middle = inset + (width - inset * 2) / 2;
                text(dc, &addition, bounds(inset, value_y, middle, detail_y), value_font, GREEN, line);
                text(dc, &deletion, bounds(middle, value_y, width - inset, detail_y), value_font, RED, line | DT_RIGHT);
            } else {
                text(dc, &primary.text(), bounds(inset, value_y, width - inset, detail_y), value_font, WHITE, line);
            }
            text(dc, lines.get(2).copied().unwrap_or(""), bounds(inset, detail_y, width - inset, height), label_font,
                if primary.partial { RED } else { MUTED }, line);
        }
        DisplayItem::Repository { bare, .. } => {
            fill(dc, bounds(0, s(8), s(3), s(28)), resources.teal);
            text(dc, lines[0], bounds(s(14), s(8), width - s(12), s(32)), resources.fonts[2], TEAL, line);
            if *bare { text(dc, lines[1], bounds(s(14), s(36), width - s(12), height), resources.fonts[0], MUTED, line); }
        }
        DisplayItem::Worktree { path, result, .. } => {
            SelectObject(dc, resources.surface);
            SelectObject(dc, resources.border);
            RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, s(10), s(10));
            let inset = s(14);
            let path_height = wrapped_height(dc, &path.display().to_string(), width - inset * 2, resources.fonts[0]).min(s(44));
            match result {
                Ok(stats) => {
                    let badge_width = wrapped_width(dc, lines[0], resources.fonts[0]).saturating_add(s(18))
                        .min((width - inset * 2) * 3 / 5).max(1);
                    let badge_left = (width - inset - badge_width).max(inset);
                    SelectObject(dc, resources.background);
                    SelectObject(dc, resources.border);
                    RoundRect(dc, badge_left, s(10), badge_left + badge_width, s(31), s(6), s(6));
                    text(dc, lines[0], bounds(badge_left + s(8), s(13), badge_left + badge_width - s(5), s(30)), resources.fonts[0], TEAL, line);
                    text(dc, lines[1], bounds(inset, s(10), badge_left - s(10), s(34)), resources.fonts[2], WHITE, line);
                    text(dc, lines[2], bounds(inset, s(37), width - inset, s(37) + path_height), resources.fonts[0], MUTED,
                        DT_WORDBREAK | DT_EDITCONTROL | DT_END_ELLIPSIS);
                    let y = s(40) + path_height;
                    let addition = format!("+{}", stats.counts.additions);
                    let deletion = format!("-{}", stats.counts.deletions);
                    let binaries = format!("binaries: {}", stats.counts.binaries);
                    let addition_width = wrapped_width(dc, &addition, resources.fonts[1]).min((width - inset * 2) / 3);
                    let deletion_width = wrapped_width(dc, &deletion, resources.fonts[1]).min((width - inset * 2) / 3);
                    text(dc, &addition, bounds(inset, y, inset + addition_width, y + s(20)), resources.fonts[1], GREEN, line);
                    text(dc, &deletion, bounds(inset + addition_width + s(12), y, inset + addition_width + s(12) + deletion_width, y + s(20)),
                        resources.fonts[1], RED, line);
                    text(dc, &binaries, bounds(inset + addition_width + deletion_width + s(28), y, width - inset, y + s(20)),
                        resources.fonts[0], MUTED, line);
                    if let Some(detail) = lines.get(4) {
                        text(dc, detail, bounds(inset, y + s(21), width - inset, height - s(5)), resources.fonts[0], MUTED, line);
                    }
                    let total = u128::from(stats.counts.additions) + u128::from(stats.counts.deletions);
                    let available = (width - inset * 2).max(0);
                    if let Some(green_width) = (available as u128 * u128::from(stats.counts.additions)).checked_div(total) {
                        let green_width = green_width as i32;
                        fill(dc, bounds(inset, height - s(4), inset + green_width, height - s(2)), resources.green);
                        fill(dc, bounds(inset + green_width, height - s(4), width - inset, height - s(2)), resources.red);
                    }
                }
                Err(_) => {
                    text(dc, lines[0], bounds(inset, s(12), width - inset, s(36)), resources.fonts[2], WHITE, line);
                    text(dc, lines[1], bounds(inset, s(38), width - inset, s(38) + path_height), resources.fonts[0], MUTED,
                        DT_WORDBREAK | DT_EDITCONTROL | DT_END_ELLIPSIS);
                    text(dc, lines[2], bounds(inset, s(41) + path_height, width - inset, s(62) + path_height), resources.fonts[0], MUTED, line);
                    text(dc, &lines[3..].join("\n"), bounds(inset, s(64) + path_height, width - inset, height - s(8)), resources.fonts[1], RED, DT_WORDBREAK);
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
        WM_SIZE if !viewport => { let saved = anchor(&*pointer); layout(window, &mut *pointer, saved); 0 }
        WM_DPICHANGED if !viewport => {
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
            if let Some(update) = state.monitor.latest() {
                if state.latest.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &update)) {
                    if state.latest.as_ref().is_none_or(|old| old.snapshot != update.snapshot) {
                        let saved = anchor(state);
                        state.view = DashboardView::from_snapshot(&update.snapshot);
                        for (card, display) in state.fixed[2..].iter_mut().zip(state.view.metrics()) { set_item(card, display); }
                        if !reconcile(state) {
                            state.failure = Some(io::Error::last_os_error());
                            PostMessageW(window, WM_CLOSE, 0, 0);
                        }
                        layout(window, state, saved);
                    }
                    set_item(&mut state.fixed[1], DisplayItem::Status { health: state.view.health, interval: update.interval_seconds });
                    state.latest = Some(update);
                }
            }
            0
        }
        WM_PAINT => {
            if let Some(resources) = &(*pointer).resources { paint_background(window, resources); 0 }
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
        WM_VSCROLL => { handle_scroll(&mut *pointer, wparam); 0 }
        WM_MOUSEWHEEL => { wheel(&mut *pointer, wparam); 0 }
        WM_KEYDOWN if keyboard(&mut *pointer, wparam) => 0,
        WM_CLOSE if !viewport => {
            KillTimer(window, 1);
            if let Err(error) = (*pointer).monitor.stop() { (*pointer).failure = Some(io::Error::other(error)); }
            DestroyWindow(window);
            0
        }
        WM_DESTROY if !viewport => { KillTimer(window, 1); PostQuitMessage(0); 0 }
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
    for (index, display) in displays.into_iter().chain(state.view.metrics()).enumerate() {
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

pub fn run(service: Service) -> Result<(), io::Error> {
    let mut state = Box::new(State { monitor: service.monitor().map_err(io::Error::other)?, viewport: null_mut(),
        fixed: Vec::new(), cards: Vec::new(), resources: None, view: DashboardView::scanning(), latest: None,
        dpi: 96, scroll: 0, content: 0, page: 0, width: 0, wheel_remainder: 0, next_card_id: 2000, failure: None });
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
