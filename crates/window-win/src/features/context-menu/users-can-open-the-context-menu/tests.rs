use super::*;

pub(super) unsafe fn label(menu: HMENU, position: u32) -> String {
    let mut text = [0u16; 64];
    let length = GetMenuStringW(menu, position, text.as_mut_ptr(), text.len() as i32, MF_BYPOSITION);
    String::from_utf16(&text[..length as usize]).unwrap()
}

#[test]
fn popup_contains_four_items() {
    unsafe {
        for visible in [false, true] {
            let menu = Menu::new(visible, Opacity::default()).unwrap();
            assert_eq!(GetMenuItemCount(menu.0), 4);
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
