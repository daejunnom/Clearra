use clearra_coverage::cover::{
    CoverSelection, CoverSelectionLimit, CoverSelectionOptimality, CoverSelectionStrategy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderCoverSelectionStrategy {
    NotRequested,
    ExactSearch,
    GreedyFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderCoverSelectionOptimality {
    NotRequested,
    ProvenMinimum,
    Approximate,
    NoCompleteCover,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderExactSearchBudget {
    row_count: usize,
    limit: usize,
}

impl RenderExactSearchBudget {
    pub fn new(row_count: usize, limit: usize) -> Self {
        Self { row_count, limit }
    }
}
impl RenderExactSearchBudget {
    pub fn row_count(self) -> usize {
        self.row_count
    }
}
impl RenderExactSearchBudget {
    pub fn limit(self) -> usize {
        self.limit
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderCoverSelection {
    complete: bool,
    proven_minimum: bool,
    strategy: RenderCoverSelectionStrategy,
    optimality: RenderCoverSelectionOptimality,
    exact_search_budget: Option<RenderExactSearchBudget>,
}

impl RenderCoverSelection {
    pub fn from_selection(selection: &CoverSelection) -> Self {
        Self {
            complete: selection.is_complete(),
            proven_minimum: selection.is_proven_minimum(),
            strategy: match selection.strategy() {
                CoverSelectionStrategy::NotRequested => RenderCoverSelectionStrategy::NotRequested,
                CoverSelectionStrategy::ExactSearch => RenderCoverSelectionStrategy::ExactSearch,
                CoverSelectionStrategy::GreedyFallback => {
                    RenderCoverSelectionStrategy::GreedyFallback
                }
            },
            optimality: match selection.optimality() {
                CoverSelectionOptimality::NotRequested => {
                    RenderCoverSelectionOptimality::NotRequested
                }
                CoverSelectionOptimality::ProvenMinimum => {
                    RenderCoverSelectionOptimality::ProvenMinimum
                }
                CoverSelectionOptimality::Approximate => {
                    RenderCoverSelectionOptimality::Approximate
                }
                CoverSelectionOptimality::NoCompleteCover => {
                    RenderCoverSelectionOptimality::NoCompleteCover
                }
            },
            exact_search_budget: match selection.limit() {
                CoverSelectionLimit::None => None,
                CoverSelectionLimit::ExactSearchRowLimitExceeded { row_count, limit } => {
                    Some(RenderExactSearchBudget::new(row_count, limit))
                }
            },
        }
    }
}
impl RenderCoverSelection {
    pub fn is_complete(&self) -> bool {
        self.complete
    }
}
impl RenderCoverSelection {
    pub fn is_proven_minimum(&self) -> bool {
        self.proven_minimum
    }
}
impl RenderCoverSelection {
    pub fn strategy(&self) -> RenderCoverSelectionStrategy {
        self.strategy
    }
}
impl RenderCoverSelection {
    pub fn optimality(&self) -> RenderCoverSelectionOptimality {
        self.optimality
    }
}
impl RenderCoverSelection {
    pub fn exact_search_budget(&self) -> Option<RenderExactSearchBudget> {
        self.exact_search_budget
    }
}

#[cfg(test)]
mod tests {
    use clearra_coverage::{cover::CoverSelection, pattern::pattern_bitset::PatternBitSet};

    use super::*;

    #[test]
    fn render_summary_exposes_greedy_fallback_budget() {
        let selection =
            CoverSelection::greedy_fallback(vec![0], PatternBitSet::new(1), true, 21, 20);

        let summary = RenderCoverSelection::from_selection(&selection);

        assert!(summary.is_complete());
        assert!(!summary.is_proven_minimum());
        assert_eq!(
            summary.strategy(),
            RenderCoverSelectionStrategy::GreedyFallback
        );
        assert_eq!(
            summary.optimality(),
            RenderCoverSelectionOptimality::Approximate
        );
        assert_eq!(
            summary.exact_search_budget(),
            Some(RenderExactSearchBudget::new(21, 20))
        );
    }
}
