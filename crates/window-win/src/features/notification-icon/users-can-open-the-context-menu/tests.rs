use super::*;

unsafe fn label(menu: HMENU, position: u32) -> String {
    let mut text = [0u16; 64];
    let length = GetMenuStringW(menu, position, text.as_mut_ptr(), text.len() as i32, MF_BYPOSITION);
    String::from_utf16(&text[..length as usize]).unwrap()
}

#[test]
fn popup_keeps_window_actions_and_selected_opacity() {
    unsafe {
        for visible in [false, true] {
            for percent in (5..=100).step_by(5) {
                let menu = Menu::new(visible, Opacity::new(percent).unwrap()).unwrap();
                assert_eq!(GetMenuItemCount(menu.0), 4);
                assert_eq!(label(menu.0, 0), "Opacity");
                assert_eq!(label(menu.0, 1), if visible { "Hide" } else { "Show" });
                assert_eq!(label(menu.0, 2), "Reset position");
                assert_eq!(label(menu.0, 3), "Exit");
                assert_eq!(GetMenuItemID(menu.0, 1), 1);
                assert_eq!(GetMenuItemID(menu.0, 2), 3);
                assert_eq!(GetMenuItemID(menu.0, 3), 2);
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
}

#[test]
fn closing_popup_releases_its_submenu() {
    unsafe {
        let menu = Menu::new(true, Opacity::default()).unwrap();
        let handle = menu.0;
        let opacity = GetSubMenu(handle, 0);
        assert_ne!(IsMenu(handle), 0);
        assert_ne!(IsMenu(opacity), 0);
        drop(menu);
        assert_eq!(IsMenu(handle), 0);
        assert_eq!(IsMenu(opacity), 0);
    }
}
