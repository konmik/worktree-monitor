use super::*;
use super::super::tests::label;

#[test]
fn opacity_submenu_lists_five_percent_steps_and_highlights_the_selection() {
    unsafe {
        for percent in (5..=100).step_by(5) {
            let menu = Menu::new(true, Opacity::new(percent).unwrap()).unwrap();
            assert_eq!(label(menu.0, 0), "Opacity");
            let opacity = GetSubMenu(menu.0, 0);
            assert!(!opacity.is_null());
            assert_eq!(GetMenuItemCount(opacity), 20);
            for (position, choice) in (5..=100).step_by(5).enumerate() {
                assert_eq!(label(opacity, position as u32), format!("{choice}%"));
                assert_eq!(GetMenuItemID(opacity, position as i32), choice);
                let state = GetMenuState(opacity, choice, MF_BYCOMMAND);
                assert_ne!(state, u32::MAX);
                assert_eq!(state & MF_CHECKED != 0, choice == u32::from(percent));
            }
        }
    }
}
