use super::{FallbackAction, PruneReason};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PruningEvidenceDecision {
    EvidenceRecorded {
        reason: PruneReason,
    },
    CandidateKeptForCompleteEvidence {
        reason: PruneReason,
        fallback: FallbackAction,
    },
    EvidenceRejectedUnconnectedReason {
        reason: PruneReason,
        fallback: FallbackAction,
    },
}
