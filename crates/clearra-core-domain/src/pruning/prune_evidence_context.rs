use super::{BatchId, EvidenceDigest, PruneEvidenceError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneEvidenceContext {
    batch_id: BatchId,
    state_layer: u8,
    affected_candidate_count: usize,
    evidence_digest: EvidenceDigest,
}

impl PruneEvidenceContext {
    pub fn new(
        batch_id: BatchId,
        state_layer: u8,
        affected_candidate_count: usize,
        evidence_digest: EvidenceDigest,
    ) -> Result<Self, PruneEvidenceError> {
        if batch_id.0 == 0 {
            return Err(PruneEvidenceError::MissingBatchId);
        }
        if affected_candidate_count == 0 {
            return Err(PruneEvidenceError::NoAffectedCandidates);
        }
        if evidence_digest.0 == 0 {
            return Err(PruneEvidenceError::MissingEvidenceDigest);
        }
        Ok(Self {
            batch_id,
            state_layer,
            affected_candidate_count,
            evidence_digest,
        })
    }

    pub const fn batch_id(self) -> BatchId {
        self.batch_id
    }

    pub const fn state_layer(self) -> u8 {
        self.state_layer
    }

    pub const fn affected_candidate_count(self) -> usize {
        self.affected_candidate_count
    }

    pub const fn evidence_digest(self) -> EvidenceDigest {
        self.evidence_digest
    }
}
