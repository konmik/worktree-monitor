use crate::typography::Typography;

#[derive(Clone, Copy, Default)]
pub(crate) struct RowSpacing {
    pub top: i32,
    pub bottom: i32,
    pub gap_after: i32,
}

impl RowSpacing {
    pub fn new(typography: Typography, attached_before: bool, attached_after: bool) -> Self {
        Self {
            top: if attached_before { 0 } else { typography.padding },
            bottom: if attached_after { (typography.advance - typography.cell).max(0) } else { typography.padding },
            gap_after: if attached_after { 0 } else { typography.padding },
        }
    }

    pub fn top(self, typography: Typography, line: usize) -> i32 { self.top + typography.top(line) - typography.padding }

    pub fn height(self, typography: Typography, lines: usize) -> i32 {
        typography.height(lines) - typography.padding * 2 + self.top + self.bottom
    }
}
