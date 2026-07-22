#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactCoverSolution {
    candidate_ids: Vec<usize>,
}

impl ExactCoverSolution {
    pub fn new(candidate_ids: Vec<usize>) -> Self {
        Self { candidate_ids }
    }
}
impl ExactCoverSolution {
    pub fn candidate_ids(&self) -> &[usize] {
        &self.candidate_ids
    }
}
