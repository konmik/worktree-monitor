use super::idle_indicator::set_status;
use super::refresh_markers::refresh_indicators;
use super::row_layout::layout;
use super::runtime::State;
use super::scroll_anchor::anchor;
use super::worktree_visibility::update_action_caption;
use wtm_ui::dashboard::{DashboardView, DisplayItem, ItemKey};
use crate::layout::Rect;
use std::collections::HashMap;
use windows_sys::Win32::Foundation::HWND;

pub(super) fn set_item(card: &mut Card, display: DisplayItem) -> bool {
    if card.display == display { return false; }
    card.display = display;
    true
}

pub(super) fn create_card(key: Option<ItemKey>, display: DisplayItem) -> Card {
    Card { action_hidden: None, key, display, refreshing: false, top: 0, height: 0, action_rect: None }
}

pub(super) fn reconcile(state: &mut State) {
    state.indicators_dirty = true;
    state.frame_dirty = true;
    let mut previous: HashMap<_, _> = std::mem::take(&mut state.cards).into_iter()
        .filter_map(|card| card.key.clone().map(|key| (key, card))).collect();
    for item in &state.view.items {
        let mut card = previous.remove(&item.key).unwrap_or_else(|| create_card(Some(item.key.clone()), item.display.clone()));
        set_item(&mut card, item.display.clone());
        update_action_caption(&mut card, &state.worktree_visibility.value);
        state.cards.push(card);
    }
}

pub(super) unsafe fn refresh(window: HWND, state: &mut State) {
    let mut view = if let Some(update) = &state.latest {
        let mut targets = update.scan_targets.clone();
        state.ordering.value.apply_targets(&mut targets);
        let mut view = DashboardView::from_scan(&update.snapshot, &targets, &state.worktree_visibility.value, state.show_all);
        if let Some(error) = &update.launch_error { view.launch_error(error); }
        view
    } else { DashboardView::scanning() };
    if let Some(warning) = &state.ordering.warning { view.presentation_warning(warning); }
    if let Some(warning) = &state.worktree_visibility.warning { view.visibility_warning(warning); }
    if view != state.view || state.cards.len() != view.items.len() {
        let warning_changed = view.items.first().is_some_and(|item| matches!(item.key, ItemKey::Presentation | ItemKey::Visibility))
            && state.view.items.first() != view.items.first();
        let saved = if warning_changed { state.scrolling.position = 0; None } else { anchor(state) };
        state.view = view;
        reconcile(state);
        layout(window, state, saved, false);
    }
    let mut action_changed = false;
    for card in &mut state.cards { action_changed |= update_action_caption(card, &state.worktree_visibility.value); }
    if action_changed && state.controls_visible { layout(window, state, None, false); }
    refresh_indicators(state);
    set_status(state);
}

pub(super) struct Card {
    pub(super) action_hidden: Option<bool>,
    pub(super) key: Option<ItemKey>,
    pub(super) display: DisplayItem,
    pub(super) refreshing: bool,
    pub(super) top: i32,
    pub(super) height: i32,
    pub(super) action_rect: Option<Rect>,
}
