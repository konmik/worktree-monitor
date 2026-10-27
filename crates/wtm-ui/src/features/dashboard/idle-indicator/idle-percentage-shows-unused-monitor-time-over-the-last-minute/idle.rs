use std::time::Duration;

pub(super) fn caption(polling_time_last_minute: Duration) -> String {
    let percent = crate::poll::idle_percent(polling_time_last_minute);
    format!("[idle: {percent}%]")
}
