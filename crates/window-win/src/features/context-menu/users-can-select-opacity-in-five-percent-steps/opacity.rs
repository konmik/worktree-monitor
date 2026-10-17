
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Opacity(u8);

impl Opacity {
    pub fn new(percent: u8) -> Option<Self> {
        ((5..=100).contains(&percent) && percent.is_multiple_of(5)).then_some(Self(percent))
    }

    pub fn percent(self) -> u8 { self.0 }

    pub fn alpha(self) -> u8 { ((u16::from(self.0) * 255 + 50) / 100) as u8 }

}

#[path = "../selected-opacity-survives-restart/storage.rs"]
mod storage;

#[path = "../../window/the-window-fades-outside-hover/opacity.rs"]
mod fading;
