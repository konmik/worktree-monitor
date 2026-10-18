use crate::dashboard::{DashboardView, DisplayItem, Health, ItemKey, ViewItem};

impl DashboardView {
    pub fn presentation_warning(&mut self, message: &str) {
        self.health = Health::Errors;
        self.items.insert(0, ViewItem { key: ItemKey::Presentation, display: DisplayItem::Notice {
            title: "Presentation order warning".into(), message: message.into(), error: true } });
    }

    pub fn visibility_warning(&mut self, message: &str) {
        self.health = Health::Errors;
        self.items.insert(0, ViewItem { key: ItemKey::Visibility, display: DisplayItem::Notice {
            title: "Worktree visibility warning".into(), message: message.into(), error: true } });
    }
}
