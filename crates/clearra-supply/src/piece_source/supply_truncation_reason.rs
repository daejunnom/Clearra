#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupplyTruncationReason {
    ObservedWindowBudgetExceeded,
    MaterializedPatternBudgetExceeded,
}

impl SupplyTruncationReason {
    pub const fn as_u16(self) -> u16 {
        match self {
            Self::ObservedWindowBudgetExceeded => 1,
            Self::MaterializedPatternBudgetExceeded => 2,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ObservedWindowBudgetExceeded => "observed_window_budget_exceeded",
            Self::MaterializedPatternBudgetExceeded => "materialized_pattern_budget_exceeded",
        }
    }
}
