use super::*;
use super::super::polling_time::PollingTime;
use std::time::Instant;

#[test]
fn idle_percentage_is_exactly_unused_time_in_a_sixty_second_window() {
    for (milliseconds, expected) in [(0, 100), (600, 99), (1499, 98), (1500, 97), (4800, 92), (60_000, 0), (70_000, 0)] {
        assert_eq!(IdlePercentage::from_reading_time(Duration::from_millis(milliseconds)).percent(), expected);
    }
}

#[test]
fn waiting_keeps_completed_reading_until_it_leaves_the_window() {
    let now = Instant::now();
    let polling = PollingTime::default();
    polling.record(now - Duration::from_millis(4800), now);
    assert_eq!(polling.idle(now).to_string(), "92%");
    assert_eq!(polling.idle(now + Duration::from_secs(50)).to_string(), "92%");
    assert_eq!(polling.idle(now + Duration::from_secs(58)).to_string(), "97%");
    assert_eq!(polling.idle(now + Duration::from_secs(59)).to_string(), "98%");
    assert_eq!(polling.idle(now + Duration::from_secs(60)).to_string(), "100%");
}
