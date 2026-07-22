#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LineClearPolicy {
    #[default]
    ClearFullRows,
}

impl LineClearPolicy {
    pub fn clears_full_rows(self) -> bool {
        matches!(self, Self::ClearFullRows)
    }
}
