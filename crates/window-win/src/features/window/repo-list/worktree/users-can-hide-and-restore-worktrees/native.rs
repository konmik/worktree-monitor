use super::dashboard_rows::{Card, refresh};
use super::runtime::{State, wide};
use crate::dashboard::{DisplayItem, ItemKey};
use crate::visibility::Visibility;
use std::path::PathBuf;
use std::ptr::null;
use std::time::Instant;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
use windows_sys::Win32::UI::Controls::BST_CHECKED;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::{BM_GETCHECK, BN_CLICKED, DefWindowProcW, GetParent, SendMessageW, SetWindowTextW, WM_COMMAND};
pub(super) unsafe fn toggle_visibility(window: HWND, state: &mut State, repository: PathBuf, worktree: PathBuf) {
    super::preference_failures::save_visibility(state, &repository, &worktree);
    refresh(window, state);
}

pub(super) unsafe fn command(window: HWND, state: &mut State, wparam: WPARAM, lparam: LPARAM, viewport: bool) -> LRESULT {
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
            if focused && GetFocus() != control { SetFocus(if state.controls_visible { state.show_all_button } else { state.viewport }); }
            return 0;
        }
    }
    DefWindowProcW(window, WM_COMMAND, wparam, lparam)
}

pub(super) struct WorktreeVisibility {
    pub(super) value: Visibility,
    pub(super) path: PathBuf,
    pub(super) warning: Option<String>,
    pub(super) write_failed: bool,
    pub(super) next_check: Instant,
}
pub(super) unsafe fn update_action_caption(card: &mut Card, visibility: &Visibility) -> bool {
    let Some(action) = card.action else { return false; };
    let Some(ItemKey::Worktree(repository, worktree)) = &card.key else { return false; };
    let hidden = visibility.contains(repository, worktree);
    if card.action_hidden != Some(hidden) {
        SetWindowTextW(action, wide(if hidden { "Unhide" } else { "Hide" }).as_ptr());
        InvalidateRect(action, null(), 0);
        card.action_hidden = Some(hidden);
        return true;
    }
    false
}

pub(super) unsafe fn show_all_checked(state: &State) -> bool {
    SendMessageW(state.show_all_button, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT
}


impl WorktreeVisibility {
    pub(super) fn load(path: PathBuf) -> Self {
        let (value, warning) = match Visibility::read(&path) {
            Ok(value) => (value, None),
            Err(error) => (Visibility::default(), Some(format!("Cannot load worktree visibility: {error}"))),
        };
        Self { value, path, warning, write_failed: false, next_check: Instant::now() }
    }
}
