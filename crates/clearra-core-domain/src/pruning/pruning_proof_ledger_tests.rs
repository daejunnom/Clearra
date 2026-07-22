use super::*;
use crate::pruning::{
    ConditionalPruningEvidence, PruneEvidenceContext, PruningEvidenceDecision,
    PruningEvidencePolicy,
};

fn test_context(digest: u64) -> PruneEvidenceContext {
    PruneEvidenceContext::new(BatchId(7), 2, 1, EvidenceDigest(digest)).unwrap()
}

fn record(
    ledger: &mut PruningProofLedger,
    reason: PruneReason,
    digest: u64,
) -> PruningEvidenceDecision {
    ledger.record_engine_drop_evidence(reason, test_context(digest), FallbackAction::RunBuildUp)
}

#[test]
fn candidate_drop_without_ledger_is_forbidden() {
    let mut ledger = PruningProofLedger::default();

    assert!(ledger.entries().is_empty());
    assert_eq!(
        record(&mut ledger, PruneReason::PlacementCollision, 0xabc),
        PruningEvidenceDecision::EvidenceRecorded {
            reason: PruneReason::PlacementCollision
        }
    );
    assert_eq!(ledger.entries().len(), 1);
    assert_eq!(
        ledger.entries()[0].prune_reason(),
        PruneReason::PlacementCollision
    );
}

#[test]
fn local_only_cannot_drop_candidate() {
    let evidence = ConditionalPruningEvidence::CellDomainEmptyUnderClearState {
        clear_state_key: ClearStateKey(9),
        evidence_digest: EvidenceDigest(11),
    };

    assert_eq!(evidence.proof_level(), ProofLevel::ClearStateConditional);
    assert_eq!(evidence.fallback(), FallbackAction::RunBuildUp);
}

#[test]
fn unconnected_all_state_reason_cannot_be_retained_as_engine_proof() {
    let mut ledger = PruningProofLedger::default();

    assert_eq!(
        record(
            &mut ledger,
            PruneReason::CellDomainEmptyForAllReachableClearStates,
            0xabc,
        ),
        PruningEvidenceDecision::EvidenceRejectedUnconnectedReason {
            reason: PruneReason::CellDomainEmptyForAllReachableClearStates,
            fallback: FallbackAction::RunBuildUp,
        }
    );
    assert!(ledger.entries().is_empty());
}

#[test]
fn resource_budget_exceeded_cannot_drop_candidate() {
    let mut ledger = PruningProofLedger::default();

    assert_eq!(
        record(&mut ledger, PruneReason::ResourceBudgetExceeded, 0xabc),
        PruningEvidenceDecision::EvidenceRejectedUnconnectedReason {
            reason: PruneReason::ResourceBudgetExceeded,
            fallback: FallbackAction::RunBuildUp,
        }
    );
    assert!(ledger.entries().is_empty());
}

#[test]
fn ledger_capacity_does_not_abort_static_safe_prunes() {
    let mut ledger = PruningProofLedger::with_entry_limit(1);

    assert_eq!(
        record(&mut ledger, PruneReason::PlacementCollision, 0xabc),
        PruningEvidenceDecision::EvidenceRecorded {
            reason: PruneReason::PlacementCollision
        }
    );
    assert_eq!(
        record(&mut ledger, PruneReason::TargetMaskOverflow, 0xdef),
        PruningEvidenceDecision::EvidenceRecorded {
            reason: PruneReason::TargetMaskOverflow
        }
    );
    assert_eq!(ledger.entries().len(), 1);
    assert!(ledger.evidence_truncated());
    assert_eq!(ledger.dropped_evidence_count(), 1);
    assert_eq!(
        ledger.prune_reason_count(PruneReason::TargetMaskOverflow),
        1
    );
}

#[test]
fn candidate_drop_allowed_requires_global_safe_but_ledger_retention_is_best_effort() {
    let mut ledger = PruningProofLedger::with_entry_limit(0);
    let local = ConditionalPruningEvidence::CellDomainEmptyUnderClearState {
        clear_state_key: ClearStateKey(9),
        evidence_digest: EvidenceDigest(11),
    };

    assert_eq!(local.proof_level(), ProofLevel::ClearStateConditional);
    assert!(!ledger.evidence_truncated());
    assert_eq!(
        record(&mut ledger, PruneReason::PlacementCollision, 0xabc),
        PruningEvidenceDecision::EvidenceRecorded {
            reason: PruneReason::PlacementCollision
        }
    );
    assert!(ledger.evidence_truncated());
    assert_eq!(ledger.entries().len(), 0);
}

#[test]
fn complete_required_capacity_keeps_candidate() {
    let mut ledger =
        PruningProofLedger::with_entry_limit_and_policy(0, PruningEvidencePolicy::CompleteRequired);

    assert_eq!(
        record(&mut ledger, PruneReason::PlacementCollision, 0x444),
        PruningEvidenceDecision::CandidateKeptForCompleteEvidence {
            reason: PruneReason::PlacementCollision,
            fallback: FallbackAction::RunBuildUp,
        }
    );
    assert!(ledger.entries().is_empty());
    assert!(!ledger.evidence_truncated());
    assert!(ledger.complete_required_capacity_hit());
    assert_eq!(ledger.candidates_kept_due_to_evidence_capacity(), 1);
}
