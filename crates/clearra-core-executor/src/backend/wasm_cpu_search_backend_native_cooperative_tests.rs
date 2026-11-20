use std::{env, process::Command};

use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionCancellationToken,
    piece::piece_kind::PieceKind, resource::ResourceLeaseToken, runtime_cpu_capacity::CpuCapacity,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;
use crate::resource::admit_budget_bound_search_execution;

const WORKERS: usize = 2;
const HEIGHT: u16 = 7;

fn fixture(empty_geometry: bool, policy: PcExecutionPolicy) -> SearchProblem {
    fixture_with_height(HEIGHT, empty_geometry, policy)
}

fn fixture_with_height(
    height: u16,
    empty_geometry: bool,
    policy: PcExecutionPolicy,
) -> SearchProblem {
    let missing_cells = if empty_geometry {
        // No row or column has four empty cells: even the I Geometry family
        // is empty, before movement or queue verification.
        (0..HEIGHT)
            .map(|row| 11 * row)
            .chain([69])
            .collect::<Vec<_>>()
    } else if height == HEIGHT {
        // Known ordinary Oracle conformance fixture with multiple supported
        // candidate ordinals, not a packet/replay/score-owned family.
        (0..height)
            .flat_map(|row| {
                let start = if row < 3 { 0 } else { 6 };
                (start..start + 4).map(move |x| 10 * row + x)
            })
            .collect()
    } else {
        // A four-cell vertical I channel per four rows. No initially full
        // rows, and one candidate even at 24L, spanning all four board words.
        (0..height).map(|row| row * 10 + row / 4).collect()
    };
    let missing = missing_cells
        .into_iter()
        .map(|cell| Board256Mask::singleton(cell).unwrap())
        .fold(Board256Mask::EMPTY, Board256Mask::union);
    let board = Board256Mask::all_cells(height * 10)
        .unwrap()
        .without(missing);
    let depth = missing.count_ones() as usize / 4;
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(height, board.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; depth])),
        PieceWindow::new(depth),
    )
    .with_exact_pieces(Some(depth))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_execution_policy(policy);
    ProblemCompiler::compile_scenario_pc(&query).unwrap()
}

fn policy() -> PcExecutionPolicy {
    PcExecutionPolicy::default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_use_all_logical_processors(true)
        .with_workers(WORKERS)
}

fn leases(session: &WasmCpuSearchSession) -> (ResourceLeaseToken, Option<ResourceLeaseToken>) {
    let WasmSearchSessionInner::ExtendedPc(extended) = &session.inner else {
        panic!("experimental admission must live inside the ordinary extended PC backend");
    };
    extended.native_cooperative_leases_for_test().unwrap()
}

fn drive(
    session: &mut WasmCpuSearchSession,
    budget: usize,
) -> Result<(CoreExecutionResult, usize), WasmCpuSearchError> {
    let control = ExecutionControl::default();
    for steps in 1..=100_000 {
        match session.advance(budget, &control)? {
            WasmCpuSearchAdvance::Completed(result) => return Ok((result, steps)),
            WasmCpuSearchAdvance::Pending => {}
            WasmCpuSearchAdvance::Cancelled => panic!("unexpected cancellation"),
        }
    }
    panic!("small native PC conformance fixture did not settle")
}

fn drive_experimental(
    session: &mut WasmCpuSearchSession,
    budget: usize,
) -> Result<(NativePcCooperativeAdvance, usize), WasmCpuSearchError> {
    let control = ExecutionControl::default();
    for steps in 1..=100_000 {
        match session.advance_native_cooperative_for_test(budget, &control)? {
            NativePcCooperativeAdvance::Pending => {}
            terminal => return Ok((terminal, steps)),
        }
    }
    panic!("small leased native PC conformance fixture did not settle")
}

fn assert_family_parity(direct: &CoreExecutionResult, experimental: &CoreExecutionResult) {
    assert_eq!(
        experimental.normalized_solution_keys(),
        direct.normalized_solution_keys()
    );
    assert_eq!(
        experimental.coverage_pattern_words(),
        direct.coverage_pattern_words()
    );
    assert_eq!(experimental.path_steps(), direct.path_steps());
    for key in [
        "normalized_solution_set_hash",
        "packing_candidate_count",
        "packing_candidate_digest",
        "coverage_probability",
        "probability_complete",
        "build_variant_count",
        "build_variant_count_exact",
        "count_complete",
        "solution_found",
        "workers_requested",
        "workers_used",
        "cpu_parallel_execution",
    ] {
        assert_eq!(experimental.field(key), direct.field(key), "{key}");
    }
}

#[test]
fn ordinary_native_pc_cooperative_session_preserves_admission_and_exact_family() {
    const CHILD: &str = "CLEARRA_TEST_EXTENDED_PC_NATIVE_ADMISSION_CHILD";
    if env::var_os(CHILD).is_none() {
        if CpuCapacity::current().hard_limit() < WORKERS {
            eprintln!("two logical processors are required to check this admission seam");
            return;
        }
        let name = format!(
            "{}::ordinary_native_pc_cooperative_session_preserves_admission_and_exact_family",
            module_path!().split_once("::").unwrap().1
        );
        let result = Command::new(env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env(CHILD, "1")
            .env("CLEARRA_EXPECTED_VCPUS", WORKERS.to_string())
            .output()
            .unwrap();
        assert!(
            result.status.success()
                && String::from_utf8_lossy(&result.stdout)
                    .lines()
                    .any(|line| line == "clearra_native_admission_seam=complete"),
            "isolated admission seam failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let control = ExecutionControl::default();
    for (height, empty_geometry) in [(7, false), (7, true), (8, false), (12, false), (24, false)] {
        let problem = fixture_with_height(height, empty_geometry, policy());
        let mut direct = WasmCpuSearchSession::new(&problem).unwrap();
        let (direct_result, _) = drive(&mut direct, 1).unwrap();
        drop(direct);
        assert_eq!(
            direct_result.normalized_solution_keys().is_empty(),
            empty_geometry
        );
        let mut experimental =
            WasmCpuSearchSession::new_native_cooperative_for_test(&problem, 0).unwrap();
        let (original, published) = leases(&experimental);
        assert!(published.is_none());
        assert_eq!(original.grant().compute_units as usize, WORKERS);
        assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_err());
        assert_eq!(
            experimental.advance(1, &control).err().unwrap().reason(),
            "extended_pc_native_cooperative_requires_leased_result_advance"
        );
        for _ in 0..3 {
            assert!(matches!(
                experimental
                    .advance_native_cooperative_for_test(0, &control)
                    .unwrap(),
                NativePcCooperativeAdvance::Pending
            ));
            assert_eq!(leases(&experimental), (original, None));
        }
        let (NativePcCooperativeAdvance::Completed(published), steps) =
            drive_experimental(&mut experimental, 1).unwrap()
        else {
            panic!("the bounded experimental session must publish a leased result");
        };
        assert!(
            steps > 1,
            "a one-unit poll cannot run the synchronous fallback"
        );
        assert_family_parity(&direct_result, published.result());
        assert_eq!(
            published.result().usize_field("workers_used"),
            Some(if empty_geometry { 1 } else { WORKERS })
        );
        assert_eq!(published.lease_token(), original);
        assert_eq!(leases(&experimental), (original, Some(original)));
        assert_eq!(
            experimental
                .advance_native_cooperative_for_test(1, &control)
                .err()
                .unwrap()
                .reason(),
            "extended_pc_native_cooperative_session_already_finished"
        );
        assert_eq!(leases(&experimental), (original, Some(original)));
        assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_err());
        drop(experimental);
        // Even after the session drops, the caller-held result keeps the
        // exact original admission, not a replacement or metadata copy.
        assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_err());
        assert_family_parity(&direct_result, published.result());
        drop(published);
        assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_ok());
    }

    let single = fixture(false, policy().with_workers(1));
    assert_eq!(
        WasmCpuSearchSession::new_native_cooperative_for_test(&single, 0)
            .err()
            .unwrap()
            .reason(),
        "extended_pc_native_cooperative_ineligible"
    );
    let gpu = fixture(
        false,
        policy().with_requested_backend(RequestedSearchBackend::Gpu),
    );
    assert_eq!(
        WasmCpuSearchSession::new_native_cooperative_for_test(&gpu, 0)
            .err()
            .unwrap()
            .reason(),
        "extended_pc_family_requires_cpu_backend"
    );
    let limited = fixture(false, policy().with_max_nodes(1));
    let mut direct = WasmCpuSearchSession::new(&limited).unwrap();
    let expected = drive(&mut direct, 1)
        .err()
        .expect("direct node limit must fail closed");
    drop(direct);
    let mut experimental =
        WasmCpuSearchSession::new_native_cooperative_for_test(&limited, 0).unwrap();
    let exhausted = drive_experimental(&mut experimental, 1)
        .err()
        .expect("experimental node limit must fail closed");
    let (
        WasmCpuSearchError::ResourceAdmission {
            resource_report: expected,
        },
        WasmCpuSearchError::ResourceAdmission {
            resource_report: exhausted,
        },
    ) = (expected, exhausted)
    else {
        panic!("the exact node cap must reject with resource-incomplete evidence");
    };
    assert_eq!(
        expected.execution_availability().reason(),
        exhausted.execution_availability().reason()
    );
    assert!(!expected.result_complete() && !exhausted.result_complete());
    assert!(leases(&experimental).1.is_none());
    assert!(admit_budget_bound_search_execution(&limited, WORKERS).is_ok());
    drop(experimental);

    let problem = fixture(false, policy());
    assert_eq!(
        WasmCpuSearchSession::new_native_cooperative_for_test(&problem, u128::MAX)
            .err()
            .unwrap()
            .reason(),
        "extended_pc_memory_projection_unavailable"
    );
    let mut experimental =
        WasmCpuSearchSession::new_native_cooperative_for_test(&problem, 0).unwrap();
    assert!(matches!(
        experimental
            .advance_native_cooperative_for_test(1, &control)
            .unwrap(),
        NativePcCooperativeAdvance::Pending
    ));
    let cancel = ExecutionCancellationToken::new();
    cancel.handle().cancel();
    assert!(matches!(
        experimental
            .advance_native_cooperative_for_test(1, &ExecutionControl::new(cancel))
            .unwrap(),
        NativePcCooperativeAdvance::Cancelled
    ));
    assert!(leases(&experimental).1.is_none());
    assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_ok());
    // The actual App-dispatched API must retain the same admission even
    // after the session is gone and while projecting the terminal result.
    let problem = fixture_with_height(24, false, policy());
    let control = ExecutionControl::default();
    let expected = WasmCpuSearchBackend::execute_with_control(&problem, &control).unwrap();
    let mut sliced = NativePcCooperativeSession::try_new(&problem, 0)
        .unwrap()
        .expect("ordinary 24L fixed queue is in the sliced domain");
    assert!(matches!(
        sliced.advance(1, &control).unwrap(),
        NativePcCooperativeAdvance::Pending
    ));
    let result = (0..100_000)
        .find_map(|_| match sliced.advance(64, &control).unwrap() {
            NativePcCooperativeAdvance::Pending => None,
            NativePcCooperativeAdvance::Cancelled => panic!("no cancellation requested"),
            NativePcCooperativeAdvance::Completed(result) => Some(result),
        })
        .expect("bounded forced 24L fixture did not settle");
    assert_eq!(result.lease_token().grant().compute_units as usize, WORKERS);
    drop(sliced);
    result.finish(|result| {
        assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_err());
        assert_family_parity(&expected, &result);
    });
    assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_ok());
    println!("clearra_native_admission_seam=complete");
}
