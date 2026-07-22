use super::exact_cover_candidate::ExactCoverCandidate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactCoverProblem {
    required_column_count: usize,
    optional_column_count: usize,
    candidates: Vec<ExactCoverCandidate>,
}

impl ExactCoverProblem {
    pub fn new(column_count: usize, candidates: Vec<ExactCoverCandidate>) -> Self {
        Self {
            required_column_count: column_count,
            optional_column_count: 0,
            candidates,
        }
    }
}
impl ExactCoverProblem {
    pub fn with_optional_columns(
        required_column_count: usize,
        optional_column_count: usize,
        candidates: Vec<ExactCoverCandidate>,
    ) -> Self {
        Self {
            required_column_count,
            optional_column_count,
            candidates,
        }
    }
}
impl ExactCoverProblem {
    pub fn column_count(&self) -> usize {
        self.required_column_count + self.optional_column_count
    }
}
impl ExactCoverProblem {
    pub fn required_column_count(&self) -> usize {
        self.required_column_count
    }
}
impl ExactCoverProblem {
    pub fn optional_column_count(&self) -> usize {
        self.optional_column_count
    }
}
impl ExactCoverProblem {
    pub fn candidates(&self) -> &[ExactCoverCandidate] {
        &self.candidates
    }
}
impl ExactCoverProblem {
    pub fn has_optional_columns(&self) -> bool {
        self.optional_column_count > 0
    }
}
