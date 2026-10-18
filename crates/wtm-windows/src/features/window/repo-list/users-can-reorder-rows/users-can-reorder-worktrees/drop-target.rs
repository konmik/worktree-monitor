use super::row_ordering::{DropTarget, reorderable};
use super::runtime::State;
use wtm_dashboard::dashboard::ItemKey;
use wtm_dashboard::order::OrderMove;
use windows_sys::Win32::Foundation::POINT;

pub(super) fn drop_target(state: &State, repository: &std::path::Path, worktree: &std::path::Path, point: POINT) -> Option<DropTarget> {
    let y = point.y + state.scrolling.position;
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
        return Some(DropTarget { movement: OrderMove::Worktree { repository: repository.to_path_buf(), worktree: worktree.to_path_buf(), before }, marker });
    }
    None
}
