use super::*;

#[test]
fn status_shows_last_minute_idle_percentage_in_every_health_state() {
    for percent in [0, 92, 97, 98, 100] {
        for health in [Health::Scanning, Health::Live, Health::Errors] {
            let idle = wtm_ui::IdlePercentage::from_percent(percent);
            let status = DisplayItem::Status { health, idle };
            assert_eq!(status.caption(), format!("[idle: {idle}]"));
            let buffer = wtm_ui::rendering::render_row(&status, 40, Default::default());
            let text: String = buffer.row(0).unwrap().iter().map(|cell| cell.text()).collect();
            assert_eq!(text.trim_end(), format!("[idle: {idle}]"));
        }
    }
}
