use super::dashboard_rows::{set_item};
use super::runtime::{State};
use crate::dashboard::DisplayItem;
use std::time::Duration;
pub(super) unsafe fn set_status(state: &mut State) {
    if let Some(card) = state.fixed.get_mut(0) {
        set_item(card, DisplayItem::Status { health: state.view.health,
            polling_time_last_minute: state.latest.as_ref().map_or(Duration::ZERO, |update| update.polling_time_last_minute) });
    }
}

