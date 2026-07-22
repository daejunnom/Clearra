#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PruneEvidenceError {
    MissingBatchId,
    NoAffectedCandidates,
    MissingEvidenceDigest,
}
