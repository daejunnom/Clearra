use crate::bag::bag_boundary::BagBoundaryCandidate;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmbiguityReason {
    EmptyObservedWindow,
    MultipleBoundaryCandidates,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmbiguityReport {
    reason: AmbiguityReason,
    observed_len: usize,
    candidates: Vec<BagBoundaryCandidate>,
}

impl AmbiguityReport {
    pub fn new(
        reason: AmbiguityReason,
        observed_len: usize,
        candidates: Vec<BagBoundaryCandidate>,
    ) -> Self {
        Self {
            reason,
            observed_len,
            candidates,
        }
    }
}
impl AmbiguityReport {
    pub fn reason(&self) -> AmbiguityReason {
        self.reason
    }
}
impl AmbiguityReport {
    pub fn observed_len(&self) -> usize {
        self.observed_len
    }
}
impl AmbiguityReport {
    pub fn candidates(&self) -> &[BagBoundaryCandidate] {
        &self.candidates
    }
}
