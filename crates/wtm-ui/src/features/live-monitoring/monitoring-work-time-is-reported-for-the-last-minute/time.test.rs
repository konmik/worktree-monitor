use super::*;

#[test]
fn active_reading_time_is_clipped_to_the_last_minute() {
    let polling = PollingTime::default();
    let reading = polling.start();
    let start = reading.start;
    assert_eq!(polling.total(start + Duration::from_secs(1)), Duration::from_secs(1));
    assert_eq!(polling.total(start + Duration::from_secs(70)), Duration::from_secs(60));
}

#[test]
fn completed_reading_time_ages_out_while_another_read_is_active() {
    let polling = PollingTime::default();
    let reading = polling.start();
    let start = reading.start;
    polling.record(start - Duration::from_secs(59), start - Duration::from_secs(58));
    assert_eq!(polling.total(start + Duration::from_secs(1)), Duration::from_secs(2));
    assert_eq!(polling.total(start + Duration::from_secs(3)), Duration::from_secs(3));
}

#[test]
fn finishing_a_read_keeps_one_completed_interval() {
    let polling = PollingTime::default();
    let reading = polling.start();
    let start = reading.start;
    let before_finish = Instant::now();
    drop(reading);
    let now = Instant::now();
    let finished = polling.total(now);
    assert!(finished >= before_finish.duration_since(start));
    assert!(finished <= now.duration_since(start));
    assert_eq!(polling.total(now + Duration::from_secs(1)), finished);
    assert_eq!(polling.total(now + Duration::from_secs(61)), Duration::ZERO);
}
