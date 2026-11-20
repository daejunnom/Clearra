#![cfg(feature = "native-c-core")]

use std::collections::BTreeSet;

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    pc::pc_target::PcTarget,
    piece::piece_kind::PieceKind,
    solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_core_executor::{
    CoreExecutionError, CoreExecutionResult, CoreExecutor, WasmCpuSearchBackend,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcSolutionProbabilityPolicy, RequestedSearchBackend,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_supply::queue::fixed_sequence::FixedSequence;

const IIOOO: [PieceKind; 5] = [
    PieceKind::I,
    PieceKind::I,
    PieceKind::O,
    PieceKind::O,
    PieceKind::O,
];

fn problem(queue: [PieceKind; 5], max_candidates: Option<usize>) -> SearchProblem {
    let mut policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(1)
        .with_worker_hardware_limit(1)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false);
    if let Some(max_candidates) = max_candidates {
        policy = policy.with_max_candidates(max_candidates);
    }
    ProblemCompiler::compile_opening_pc_tiling(
        &OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(
                queue.to_vec(),
            )))
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_objective(ObjectivePolicy::tiling())
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
            .with_execution_policy(policy),
    )
    .expect("valid canonical two-line tiling request")
}

fn expected_iiooo_identities() -> BTreeSet<StandardBoard64TilingIdentity> {
    // Each I must be horizontal. An O occupies the same columns in both
    // rows, so the two I pieces occupy the same four-column interval.
    // Its left edge must be even to tile both sides with 2×2 O pieces.
    [0, 2, 4, 6]
        .into_iter()
        .map(|left| {
            let mut pieces = vec![
                PiecePlacementMask::new(PieceKind::I, 0b1111_u64 << left),
                PiecePlacementMask::new(PieceKind::I, 0b1111_u64 << (10 + left)),
            ];
            for column in [0, 2, 4, 6, 8] {
                if !(left..left + 4).contains(&column) {
                    let row = 0b11_u64 << column;
                    pieces.push(PiecePlacementMask::new(PieceKind::O, row | (row << 10)));
                }
            }
            StandardBoard64TilingIdentity::from_placements(0, pieces)
                .expect("independent exact rectangle tiling")
        })
        .collect()
}

fn page_identities(result: &CoreExecutionResult) -> BTreeSet<StandardBoard64TilingIdentity> {
    let store = result
        .tiling_solution_page_store()
        .expect("canonical tiling result has a page store");
    let page = store
        .page_identities(0, store.len())
        .expect("compact tiling identities should be available");
    let identities: BTreeSet<_> = page.into_iter().collect();
    assert_eq!(identities.len(), store.len());
    assert_eq!(
        result.usize_field("normalized_unique_solution_count"),
        Some(store.len())
    );
    identities
}

type Execute =
    fn(&SearchProblem, &ExecutionControl) -> Result<CoreExecutionResult, CoreExecutionError>;

fn execute_rust(
    problem: &SearchProblem,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    WasmCpuSearchBackend::execute_with_control(problem, control)
        .map_err(|error| error.into_core_execution_error())
}

#[test]
fn native_rust_pc_tiling_controls_do_not_conflate_cancelled_incomplete_and_empty() {
    let complete = problem(IIOOO, None);
    let capped = problem(IIOOO, Some(1));
    // Every T contributes an imbalance of either +2 or -2 in the
    // checkerboard coloring. Five such pieces can never fill a 10×2
    // rectangle, which has ten cells of each color.
    let impossible = problem([PieceKind::T; 5], None);
    let expected = expected_iiooo_identities();
    assert_eq!(expected.len(), 4);

    let backends: [(&str, Execute); 2] = [
        ("native C", CoreExecutor::execute_with_control),
        ("Rust CPU", execute_rust),
    ];
    for (label, execute) in backends {
        let cancelled = ExecutionCancellationToken::new();
        cancelled.handle().cancel();
        assert_eq!(
            execute(&complete, &ExecutionControl::new(cancelled)).unwrap_err(),
            CoreExecutionError::Cancelled,
            "{label}"
        );

        let result = execute(&complete, &ExecutionControl::default())
            .unwrap_or_else(|error| panic!("{label} complete: {error:?}"));
        assert_eq!(result.bool_field("count_complete"), Some(true), "{label}");
        assert_eq!(
            result.bool_field("tiling_family_complete"),
            Some(true),
            "{label}"
        );
        assert_eq!(
            result.bool_field("resource_truncated"),
            Some(false),
            "{label}"
        );
        assert_eq!(page_identities(&result), expected, "{label}");
        drop(result);

        let result = execute(&capped, &ExecutionControl::default())
            .unwrap_or_else(|error| panic!("{label} candidate cut: {error:?}"));
        assert_eq!(result.bool_field("count_complete"), Some(false), "{label}");
        assert_eq!(
            result.bool_field("tiling_family_complete"),
            Some(false),
            "{label}"
        );
        assert_eq!(
            result.bool_field("resource_truncated"),
            Some(true),
            "{label}"
        );
        assert_eq!(
            result.field("resource_truncation_reason"),
            Some("candidate_budget_exceeded"),
            "{label}"
        );
        let prefix = page_identities(&result);
        assert_eq!(prefix.len(), 1, "{label} retained exactly one result");
        assert!(prefix.is_subset(&expected), "{label}");
        // Producer enumeration order is not a parity requirement when
        // neither result claims a complete family.
        assert!(!result.pc_tiling_family_publication_contract_is_valid());
        drop(result);

        let result = execute(&impossible, &ExecutionControl::default())
            .unwrap_or_else(|error| panic!("{label} complete empty: {error:?}"));
        assert_eq!(result.bool_field("count_complete"), Some(true), "{label}");
        assert_eq!(
            result.bool_field("tiling_family_complete"),
            Some(true),
            "{label}"
        );
        assert_eq!(
            result.bool_field("resource_truncated"),
            Some(false),
            "{label}"
        );
        assert_eq!(
            result.field("resource_truncation_reason"),
            Some("none"),
            "{label}"
        );
        assert!(page_identities(&result).is_empty(), "{label}");
    }
}
