//! Native Full 6L global probability, including the live App certificate gate.
//!
//! Build with compile-time CLEARRA_SOURCE_COMMIT, --release --no-default-features
//! and --features parallel,wasm-stage-profiling. Product measurements keep the
//! local-search-ab feature disabled. Full runs require
//! clearra-manage's benchmark-search profile; --small verifies the same public
//! App entry point on a one-I fixture. Deadlines are diagnostic cancellation,
//! never a source/candidate sampling limit or probability-completion authority.

use clearra_app::{PcChanceIngressOrigin, PcGlobalProbabilityV1Result};
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
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
    WorkerPolicy,
};
use clearra_problem::{PcChanceEvidencePolicy, ProblemCompiler, SearchOutputPolicy, SearchProblem};
use clearra_rules::{kicks::KickTableProfileId, profile::builtin_rules::srs_plus};
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum Fixture {
    Full,
    Small,
}

impl Fixture {
    const fn pattern_count(self) -> usize {
        if matches!(self, Self::Full) {
            177_811_200
        } else {
            1
        }
    }
    const fn height(self) -> u16 {
        if matches!(self, Self::Full) {
            6
        } else {
            1
        }
    }
    const fn draws(self) -> usize {
        if matches!(self, Self::Full) {
            15
        } else {
            1
        }
    }
    const fn name(self) -> &'static str {
        if matches!(self, Self::Full) {
            "empty-6L-P1P7P7-srs-plus"
        } else {
            "one-I-preflight-srs-plus"
        }
    }
}

struct Options {
    run: bool,
    fixture: Fixture,
    profile: bool,
    cancel_after_seconds: Option<u64>,
}

fn main() {
    let status = thread::Builder::new()
        .name("full6l-global-app-benchmark".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(main_on_product_stack)
        .expect("product-stack benchmark thread")
        .join()
        .expect("benchmark thread completed");
    std::process::exit(status);
}

fn main_on_product_stack() -> i32 {
    let started = Instant::now();
    let mut report = json!({
        "schema": "clearra.full6l-global-app-bench.v1",
        "execution_surface": "native-rust-core-and-live-app-global-validator",
        "complete": false, "phase": "arguments",
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
    let usage = || {
        "usage: full6l_global_bench compile|run [--small] [--profile] [--cancel-after-seconds N]"
            .to_owned()
    };
    let mut args = std::env::args().skip(1);
    let run = match args.next().as_deref() {
        Some("run") => true,
        Some("compile") => false,
        _ => return Err(usage()),
    };
    let mut options = Options {
        run,
        fixture: Fixture::Full,
        profile: false,
        cancel_after_seconds: None,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--small" if options.fixture == Fixture::Full => options.fixture = Fixture::Small,
            "--profile" if !options.profile => options.profile = true,
            "--cancel-after-seconds" if options.cancel_after_seconds.is_none() => {
                let seconds: u64 = args
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
        return Err("profiling and diagnostic cancellation require run".into());
    }
    Ok(options)
}

fn run(options: &Options, report: &mut Value) -> Result<(), String> {
    report["fixture"] = json!(options.fixture.name());
    report["mode"] = json!(if options.run { "run" } else { "compile" });
    report["requested_objective"] = json!("global-probability");
    report["diagnostic_only"] = json!(options.profile || options.cancel_after_seconds.is_some());
    report["diagnostic_deadline_seconds"] = json!(options.cancel_after_seconds);
    report["deadline_scope"] = json!("starts before source preparation; cooperative checks before compilation, execution and App validation; manager owns the hard timeout");
    report["supervision"] = json!({
        "supervised": std::env::var("CLEARRA_RUNTIME_SUPERVISED").ok(),
        "profile": std::env::var("CLEARRA_RUNTIME_PROFILE").ok(),
        "run_id": std::env::var("CLEARRA_RUNTIME_RUN_ID").ok(),
    });
    if options.run && !cfg!(all(feature = "parallel", not(target_arch = "wasm32"))) {
        return Err(
            "native run requires the parallel feature; no serial substitute was selected".into(),
        );
    }
    if options.run
        && options.fixture == Fixture::Full
        && (std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
            || std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search"))
    {
        return Err("Full6L run requires supervised benchmark-search".into());
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
                eprintln!("full6l_global_bench diagnostic_cancel_after_seconds={seconds}");
                handle.cancel();
            }
        });
        (done, timer)
    });
    let control = ExecutionControl::new(cancellation.clone()).with_progress_sink(Arc::new(
        DiagnosticProgress(Mutex::new(("", Instant::now()))),
    ));
    let outcome = (|| {
        let profile = start_profile(options.profile)?;
        let outcome = execute(options, report, &control);
        report["profile"] = finish_profile(profile);
        outcome
    })();
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
) -> Result<(), String> {
    phase(report, "build-identity");
    report["build"] = json!({
        "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
        "binary_sha256": binary_sha256()?,
        "parallel_feature": cfg!(feature = "parallel"),
        "stage_profiling_feature": cfg!(feature = "wasm-stage-profiling"),
        "local_search_ab_feature": cfg!(feature = "local-search-ab"),
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
    phase(report, "source-preparation");
    let started = Instant::now();
    let query = query(options.fixture, capacity.hard_limit())?;
    report["source_preparation_seconds"] = json!(started.elapsed().as_secs_f64());
    check_cancelled(control, "before compilation")?;
    phase(report, "problem-compilation");
    let started = Instant::now();
    let problem = ProblemCompiler::compile_scenario_percent(&query)
        .map_err(|error| format!("compile original PC source: {error:?}"))?
        .with_pc_global_probability_v1_evidence();
    report["problem_compilation_seconds"] = json!(started.elapsed().as_secs_f64());
    validate_compiled(&problem, options.fixture, report)?;
    eprintln!(
        "full6l_global_bench preflight={}",
        json!({
            "build": &report["build"], "source": &report["source"],
            "compiled_request": &report["compiled_request"], "cpu": &report["cpu"],
            "local_controls": &report["local_controls"],
        })
    );
    if !options.run {
        report["status"] = json!("compiled-full-source");
        return Ok(());
    }
    check_cancelled(control, "before native execution")?;
    phase(report, "native-execution-including-preparation");
    let started = Instant::now();
    let result = WasmCpuSearchBackend::execute_with_control(&problem, control);
    report["native_execution_seconds"] = json!(started.elapsed().as_secs_f64());
    let result = result.map_err(|error| format!("native Rust PC: {error:?}"))?;
    phase(report, "core-result-recording");
    record_core(&result, report);
    check_cancelled(control, "before App validation")?;
    phase(report, "live-app-validation");
    let started = Instant::now();
    let typed = PcGlobalProbabilityV1Result::from_scenario_execution(
        &query,
        PcChanceIngressOrigin::CanonicalPcChance,
        &result,
    );
    report["app_validation_seconds"] = json!(started.elapsed().as_secs_f64());
    let typed = typed.map_err(|error| format!("live App certificate validation: {error}"))?;
    if typed.pattern_count() != options.fixture.pattern_count()
        || typed.total_pattern_count() != options.fixture.pattern_count() as u128
        || typed.coverage_pattern_words() != result.coverage_pattern_words()
        || result.usize_field("source_sequence_length") != Some(options.fixture.draws())
        || result.bool_field("backend_fallback_used") == Some(true)
        || (options.fixture == Fixture::Full
            && (result.usize_field("workers_used") != Some(capacity.hard_limit())
                || result.bool_field("cpu_parallel_execution") != Some(capacity.hard_limit() > 1)))
    {
        return Err(
            "completed global result changed original source or required Auto worker contract"
                .into(),
        );
    }
    report["typed_report"] = json!({
        "contract_id": typed.contract_id(), "validated": true,
        "problem_id": typed.problem_id(), "piece_source_id": typed.piece_source_id(),
        "pattern_universe_id": typed.pattern_universe_id(), "pattern_weight_model_id": typed.pattern_weight_model_id(),
        "pattern_count": typed.pattern_count(), "total_pattern_count": typed.total_pattern_count(),
        "covered_pattern_count": typed.covered_pattern_count(),
        "completion_basis": typed.completion_basis().as_str(),
        "enumeration_complete": typed.enumeration_complete(), "count_complete": typed.count_complete(),
        "probability_complete": typed.probability_complete(),
        "weighted_probability": typed.weighted_probability(), "weighted_probability_bits": typed.weighted_probability_bits(),
        "solution_count": typed.solution_count(), "normalized_solution_set_hash": typed.normalized_solution_set_hash(),
    });
    report["complete"] = json!(true);
    report["status"] = json!("completed");
    phase(report, "completed");
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
        .with_max_nodes(0)
        .with_max_frontier_states(0)
        .with_max_candidates(0)
        .with_max_patterns(0)
        .with_max_memory_mib(None);
    let full = fixture == Fixture::Full;
    let queue = if full {
        PcQueueInput::pattern_expression(
            QueuePatternExpression::parse("P1P7P7", 0)
                .map_err(|error| format!("P1P7P7: {error:?}"))?,
        )
    } else {
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I]))
    };
    Ok(PcScenarioQuery::new(
        PcScenarioBoard::standard_10(fixture.height(), if full { 0 } else { 0x3f0 }),
        queue,
        PieceWindow::new(fixture.draws()),
    )
    .with_rule(srs_plus())
    .with_exact_pieces(Some(fixture.draws()))
    .with_supply_window_size(SupplyWindowSize::new(fixture.draws()))
    .with_min_remaining_queue(0)
    .with_hold_piece(None)
    .with_allow_hold(full)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_retained_trace_limit(1)
    .with_execution_policy(policy))
}

fn validate_compiled(
    problem: &SearchProblem,
    fixture: Fixture,
    report: &mut Value,
) -> Result<(), String> {
    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .ok_or("missing original source universe")?;
    let supply = problem.supply();
    let hold = problem.initial_hold();
    let full = fixture == Fixture::Full;
    let factorized = match universe.uniform_compact_source() {
        Some(UniformCompactPatternSource::FactorizedExpression(shape)) => Some(shape),
        _ => None,
    };
    report["source"] = json!({
        "pattern": if full { "P1P7P7" } else { "I" }, "piece_source_id": source.id().get(),
        "pattern_universe_id": universe.pattern_universe_id().get(), "pattern_weight_model_id": universe.pattern_weight_model_id().get(),
        "source_draws": supply.source_sequence_length(), "original_pattern_count": universe.pattern_count(),
        "total_possible_pattern_count": universe.total_possible_pattern_count(), "source_complete": source.complete() && universe.complete(),
        "materialized_probability_mass_bits": universe.materialized_probability_mass().get().to_bits(),
        "total_weight_bits": universe.weights().total_weight().get().to_bits(),
        "truncation_reason": universe.truncation_reason().map(|reason| reason.as_str()),
        "factorized": factorized.is_some(), "atoms": factorized.map(|shape| shape.atoms().map(|atom| json!({
            "draw_count": atom.draw_count(), "variant_count": atom.variant_count(),
            "choices": atom.choices().iter().map(|piece| piece.as_ascii()).collect::<String>(),
        })).collect::<Vec<_>>()),
        "full_sequence_len": factorized.map(|shape| shape.full_sequence_len()),
        "visible_sequence_len": factorized.map(|shape| shape.visible_sequence_len()),
        "initial_hold": { "cursor": hold.cursor(), "held": hold.hold_piece().map(|piece| piece.as_ascii()),
            "bag_epoch": hold.bag_epoch(), "bag_remainder_key": hold.bag_remainder_key(),
            "provenance": format!("{:?}", hold.provenance()), "observation": format!("{:?}", hold.observation()), },
        "hold_enabled": supply.hold_enabled(), "projects_unplaced_lookahead": supply.projects_unplaced_lookahead(),
        "projects_standard_bag_lookahead": supply.projects_standard_bag_lookahead(),
        "queue_knowledge": format!("{:?}", problem.queue_observation_policy()),
    });
    report["compiled_request"] = json!({
        "problem_id": problem.problem_id().as_str(), "height": problem.visible_height(),
        "geometry_height": problem.compact_pc_geometry_height(), "initial_board_words": problem.initial_board().occupied_words(),
        "exact_locks": problem.exact_pieces(), "goal": problem.search_goal().as_str(),
        "rule": format!("{:?}", problem.rule_profile_value()), "kick_profile": format!("{:?}", problem.kick_profile()),
        "count_policy": format!("{:?}", problem.count_policy()), "output_policy": problem.output_policy().as_str(),
        "pc_chance_evidence_policy": format!("{:?}", problem.pc_chance_evidence_policy()),
        "workers": problem.backend_request().workers(), "worker_policy": problem.backend_request().worker_policy().as_str(),
        "max_patterns": problem.backend_request().max_patterns(), "max_memory_mib": problem.backend_request().max_memory_mib(),
        "max_nodes": problem.backend_policy().max_nodes(),
        "max_frontier_states": problem.backend_policy().max_frontier_states(),
        "max_candidates": problem.backend_policy().max_candidates(),
        "cpu_warmup": problem.backend_policy().cpu_warmup(),
        "use_all_logical_processors": problem.backend_policy().use_all_logical_processors(),
        "requested_backend": problem.backend_policy().requested_backend().as_str(),
        "backend_fallback_allowed": problem.backend_policy().allow_backend_fallback(),
        "tablebase_requested": problem.backend_policy().tablebase_requested(),
        "exact_legal_board_enabled": problem.backend_policy().exact_legal_board_enabled(),
        "conditioned_reachability_enabled": problem.backend_policy().conditioned_reachability_enabled(),
        "resource_budget": format!("{:?}", problem.resource_budget()),
    });
    if universe.pattern_count() != fixture.pattern_count()
        || universe.total_possible_pattern_count() != fixture.pattern_count() as u128
        || !source.complete()
        || !universe.complete()
        || universe.truncation_reason().is_some()
        || universe.materialized_probability_mass().get().to_bits() != 1_f64.to_bits()
        || universe.weights().total_weight().get().to_bits() != 1_f64.to_bits()
        || universe.sequence_len_at(0) != fixture.draws()
        || universe.sequence_len_at(fixture.pattern_count() - 1) != fixture.draws()
        || supply.source_sequence_length() != fixture.draws()
        || problem.exact_pieces() != Some(fixture.draws())
        || problem.visible_height() != fixture.height()
        || problem.compact_pc_geometry_height() != fixture.height()
        || problem.initial_board().width() != 10
        || problem.initial_board().occupied_words() != [if full { 0 } else { 0x3f0 }, 0, 0, 0]
        || problem.search_goal().as_str() != "clear-to-empty"
        || problem.kick_profile().profile_id() != KickTableProfileId::SrsPlus
        || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
        || hold.cursor() != 0
        || hold.hold_piece().is_some()
        || supply.hold_piece().is_some()
        || supply.hold_enabled() != full
        || supply.projects_unplaced_lookahead() != full
        || supply.projects_standard_bag_lookahead()
        || problem.output_policy() != SearchOutputPolicy::CoverageSummary
        || problem.pc_chance_evidence_policy() != PcChanceEvidencePolicy::PcGlobalProbabilityV1
        || problem.backend_request().max_patterns() != 0
        || problem.backend_policy().max_nodes() != 0
        || problem.backend_policy().max_frontier_states() != 0
        || problem.backend_policy().max_candidates() != 0
        || problem.backend_policy().max_memory_mib().is_some()
        || problem.backend_policy().worker_policy() != WorkerPolicy::Auto
        || !problem.backend_policy().use_all_logical_processors()
        || problem.backend_policy().requested_backend() != RequestedSearchBackend::Cpu
        || problem.backend_policy().allow_backend_fallback()
        || problem.backend_policy().cpu_warmup()
        || problem.backend_policy().tablebase_requested()
        || problem.backend_policy().exact_legal_board_enabled()
        || problem.backend_policy().conditioned_reachability_enabled()
    {
        return Err("compiled original PC/hold/EOF/coverage contract changed".into());
    }
    if full
        && !factorized.is_some_and(|shape| {
            shape.pattern_count() == fixture.pattern_count()
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
        return Err("P1P7P7 lost its complete 1+7+7 ranked source".into());
    }
    report["compiled_contract_valid"] = json!(true);
    Ok(())
}

fn record_core(result: &CoreExecutionResult, report: &mut Value) {
    let words = result.coverage_pattern_words();
    let mut digest = Sha256::new();
    for word in words {
        digest.update(word.to_le_bytes());
    }
    report["coverage_bitmap"] = json!({
        "word_count": words.len(), "encoding": "little-endian-u64-original-pattern-order",
        "sha256": (!words.is_empty()).then(|| format!("{:x}", digest.finalize())),
        "popcount": words.iter().map(|word| u64::from(word.count_ones())).sum::<u64>(),
    });
    report["summary_fields"] = json!(result.summary_fields());
    report["count_complete"] = json!(result.bool_field("count_complete"));
    report["probability_complete"] = json!(result.bool_field("probability_complete"));
    report["enumeration_complete"] = json!(result.bool_field("enumeration_complete"));
    report["resource_truncated"] = json!(result.bool_field("resource_truncated"));
    report["workers_used"] = json!(result.usize_field("workers_used"));
    report["active_workers"] = json!(result.usize_field("parallel_active_workers"));
    report["result_retained_bytes"] = json!(result.checked_resource_retained_bytes());
}

fn phase(report: &mut Value, name: &'static str) {
    report["phase"] = json!(name);
    eprintln!(
        "full6l_global_bench phase={name} process_memory={}",
        process_memory()
    );
}

fn check_cancelled(control: &ExecutionControl, where_: &str) -> Result<(), String> {
    if control.is_cancelled() {
        Err(format!("diagnostic cancellation {where_}"))
    } else {
        Ok(())
    }
}

struct DiagnosticProgress(Mutex<(&'static str, Instant)>);
impl ProgressSink for DiagnosticProgress {
    fn report(&self, progress: ExecutionProgress) {
        if let Ok(mut last) = self.0.lock() {
            if last.0 != progress.stage || last.1.elapsed() >= Duration::from_secs(1) {
                eprintln!(
                    "full6l_global_bench product_progress stage={} completed={} total={:?}",
                    progress.stage, progress.completed, progress.total
                );
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
            Err("--profile requires the native wasm-stage-profiling feature".into())
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
        let rows = |stages: Vec<ExecutorSearchProfileStage>| stages.into_iter().filter(|stage| stage.invocation_count != 0).map(|stage| json!({
            "name": stage.name, "duration_ns": stage.duration_ns, "invocation_count": stage.invocation_count, "work_item_count": stage.work_item_count,
        })).collect::<Vec<_>>();
        json!({ "timing_contract": "accumulated overlapping/nested spans, not exclusive wall time", "caller": rows(caller), "pooled_workers": rows(workers) })
    })
    }
    #[cfg(not(all(feature = "wasm-stage-profiling", not(target_family = "wasm"))))]
    {
        let () = profile;
        Value::Null
    }
}

fn binary_sha256() -> Result<String, String> {
    let mut reader = BufReader::new(
        File::open(std::env::current_exe().map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?,
    );
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
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
        json!({ "rss_bytes": bytes("VmRSS:"), "rss_high_water_bytes": bytes("VmHWM:"), "contract": "Linux process kernel RSS; manager samples the owned process tree separately" })
    }
    #[cfg(not(target_os = "linux"))]
    {
        Value::Null
    }
}
