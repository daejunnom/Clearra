use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorePostProcessSpinCoverage {
    target_id: String,
    pass_index: usize,
    pattern_count: usize,
    covered_pattern_words: Vec<u64>,
    candidate_identities: Vec<StandardBoard64TilingIdentity>,
    execution_count: u128,
    complete: bool,
}

impl CorePostProcessSpinCoverage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target_id: impl Into<String>,
        pass_index: usize,
        pattern_count: usize,
        covered_pattern_words: Vec<u64>,
        mut candidate_identities: Vec<StandardBoard64TilingIdentity>,
        execution_count: u128,
        complete: bool,
    ) -> Self {
        candidate_identities.sort_unstable();
        candidate_identities.dedup();
        Self {
            target_id: target_id.into(),
            pass_index,
            pattern_count,
            covered_pattern_words,
            candidate_identities,
            execution_count,
            complete,
        }
    }

    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    pub const fn pass_index(&self) -> usize {
        self.pass_index
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub fn covered_pattern_words(&self) -> &[u64] {
        &self.covered_pattern_words
    }

    pub fn candidate_identities(&self) -> &[StandardBoard64TilingIdentity] {
        &self.candidate_identities
    }

    pub const fn execution_count(&self) -> u128 {
        self.execution_count
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }
}
