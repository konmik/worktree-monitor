
use wtm_ui::dashboard::ItemKey;
use super::runtime::State;
pub(super) fn anchor(state: &State) -> Option<(ItemKey, i32)> {
    state.cards.iter().find(|card| card.top + card.height > state.scrolling.position)
        .and_then(|card| card.key.clone().map(|key| (key, state.scrolling.position - card.top)))
}


pub(super) fn restore_anchor(state: &mut State, restore: Option<&(ItemKey, i32)>) {
    if let Some((key, offset)) = restore {
        if let Some(card) = state.cards.iter().find(|card| card.key.as_ref() == Some(key)) {
            state.scrolling.position = card.top.saturating_add(*offset);
        }
    }
}
