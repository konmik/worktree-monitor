use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Eq, PartialEq)]
pub(super) enum PendingScan {
    Forced,
    Paths(HashSet<PathBuf>),
}

impl PendingScan {
    pub(super) fn merge(&mut self, incoming: Self) {
        match (self, incoming) {
            (Self::Paths(current), Self::Paths(paths)) => current.extend(paths),
            (current, Self::Forced) => *current = Self::Forced,
            (Self::Forced, Self::Paths(_)) => {}
        }
    }
}
