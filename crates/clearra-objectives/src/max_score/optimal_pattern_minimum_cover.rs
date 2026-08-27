use clearra_coverage::{
    cover::{exact_minimum_cover::exact_minimum_cover, ExactMinimumCoverPortfolioEnumerator},
    pattern::pattern_bitset::PatternBitSet,
};

#[derive(Clone, Debug)]
pub(super) struct OptimalCoverageRow {
    candidate_id: usize,
    coverage: PatternBitSet,
}

impl OptimalCoverageRow {
    pub(super) fn new(candidate_id: usize, coverage: PatternBitSet) -> Self {
        Self {
            candidate_id,
            coverage,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct OptimalMinimumCover {
    selected_candidate_ids: Vec<usize>,
    covered_patterns: PatternBitSet,
    complete: bool,
}

impl OptimalMinimumCover {
    pub(super) fn selected_candidate_ids(&self) -> &[usize] {
        &self.selected_candidate_ids
    }

    pub(super) fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }

    pub(super) const fn complete(&self) -> bool {
        self.complete
    }
}

pub(super) fn exact_optimal_pattern_cover(
    pattern_count: usize,
    required: &PatternBitSet,
    mut rows: Vec<OptimalCoverageRow>,
) -> OptimalMinimumCover {
    rows.sort_unstable_by_key(|row| row.candidate_id);
    let coverage = rows
        .iter()
        .map(|row| row.coverage.clone())
        .collect::<Vec<_>>();
    let cover = exact_minimum_cover(required, &coverage)
        .expect("score rows and required patterns share one pattern universe");
    let selected_row_indices = if cover.complete() {
        let mut portfolios = ExactMinimumCoverPortfolioEnumerator::new(required, &coverage)
            .expect("a complete exact proof admits portfolio enumeration");
        portfolios
            .next_portfolio()
            .expect("canonical portfolio allocation")
            .expect("a complete cover has one canonical optimum")
            .into_row_indices()
    } else {
        cover.row_indices().to_vec()
    };
    let selected_candidate_ids = selected_row_indices
        .into_iter()
        .map(|row_index| rows[row_index].candidate_id)
        .collect();

    debug_assert_eq!(required.pattern_count(), pattern_count);
    OptimalMinimumCover {
        selected_candidate_ids,
        covered_patterns: cover.covered_patterns().clone(),
        complete: cover.complete(),
    }
}
