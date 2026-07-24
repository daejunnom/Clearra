#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorePostProcessSpinCoverage {
    target_id: String,
    pass_index: usize,
    pattern_count: usize,
    covered_pattern_words: Vec<u64>,
    candidate_keys: Vec<String>,
    witnessed_pattern_count: u128,
    complete: bool,
}

impl CorePostProcessSpinCoverage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target_id: impl Into<String>,
        pass_index: usize,
        pattern_count: usize,
        covered_pattern_words: Vec<u64>,
        mut candidate_keys: Vec<String>,
        witnessed_pattern_count: u128,
        complete: bool,
    ) -> Self {
        candidate_keys.sort_unstable();
        candidate_keys.dedup();
        Self {
            target_id: target_id.into(),
            pass_index,
            pattern_count,
            covered_pattern_words,
            candidate_keys,
            witnessed_pattern_count,
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

    pub fn candidate_keys(&self) -> &[String] {
        &self.candidate_keys
    }

    pub const fn witnessed_pattern_count(&self) -> u128 {
        self.witnessed_pattern_count
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }
}
