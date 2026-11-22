//! Native Rust PC diagnostics for the complete finite P1P7P7 source.
//!
//! Build with `--release --features parallel,wasm-stage-profiling`
//! and compile-time `CLEARRA_SOURCE_COMMIT`. Full runs require clearra-manage's
//! benchmark-search profile. `--small` is a one-I executable-contract check.
//! The diagnostic deadline starts before source preparation; cancellation does
//! not turn partial work into a complete probability or solution-set result.

use clearra_core_domain::{
    execution_cancellation::{
        ExecutionCancellationToken, ExecutionControl, ExecutionProgress, ProgressSink,
    },
    piece::piece_kind::PieceKind,
    runtime_cpu_capacity::CpuCapacity,
};
use clearra_core_executor::{CoreExecutionResult, WasmCpuSearchBackend};
#[cfg(all(feature = "wasm-stage-profiling", not(target_family = "wasm")))]
use clearra_core_executor::{ExecutorSearchProfileSession, ExecutorSearchProfileStage};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
    WorkerPolicy,
};
use clearra_problem::{
    CountPolicy, PcChanceEvidencePolicy, ProblemCompiler, SearchOutputPolicy, SearchProblem,
};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::{
    pattern_universe::materialized_pattern_universe::UniformCompactPatternSource,
    queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
    QueueObservationPolicy,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, Read},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const FULL_PATTERN_COUNT: usize = 177_811_200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fixture {
    Full6L,
    OneI,
}

impl Fixture {
    const fn name(self) -> &'static str {
        match self {
            Self::Full6L => "empty-6L-P1P7P7-srs-plus",
            Self::OneI => "one-I-preflight-srs-plus",
        }
    }

    const fn height(self) -> u16 {
        match self {
            Self::Full6L => 6,
            Self::OneI => 1,
        }
    }

    const fn draws(self) -> usize {
        match self {
            Self::Full6L => 15,
            Self::OneI => 1,
        }
    }

    const fn patterns(self) -> usize {
        match self {
            Self::Full6L => FULL_PATTERN_COUNT,
            Self::OneI => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Objective {
    Unique,
    Chance,
}

impl Objective {
    const fn name(self) -> &'static str {
        match self {
            Self::Unique => "unique",
            Self::Chance => "chance",
        }
    }
}

struct Options {
    run: bool,
    fixture: Fixture,
    objective: Objective,
    profile: bool,
    cancel_after_seconds: Option<u64>,
}

fn main() {
    // Match the public native CLI stack while avoiding the App/UI dependency.
    let status = thread::Builder::new()
        .name("full6l-benchmark".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(main_on_product_stack)
        .expect("benchmark product thread")
        .join()
        .expect("benchmark product thread completed");
    std::process::exit(status);
}

fn main_on_product_stack() -> i32 {
    let started = Instant::now();
    let mut report = json!({
        "schema": "clearra.full6l-bench.v1", "complete": false,
        "execution_surface": "native-rust-pc-core", "phase": "arguments",
    });
    let outcome = options().and_then(|options| run(&options, &mut report));
    report["total_seconds"] = json!(started.elapsed().as_secs_f64());
    report["process_memory"] = process_memory();
    if let Err(error) = &outcome {
        report["status"] = json!("error-or-incomplete");
        report["error"] = json!(error);
    }
    println!("{report}");
    i32::from(outcome.is_err())
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let run = match args.next().as_deref() {
        Some("compile") => false,
        Some("run") => true,
        _ => return Err(usage()),
    };
    let mut options = Options {
        run,
        fixture: Fixture::Full6L,
        objective: Objective::Unique,
        profile: false,
        cancel_after_seconds: None,
    };
    let mut selected_objective = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--small" if options.fixture == Fixture::Full6L => options.fixture = Fixture::OneI,
            "--objective" if !selected_objective => {
                options.objective = match args.next().as_deref() {
                    Some("unique") => Objective::Unique,
                    Some("chance") => Objective::Chance,
                    _ => return Err(usage()),
                };
                selected_objective = true;
            }
            "--profile" if !options.profile => options.profile = true,
            "--cancel-after-seconds" if options.cancel_after_seconds.is_none() => {
                let seconds = args
                    .next()
                    .ok_or_else(usage)?
                    .parse()
                    .map_err(|_| usage())?;
                if seconds == 0 {
                    return Err(usage());
                }
                options.cancel_after_seconds = Some(seconds);
            }
            _ => return Err(usage()),
        }
    }
    if !run && (options.profile || options.cancel_after_seconds.is_some()) {
        return Err("profiling and the diagnostic deadline require run".into());
    }
    Ok(options)
}

fn usage() -> String {
    "usage: full6l_bench compile|run [--small] [--objective unique|chance] [--profile] [--cancel-after-seconds N]".into()
}

fn run(options: &Options, report: &mut Value) -> Result<(), String> {
    report["fixture"] = json!(options.fixture.name());
    report["mode"] = json!(if options.run { "run" } else { "compile" });
    report["requested_objective"] = json!(options.objective.name());
    report["diagnostic_only"] = json!(options.profile || options.cancel_after_seconds.is_some());
    report["diagnostic_deadline_seconds"] = json!(options.cancel_after_seconds);
    report["deadline_scope"] = json!("source preparation, compilation, execution and result recording; cooperative cancellation, supervisor owns hard timeout");
    report["supervision"] = json!({
        "supervised": std::env::var("CLEARRA_RUNTIME_SUPERVISED").ok(),
        "profile": std::env::var("CLEARRA_RUNTIME_PROFILE").ok(),
        "run_id": std::env::var("CLEARRA_RUNTIME_RUN_ID").ok(),
    });
    if options.run {
        if !cfg!(all(feature = "parallel", not(target_arch = "wasm32"))) {
            return Err(
                "run requires native --features parallel; no serial substitute was used".into(),
            );
        }
        if options.fixture == Fixture::Full6L
            && (std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
                || std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search"))
        {
            return Err("Full6L run requires the supervised benchmark-search profile".into());
        }
    }
    let cancellation = ExecutionCancellationToken::new();
    let timer = options.cancel_after_seconds.map(|seconds| {
        let (done, wait) = mpsc::channel::<()>();
        let handle = cancellation.handle();
        let timer = thread::spawn(move || {
            if matches!(
                wait.recv_timeout(Duration::from_secs(seconds)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                eprintln!("full6l_bench diagnostic_cancel_after_seconds={seconds}");
                handle.cancel();
            }
        });
        (done, timer)
    });
    let control = ExecutionControl::new(cancellation.clone())
        .with_progress_sink(Arc::new(DiagnosticProgress::new()));
    let mut phases = PhaseClock::new();
    let outcome = (|| {
        let profile = start_profile(options.profile)?;
        let result = execute(options, report, &control, &mut phases);
        report["profile"] = finish_profile(profile);
        result
    })();
    phases.finish(report);
    if let Some((done, timer)) = timer {
        let _ = done.send(());
        timer.join().map_err(|_| "diagnostic timer panicked")?;
    }
    report["cancellation_requested"] = json!(cancellation.is_cancelled());
    outcome
}

fn execute(
    options: &Options,
    report: &mut Value,
    control: &ExecutionControl,
    phases: &mut PhaseClock,
) -> Result<(), String> {
    phases.enter(report, "build-identity");
    report["build"] = json!({
        "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
        "binary_sha256": binary_sha256()?,
        "parallel_feature": cfg!(feature = "parallel"),
        "stage_profiling_feature": cfg!(feature = "wasm-stage-profiling"),
        "local_search_ab_feature": cfg!(feature = "local-search-ab"),
        "placement_dependency_feature": cfg!(feature = "placement-dependency-pc"),
    });
    report["local_controls"] = json!({
        "placement_dependency": std::env::var("CLEARRA_PLACEMENT_DEPENDENCY").ok(),
        "structural_order": std::env::var("CLEARRA_STRUCTURAL_ORDER").ok(),
        "strict_prefix": std::env::var("CLEARRA_STRICT_PREFIX").ok(),
        "standard_bag_absorb": std::env::var("CLEARRA_STANDARD_BAG_ABSORB").ok(),
        "trace_memory_admission": std::env::var("CLEARRA_TRACE_MEMORY_ADMISSION").ok(),
    });
    let capacity = CpuCapacity::current();
    report["cpu"] = json!({
        "policy": "auto-all-logical", "required_workers": capacity.hard_limit(),
        "hard_limit": capacity.hard_limit(), "recommended_parallelism": capacity.recommended_parallelism(),
        "affinity_limit": capacity.affinity_limit(), "expected_vcpus": capacity.expected_vcpus(),
    });
    phases.enter(report, "source-preparation");
    let query = query(options.fixture, capacity.hard_limit())?;
    if control.is_cancelled() {
        return Err("diagnostic cancellation before compilation".into());
    }
    phases.enter(report, "problem-compilation");
    let problem = compile(&query, options.objective)?;
    phases.enter(report, "compiled-contract");
    validate_compiled(&problem, options.fixture, options.objective, report)?;
    // Preserve the actual input identity even if subsequent preparation is killed.
    eprintln!(
        "full6l_bench preflight={}",
        json!({
            "source": &report["source"], "compiled_request": &report["compiled_request"],
            "cpu": &report["cpu"], "build": &report["build"],
            "local_controls": &report["local_controls"],
        })
    );
    if !options.run {
        report["status"] = json!("compiled-full-source");
        return Ok(());
    }
    if control.is_cancelled() {
        return Err("diagnostic cancellation before native execution".into());
    }
    phases.enter(report, "native-execution-including-preparation");
    let result = WasmCpuSearchBackend::execute_with_control(&problem, control)
        .map_err(|error| format!("native Rust PC: {error:?}"))?;
    phases.enter(report, "result-recording");
    record_result(&result, &problem, options, capacity.hard_limit(), report)?;
    report["status"] = json!("completed");
    Ok(())
}

fn query(fixture: Fixture, hardware_limit: usize) -> Result<PcScenarioQuery, String> {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_worker_policy(WorkerPolicy::Auto)
        .with_worker_hardware_limit(hardware_limit)
        .with_use_all_logical_processors(true)
        .with_cpu_warmup(false)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false)
        .with_max_patterns(0);
    let (mask, queue, hold) = match fixture {
        Fixture::Full6L => (
            0,
            PcQueueInput::pattern_expression(
                QueuePatternExpression::parse("P1P7P7", 0)
                    .map_err(|error| format!("P1P7P7 expression: {error:?}"))?,
            ),
            true,
        ),
        Fixture::OneI => (
            0x3f0,
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            false,
        ),
    };
    Ok(PcScenarioQuery::new(
        PcScenarioBoard::standard_10(fixture.height(), mask),
        queue,
        PieceWindow::new(fixture.draws()),
    )
    .with_rule(srs_plus())
    .with_exact_pieces(Some(fixture.draws()))
    .with_supply_window_size(SupplyWindowSize::new(fixture.draws()))
    .with_min_remaining_queue(0)
    .with_hold_piece(None)
    .with_allow_hold(hold)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_retained_trace_limit(1)
    .with_execution_policy(policy))
}

fn compile(query: &PcScenarioQuery, objective: Objective) -> Result<SearchProblem, String> {
    match objective {
        Objective::Unique => ProblemCompiler::compile_scenario_pc(query),
        Objective::Chance => ProblemCompiler::compile_scenario_percent(query)
            .map(SearchProblem::with_pc_chance_probability_v2_evidence),
    }
    .map_err(|error| format!("problem compilation: {error:?}"))
}

fn validate_compiled(
    problem: &SearchProblem,
    fixture: Fixture,
    objective: Objective,
    report: &mut Value,
) -> Result<(), String> {
    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .ok_or("compiled source has no pattern universe")?;
    let supply = problem.supply();
    let hold = problem.initial_hold();
    let factorized = match universe.uniform_compact_source() {
        Some(UniformCompactPatternSource::FactorizedExpression(shape)) => Some(shape),
        _ => None,
    };
    let atoms = factorized.map(|shape| {
        shape.atoms().map(|atom| json!({
        "choices": atom.choices().iter().map(|piece| piece.as_ascii()).collect::<String>(),
        "draw_count": atom.draw_count(), "variant_count": atom.variant_count(),
    })).collect::<Vec<_>>()
    });
    report["source"] = json!({
        "pattern": if fixture == Fixture::Full6L { "P1P7P7" } else { "I" },
        "problem_id": problem.problem_id().as_str(),
        "piece_source_id": source.id().get(), "source_kind": source.kind().as_str(),
        "pattern_universe_id": universe.pattern_universe_id().get(),
        "pattern_weight_model_id": universe.pattern_weight_model_id().get(),
        "queue_mode": supply.queue_mode(), "structure": format!("{:?}", universe.structure()),
        "source_draws": supply.source_sequence_length(), "original_pattern_count": universe.pattern_count(),
        "total_possible_pattern_count": universe.total_possible_pattern_count(),
        "source_complete": source.complete() && universe.complete(),
        "truncation_reason": universe.truncation_reason().map(|reason| reason.as_str()),
        "factorized": factorized.is_some(), "atoms": atoms,
        "full_sequence_len": factorized.map(|shape| shape.full_sequence_len()),
        "visible_sequence_len": factorized.map(|shape| shape.visible_sequence_len()),
        "first_sequence": universe.sequence_at(0).iter().map(|piece| piece.as_ascii()).collect::<String>(),
        "last_sequence": universe.sequence_at(universe.pattern_count() - 1).iter().map(|piece| piece.as_ascii()).collect::<String>(),
        "retained_capacity_bytes": source.checked_retained_capacity_bytes(),
        "initial_hold": {
            "source_id": hold.piece_source_id().get(), "source_kind": hold.source_kind().as_str(),
            "cursor": hold.cursor(), "held": hold.hold_piece().map(|piece| piece.as_ascii()),
            "empty": hold.hold_empty(), "policy": format!("{:?}", hold.hold_policy()),
            "bag_epoch": hold.bag_epoch(), "bag_remainder_key": hold.bag_remainder_key(),
            "observation": format!("{:?}", hold.observation()), "provenance": format!("{:?}", hold.provenance()),
        },
        "hold_enabled": supply.hold_enabled(),
        "projects_unplaced_lookahead": supply.projects_unplaced_lookahead(),
        "projects_standard_bag_lookahead": supply.projects_standard_bag_lookahead(),
        "supply_window_resolution": supply.supply_window_resolution(),
        "queue_knowledge": format!("{:?}", problem.queue_observation_policy()),
    });
    report["compiled_request"] = json!({
        "width": 10, "height": problem.visible_height(), "geometry_height": problem.compact_pc_geometry_height(),
        "initial_board_words": problem.initial_board().occupied_words(), "exact_locks": problem.exact_pieces(),
        "goal": problem.search_goal().as_str(), "rule": format!("{:?}", problem.rule_profile_value()),
        "kick_profile": format!("{:?}", problem.kick_profile()),
        "count_policy": format!("{:?}", problem.count_policy()), "objective": format!("{:?}", problem.objective()),
        "output_policy": problem.output_policy().as_str(),
        "solution_probability_policy": format!("{:?}", problem.solution_probability_policy()),
        "pc_chance_evidence_policy": format!("{:?}", problem.pc_chance_evidence_policy()),
        "workers": problem.backend_request().workers(), "worker_policy": problem.backend_request().worker_policy().as_str(),
        "max_patterns": problem.backend_request().max_patterns(),
        "max_nodes": problem.backend_request().max_nodes(),
        "max_candidates": problem.backend_request().max_candidates(),
        "max_frontier_states": problem.backend_request().max_frontier_states(),
        "max_memory_mib": problem.backend_request().max_memory_mib(),
        "resource_budget": format!("{:?}", problem.resource_budget()),
    });
    let full = fixture == Fixture::Full6L;
    let expected_output = if objective == Objective::Unique {
        SearchOutputPolicy::Trace
    } else {
        SearchOutputPolicy::CoverageSummary
    };
    let expected_evidence = match objective {
        Objective::Chance => PcChanceEvidencePolicy::PcProbabilityV2,
        Objective::Unique => PcChanceEvidencePolicy::Disabled,
    };
    if universe.pattern_count() != fixture.patterns()
        || universe.total_possible_pattern_count() != fixture.patterns() as u128
        || !source.complete()
        || !universe.complete()
        || universe.truncation_reason().is_some()
        || supply.source_sequence_length() != fixture.draws()
        || universe.sequence_len_at(0) != fixture.draws()
        || universe.sequence_len_at(fixture.patterns() - 1) != fixture.draws()
        || problem.exact_pieces() != Some(fixture.draws())
        || problem.visible_height() != fixture.height()
        || problem.compact_pc_geometry_height() != fixture.height()
        || problem.initial_board().width() != 10
        || problem.initial_board().occupied_words() != [if full { 0 } else { 0x3f0 }, 0, 0, 0]
        || problem.search_goal().as_str() != "clear-to-empty"
        || problem.count_policy() != CountPolicy::CountUnique
        || problem.objective() != ObjectivePolicy::unique()
        || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
        || supply.hold_enabled() != full
        || supply.hold_piece().is_some()
        || hold.cursor() != 0
        || supply.projects_unplaced_lookahead() != full
        || supply.projects_standard_bag_lookahead()
        || problem.output_policy() != expected_output
        || problem.pc_chance_evidence_policy() != expected_evidence
        || problem.backend_request().max_patterns() != 0
        || problem.backend_request().max_nodes() != 0
        || problem.backend_request().max_candidates() != 0
        || problem.backend_request().max_frontier_states() != 0
    {
        return Err(
            "compiled PC/source/EOF contract changed; no replacement fixture was used".into(),
        );
    }
    if full
        && !factorized.is_some_and(|shape| {
            shape.pattern_count() == FULL_PATTERN_COUNT
                && shape.full_sequence_len() == 15
                && shape.visible_sequence_len() == 15
                && shape
                    .atoms()
                    .map(|atom| (atom.draw_count(), atom.variant_count()))
                    .eq([(1, 7), (7, 5040), (7, 5040)])
                && shape
                    .atoms()
                    .all(|atom| atom.choices() == PieceKind::STANDARD_TETROMINOES)
        })
    {
        return Err("P1P7P7 lost its complete ranked 1+7+7 factorized source".into());
    }
    report["compiled_contract_valid"] = json!(true);
    Ok(())
}

fn record_result(
    result: &CoreExecutionResult,
    problem: &SearchProblem,
    options: &Options,
    expected_workers: usize,
    report: &mut Value,
) -> Result<(), String> {
    report["summary_fields"] = json!(result.summary_fields());
    let words = result.coverage_pattern_words();
    let mut coverage_hash = Sha256::new();
    for word in words {
        coverage_hash.update(word.to_le_bytes());
    }
    let covered = words
        .iter()
        .map(|word| u64::from(word.count_ones()))
        .sum::<u64>();
    report["coverage_bitmap"] = json!({
        "word_count": words.len(), "encoding": "little-endian-u64-original-pattern-order",
        "sha256": (!words.is_empty()).then(|| format!("{:x}", coverage_hash.finalize())),
        "popcount": covered, "all_original_queues_successful": covered == options.fixture.patterns() as u64,
    });
    let keys = result.normalized_solution_keys();
    let mut key_hash = Sha256::new();
    for key in keys {
        key_hash.update((key.len() as u64).to_le_bytes());
        key_hash.update(key.as_bytes());
    }
    let identities = result.normalized_solution_identities();
    let mut identity_hash = Sha256::new();
    for identity in identities {
        identity_hash.update(identity.initial_board_mask().to_le_bytes());
        identity_hash.update((identity.placement_count() as u64).to_le_bytes());
        identity_hash.update(identity.packed_piece_codes().to_le_bytes());
        for mask in identity.placement_masks() {
            identity_hash.update(mask.to_le_bytes());
        }
    }
    let solution_count = result.usize_field("normalized_unique_solution_count");
    report["solutions"] = json!({
        "normalized_unique_solution_count": solution_count,
        "normalized_solution_set_hash": result.field("normalized_solution_set_hash"),
        "key_count": keys.len(), "key_encoding": "ordered-u64-le-length-prefixed-utf8",
        "keys_sha256": (!keys.is_empty()).then(|| format!("{:x}", key_hash.finalize())),
        "identity_count": identities.len(),
        "identity_encoding": "ordered-u64-le: initial-mask,placement-count,packed-piece-codes,placement-masks",
        "identities_sha256": (!identities.is_empty()).then(|| format!("{:x}", identity_hash.finalize())),
        "any_solution_found": covered != 0,
        "representative_path": result.path_steps().iter().map(|step| json!({
            "piece": format!("{:?}", step.piece()), "rotation": step.rotation(),
            "x": step.x(), "y": step.y(), "hold": step.hold(), "cleared_lines": step.cleared_lines(),
        })).collect::<Vec<_>>(),
    });
    let evidence = result.pc_chance_coverage_evidence();
    report["chance_evidence"] = evidence.map_or(Value::Null, |evidence| json!({
        "complete": evidence.complete(), "pattern_count": evidence.pattern_count(),
        "row_count": evidence.row_count(), "matches_executed_problem": evidence.problem().matches_search_problem(problem),
        "piece_source_id": evidence.piece_source_id(), "pattern_universe_id": evidence.pattern_universe_id().get(),
        "pattern_weight_model_id": evidence.pattern_weight_model_id().get(),
    }));
    report["result_retained_bytes"] = json!(result.checked_resource_retained_bytes());
    let count_complete = result.unique_field("count_complete") == Some("true");
    let probability_complete = result.unique_field("probability_complete") == Some("true");
    report["count_complete"] = json!(count_complete);
    report["probability_complete"] = json!(probability_complete);
    let evidence_complete = match options.objective {
        Objective::Unique => true,
        Objective::Chance => evidence.is_some_and(|evidence| {
            evidence.complete()
                && evidence.pattern_count() == options.fixture.patterns()
                && evidence.problem().matches_search_problem(problem)
        }),
    };
    let complete = probability_complete
        && evidence_complete
        && (options.objective != Objective::Unique || count_complete)
        && result.unique_field("resource_truncated") == Some("false");
    if !complete {
        return Err(
            "native PC returned incomplete objective evidence; see preserved product fields".into(),
        );
    }
    if result.usize_field("coverage_pattern_count") != Some(options.fixture.patterns())
        || result.usize_field("source_sequence_length") != Some(options.fixture.draws())
        || words.len() != options.fixture.patterns().div_ceil(64)
        || result.usize_field("covered_pattern_count") != Some(covered as usize)
        || result.bool_field("backend_fallback_used") == Some(true)
        || (options.fixture == Fixture::Full6L
            && (result.usize_field("workers_used") != Some(expected_workers)
                || result.bool_field("cpu_parallel_execution") != Some(expected_workers > 1)))
    {
        return Err(
            "completed result violates the original source/coverage/Auto worker contract".into(),
        );
    }
    report["complete"] = json!(true);
    Ok(())
}

struct PhaseClock {
    current: Option<(&'static str, Instant)>,
    rows: Vec<Value>,
}

impl PhaseClock {
    fn new() -> Self {
        Self {
            current: None,
            rows: Vec::new(),
        }
    }

    fn close(&mut self) {
        if let Some((name, started)) = self.current.take() {
            let row = json!({ "name": name, "seconds": started.elapsed().as_secs_f64(), "process_memory": process_memory() });
            eprintln!("full6l_bench phase_completed={row}");
            self.rows.push(row);
        }
    }

    fn enter(&mut self, report: &mut Value, name: &'static str) {
        self.close();
        self.current = Some((name, Instant::now()));
        report["phase"] = json!(name);
        eprintln!("full6l_bench phase={name}");
    }

    fn finish(&mut self, report: &mut Value) {
        self.close();
        report["phase_wall_times"] = json!(self.rows);
    }
}

struct DiagnosticProgress {
    last: Mutex<(&'static str, Instant)>,
    started: Instant,
}
impl DiagnosticProgress {
    fn new() -> Self {
        Self {
            last: Mutex::new(("", Instant::now())),
            started: Instant::now(),
        }
    }
}
impl ProgressSink for DiagnosticProgress {
    fn report(&self, progress: ExecutionProgress) {
        if let Ok(mut last) = self.last.lock() {
            if last.0 != progress.stage || last.1.elapsed() >= Duration::from_secs(1) {
                eprintln!(
                    "full6l_bench product_progress stage={} completed={} total={:?}",
                    progress.stage, progress.completed, progress.total
                );
                #[allow(unused_mut)]
                let mut checkpoint = json!({
                    "stage": progress.stage,
                    "completed": progress.completed,
                    "total": progress.total,
                    "seconds": self.started.elapsed().as_secs_f64(),
                    "complete": false,
                    "process_memory": process_memory(),
                });
                #[cfg(all(feature = "wasm-stage-profiling", not(target_family = "wasm")))]
                if let Some(stages) = ExecutorSearchProfileSession::snapshot_current_thread() {
                    checkpoint["current_thread_profile"] = json!(stages
                        .into_iter()
                        .filter(|stage| stage.invocation_count != 0 || stage.work_item_count != 0)
                        .map(|stage| json!({
                            "name": stage.name,
                            "duration_ns": stage.duration_ns,
                            "invocation_count": stage.invocation_count,
                            "work_item_count": stage.work_item_count,
                        }))
                        .collect::<Vec<_>>());
                }
                eprintln!("full6l_bench checkpoint={checkpoint}");
                *last = (progress.stage, Instant::now());
            }
        }
    }
}

#[cfg(all(feature = "wasm-stage-profiling", not(target_family = "wasm")))]
type Profile = Option<ExecutorSearchProfileSession>;
#[cfg(not(all(feature = "wasm-stage-profiling", not(target_family = "wasm"))))]
type Profile = ();

fn start_profile(enabled: bool) -> Result<Profile, String> {
    #[cfg(all(feature = "wasm-stage-profiling", not(target_family = "wasm")))]
    {
        enabled
            .then(ExecutorSearchProfileSession::start)
            .transpose()
            .map_err(|error| format!("stage profile: {error:?}"))
    }
    #[cfg(not(all(feature = "wasm-stage-profiling", not(target_family = "wasm"))))]
    {
        if enabled {
            Err("--profile requires native --features wasm-stage-profiling".into())
        } else {
            Ok(())
        }
    }
}

fn finish_profile(profile: Profile) -> Value {
    #[cfg(all(feature = "wasm-stage-profiling", not(target_family = "wasm")))]
    {
        profile.map_or(Value::Null, |profile| {
            let (caller, workers) = profile.finish_partitioned();
            let rows = |stages: Vec<ExecutorSearchProfileStage>| stages.into_iter().filter(|stage| stage.invocation_count != 0 || stage.work_item_count != 0).map(|stage| json!({
                "name": stage.name, "duration_ns": stage.duration_ns,
                "invocation_count": stage.invocation_count, "work_item_count": stage.work_item_count,
            })).collect::<Vec<_>>();
            json!({ "timing_contract": "accumulated overlapping/nested spans; not exclusive wall time", "geometry_work_contract": "family substage work counts are exact owner totals; sampled duration and invocation counts remain weighted estimates", "caller": rows(caller), "pooled_workers": rows(workers) })
        })
    }
    #[cfg(not(all(feature = "wasm-stage-profiling", not(target_family = "wasm"))))]
    {
        let () = profile;
        Value::Null
    }
}

fn binary_sha256() -> Result<String, String> {
    let mut file = BufReader::new(
        File::open(std::env::current_exe().map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?,
    );
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65_536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn process_memory() -> Value {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let bytes = |label: &str| -> Option<u64> {
            let mut fields = status
                .lines()
                .find_map(|line| line.strip_prefix(label))?
                .split_ascii_whitespace();
            let kib = fields.next()?.parse::<u64>().ok()?;
            (fields.next() == Some("kB") && fields.next().is_none())
                .then_some(kib)?
                .checked_mul(1024)
        };
        json!({ "rss_bytes": bytes("VmRSS:"), "rss_high_water_bytes": bytes("VmHWM:"), "contract": "Linux process kernel RSS; supervisor separately samples owned process tree" })
    }
    #[cfg(not(target_os = "linux"))]
    {
        Value::Null
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full6l_retains_all_original_draws_and_factorized_ordinal_authority() {
        let query = query(Fixture::Full6L, 2).unwrap();
        let problem = compile(&query, Objective::Unique).unwrap();
        validate_compiled(&problem, Fixture::Full6L, Objective::Unique, &mut json!({})).unwrap();
        assert_eq!(
            problem.backend_request().worker_policy(),
            WorkerPolicy::Auto
        );
        assert_eq!(problem.initial_board().occupied_words(), [0; 4]);
        assert_eq!(problem.initial_hold().bag_remainder_key(), 0);
    }

    #[test]
    fn chance_selects_existing_typed_probability_evidence_without_changing_source() {
        let query = query(Fixture::Full6L, 2).unwrap();
        let unique = compile(&query, Objective::Unique).unwrap();
        let chance = compile(&query, Objective::Chance).unwrap();
        validate_compiled(&chance, Fixture::Full6L, Objective::Chance, &mut json!({})).unwrap();
        assert_eq!(unique.piece_source(), chance.piece_source());
        assert_eq!(unique.initial_hold(), chance.initial_hold());
    }
}
