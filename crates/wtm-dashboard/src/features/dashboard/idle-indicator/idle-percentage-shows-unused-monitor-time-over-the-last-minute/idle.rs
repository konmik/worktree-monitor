use std::time::Duration;

pub(super) fn caption(polling_time_last_minute: Duration) -> String {
    let percent = 100 - (polling_time_last_minute.as_millis().saturating_add(300) / 600).min(100);
    format!("[idle: {percent}%]")
}
