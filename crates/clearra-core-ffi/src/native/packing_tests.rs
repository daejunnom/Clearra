#[test]
fn native_packing_buffer_maps_soa_candidates_to_rust_view() {
    let mut buffer = crate::raw::owned_packing_buffer::new_zeroed_packing_candidate_buffer();
    buffer.count = 1;
    buffer.final_boards[0] = 0x3ff;
    buffer.shape_masks[0] = 0x00f;
    buffer.shape_keys[0] = 11;
    buffer.tiling_keys[0] = 12;
    buffer.operation_set_keys[0] = 13;
    buffer.placed_counts[0] = 1;
    buffer.cleared_lines[0] = 2;
    buffer.pieces[0][0] = 1;
    buffer.operation_masks[0][0] = 0x00f;

    let candidates = buffer.to_candidates();

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].candidate_id, 1);
    assert_eq!(candidates[0].operation_count, 1);
    assert_eq!(candidates[0].cleared_lines, 2);
    assert_eq!(candidates[0].operations[0].piece, 1);
    assert_eq!(candidates[0].operations[0].mask, 0x00f);
}

#[cfg(feature = "native-c-core")]
#[test]
fn native_packing_capacity_preserves_partial_candidates_and_resource_report() {
    use clearra_core_domain::{
        pc::pc_target::PcTarget, piece::piece_kind::PieceKind, resource::ResourceTruncationReason,
    };
    use clearra_pc_graph::request::{OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput};
    use clearra_problem::{ProblemCompiler, SearchProblem};
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(PcHoldPolicy::Disabled);
    let problem: SearchProblem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
    let compact = crate::problem::CPackingProblemBuilder::from_search_problem(&problem)
        .expect("compact problem");

    let outcome = super::generate_packing_candidates(
        &compact,
        &clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new(),
    )
    .expect("partial outcome");

    assert_eq!(outcome.status, super::C_PACKING_STATUS_CAPACITY_EXCEEDED);
    assert_eq!(outcome.candidates.len(), problem.budget().max_results());
    assert!(!outcome.count_complete());
    assert!(outcome.resource_report.truncated);
    assert_eq!(
        outcome.resource_report.truncation_reason,
        Some(ResourceTruncationReason::CandidateBudgetExceeded)
    );
    assert!(!outcome.resource_report.probability_complete);
}

#[cfg(feature = "native-c-core")]
#[test]
fn native_incomplete_observed_source_returns_truncated_empty_preview() {
    use clearra_core_domain::{pc::pc_target::PcTarget, resource::ResourceTruncationReason};
    use clearra_pc_graph::request::OpeningPcSearchQuery;
    use clearra_problem::ProblemCompiler;

    let problem =
        ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::two_lines()))
            .expect("problem");
    let compact = crate::problem::CPackingProblemBuilder::from_search_problem(&problem)
        .expect("compact problem");

    let outcome = super::generate_packing_candidates(
        &compact,
        &clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new(),
    )
    .expect("partial outcome");

    assert_eq!(outcome.status, super::C_PACKING_STATUS_CAPACITY_EXCEEDED);
    assert!(outcome.candidates.is_empty());
    assert!(!outcome.count_complete());
    assert_eq!(
        outcome.resource_report.truncation_reason,
        Some(ResourceTruncationReason::ObservedUniverseTruncated)
    );
    assert!(!outcome.resource_report.probability_complete);
}

#[cfg(feature = "native-c-core")]
#[test]
fn native_packing_pruning_e2e_records_problem_identity_and_collision_proof() {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        pruning::{ProofLevel, PruneReason},
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let compact = crate::problem::CPackingProblemBuilder::from_search_problem(&problem)
        .expect("compact problem");

    let outcome = super::generate_packing_candidates(
        &compact,
        &clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new(),
    )
    .expect("native outcome");

    assert_eq!(outcome.status, super::C_PACKING_STATUS_OK);
    assert_eq!(outcome.candidates.len(), 2);
    let first = outcome.candidates.candidate_at(0).expect("first candidate");
    let second = outcome
        .candidates
        .candidate_at(1)
        .expect("second candidate");
    assert_eq!(first.final_board, 0);
    assert_eq!(second.final_board, 0);
    assert_eq!(first.cleared_lines, 1);
    assert_eq!(second.cleared_lines, 1);
    assert_ne!(
        first.operations[0].operation_id,
        second.operations[0].operation_id
    );
    assert!(!outcome.pruning_ledger.entries.is_empty());
    let batch_id = outcome.pruning_ledger.entries[0].batch_id;
    assert_ne!(batch_id.0, 0);
    assert_ne!(batch_id.0, 1);
    assert!(outcome.pruning_ledger.entries.iter().all(|entry| {
        entry.batch_id == batch_id
            && entry.prune_reason == PruneReason::PlacementCollision
            && entry.proof_level == ProofLevel::GlobalSafe
            && entry.evidence_digest.0 != 0
    }));
}

#[cfg(feature = "native-c-core")]
#[test]
fn native_packing_complete_required_evidence_policy_is_preserved() {
    use clearra_core_domain::{piece::piece_kind::PieceKind, pruning::PruningEvidencePolicy};
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0xfc3f0fc3f),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
        ])),
        PieceWindow::new(4),
    )
    .with_exact_pieces(Some(4));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let compact = crate::problem::CPackingProblemBuilder::from_search_problem(&problem)
        .expect("compact problem");

    let outcome = super::generate_packing_candidates_with_pruning_policy(
        &compact,
        PruningEvidencePolicy::CompleteRequired,
    )
    .expect("strict native outcome");

    assert_eq!(
        outcome.pruning_ledger.evidence_policy,
        PruningEvidencePolicy::CompleteRequired
    );
    assert!(!outcome.pruning_ledger.evidence_truncated);
    assert!(outcome.pruning_ledger.complete_required_capacity_hit);
    assert!(
        outcome
            .pruning_ledger
            .candidates_kept_due_to_evidence_capacity
            > 0
    );
}
