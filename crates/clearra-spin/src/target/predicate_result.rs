#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredicateResult {
    True,
    False,
    Unknown,
}

impl PredicateResult {
    pub const fn is_false_for_pc_pruning(self) -> bool {
        matches!(self, Self::False)
    }

    pub const fn probability_complete_if_excluded(self) -> bool {
        !matches!(self, Self::Unknown)
    }
}
