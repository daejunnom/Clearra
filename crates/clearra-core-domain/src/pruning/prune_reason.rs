#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PruneReason {
    AreaOverflow,
    PieceCountOverflow,
    PlacementCollision,
    TargetMaskOverflow,
    RowCapacityOverflow,
    CellDomainEmptyUnderClearState,
    CellDomainEmptyForAllReachableClearStates,
    ForcedPieceFamilyUnderClearState,
    ForcedPieceFamilyForAllReachableClearStates,
    CandidateViolatesGloballyForcedPieceFamily,
    ComponentExactCoverImpossible,
    HoldAutomatonImpossible,
    ReachabilityImpossible,
    BuildOrdersHoldReachableIntersectionEmpty,
    ResourceBudgetExceeded,
    LineClearOrderImpossible,
    ColumnDemandOverflow,
    FullParentDomainEmpty,
    SameTileParentDomainEmpty,
    AdditiveInvariantMismatch,
    SeparatorComponentInfeasible,
    ParentDomainHallViolation,
    ColumnDemandUnreachable,
    BumperDomainEmpty,
    BumperBridgeIncompatible,
    RealizationDomainEmpty,
}

impl PruneReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AreaOverflow => "AreaOverflow",
            Self::PieceCountOverflow => "PieceCountOverflow",
            Self::PlacementCollision => "PlacementCollision",
            Self::TargetMaskOverflow => "TargetMaskOverflow",
            Self::RowCapacityOverflow => "RowCapacityOverflow",
            Self::CellDomainEmptyUnderClearState => "CellDomainEmptyUnderClearState",
            Self::CellDomainEmptyForAllReachableClearStates => {
                "CellDomainEmptyForAllReachableClearStates"
            }
            Self::ForcedPieceFamilyUnderClearState => "ForcedPieceFamilyUnderClearState",
            Self::ForcedPieceFamilyForAllReachableClearStates => {
                "ForcedPieceFamilyForAllReachableClearStates"
            }
            Self::CandidateViolatesGloballyForcedPieceFamily => {
                "CandidateViolatesGloballyForcedPieceFamily"
            }
            Self::ComponentExactCoverImpossible => "ComponentExactCoverImpossible",
            Self::HoldAutomatonImpossible => "HoldAutomatonImpossible",
            Self::ReachabilityImpossible => "ReachabilityImpossible",
            Self::BuildOrdersHoldReachableIntersectionEmpty => {
                "BuildOrdersHoldReachableIntersectionEmpty"
            }
            Self::ResourceBudgetExceeded => "ResourceBudgetExceeded",
            Self::LineClearOrderImpossible => "LineClearOrderImpossible",
            Self::ColumnDemandOverflow => "ColumnDemandOverflow",
            Self::FullParentDomainEmpty => "FullParentDomainEmpty",
            Self::SameTileParentDomainEmpty => "SameTileParentDomainEmpty",
            Self::AdditiveInvariantMismatch => "AdditiveInvariantMismatch",
            Self::SeparatorComponentInfeasible => "SeparatorComponentInfeasible",
            Self::ParentDomainHallViolation => "ParentDomainHallViolation",
            Self::ColumnDemandUnreachable => "ColumnDemandUnreachable",
            Self::BumperDomainEmpty => "BumperDomainEmpty",
            Self::BumperBridgeIncompatible => "BumperBridgeIncompatible",
            Self::RealizationDomainEmpty => "RealizationDomainEmpty",
        }
    }
}
impl PruneReason {
    pub fn forbidden_name(name: &str) -> bool {
        matches!(
            name,
            "LooksBad"
                | "RareShape"
                | "ProbablyImpossible"
                | "MctsLowScore"
                | "NoImmediatePlacement"
                | "ThisCellLooksLikeLOnly"
                | "FloatingInTargetFrame"
                | "ScoreTooLow"
                | "SpinUnknown"
        )
    }
}

#[cfg(test)]
#[path = "prune_reason_tests.rs"]
mod tests;
