#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ObjectiveIncompleteReason {
    PatternWeightModelNotMaterialized,
    PatternWeightCountMismatch,
    CoverageNotRequestedForUniqueSolutionSet,
}

impl ObjectiveIncompleteReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::PatternWeightModelNotMaterialized => "pattern_weight_model_not_materialized",
            Self::PatternWeightCountMismatch => "pattern_weight_count_mismatch",
            Self::CoverageNotRequestedForUniqueSolutionSet => {
                "coverage_not_requested_for_unique_solution_set"
            }
        }
    }
}
