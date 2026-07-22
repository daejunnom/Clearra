use super::*;
use crate::pruning::{
    BatchId, EvidenceDigest, FallbackAction, PruneEvidenceContext, PruningEvidenceDecision,
    PruningProofLedger,
};

#[test]
fn mcts_priority_does_not_prune() {
    assert!(PruneReason::forbidden_name("MctsLowScore"));
    assert!(!PruneReason::forbidden_name(
        PruneReason::AreaOverflow.as_str()
    ));
}

#[test]
fn rare_piece_heuristic_does_not_prune() {
    assert!(PruneReason::forbidden_name("RareShape"));
}

#[test]
fn score_too_low_is_not_a_prune_reason() {
    assert!(PruneReason::forbidden_name("ScoreTooLow"));
}

#[test]
fn spin_unknown_is_not_a_prune_reason() {
    assert!(PruneReason::forbidden_name("SpinUnknown"));
}

#[test]
fn target_frame_floating_not_global_pruned() {
    assert!(PruneReason::forbidden_name("FloatingInTargetFrame"));
}

#[test]
fn resource_budget_exceeded_is_not_a_candidate_prune_proof() {
    let mut ledger = PruningProofLedger::default();
    let context = PruneEvidenceContext::new(BatchId(1), 0, 1, EvidenceDigest(1)).unwrap();
    assert_eq!(
        ledger.record_engine_drop_evidence(
            PruneReason::ResourceBudgetExceeded,
            context,
            FallbackAction::RunBuildUp,
        ),
        PruningEvidenceDecision::EvidenceRejectedUnconnectedReason {
            reason: PruneReason::ResourceBudgetExceeded,
            fallback: FallbackAction::RunBuildUp,
        }
    );
}
