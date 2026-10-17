use super::*;

#[test]
fn line_origins_follow_the_shared_factor_at_each_dpi() {
    for (dpi, advance, cell) in [(96, 14, 15), (120, 18, 18), (144, 22, 22), (192, 29, 30)] {
        let typography = Typography::new(dpi, cell);
        assert_eq!(typography.top(1) - typography.top(0), advance);
        assert_eq!(typography.top(4) - typography.top(3), advance);
        assert_eq!(typography.height(5) - typography.top(4), cell + 8 * dpi / 96);
    }
}
