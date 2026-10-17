use super::*;

pub(super) fn content_inset(width: i32, dpi: i32, resources: &Resources) -> i32 {
    (scaled(14, dpi) - resources.space_width).max(0).min(width.saturating_sub(1) / 2)
}
pub(super) fn content_text_width(width: i32, dpi: i32, resources: &Resources) -> i32 {
    (width - content_inset(width, dpi, resources) * 2).max(1)
}

pub(super) unsafe fn wrapped_lines(dc: HDC, text: &str, width: i32, font: HFONT) -> Vec<String> {
    typography::wrap(text, width, |value| wrapped_width(dc, value, font))
}

pub(super) unsafe fn item_height(card: &Card, dc: HDC, width: i32, dpi: i32, resources: &Resources) -> i32 {
    let lines = match &card.display {
        DisplayItem::Repository { bare, .. } => 1 + usize::from(*bare),
        DisplayItem::PendingWorktree { .. } => 2,
        DisplayItem::Worktree { result, .. } => {
            let text_width = content_text_width(width, dpi, resources);
            match result {
                Ok(_) => 2,
                Err(error) => 2 + wrapped_lines(dc, &format!("ERROR ({:?}): {}", error.kind, error.message), text_width, resources.font).len(),
            }
        }
        DisplayItem::Notice { message, .. } => 1 + if message.is_empty() { 0 } else {
            let text_width = content_text_width(width, dpi, resources);
            wrapped_lines(dc, message, text_width, resources.font).len()
        },
        _ => 1,
    };
    card.spacing.height(resources.typography, lines)
}

pub(super) fn attached_rows(previous: &DisplayItem, next: &DisplayItem) -> bool {
    matches!(previous, DisplayItem::Repository { .. } | DisplayItem::Worktree { .. } | DisplayItem::PendingWorktree { .. })
        && matches!(next, DisplayItem::Worktree { .. } | DisplayItem::PendingWorktree { .. })
}

