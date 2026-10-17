#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellFlags {
    pub stressed: bool,
    pub bold: bool,
    pub underline: bool,
}
