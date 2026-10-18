use super::*;
use super::super::tests::label;

#[test]
fn visibility_item_matches_the_window_state() {
    unsafe {
        for visible in [false, true] {
            let menu = Menu::new(visible, Opacity::default()).unwrap();
            assert_eq!(label(menu.0, 1), if visible { "Hide" } else { "Show" });
            assert_eq!(GetMenuItemID(menu.0, 1), COMMAND);
            assert!(matches!((visible, action(visible)), (true, Action::Hide) | (false, Action::Show)));
        }
    }
}
