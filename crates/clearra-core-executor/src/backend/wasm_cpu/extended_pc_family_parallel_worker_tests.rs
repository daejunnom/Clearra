use std::{env, process::Command, thread};

use crate::resource::{admit_budget_bound_search_execution, ExecutionAdmission};
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    execution_cancellation::{
        ExecutionCancellationHandle, ExecutionCancellationToken, ExecutionProgress, ProgressSink,
    },
    piece::piece_kind::PieceKind,
    runtime_cpu_capacity::CpuCapacity,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::{BuildProbabilityField, ProblemCompiler, SearchProblem};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::{
    cooperative::{DryRunOwner, WorkerCursor, WorkerWaves},
    *,
};

const WORKERS: usize = 2;
const HEIGHT: u8 = 7;

#[test]
fn huge_branch_digest_reservation_is_metadata_only_and_cancel_remains_observable() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(HEIGHT.into(), 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 2])),
        PieceWindow::new(2),
    )
    .with_exact_pieces(Some(2))
    .with_allow_hold(false);
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let field =
        BuildProbabilityField::from_words_preserving_height(HEIGHT, [0; 4], [0x3c0f, 0, 0, 0])
            .unwrap();
    let mut root = ExtendedBuildProbabilitySession::new(
        &problem,
        field,
        clearra_problem::BuildProbabilityAggregation::Buildability,
    )
    .unwrap();
    for _ in 0..10_000 {
        if !root.geometry.is_compiling() {
            break;
        }
        assert!(matches!(
            root.advance_geometry_bounded().unwrap(),
            ExtendedGeometryAdvance::Pending
        ));
    }
    assert!(!root.geometry.is_compiling());
    let live = root.checked_retained_bytes().unwrap();
    let mut plan = root
        .geometry
        .take_parallel_plan(1, root.memory_bound, live)
        .unwrap()
        .unwrap();
    let mut branch = plan.branches.pop().unwrap();
    // The cardinality header must not be an allocation instruction. Its
    // value cannot even be represented as a Vec length on this machine.
    branch.candidate_count = u128::MAX;
    let fixed = root.checked_retained_bytes().unwrap()
        + plan.shared_retained_bytes() as u128
        + plan.branch_retained_bytes() as u128;
    let aggregate = SharedExecutionMemoryBudget::new(root.memory_bound, fixed, 1).unwrap();
    let engine = root
        .new_shared_family_verifier_with_resources(root.memory_bound, Some(aggregate.owner(0)))
        .unwrap();
    let mut worker = WorkerResult {
        engine,
        digests: Vec::new(),
        private_peak_bytes: 0,
        bounded_rows: None,
        terminal_base: None,
    };
    let index = worker.reserve_branch::<false>(&branch).unwrap();
    assert_eq!(worker.digests[index].first_ordinal, branch.first_ordinal);
    assert_eq!(worker.digests[index].values.len(), 0);
    assert_eq!(worker.digests[index].values.retained_bytes(), 0);
    assert_eq!(worker.engine.processed_candidate_count, 0);
    assert_eq!(
        worker.checked_auxiliary_retained_bytes::<false>().unwrap(),
        (worker.digests.capacity() * size_of::<DigestSegment>()) as u128
    );

    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    let queue = Mutex::new(Vec::new());
    assert!(matches!(
        run_worker(
            worker.engine,
            Some(branch),
            &queue,
            &aggregate.owner(0),
            &AtomicBool::new(false),
            &AtomicUsize::new(0),
            &ExecutionControl::new(token),
        ),
        Err(WasmExactSearchError::Cancelled)
    ));
}

#[test]
fn symbolic_worker_finalization_keeps_digest_bytes_in_aggregate_admission() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(8, 0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(2),
    )
    .with_exact_pieces(Some(2))
    .with_supply_window_size(clearra_pc_graph::request::SupplyWindowSize::new(2))
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_queue_observation_policy(clearra_supply::QueueObservationPolicy::FullQueueOracle);
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let field =
        BuildProbabilityField::from_words_preserving_height(8, [0; 4], [0x0003_00cf, 0, 0, 0])
            .unwrap();
    let engine = ExtendedBuildProbabilitySession::new(
        &problem,
        field,
        clearra_problem::BuildProbabilityAggregation::Buildability,
    )
    .unwrap();
    assert!(engine.standard_bag_coverage.is_some());
    let mut values = OrderedCandidateDigest::default();
    for _ in 0..4_096 {
        values.try_push(0).unwrap();
    }
    let mut worker = WorkerResult {
        engine,
        digests: vec![DigestSegment {
            first_ordinal: 0,
            values,
        }],
        private_peak_bytes: 0,
        bounded_rows: None,
        terminal_base: None,
    };
    let future = PatternBitSet::checked_external_words_materialize_union_future_bytes(
        worker.engine.covered_patterns.pattern_count(),
    )
    .unwrap();
    let retained = worker.checked_retained_bytes::<false>().unwrap();
    let original_bound = worker.engine.memory_bound;
    // The aggregate also owns its control block and owner array. Read their
    // admitted size before adding the live worker and finalization projection.
    let control_bytes = SharedExecutionMemoryBudget::new(original_bound, 0, 1)
        .unwrap()
        .admitted_peak_bytes();
    let required = control_bytes + retained + future;
    for shortage in [1, 0] {
        let bound = original_bound.with_cap(required - shortage).unwrap();
        let aggregate = SharedExecutionMemoryBudget::new(bound, 0, 1).unwrap();
        worker.engine.memory_bound = bound;
        worker.engine.private_memory_owner = Some(aggregate.owner(0));
        worker.settle_without_branch::<false>().unwrap();
        assert_eq!(worker.engine.coexisting_retained_bytes, 0);
        let result = worker.finalize_source_coverage::<false>();
        if shortage == 1 {
            assert!(matches!(
                result,
                Err(WasmExactSearchError::ResourceAdmission(_))
            ));
        } else {
            result.expect("the exact aggregate live-plus-finalization cap must suffice");
            assert_eq!(aggregate.admitted_peak_bytes(), required);
        }
        assert_eq!(worker.engine.coexisting_retained_bytes, 0);
    }
}

fn fixture() -> (SearchProblem, BuildProbabilityField) {
    let mut holes = Board256Mask::EMPTY;
    for row in 0..u16::from(HEIGHT) {
        let start = if row < 3 { 0 } else { 6 };
        for x in start..start + 4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + x).unwrap());
        }
    }
    let initial = Board256Mask::all_cells(u16::from(HEIGHT) * 10)
        .unwrap()
        .without(holes);
    let seven = usize::from(HEIGHT);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(u16::from(HEIGHT), initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; seven])),
        PieceWindow::new(seven),
    )
    .with_exact_pieces(Some(seven))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_use_all_logical_processors(true)
            .with_workers(WORKERS),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let field =
        BuildProbabilityField::from_words_preserving_height(HEIGHT, initial.words(), holes.words())
            .unwrap();
    (problem, field)
}

fn engine(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    admission: &ExecutionAdmission,
) -> ExtendedBuildProbabilitySession {
    ExtendedBuildProbabilitySession::new_pc_family(problem, field, admission.memory_bound(), 0)
        .unwrap()
}

fn make_waves(
    root: &mut ExtendedBuildProbabilitySession,
    admission: &ExecutionAdmission,
) -> WorkerWaves {
    for _ in 0..100_000 {
        if !root.geometry.is_compiling() {
            break;
        }
        assert!(matches!(
            root.advance_geometry_bounded().unwrap(),
            ExtendedGeometryAdvance::Pending
        ));
    }
    assert!(!root.geometry.is_compiling(), "bounded seven-line aperture");
    let live = root.checked_retained_bytes().unwrap();
    let mut plan = root
        .geometry
        .take_parallel_plan(WORKERS * 4, root.memory_bound, live)
        .unwrap()
        .expect("nonempty seven-line family");
    assert!(plan.branches.len() >= WORKERS);
    let private_queued = plan
        .branches
        .iter()
        .map(|branch| branch.private_retained_bytes() as u128)
        .sum::<u128>();
    let fixed = root.checked_retained_bytes().unwrap()
        + plan.shared_retained_bytes() as u128
        + plan.branch_retained_bytes() as u128
        - private_queued
        + WorkerWaves::checked_fixed_backing_bytes(WORKERS).unwrap();
    let aggregate =
        SharedExecutionMemoryBudget::new(root.memory_bound, fixed, WORKERS + 1).unwrap();
    let queued = aggregate.owner(WORKERS);
    queued.ensure(private_queued).unwrap();
    let slots = (0..WORKERS)
        .map(|index| {
            let worker = root
                .new_shared_pc_verifier_with_resources(
                    root.memory_bound,
                    Some(aggregate.owner(index)),
                )
                .unwrap();
            let first = plan.branches.pop().unwrap();
            queued
                .transfer_retained_to(
                    worker.private_memory_owner.as_ref().unwrap(),
                    first.private_retained_bytes() as u128,
                )
                .unwrap();
            Some(WorkerCursor::new(worker, Some(first)))
        })
        .collect();
    WorkerWaves::new(admission, slots, plan.branches, queued).unwrap()
}

struct WaveObserver {
    caller: thread::ThreadId,
    background_progress: AtomicUsize,
    cancel_at_progress: Option<ExecutionCancellationHandle>,
    panic_on_background: bool,
}

impl ProgressSink for WaveObserver {
    fn report(&self, progress: ExecutionProgress) {
        if progress.stage == "buildup" {
            if thread::current().id() != self.caller {
                self.background_progress.fetch_add(1, Ordering::Relaxed);
                assert!(!self.panic_on_background, "injected native worker failure");
            }
            if let Some(cancel) = &self.cancel_at_progress {
                cancel.cancel();
            }
        }
    }
}

#[test]
fn extended_pc_native_cooperative_worker_waves_preserve_exact_family_and_drain() {
    const CHILD: &str = "CLEARRA_TEST_EXTENDED_PC_WAVE_CHILD";
    if env::var_os(CHILD).is_none() {
        if CpuCapacity::current().hard_limit() < WORKERS {
            eprintln!("native-wave conformance requires two admitted logical processors");
            return;
        }
        // The actual process-wide execution authority is not shared with
        // other unit tests. Do not alter the runner's global CPU settings.
        let name = format!(
            "{}::extended_pc_native_cooperative_worker_waves_preserve_exact_family_and_drain",
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
                    .any(|line| line == "clearra_worker_wave_conformance=complete"),
            "isolated real wave regression failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }

    let (problem, field) = fixture();
    assert_eq!(problem.backend_policy().workers(), WORKERS);
    let admission = admit_budget_bound_search_execution(&problem, WORKERS).unwrap();
    let expected = {
        let mut direct = engine(&problem, field, &admission);
        direct
            .execute_pc_family_parallel(WORKERS, &ExecutionControl::default())
            .unwrap()
            .expect("ordinary native parallel result")
    };
    assert_eq!(expected.bool_field("count_complete"), Some(true));
    assert!(expected.normalized_solution_keys().len() >= WORKERS);

    let mut root = engine(&problem, field, &admission);
    let mut wave = make_waves(&mut root, &admission);
    let observer = Arc::new(WaveObserver {
        caller: thread::current().id(),
        background_progress: AtomicUsize::new(0),
        cancel_at_progress: None,
        panic_on_background: false,
    });
    let control = ExecutionControl::default().with_progress_sink(observer.clone());
    assert_eq!(wave.advance(&admission, 0, &control).unwrap(), (false, 0));
    for _ in 0..WORKERS {
        assert_eq!(wave.advance(&admission, 1, &control).unwrap(), (false, 1));
    }
    let completed = (0..512).any(|_| {
        let (done, used) = wave.advance(&admission, WORKERS, &control).unwrap();
        assert!(used <= WORKERS);
        done
    });
    assert!(completed && observer.background_progress.load(Ordering::Relaxed) > 0);
    assert_eq!(
        Arc::strong_count(&observer),
        2,
        "no worker control should survive a wave"
    );

    let mut segments = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    let mut covered = PatternBitSet::new(root.covered_patterns.pattern_count());
    let mut counts = 0;
    let mut variants = 0;
    for worker in wave.into_completed_slots().unwrap() {
        assert!(Arc::ptr_eq(&root.catalog, &worker.engine.catalog));
        assert!(worker.engine.processed_candidate_count > 0);
        counts += worker.engine.processed_candidate_count;
        variants += worker.engine.pc_build_variant_count;
        assert!(worker.engine.pc_build_variant_count_complete);
        covered.union_with(&worker.engine.covered_patterns).unwrap();
        for key in &worker.engine.buildable_tilings {
            keys.insert(key.canonical_key(root.catalog.initial_board(), HEIGHT));
        }
        segments.extend(worker.digests);
    }
    segments.sort_unstable_by_key(|segment| segment.first_ordinal);
    let (mut digest, mut ordinal) = (0, 0_u128);
    for segment in segments {
        assert_eq!(segment.first_ordinal, ordinal);
        for &value in segment.values.iter() {
            ordinal += 1;
            digest = mix_digest(digest, value);
        }
    }
    assert_eq!(ordinal, counts as u128);
    assert_eq!(
        Some(counts),
        expected.usize_field("packing_candidate_count")
    );
    assert_eq!(
        expected.field("packing_candidate_digest"),
        Some(format!("{digest:016x}").as_str())
    );
    assert_eq!(
        expected.field("build_variant_count"),
        Some(variants.to_string().as_str())
    );
    assert_eq!(
        expected.usize_field("covered_pattern_count"),
        Some(covered.count_ones() as usize)
    );
    assert_eq!(
        keys.into_iter().collect::<Vec<_>>().as_slice(),
        expected.normalized_solution_keys()
    );

    let mut root = engine(&problem, field, &admission);
    let mut wave = make_waves(&mut root, &admission);
    let cancellation = ExecutionCancellationToken::new();
    let observer = Arc::new(WaveObserver {
        caller: thread::current().id(),
        background_progress: AtomicUsize::new(0),
        cancel_at_progress: Some(cancellation.handle()),
        panic_on_background: false,
    });
    let control = ExecutionControl::new(cancellation.clone()).with_progress_sink(observer.clone());
    assert_eq!(
        wave.advance(&admission, WORKERS, &control).unwrap(),
        (false, WORKERS)
    );
    assert!(matches!(
        wave.advance(&admission, WORKERS, &control),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert!(cancellation.is_cancelled());
    assert_eq!(
        Arc::strong_count(&observer),
        2,
        "cancelled move-outs must also drain"
    );
    assert!(wave.into_completed_slots().is_err());
    drop(root);

    let mut root = engine(&problem, field, &admission);
    let mut wave = make_waves(&mut root, &admission);
    let observer = Arc::new(WaveObserver {
        caller: thread::current().id(),
        background_progress: AtomicUsize::new(0),
        cancel_at_progress: None,
        panic_on_background: true,
    });
    let control = ExecutionControl::default().with_progress_sink(observer.clone());
    assert_eq!(
        wave.advance(&admission, WORKERS, &control).unwrap(),
        (false, WORKERS)
    );
    assert!(matches!(
        wave.advance(&admission, WORKERS, &control),
        Err(WasmExactSearchError::InvalidProblem(
            "extended_pc_family_worker_panicked"
        ))
    ));
    assert!(
        !control.is_cancelled(),
        "worker failure is not caller cancellation"
    );
    assert_eq!(
        Arc::strong_count(&observer),
        2,
        "failed move-outs must also drain"
    );
    assert!(wave.into_completed_slots().is_err());
    drop(root);
    drop(admission);
    assert_exact_composed_dry_run(&problem, field, &expected);
    assert!(admit_budget_bound_search_execution(&problem, WORKERS).is_ok());
    println!("\nclearra_worker_wave_conformance=complete");
}

fn assert_exact_composed_dry_run(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    expected: &CoreExecutionResult,
) {
    let mut owner = DryRunOwner::new(problem, field).unwrap();
    let control = ExecutionControl::default();
    assert_eq!(owner.advance(0, &control).unwrap(), (false, 0));
    assert!(owner.completed().is_none());

    let mut pending = 0;
    let mut multi_unit = false;
    let done = (0..100_000).any(|index| {
        let budget = [1, 3, 2, 1][index % 4];
        let (terminal, used) = owner.advance(budget, &control).unwrap();
        assert!(
            (1..=budget).contains(&used),
            "one shared creation/work/seal debit"
        );
        multi_unit |= used > 1;
        if !terminal {
            pending += 1;
            assert!(owner.completed().is_none(), "no partial terminal authority");
        }
        terminal
    });
    assert!(done && pending > 10 && multi_unit);
    let completed = owner.completed().unwrap();
    let workers = completed.workers;
    assert_eq!(workers.len(), WORKERS);
    assert_eq!(
        expected.field("packing_candidate_digest"),
        Some(format!("{:016x}", completed.digest).as_str())
    );
    assert_eq!(
        expected.usize_field("packing_candidate_count"),
        Some(completed.candidate_count as usize)
    );

    // Keys were transferred one at a time into the canonical cursor under
    // this admission; there is no test-side re-reduction of worker sets.
    assert_eq!(completed.keys, expected.normalized_solution_keys());
    assert_eq!(
        Some(completed.hash),
        expected.field("normalized_solution_set_hash")
    );
    let catalog = owner.catalog();
    let mut covered = PatternBitSet::new(workers[0].engine.covered_patterns.pattern_count());
    let mut variants = 0;
    for worker in workers {
        assert!(Arc::ptr_eq(catalog, &worker.engine.catalog));
        assert!(worker.engine.pc_build_variant_count_complete);
        variants += worker.engine.pc_build_variant_count;
        covered.union_with(&worker.engine.covered_patterns).unwrap();
        assert!(worker.engine.buildable_tilings.is_empty());
        let rows = worker.bounded_rows.as_ref().unwrap();
        assert_eq!((rows.len, rows.retained), (0, 0));
        assert!(rows.head.is_none());
    }
    assert_eq!(
        Some(variants.to_string().as_str()),
        expected.field("build_variant_count")
    );
    assert_eq!(
        Some(covered.count_ones() as usize),
        expected.usize_field("covered_pattern_count")
    );
    assert!(
        admit_budget_bound_search_execution(problem, WORKERS).is_err(),
        "borrowed worker evidence retains the exact parent admission"
    );
    drop(owner);

    let mut owner = DryRunOwner::new(problem, field).unwrap();
    let token = ExecutionCancellationToken::new();
    let control = ExecutionControl::new(token.clone());
    assert!((0..100_000).any(|_| {
        assert_eq!(owner.advance(1, &control).unwrap(), (false, 1));
        owner.canonical_last_row_imported()
    }));
    assert!(owner.completed().is_none(), "last source row is not a seal");
    token.handle().cancel();
    assert!(matches!(
        owner.advance(2, &control),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert!(owner.completed().is_none());
    drop(owner);
    assert!(admit_budget_bound_search_execution(problem, WORKERS).is_ok());
}
