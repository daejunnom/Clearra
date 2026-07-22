#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceSourceKind {
    FixedQueue,
    BagUniverse,
    ObservedWindow,
    MaterializedPatternUniverse,
}

impl PieceSourceKind {
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::FixedQueue => 1,
            Self::BagUniverse => 2,
            Self::ObservedWindow => 3,
            Self::MaterializedPatternUniverse => 4,
        }
    }
}
impl PieceSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixedQueue => "fixed-queue",
            Self::BagUniverse => "bag-universe",
            Self::ObservedWindow => "observed-window",
            Self::MaterializedPatternUniverse => "materialized-pattern-universe",
        }
    }
}
