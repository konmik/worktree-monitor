use super::dashboard_rows::{Card, refresh};
use super::runtime::State;
use wtm_dashboard::dashboard::ItemKey;
use wtm_dashboard::visibility::Visibility;
use std::path::PathBuf;
use std::time::Instant;
use windows_sys::Win32::Foundation::HWND;

pub(super) unsafe fn toggle_visibility(window: HWND, state: &mut State, repository: PathBuf, worktree: PathBuf) {
    super::preference_failures::save_visibility(state, &repository, &worktree);
    refresh(window, state);
}

pub(super) struct WorktreeVisibility {
    pub(super) value: Visibility,
    pub(super) path: PathBuf,
    pub(super) warning: Option<String>,
    pub(super) write_failed: bool,
    pub(super) next_check: Instant,
}

pub(super) fn update_action_caption(card: &mut Card, visibility: &Visibility) -> bool {
    let Some(ItemKey::Worktree(repository, worktree)) = &card.key else { return false; };
    let hidden = visibility.contains(repository, worktree);
    if card.action_hidden == Some(hidden) { return false; }
    card.action_hidden = Some(hidden);
    true
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
