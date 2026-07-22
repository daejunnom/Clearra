use super::{
    ClearStateKey, ProofLevel, PruneEvidenceContext, PruneReason, PruningEvidenceDecision,
    PruningEvidencePolicy,
};

pub const DEFAULT_MAX_RETAINED_EVIDENCE_ENTRIES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BatchId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceDigest(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FallbackAction {
    KeepCandidate,
    RunBuildUp,
    DisableDomainPruning,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PruningProofLedgerEntry {
    batch_id: BatchId,
    state_layer: u8,
    prune_reason: PruneReason,
    affected_candidate_count: usize,
    proof_level: ProofLevel,
    clear_state_key: Option<ClearStateKey>,
    fallback_if_invalid: FallbackAction,
    evidence_digest: EvidenceDigest,
}

impl PruningProofLedgerEntry {
    fn from_engine_evidence(
        reason: PruneReason,
        context: PruneEvidenceContext,
        fallback_if_invalid: FallbackAction,
    ) -> Self {
        Self {
            batch_id: context.batch_id(),
            state_layer: context.state_layer(),
            prune_reason: reason,
            affected_candidate_count: context.affected_candidate_count(),
            proof_level: ProofLevel::GlobalSafe,
            clear_state_key: None,
            fallback_if_invalid,
            evidence_digest: context.evidence_digest(),
        }
    }
}
impl PruningProofLedgerEntry {
    pub const fn batch_id(&self) -> BatchId {
        self.batch_id
    }

    pub const fn state_layer(&self) -> u8 {
        self.state_layer
    }

    pub const fn prune_reason(&self) -> PruneReason {
        self.prune_reason
    }

    pub const fn affected_candidate_count(&self) -> usize {
        self.affected_candidate_count
    }

    pub const fn proof_level(&self) -> ProofLevel {
        self.proof_level
    }

    pub const fn clear_state_key(&self) -> Option<ClearStateKey> {
        self.clear_state_key
    }

    pub const fn fallback_if_invalid(&self) -> FallbackAction {
        self.fallback_if_invalid
    }

    pub const fn evidence_digest(&self) -> EvidenceDigest {
        self.evidence_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PruningProofLedger {
    entries: Vec<PruningProofLedgerEntry>,
    max_entries: usize,
    evidence_policy: PruningEvidencePolicy,
    evidence_truncated: bool,
    dropped_evidence_count: usize,
    complete_required_capacity_hit: bool,
    candidates_kept_due_to_evidence_capacity: usize,
    prune_reason_counts: Vec<(PruneReason, usize)>,
}

impl Default for PruningProofLedger {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: DEFAULT_MAX_RETAINED_EVIDENCE_ENTRIES,
            evidence_policy: PruningEvidencePolicy::BestEffort,
            evidence_truncated: false,
            dropped_evidence_count: 0,
            complete_required_capacity_hit: false,
            candidates_kept_due_to_evidence_capacity: 0,
            prune_reason_counts: Vec::new(),
        }
    }
}

impl PruningProofLedger {
    pub fn with_entry_limit(max_entries: usize) -> Self {
        Self {
            max_entries,
            ..Self::default()
        }
    }
}
impl PruningProofLedger {
    pub fn with_entry_limit_and_policy(
        max_entries: usize,
        evidence_policy: PruningEvidencePolicy,
    ) -> Self {
        Self {
            max_entries,
            evidence_policy,
            ..Self::default()
        }
    }
}
impl PruningProofLedger {
    fn record(&mut self, entry: PruningProofLedgerEntry) {
        if self.entries.len() >= self.max_entries {
            self.record_truncated_evidence(entry.prune_reason());
            return;
        }
        self.entries.push(entry);
    }
}
impl PruningProofLedger {
    /// Retains evidence emitted after the native engine has already decided a drop.
    ///
    /// This reporting API cannot authorize pruning. Only connected native engine
    /// reasons are retained; all unconnected proof reasons are rejected.
    pub fn record_engine_drop_evidence(
        &mut self,
        reason: PruneReason,
        context: PruneEvidenceContext,
        fallback_if_invalid: FallbackAction,
    ) -> PruningEvidenceDecision {
        if !matches!(
            reason,
            PruneReason::PieceCountOverflow
                | PruneReason::PlacementCollision
                | PruneReason::TargetMaskOverflow
                | PruneReason::ComponentExactCoverImpossible
                | PruneReason::LineClearOrderImpossible
                | PruneReason::ColumnDemandOverflow
                | PruneReason::FullParentDomainEmpty
                | PruneReason::SameTileParentDomainEmpty
                | PruneReason::AdditiveInvariantMismatch
                | PruneReason::SeparatorComponentInfeasible
                | PruneReason::ParentDomainHallViolation
                | PruneReason::ColumnDemandUnreachable
                | PruneReason::BumperDomainEmpty
                | PruneReason::BumperBridgeIncompatible
                | PruneReason::RealizationDomainEmpty
        ) {
            return PruningEvidenceDecision::EvidenceRejectedUnconnectedReason {
                reason,
                fallback: fallback_if_invalid,
            };
        }
        let entry =
            PruningProofLedgerEntry::from_engine_evidence(reason, context, fallback_if_invalid);
        let reason = entry.prune_reason();
        if self.entries.len() >= self.max_entries
            && self.evidence_policy == PruningEvidencePolicy::CompleteRequired
        {
            self.complete_required_capacity_hit = true;
            self.candidates_kept_due_to_evidence_capacity = self
                .candidates_kept_due_to_evidence_capacity
                .saturating_add(entry.affected_candidate_count());
            return PruningEvidenceDecision::CandidateKeptForCompleteEvidence {
                reason,
                fallback: entry.fallback_if_invalid(),
            };
        }
        self.record(entry);
        PruningEvidenceDecision::EvidenceRecorded { reason }
    }
}
impl PruningProofLedger {
    pub fn entries(&self) -> &[PruningProofLedgerEntry] {
        &self.entries
    }
}
impl PruningProofLedger {
    pub const fn evidence_truncated(&self) -> bool {
        self.evidence_truncated
    }
}
impl PruningProofLedger {
    pub const fn dropped_evidence_count(&self) -> usize {
        self.dropped_evidence_count
    }
}
impl PruningProofLedger {
    pub fn prune_reason_count(&self, reason: PruneReason) -> usize {
        self.prune_reason_counts
            .iter()
            .find_map(|(candidate, count)| (*candidate == reason).then_some(*count))
            .unwrap_or(0)
    }
}
impl PruningProofLedger {
    fn record_truncated_evidence(&mut self, reason: PruneReason) {
        self.evidence_truncated = true;
        self.dropped_evidence_count = self.dropped_evidence_count.saturating_add(1);
        if let Some((_, count)) = self
            .prune_reason_counts
            .iter_mut()
            .find(|(candidate, _)| *candidate == reason)
        {
            *count = count.saturating_add(1);
        } else {
            self.prune_reason_counts.push((reason, 1));
        }
    }
}
impl PruningProofLedger {
    pub const fn evidence_policy(&self) -> PruningEvidencePolicy {
        self.evidence_policy
    }
}
impl PruningProofLedger {
    pub const fn complete_required_capacity_hit(&self) -> bool {
        self.complete_required_capacity_hit
    }
}
impl PruningProofLedger {
    pub const fn candidates_kept_due_to_evidence_capacity(&self) -> usize {
        self.candidates_kept_due_to_evidence_capacity
    }
}

#[cfg(test)]
#[path = "pruning_proof_ledger_tests.rs"]
mod tests;
