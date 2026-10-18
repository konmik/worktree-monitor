use super::*;

#[test]
fn status_shows_idle_percentage_for_the_last_minute() {
    for (millis, percent) in [(0, 100), (299, 100), (300, 99), (1200, 98), (1800, 97), (60_000, 0), (120_000, 0)] {
        for health in [Health::Scanning, Health::Live, Health::Errors] {
            let status = DisplayItem::Status { health, polling_time_last_minute: Duration::from_millis(millis) };
            assert_eq!(status.caption(), format!("[idle: {percent}%]"));
        }
    }
    let status = DisplayItem::Status { health: Health::Live, polling_time_last_minute: Duration::MAX };
    assert_eq!(status.caption(), "[idle: 0%]");
}

