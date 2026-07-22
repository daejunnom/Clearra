use super::{ClearStateKey, EvidenceDigest, FallbackAction, PieceFamily, ProofLevel, PruneReason};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionalPruningEvidence {
    CellDomainEmptyUnderClearState {
        clear_state_key: ClearStateKey,
        evidence_digest: EvidenceDigest,
    },
    ForcedPieceFamilyUnderClearState {
        clear_state_key: ClearStateKey,
        forced_piece_family: PieceFamily,
        evidence_digest: EvidenceDigest,
    },
}

impl ConditionalPruningEvidence {
    pub const fn reason(self) -> PruneReason {
        match self {
            Self::CellDomainEmptyUnderClearState { .. } => {
                PruneReason::CellDomainEmptyUnderClearState
            }
            Self::ForcedPieceFamilyUnderClearState { .. } => {
                PruneReason::ForcedPieceFamilyUnderClearState
            }
        }
    }

    pub fn proof_level(self) -> ProofLevel {
        match self {
            Self::CellDomainEmptyUnderClearState { .. }
            | Self::ForcedPieceFamilyUnderClearState { .. } => ProofLevel::ClearStateConditional,
        }
    }

    pub const fn fallback(self) -> FallbackAction {
        match self {
            Self::CellDomainEmptyUnderClearState { .. }
            | Self::ForcedPieceFamilyUnderClearState { .. } => FallbackAction::RunBuildUp,
        }
    }
}
