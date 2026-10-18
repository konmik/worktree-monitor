use super::row_ordering::{DropTarget, reorderable};
use super::runtime::State;
use wtm_dashboard::dashboard::ItemKey;
use wtm_dashboard::order::OrderMove;
use windows_sys::Win32::Foundation::POINT;

pub(super) fn drop_target(state: &State, repository: &std::path::Path, point: POINT) -> Option<DropTarget> {
    let y = point.y + state.scrolling.position;
    let groups: Vec<_> = state.cards.iter().filter(|card| matches!(card.key, Some(ItemKey::Repository(_)))
        && card.key.as_ref().is_some_and(|key| reorderable(state, key))).collect();
    for (index, card) in groups.iter().enumerate() {
        let next = groups.get(index + 1);
        let end = next.map_or(state.scrolling.content, |card| card.top);
        if y < card.top || y >= end { continue; }
        let Some(ItemKey::Repository(target)) = &card.key else { return None; };
        let after = y >= card.top + card.height / 2;
        let before = if after { next.and_then(|card| match &card.key {
            Some(ItemKey::Repository(path)) => Some(path.clone()), _ => None,
        }) } else { Some(target.clone()) };
        let marker = if after { next.map_or_else(|| state.cards.last().map_or(end, |card| card.top + card.height), |card| card.top) } else { card.top };
        return Some(DropTarget { movement: OrderMove::Repository { repository: repository.to_path_buf(), before }, marker });
    }
    None
}
