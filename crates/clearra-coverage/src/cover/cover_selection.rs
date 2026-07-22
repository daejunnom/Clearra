use crate::pattern::pattern_bitset::PatternBitSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverSelectionStrategy {
    NotRequested,
    ExactSearch,
    GreedyFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverSelectionOptimality {
    NotRequested,
    ProvenMinimum,
    Approximate,
    NoCompleteCover,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverSelectionLimit {
    None,
    ExactSearchRowLimitExceeded { row_count: usize, limit: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverSelection {
    row_indices: Vec<usize>,
    covered_patterns: PatternBitSet,
    complete: bool,
    strategy: CoverSelectionStrategy,
    optimality: CoverSelectionOptimality,
    limit: CoverSelectionLimit,
}

impl CoverSelection {
    pub fn not_requested(pattern_count: usize) -> Self {
        Self::new(
            Vec::new(),
            PatternBitSet::new(pattern_count),
            false,
            CoverSelectionStrategy::NotRequested,
            CoverSelectionOptimality::NotRequested,
            CoverSelectionLimit::None,
        )
    }
}
impl CoverSelection {
    pub fn new(
        row_indices: Vec<usize>,
        covered_patterns: PatternBitSet,
        complete: bool,
        strategy: CoverSelectionStrategy,
        optimality: CoverSelectionOptimality,
        limit: CoverSelectionLimit,
    ) -> Self {
        Self {
            row_indices,
            covered_patterns,
            complete,
            strategy,
            optimality,
            limit,
        }
    }
}
impl CoverSelection {
    pub fn exact_minimum(row_indices: Vec<usize>, covered_patterns: PatternBitSet) -> Self {
        Self::new(
            row_indices,
            covered_patterns,
            true,
            CoverSelectionStrategy::ExactSearch,
            CoverSelectionOptimality::ProvenMinimum,
            CoverSelectionLimit::None,
        )
    }
}
impl CoverSelection {
    pub fn exact_incomplete(covered_patterns: PatternBitSet) -> Self {
        Self::new(
            Vec::new(),
            covered_patterns,
            false,
            CoverSelectionStrategy::ExactSearch,
            CoverSelectionOptimality::NoCompleteCover,
            CoverSelectionLimit::None,
        )
    }
}
impl CoverSelection {
    pub fn greedy_fallback(
        row_indices: Vec<usize>,
        covered_patterns: PatternBitSet,
        complete: bool,
        row_count: usize,
        exact_row_limit: usize,
    ) -> Self {
        Self::new(
            row_indices,
            covered_patterns,
            complete,
            CoverSelectionStrategy::GreedyFallback,
            CoverSelectionOptimality::Approximate,
            CoverSelectionLimit::ExactSearchRowLimitExceeded {
                row_count,
                limit: exact_row_limit,
            },
        )
    }
}
impl CoverSelection {
    pub fn row_indices(&self) -> &[usize] {
        &self.row_indices
    }
}
impl CoverSelection {
    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }
}
impl CoverSelection {
    pub fn is_complete(&self) -> bool {
        self.complete
    }
}
impl CoverSelection {
    pub fn strategy(&self) -> CoverSelectionStrategy {
        self.strategy
    }
}
impl CoverSelection {
    pub fn optimality(&self) -> CoverSelectionOptimality {
        self.optimality
    }
}
impl CoverSelection {
    pub fn limit(&self) -> CoverSelectionLimit {
        self.limit
    }
}
impl CoverSelection {
    pub fn is_proven_minimum(&self) -> bool {
        self.optimality == CoverSelectionOptimality::ProvenMinimum
    }
}
impl CoverSelection {
    pub fn used_greedy_fallback(&self) -> bool {
        self.strategy == CoverSelectionStrategy::GreedyFallback
    }
}
impl CoverSelection {
    pub fn exceeded_exact_search_budget(&self) -> bool {
        matches!(
            self.limit,
            CoverSelectionLimit::ExactSearchRowLimitExceeded { .. }
        )
    }
}
