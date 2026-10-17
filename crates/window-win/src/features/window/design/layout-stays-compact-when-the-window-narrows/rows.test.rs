use crate::dashboard::{DisplayItem};
use crate::spacing::{RowSpacing};
use std::path::{PathBuf};
use std::ptr::{null_mut};
use super::dashboard_rows::{Card};
use super::font_resources::{Resources, wrapped_width};
use super::row_layout::{attached_rows, content_inset, content_text_width, item_height};
use super::runtime::{scaled};
use windows_sys::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, GetDC, ReleaseDC};


#[test]
fn content_padding_shrinks_by_one_font_space_at_each_dpi() {
    unsafe {
        let dc = CreateCompatibleDC(null_mut());
        for dpi in [96, 120, 144, 192] {
            let resources = Resources::new(dpi).unwrap();
            let space_width = wrapped_width(dc, " ", resources.font);
            let width = scaled(400, dpi);
            let inset = content_inset(width, dpi, &resources);
            assert_eq!(inset, scaled(14, dpi) - space_width);
            assert_eq!(content_text_width(width, dpi, &resources), width - scaled(14, dpi) * 2 + space_width * 2);
            for width in 1..scaled(28, dpi) {
                let inset = content_inset(width, dpi, &resources);
                assert!(inset >= 0);
                assert!(width - inset * 2 > 0);
            }
        }
        DeleteDC(dc);
    }
}

#[test]
fn narrowing_only_adds_height_for_wrapped_text() {
    use wtm_core::{Counts, Error, ErrorKind, Head, WorktreeStats};
    let items = [
        DisplayItem::Repository { name: "repository".into(), bare: false },
        DisplayItem::Repository { name: "bare".into(), bare: true },
        DisplayItem::PendingWorktree { path: "branch".into() },
        DisplayItem::Worktree { path: "branch".into(), result: Ok(WorktreeStats {
            head: Head::Branch("master".into()), counts: Counts::default(),
        }) },
        DisplayItem::Worktree { path: "branch".into(), result: Err(Error {
            kind: ErrorKind::Git, message: "failure".into(),
        }) },
    ];
    for dpi in [96, 120, 144, 192] {
        let resources = unsafe { Resources::new(dpi) }.unwrap();
        let dc = unsafe { GetDC(null_mut()) };
        for display in &items {
            for attached_before in [false, true] {
                let card = Card { window: null_mut(), action: None, action_hidden: None, key: None,
                    display: display.clone(), refreshing: false, top: 0, height: 0, action_rect: None,
                    spacing: RowSpacing::new(resources.typography, attached_before, false) };
                let height = unsafe { item_height(&card, dc, scaled(800, dpi), dpi, &resources) };
                if matches!(display, DisplayItem::PendingWorktree { .. }) {
                    assert_eq!(height, card.spacing.height(resources.typography, 2));
                }
                for width in [359, 360, 361] {
                    assert_eq!(unsafe { item_height(&card, dc, scaled(width, dpi), dpi, &resources) }, height);
                }
                if matches!(display, DisplayItem::Worktree { result: Err(_), .. }) {
                    assert!(unsafe { item_height(&card, dc, scaled(100, dpi), dpi, &resources) } > height);
                }
            }
        }
        unsafe { ReleaseDC(null_mut(), dc); }
    }
}

#[test]
fn attached_narrow_heading_keeps_refresh_inline() {
    let repository = DisplayItem::Repository { name: "repository".into(), bare: false };
    let branch = DisplayItem::PendingWorktree { path: PathBuf::from("branch") };
    let notice = DisplayItem::Notice { title: "error".into(), message: "failure".into(), error: true };
    assert!(attached_rows(&repository, &branch));
    assert!(!attached_rows(&repository, &notice));
    assert!(attached_rows(&branch, &branch));
    assert!(!attached_rows(&branch, &repository));
    for dpi in [96, 120, 144, 192] {
        let resources = unsafe { Resources::new(dpi) }.unwrap();
        let typography = resources.typography;
        let spacing = RowSpacing::new(typography, false, true);
        let card = Card { window: null_mut(), action: None, action_hidden: None, key: None,
            display: repository.clone(), refreshing: false, top: 0, height: 0, action_rect: None, spacing };
        assert_eq!(unsafe { item_height(&card, null_mut(), scaled(180, dpi), dpi, &resources) },
            typography.padding + typography.cell.max(typography.advance));
    }
}
