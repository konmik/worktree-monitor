use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct IdlePercentage(u8);

impl IdlePercentage {
    pub const fn from_percent(value: u8) -> Self {
        assert!(value <= 100);
        Self(value)
    }

    pub const fn percent(self) -> u8 { self.0 }

    pub(super) fn from_reading_time(reading: Duration) -> Self {
        Self((100 - (reading.as_millis().saturating_add(300) / 600).min(100)) as u8)
    }
}

impl Default for IdlePercentage {
    fn default() -> Self { Self(100) }
}

impl std::fmt::Display for IdlePercentage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}%", self.0)
    }
}

#[cfg(test)]
#[path = "idle.test.rs"]
mod tests;
