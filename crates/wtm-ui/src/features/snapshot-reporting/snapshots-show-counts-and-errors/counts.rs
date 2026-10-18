use crate::Counts;
use std::fmt;

impl fmt::Display for Counts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "S +{}/-{}", self.source.additions, self.source.deletions)?;
        for (label, counts) in [("T", self.test), ("X", self.extra)] {
            if counts.additions > 0 || counts.deletions > 0 {
                write!(f, " · {label} +{}/-{}", counts.additions, counts.deletions)?;
            }
        }
        if self.binaries > 0 { write!(f, " · binaries: {}", self.binaries)?; }
        Ok(())
    }
}
