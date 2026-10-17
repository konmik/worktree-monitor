use crate::{Error, ErrorKind};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LineCounts {
    pub additions: u64,
    pub deletions: u64,
}

impl LineCounts {
    fn add(self, other: Self) -> Result<Self, Error> {
        let overflow = || Error::new(ErrorKind::InvalidOutput, "Git line counts overflowed");
        Ok(Self {
            additions: self.additions.checked_add(other.additions).ok_or_else(overflow)?,
            deletions: self.deletions.checked_add(other.deletions).ok_or_else(overflow)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counts {
    pub source: LineCounts,
    pub test: LineCounts,
    pub extra: LineCounts,
    pub binaries: u64,
}

impl Counts {
    pub(crate) fn add(&mut self, other: Self) -> Result<(), Error> {
        let overflow = || Error::new(ErrorKind::InvalidOutput, "Git line counts overflowed");
        let source = self.source.add(other.source)?;
        let test = self.test.add(other.test)?;
        let extra = self.extra.add(other.extra)?;
        let binaries = self.binaries.checked_add(other.binaries).ok_or_else(overflow)?;
        self.source = source;
        self.test = test;
        self.extra = extra;
        self.binaries = binaries;
        Ok(())
    }
}

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

