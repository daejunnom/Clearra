#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RotationSystem {
    #[default]
    Srs,
    None,
}

impl RotationSystem {
    pub fn uses_kicks(self) -> bool {
        matches!(self, Self::Srs)
    }
}
