use std::{env, process::Command};

use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionCancellationToken,
    piece::piece_kind::PieceKind, resource::ResourceLeaseToken, runtime_cpu_capacity::CpuCapacity,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::{BuildProbabilityField, ProblemCompiler, SearchProblem};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::{cooperative::DryRunOwner, *};
use crate::resource::admit_budget_bound_search_execution;

const HEIGHT: u8 = 7;
const WORKERS: usize = 2;

fn holes(cells: impl IntoIterator<Item = u16>) -> Board256Mask {
    cells
        .into_iter()
        .map(|cell| Board256Mask::singleton(cell).unwrap())
        .fold(Board256Mask::EMPTY, Board256Mask::union)
}

fn fixture(cells: Board256Mask, queue: Vec<PieceKind>) -> (SearchProblem, BuildProbabilityField) {
    let initial = Board256Mask::all_cells(u16::from(HEIGHT) * 10)
        .unwrap()
        .without(cells);
    let depth = queue.len();
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(u16::from(HEIGHT), initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(queue)),
        PieceWindow::new(depth),
    )
    .with_exact_pieces(Some(depth))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_use_all_logical_processors(true)
            .with_workers(WORKERS),
    );
    (
        ProblemCompiler::compile_scenario_pc(&query).unwrap(),
        BuildProbabilityField::from_words_preserving_height(HEIGHT, initial.words(), cells.words())
            .unwrap(),
    )
}

fn drive(owner: &mut DryRunOwner, control: &ExecutionControl) {
    assert!((0..100_000).any(|index| {
        let budget = [1, 3, 2, 1][index % 4];
        let (done, used) = owner.advance(budget, control).unwrap();
        assert!((1..=budget).contains(&used));
        if !done {
            assert!(owner.completed().is_none());
            assert!(owner.serial_handoff_pending().is_none());
            assert!(owner.parallel_result_pending().is_none());
        }
        done
    }));
}

fn direct_parallel(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    control: &ExecutionControl,
) -> CoreExecutionResult {
    let admission = admit_budget_bound_search_execution(problem, WORKERS).unwrap();
    let mut engine =
        ExtendedBuildProbabilitySession::new_pc_family(problem, field, admission.memory_bound(), 0)
            .unwrap();
    engine
        .execute_pc_family_parallel(WORKERS, control)
        .unwrap()
        .expect("a nonempty Geometry plan")
}

fn handoff_existing(
    problem: &SearchProblem,
    field: BuildProbabilityField,
) -> (DryRunOwner, ResourceLeaseToken) {
    assert!(DryRunOwner::is_eligible(problem, field));
    let admission = admit_budget_bound_search_execution(problem, WORKERS).unwrap();
    let original = admission.lease_token();
    let engine =
        ExtendedBuildProbabilitySession::new_pc_family(problem, field, admission.memory_bound(), 0)
            .unwrap();
    assert!(admit_budget_bound_search_execution(problem, WORKERS).is_err());
    (
        DryRunOwner::from_admitted(engine, admission, original).unwrap(),
        original,
    )
}

fn assert_parallel_terminal_matches_direct(
    owner: DryRunOwner,
    problem: &SearchProblem,
    expected: &CoreExecutionResult,
    control: &ExecutionControl,
) {
    let lease = owner
        .parallel_result_pending()
        .expect("parallel terminal ready");
    let keys = owner.completed().unwrap().keys;
    let backing = keys.as_ptr();
    let first_text = keys.first().map(|key| key.as_ptr());
    let (actual, retention) = owner
        .into_parallel_result(control)
        .unwrap()
        .into_result_and_retention();
    assert_eq!(retention.lease_token(), lease, "same original lease");
    assert!(admit_budget_bound_search_execution(problem, WORKERS).is_err());
    assert_eq!(actual.normalized_solution_keys().as_ptr(), backing);
    assert_eq!(
        actual
            .normalized_solution_keys()
            .first()
            .map(|key| key.as_ptr()),
        first_text
    );
    assert_eq!(
        actual.normalized_solution_keys(),
        expected.normalized_solution_keys()
    );
    assert_eq!(
        actual.coverage_pattern_words(),
        expected.coverage_pattern_words()
    );
    assert_eq!(actual.path_steps(), expected.path_steps());
    for key in [
        "normalized_solution_set_hash",
        "actual_normalized_solution_set_hash",
        "packing_candidate_count",
        "packing_candidate_digest",
        "build_variant_count",
        "build_variant_count_exact",
        "count_complete",
        "objective_complete",
        "solution_found",
        "representative_physical_replay_validated",
        "coverage_pattern_count",
        "covered_pattern_count",
        "coverage_probability",
        "probability_complete",
        "workers_requested",
        "workers_used",
        "cpu_parallel_execution",
    ] {
        assert_eq!(actual.field(key), expected.field(key), "{key}");
    }
    drop(actual);
    assert!(admit_budget_bound_search_execution(problem, WORKERS).is_err());
    drop(retention);
    assert!(admit_budget_bound_search_execution(problem, WORKERS).is_ok());
}

#[test]
fn cooperative_fixed_ordinal_verification_stats_and_serial_handoff() {
    const CHILD: &str = "CLEARRA_TEST_EXTENDED_PC_COOPERATIVE_STATS_CHILD";
    if env::var_os(CHILD).is_none() {
        if CpuCapacity::current().hard_limit() < WORKERS {
            eprintln!("cooperative verification stats require two admitted logical processors");
            return;
        }
        let name = format!(
            "{}::cooperative_fixed_ordinal_verification_stats_and_serial_handoff",
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
                    .any(|line| line == "clearra_cooperative_verification_stats=complete"),
            "isolated verification fold failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let control = ExecutionControl::default();

    // The tested 7L worker-wave fixture: multiple compatible witnesses,
    // but the original materialized fixed-queue language is exactly bit 0.
    let (problem, field) = fixture(
        holes((0_u16..7).flat_map(|row| {
            let start = if row < 3 { 0 } else { 6 };
            (start..start + 4).map(move |x| 10 * row + x)
        })),
        vec![PieceKind::I; 7],
    );
    let expected = direct_parallel(&problem, field, &control);
    assert_eq!(expected.bool_field("build_variant_count_exact"), Some(true));
    assert!(!expected.path_steps().is_empty());
    // Capture an older lease with identical requested width and memory.
    // Matching limits do not authorize replacing the engine's lease epoch.
    let stale = admit_budget_bound_search_execution(&problem, WORKERS)
        .unwrap()
        .lease_token();
    let current = admit_budget_bound_search_execution(&problem, WORKERS).unwrap();
    assert_ne!(stale, current.lease_token());
    let engine =
        ExtendedBuildProbabilitySession::new_pc_family(&problem, field, current.memory_bound(), 0)
            .unwrap();
    assert!(matches!(
        DryRunOwner::from_admitted(engine, current, stale),
        Err(WasmExactSearchError::InvalidProblem(_))
    ));

    let (mut owner, original) = handoff_existing(&problem, field);
    drive(&mut owner, &control);
    assert_eq!(owner.parallel_result_pending(), Some(original));
    let facts = owner.completed().unwrap();
    assert!(facts
        .workers
        .iter()
        .any(|worker| worker.engine.peak_build_scratch_bytes != 0));
    assert!(facts.workers.iter().all(|worker| {
        worker.engine.build_order_workspace.retained_bytes() == 0
            && worker.engine.coverage_evaluator.retained_bytes() == 0
            && worker.engine.buildable_tilings.capacity() == 0
            && worker.digests.capacity() == 0
    }));
    assert!(owner.serial_handoff_pending().is_none());
    assert_eq!(facts.keys, expected.normalized_solution_keys());
    assert_eq!(
        Some(facts.hash),
        expected.field("normalized_solution_set_hash")
    );
    let stats = facts.verification;
    assert_eq!(
        [stats.coverage_word].as_slice(),
        expected.coverage_pattern_words()
    );
    assert_eq!(
        Some(stats.count_all.to_string().as_str()),
        expected.field("build_variant_count")
    );
    let (rank, pattern, path) = stats.representative.unwrap();
    assert_eq!(pattern, 0);
    assert_eq!(
        Some(rank),
        facts
            .workers
            .iter()
            .filter_map(|worker| worker.engine.representative_rank)
            .min()
    );
    assert_eq!(path, expected.path_steps());
    assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_err());
    assert_parallel_terminal_matches_direct(owner, &problem, &expected, &control);

    // Cancel only after all immutable facts and the terminal hash are
    // available. No result is returned and dropping the consumed owner frees
    // the original lease, rather than allowing this cancellation to retry.
    let mut owner = DryRunOwner::new(&problem, field).unwrap();
    drive(&mut owner, &control);
    assert!(owner.parallel_result_pending().is_some());
    let cancellation = ExecutionCancellationToken::new();
    cancellation.handle().cancel();
    assert!(matches!(
        owner.into_parallel_result(&ExecutionControl::new(cancellation)),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_ok());

    // Geometry can cover the sealed O and the five disjoint horizontal I
    // slots, but the fixed queue forces O to play before any uncovering I.
    // Its 2x2 cavity has no open entry from above or either side.
    let (problem, field) = fixture(
        holes(
            [0_u16, 1, 10, 11]
                .into_iter()
                .chain((2_u16..7).flat_map(|row| {
                    let start = if row % 2 == 0 { 6 } else { 0 };
                    (start..start + 4).map(move |x| 10 * row + x)
                })),
        ),
        [PieceKind::O]
            .into_iter()
            .chain([PieceKind::I; 5])
            .collect(),
    );
    let expected = direct_parallel(&problem, field, &control);
    assert!(expected.usize_field("packing_candidate_count").unwrap() > 0);
    assert!(expected.normalized_solution_keys().is_empty());
    assert_eq!(expected.coverage_pattern_words(), &[0]);
    let mut owner = DryRunOwner::new(&problem, field).unwrap();
    drive(&mut owner, &control);
    let empty = owner.completed().unwrap();
    assert!(owner.serial_handoff_pending().is_none());
    assert!(empty.candidate_count > 0 && empty.keys.is_empty());
    assert_eq!(
        (
            empty.verification.coverage_word,
            empty.verification.count_all
        ),
        (0, 0)
    );
    assert!(empty.verification.representative.is_none());
    assert_parallel_terminal_matches_direct(owner, &problem, &expected, &control);

    // Each row and every column has fewer than four holes: no I layout
    // exists even in original-row geometry, independently of reachability.
    let (problem, field) = fixture(
        holes((0_u16..7).map(|row| 11 * row).chain([69])),
        vec![PieceKind::I; 2],
    );
    let (mut owner, original) = handoff_existing(&problem, field);
    drive(&mut owner, &control);
    assert!(owner.completed().is_none(), "no parallel terminal report");
    assert!(owner.parallel_result_pending().is_none());
    let lease = owner
        .serial_handoff_pending()
        .expect("serial owner handoff");
    assert_eq!(lease, original);
    let mut serial = owner.into_serial_handoff(&control).unwrap();
    assert_eq!(serial.admission.lease_token(), lease);
    assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_err());
    let result = (0..100)
        .find_map(|_| match serial.engine.advance(1, &control).unwrap() {
            BuildProbabilityAdvance::Pending => None,
            BuildProbabilityAdvance::Completed(result) => Some(result),
            BuildProbabilityAdvance::Cancelled => panic!("unexpected handoff cancellation"),
        })
        .expect("the original serial state settles its own empty Geometry");
    assert_eq!(result.bool_field("count_complete"), Some(true));
    assert_eq!(result.usize_field("packing_candidate_count"), Some(0));
    assert_eq!(result.usize_field("workers_used"), Some(1));
    drop(serial);
    assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_ok());
    println!("\nclearra_cooperative_verification_stats=complete");
}
