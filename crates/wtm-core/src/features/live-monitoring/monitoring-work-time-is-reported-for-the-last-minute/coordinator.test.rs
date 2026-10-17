use super::*;

#[test]
fn polling_time_only_counts_serial_work_inside_the_last_minute() {
    let now = Instant::now();
    let mut polling = PollingTime::default();
    polling.record(now - Duration::from_secs(70), now - Duration::from_secs(55));
    polling.record(now - Duration::from_secs(40), now - Duration::from_secs(35));
    assert_eq!(polling.total(now), Duration::from_secs(10));
}
