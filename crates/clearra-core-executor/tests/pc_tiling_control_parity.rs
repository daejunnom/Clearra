#![cfg(feature = "native-c-core")]

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use clearra_core_domain::{
    execution_cancellation::{
        ExecutionCancellationToken, ExecutionControl, ExecutionProgress, ProgressSink,
    },
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

// The native reference holds a process-wide packing lease. These independent
// assertions exercise real admission, but must not compete for the same lease.
static NATIVE_REFERENCE_FIXTURE: Mutex<()> = Mutex::new(());

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

#[derive(Default)]
struct ProgressLog(Mutex<Vec<ExecutionProgress>>);

impl ProgressSink for ProgressLog {
    fn report(&self, progress: ExecutionProgress) {
        self.0.lock().expect("progress log").push(progress);
    }
}

fn count_all_problem(workers: usize) -> SearchProblem {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(workers)
        .with_worker_hardware_limit(workers)
        .with_use_all_logical_processors(true)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false);
    ProblemCompiler::compile_opening_pc(
        &OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(
                IIOOO.to_vec(),
            )))
            .with_hold_policy(PcHoldPolicy::Disabled)
            .with_objective(ObjectivePolicy::all())
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
            .with_execution_policy(policy),
    )
    .expect("valid fixed-queue two-line CountAll request")
}

fn report_count_all_witness(
    backend: &str,
    requested_workers: usize,
    result: &CoreExecutionResult,
    progress: &ProgressLog,
) {
    let active_workers = result.usize_field("parallel_active_workers");
    let parallel_observed = result.bool_field("cpu_parallel_execution") == Some(true)
        && active_workers == Some(requested_workers);
    let observed_stages: Vec<_> = progress
        .0
        .lock()
        .expect("progress log")
        .iter()
        .map(|event| {
            serde_json::json!({
                "stage": event.stage,
                "completed": event.completed,
                "total": event.total,
            })
        })
        .collect();
    // The legacy result's workers_used describes its packing executor,
    // not the active workers of the independent BuildUp stage. A request
    // for two workers is not evidence of a whole-search parallel run.
    let witness = serde_json::json!({
        "fixture": "empty-2l-iiooo-no-hold-count-all",
        "backend": backend,
        "requested_workers": requested_workers,
        "reported_workers_used": result.usize_field("workers_used"),
        "reported_active_workers": active_workers,
        "native_cpu_geometry_dag_workers": result.usize_field("native_cpu_geometry_dag_workers"),
        "native_cpu_candidate_workers": result.usize_field("native_cpu_candidate_workers"),
        "native_cpu_buildup_workers": result.usize_field("native_cpu_buildup_workers"),
        "parallel_observed": requested_workers > 1 && parallel_observed,
        "packing_candidate_count": result.usize_field("packing_candidate_count"),
        "backend_searched_nodes": result.usize_field("searched_nodes"),
        "buildup_backend": result.field("buildup_backend"),
        "build_variant_count": result.field("build_variant_count"),
        "build_order_nodes": result.usize_field("total_build_order_nodes"),
        "covered_pattern_count": result.usize_field("covered_pattern_count"),
        "normalized_solution_count": result.usize_field("normalized_unique_solution_count"),
        "normalized_solution_set_hash": result.field("normalized_solution_set_hash"),
        "observed_stages": observed_stages,
    });
    println!("{witness}");
}

#[test]
fn native_rust_fixed_queue_count_all_preserves_verified_identity_family() {
    let _lease = NATIVE_REFERENCE_FIXTURE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // On this empty 2L board the first I can rest on the floor, the second
    // on the first, and the O pieces can hard-drop into the remaining gaps.
    let expected: BTreeSet<_> = expected_iiooo_identities()
        .into_iter()
        .map(|identity| {
            let mut key = String::new();
            identity
                .write_canonical(&mut key)
                .expect("write canonical identity");
            key
        })
        .collect();
    assert_eq!(expected.len(), 4);
    let backends: [(&str, Execute); 2] = [
        (
            "native-c",
            CoreExecutor::execute_native_c_reference_with_control,
        ),
        ("rust-cpu", execute_rust),
    ];
    for requested_workers in [1, 2] {
        let problem = count_all_problem(requested_workers);
        for (backend, execute) in backends {
            let progress = Arc::new(ProgressLog::default());
            let control = ExecutionControl::default().with_progress_sink(progress.clone());
            let result = execute(&problem, &control)
                .unwrap_or_else(|error| panic!("{backend} CountAll: {error:?}"));
            report_count_all_witness(backend, requested_workers, &result, &progress);
            assert_eq!(result.bool_field("count_complete"), Some(true), "{backend}");
            assert!(result.solution_found(), "{backend}");
            if backend == "native-c" {
                assert_eq!(
                    result.usize_field("native_cpu_geometry_dag_workers"),
                    Some(1)
                );
                assert!(result
                    .usize_field("native_cpu_candidate_workers")
                    .is_some_and(|workers| (1..=requested_workers).contains(&workers)));
                // Four retained candidates do not meet the 64-candidate
                // threshold for the second, independent final BuildUp pass.
                assert_eq!(result.usize_field("native_cpu_buildup_workers"), Some(1));
            } else {
                assert_eq!(result.usize_field("native_cpu_candidate_workers"), None);
                assert_eq!(result.usize_field("native_cpu_buildup_workers"), None);
            }
            assert_eq!(
                result.usize_field("normalized_unique_solution_count"),
                Some(expected.len()),
                "{backend}",
            );
            assert_eq!(
                result
                    .normalized_solution_keys()
                    .iter()
                    .collect::<BTreeSet<_>>(),
                expected.iter().collect::<BTreeSet<_>>(),
                "{backend}",
            );
            assert!(
                result
                    .field("build_variant_count")
                    .and_then(|count| count.parse::<u128>().ok())
                    .is_some_and(|count| count > 0),
                "{backend} must run BuildUp",
            );
        }
    }
}

#[test]
fn native_rust_pc_tiling_controls_do_not_conflate_cancelled_incomplete_and_empty() {
    let _lease = NATIVE_REFERENCE_FIXTURE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let complete = problem(IIOOO, None);
    let capped = problem(IIOOO, Some(1));
    // Every T contributes an imbalance of either +2 or -2 in the
    // checkerboard coloring. Five such pieces can never fill a 10×2
    // rectangle, which has ten cells of each color.
    let impossible = problem([PieceKind::T; 5], None);
    let expected = expected_iiooo_identities();
    assert_eq!(expected.len(), 4);

    let backends: [(&str, Execute); 2] = [
        (
            "native C",
            CoreExecutor::execute_native_c_reference_with_control,
        ),
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
        if label == "native C" {
            assert_eq!(
                result.usize_field("native_cpu_raw_geometry_workers"),
                Some(1)
            );
            assert_eq!(result.usize_field("native_cpu_geometry_dag_workers"), None);
            assert_eq!(result.usize_field("native_cpu_buildup_workers"), Some(0));
        }
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
