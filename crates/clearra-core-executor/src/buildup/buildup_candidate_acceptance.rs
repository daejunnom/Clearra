use clearra_core_ffi::CBuildUpResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildUpCandidateAcceptance {
    Explicit(Vec<CBuildUpResult>),
    AllPackingCandidatesAccepted { candidate_count: usize },
}

impl BuildUpCandidateAcceptance {
    pub(crate) fn explicit(results: Vec<CBuildUpResult>) -> Self {
        Self::Explicit(results)
    }

    pub(crate) fn all_packing_candidates(candidate_count: usize) -> Self {
        Self::AllPackingCandidatesAccepted { candidate_count }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Explicit(results) => results.len(),
            Self::AllPackingCandidatesAccepted { candidate_count } => *candidate_count,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn candidate_accepted(&self, candidate_index: usize, candidate_id: u64) -> Option<bool> {
        match self {
            Self::Explicit(results) => results.get(candidate_index).and_then(|result| {
                (result.candidate_id == candidate_id).then_some(result.success != 0)
            }),
            Self::AllPackingCandidatesAccepted { candidate_count } => {
                (candidate_index < *candidate_count).then_some(true)
            }
        }
    }

    pub(crate) fn explicit_results(&self) -> Option<&[CBuildUpResult]> {
        match self {
            Self::Explicit(results) => Some(results),
            Self::AllPackingCandidatesAccepted { .. } => None,
        }
    }
}
