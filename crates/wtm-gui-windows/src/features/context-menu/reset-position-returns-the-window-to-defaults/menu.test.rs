use super::*;
use super::super::tests::label;

#[test]
fn menu_contains_the_command() {
    unsafe {
        let menu = Menu::new(true, Opacity::default()).unwrap();
        assert_eq!(label(menu.0, 2), "Reset position");
        assert_eq!(GetMenuItemID(menu.0, 2), COMMAND);
    }
}
