//! Exact CTK3 colored-union Build through the native product, with full P7P7.
//!
//! `inspect` decodes only; `compile` additionally checks the full source; `run`
//! enters the registered native Build host with Auto/all-logical workers.
//! Build with `parallel` and compile-time CLEARRA_SOURCE_COMMIT. `--profile`
//! additionally needs `local-search-ab,wasm-stage-profiling`. Use clearra-manage.
//! `--cancel-after-seconds N` is a diagnostic deadline from execution start,
//! never a smaller source or a completeness claim.

use clearra_app::{
    AppCoreExecutorService, BuildColoredTargetDocument, Ctk3Color, Ctk3Piece,
    ExecutionCancellationToken, ExecutionControl, ExecutionProgress, FieldDocumentFormat,
    ProductBuildIdentity, ProgressSink, decode_ctk3_exact,
};
#[cfg(all(
    feature = "wasm-stage-profiling",
    feature = "local-search-ab",
    not(target_family = "wasm")
))]
use clearra_app::{ExecutorSearchProfileSession, ExecutorSearchProfileStage};
use clearra_core_domain::runtime_cpu_capacity::CpuCapacity;
use clearra_core_executor::CoreExecutionResult;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize, WorkerPolicy,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityQuery, ProblemCompiler,
    SearchProblem,
};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::QueueObservationPolicy;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, Read},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

const SOURCE: &str = "ctk3_w0kCoLsawgRDCgEBkPgHQBoAAIAEAOBvB0B6uw2QbjtJYqllLLVkbD7bvh8yLw";
const BASE: [u64; 4] = [0x0000_00c0_383f_3fc7, 0, 0, 0];
const TARGET: [u64; 4] = [0xfe3f_ff3f_c7c0_c038, 0x0000_000c_120f_c1fc, 0, 0];
const ORIGINAL_PATTERNS: usize = 25_401_600;
const SOURCE_DRAWS: usize = 14;

struct Options {
    mode: String,
    profile: bool,
    cancel_after_seconds: Option<u64>,
}

fn main() {
    let started = Instant::now();
    let mut report = json!({
        "schema": "clearra.oracle-ctk3-bench.v2",
        "fixture": "user-ctk3-colored-union-p7p7",
        "complete": false, "phase": "arguments",
    });
    let outcome = options().and_then(|options| run(&options, &mut report));
    report["total_seconds"] = json!(started.elapsed().as_secs_f64());
    if let Err(error) = &outcome {
        report["status"] = json!("error-or-incomplete");
        report["error"] = json!(error);
    }
    println!("{report}");
    if outcome.is_err() {
        std::process::exit(1);
    }
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().ok_or_else(usage)?;
    if !matches!(mode.as_str(), "inspect" | "compile" | "run") {
        return Err(usage());
    }
    let mut options = Options {
        mode,
        profile: false,
        cancel_after_seconds: None,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile" if !options.profile => options.profile = true,
            "--cancel-after-seconds" if options.cancel_after_seconds.is_none() => {
                let seconds = args
                    .next()
                    .ok_or_else(usage)?
                    .parse::<u64>()
                    .map_err(|_| usage())?;
                if seconds == 0 {
                    return Err(usage());
                }
                options.cancel_after_seconds = Some(seconds);
            }
            _ => return Err(usage()),
        }
    }
    if options.mode != "run" && (options.profile || options.cancel_after_seconds.is_some()) {
        return Err("profiling and diagnostic cancellation require run".into());
    }
    Ok(options)
}

fn usage() -> String {
    "usage: oracle_ctk3_bench inspect|compile|run [--profile] [--cancel-after-seconds N]".into()
}

fn phase(report: &mut Value, name: &'static str) {
    report["phase"] = json!(name);
    eprintln!("oracle_ctk3_bench phase={name}");
}

fn run(options: &Options, report: &mut Value) -> Result<(), String> {
    report["mode"] = json!(options.mode);
    report["diagnostic_deadline_seconds"] = json!(options.cancel_after_seconds);
    report["diagnostic_only"] = json!(options.profile || options.cancel_after_seconds.is_some());
    report["local_controls"] = json!({
        "override_feature_enabled": cfg!(feature = "local-search-ab"),
        "extended_standard_bag_coverage": std::env::var("CLEARRA_EXTENDED_STANDARD_BAG_COVERAGE").ok(),
        "extended_reachability_prefilter": std::env::var("CLEARRA_EXTENDED_REACHABILITY_PREFILTER").ok(),
        "extended_harddrop_proof": std::env::var("CLEARRA_EXTENDED_HARDDROP_PROOF").ok(),
        "trace_memory_admission": std::env::var("CLEARRA_TRACE_MEMORY_ADMISSION").ok(),
        "extended_full_draw_multiset": std::env::var("CLEARRA_EXTENDED_FULL_DRAW_MULTISET").ok(),
        "standard_bag_absorb": std::env::var("CLEARRA_STANDARD_BAG_ABSORB").ok(),
    });
    let identity = ProductBuildIdentity::current();
    report["build"] = json!({
        "source_commit": identity.source_commit(), "engine_build_id": identity.engine_build_id(),
        "supply_semantics_id": identity.supply_semantics_id(), "binary_sha256": binary_sha256()?,
        "parallel_feature": cfg!(feature = "parallel"),
        "stage_profiling_feature": cfg!(feature = "wasm-stage-profiling"),
        "local_search_ab_feature": cfg!(feature = "local-search-ab"),
    });
    phase(report, "decode");
    let field = decode_fixture(report)?;
    if options.mode == "inspect" {
        report["status"] = json!("inspected");
        return Ok(());
    }

    phase(report, "source-compile");
    let capacity = CpuCapacity::current();
    report["cpu"] = json!({
        "policy": "auto-all-logical", "required_workers": capacity.hard_limit(),
        "hard_limit": capacity.hard_limit(), "recommended_parallelism": capacity.recommended_parallelism(),
        "affinity_limit": capacity.affinity_limit(), "expected_vcpus": capacity.expected_vcpus(),
    });
    let compile_started = Instant::now();
    let query = build_query(field, capacity.hard_limit());
    let compiled = ProblemCompiler::compile_scenario_pc(query.core_query());
    report["compile_seconds"] = json!(compile_started.elapsed().as_secs_f64());
    let problem = compiled.map_err(|error| format!("problem compilation: {error:?}"))?;
    validate_compiled(&problem, report)?;
    if options.mode == "compile" {
        report["status"] = json!("compiled-full-source");
        return Ok(());
    }
    if problem.backend_request().workers() != capacity.hard_limit() {
        return Err(
            "requires Auto/all-logical workers from this environment; no topology substitution was made".into(),
        );
    }

    phase(report, "native-host-registration");
    let registration_started = Instant::now();
    let registration = register_native_host();
    report["host_registration_seconds"] = json!(registration_started.elapsed().as_secs_f64());
    registration?;
    phase(report, "execution");
    let profile = start_profile(options.profile)?;
    let cancellation = ExecutionCancellationToken::new();
    let control = if options.profile {
        ExecutionControl::new(cancellation.clone())
            .with_progress_sink(Arc::new(DiagnosticProgress::new()))
    } else {
        ExecutionControl::new(cancellation.clone())
    };
    let timer = options.cancel_after_seconds.map(|seconds| {
        let (done, wait) = mpsc::channel::<()>();
        let handle = cancellation.handle();
        let timer = thread::spawn(move || {
            if matches!(
                wait.recv_timeout(Duration::from_secs(seconds)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                eprintln!("oracle_ctk3_bench diagnostic_cancel_after_seconds={seconds}");
                handle.cancel();
            }
        });
        (done, timer)
    });
    let execute_started = Instant::now();
    let outcome = AppCoreExecutorService::wasm_cpu().execute_build_probability_with_control(
        &problem,
        field,
        query.aggregation(),
        query.finesse_request().clone(),
        query.solution_probability_policy(),
        &control,
    );
    report["execution_seconds"] = json!(execute_started.elapsed().as_secs_f64());
    if let Some((done, timer)) = timer {
        let _ = done.send(());
        timer.join().map_err(|_| "diagnostic timer panicked")?;
    }
    report["cancellation_requested"] = json!(cancellation.is_cancelled());
    report["profile"] = finish_profile(profile);
    phase(report, "result");
    let result = outcome.map_err(|error| format!("native product Build: {error:?}"))?;
    record_result(&result, capacity.hard_limit(), report)?;
    report["status"] = json!("completed");
    Ok(())
}

fn decode_fixture(report: &mut Value) -> Result<BuildProbabilityField, String> {
    let document = decode_ctk3_exact(SOURCE).map_err(|error| format!("CTK3: {error:?}"))?;
    if document.width != 10 || document.pages.len() != 1 {
        return Err("fixture CTK3 width/page contract changed".into());
    }
    let page = &document.pages[0];
    if page.height != 10 || page.operation.is_some() || page.garbage.is_some() {
        return Err("fixture CTK3 page has unexpected height/operation/garbage".into());
    }
    let mut base = [0_u64; 4];
    let mut target = [0_u64; 4];
    let mut pieces = [[0_u64; 4]; 7];
    for (cell, color) in page.cells.iter().enumerate() {
        if cell >= 256 {
            return Err("CTK3 exceeds Board256".into());
        }
        let (word, bit) = (cell / 64, 1_u64 << (cell % 64));
        match color {
            Ctk3Color::Empty => {}
            Ctk3Color::Gray => base[word] |= bit,
            Ctk3Color::Piece(piece) => {
                target[word] |= bit;
                pieces[piece_index(*piece)][word] |= bit;
            }
        }
    }
    if base != BASE || target != TARGET {
        return Err("decoded cells differ from the pinned user fixture".into());
    }
    let height =
        u8::try_from(page.height).map_err(|_| "CTK3 height exceeds Build representation")?;
    let field = BuildProbabilityField::from_words(height, base, target)
        .map_err(|error| format!("generic Build field: {error:?}"))?;
    let (final_words, cleared_rows) =
        clear_completed_rows(std::array::from_fn(|i| base[i] | target[i]));
    report["fixture_input"] = json!({
        "ctk3": SOURCE, "width": document.width, "height": page.height,
        "page_count": document.pages.len(), "page_flags": format!("{:?}", page.flags),
        "page_comment": page.comment,
        "base_words_le": hex_words(base), "base_cells": count_cells(base),
        "required_target_words_le": hex_words(target), "target_cells": count_cells(target),
        "target_operations": field.target_piece_count(),
        "displayed_kind_cell_counts": pieces.map(count_cells),
        "displayed_kind_order": ["I", "O", "T", "S", "Z", "J", "L"],
        "displayed_kind_words_le": pieces.map(hex_words),
        "colored_identity_filter": false, "horizontal_mirror_included": false,
        "goal": "general-build-colored-union", "perfect_clear_goal": false,
        "target_after_line_clears_words_le": hex_words(final_words),
        "target_after_line_clears_cells": count_cells(final_words),
        "completed_original_rows_mask": format!("0x{cleared_rows:x}"),
        "separate_colored_identity_projection": format!("{:?}",
            BuildColoredTargetDocument::decode(FieldDocumentFormat::Ctk3, SOURCE)),
    });
    if field.target_piece_count() != SOURCE_DRAWS
        || count_cells(base) != 22
        || count_cells(target) != 56
    {
        return Err("fixture area/operation contract changed".into());
    }
    Ok(field)
}

fn build_query(field: BuildProbabilityField, hardware_limit: usize) -> BuildProbabilityQuery {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_worker_policy(WorkerPolicy::Auto)
        .with_worker_hardware_limit(hardware_limit)
        .with_use_all_logical_processors(true)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false)
        .with_max_patterns(0);
    // Canonical Web Build assembly: the PC query is an extended-field supply
    // shell. The actual Board256 base/target are passed separately to Build.
    let core = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(
            u16::from(field.height()),
            field.compact_base_mask().unwrap_or(0),
        ),
        PcQueueInput::Standard7Bag,
        PieceWindow::new(field.target_piece_count()),
    )
    .with_rule(srs_plus())
    .with_exact_pieces(Some(field.target_piece_count()))
    .with_supply_window_size(SupplyWindowSize::new(SOURCE_DRAWS))
    .with_min_remaining_queue(0)
    .with_hold_piece(None)
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::all())
    .with_retained_trace_limit(0)
    .with_execution_policy(policy);
    BuildProbabilityQuery::new(core, field)
        .with_aggregation(BuildProbabilityAggregation::Buildability)
        .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
}

fn validate_compiled(problem: &SearchProblem, report: &mut Value) -> Result<(), String> {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or("compiled source has no materialized universe")?;
    let supply = problem.supply();
    report["source"] = json!({
        "pattern": "P7P7", "queue_mode": supply.queue_mode(), "source_draws": supply.source_sequence_length(),
        "original_pattern_count": universe.pattern_count(), "total_possible_pattern_count": universe.total_possible_pattern_count(),
        "source_complete": universe.complete(), "truncation_reason": universe.truncation_reason().map(|reason| format!("{reason:?}")),
        "max_patterns": problem.backend_request().max_patterns(),
        "queue_knowledge": format!("{:?}", problem.queue_observation_policy()),
        "hold_enabled": supply.hold_enabled(), "initial_hold": format!("{:?}", supply.hold_piece()),
        "projects_unplaced_lookahead": supply.projects_unplaced_lookahead(),
        "projects_standard_bag_lookahead": supply.projects_standard_bag_lookahead(),
        "supply_window_resolution": supply.supply_window_resolution(),
        "full_draw_contract": "all 14 original draws retained; canonical EOF held release preserved",
        "rule": format!("{:?}", problem.rule_profile_value()), "kick_profile": format!("{:?}", problem.kick_profile()),
        "workers": problem.backend_request().workers(), "output_policy": problem.output_policy().as_str(),
        "count_policy": format!("{:?}", problem.count_policy()),
    });
    if supply.source_sequence_length() != SOURCE_DRAWS
        || universe.pattern_count() != ORIGINAL_PATTERNS
        || universe.total_possible_pattern_count() != ORIGINAL_PATTERNS as u128
        || !universe.complete()
        || universe.truncation_reason().is_some()
        || universe.sequence_len_at(0) != SOURCE_DRAWS
        || universe.sequence_len_at(ORIGINAL_PATTERNS - 1) != SOURCE_DRAWS
        || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
        || problem.output_policy() != clearra_problem::SearchOutputPolicy::Trace
        || !supply.hold_enabled()
        || supply.hold_piece().is_some()
        || !supply.projects_unplaced_lookahead()
        || !supply.projects_standard_bag_lookahead()
    {
        return Err(
            "compiled full P7P7/Hold/oracle contract differs; no replacement source will be used"
                .into(),
        );
    }
    Ok(())
}

fn record_result(
    result: &CoreExecutionResult,
    expected_workers: usize,
    report: &mut Value,
) -> Result<(), String> {
    // Keep exact product strings, including duplicate keys if present.
    report["summary_fields"] = json!(result.summary_fields());
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
    let keys = result.normalized_solution_keys();
    let mut key_digest = Sha256::new();
    for key in keys {
        key_digest.update((key.len() as u64).to_le_bytes());
        key_digest.update(key.as_bytes());
    }
    report["solutions"] = json!({
        "key_count": keys.len(),
        "key_encoding": "ordered-u64-le-byte-length-prefixed-utf8",
        "keys_sha256": format!("{:x}", key_digest.finalize()),
        "representative_path": result.path_steps().iter().map(|step| json!({
            "piece": format!("{:?}", step.piece()), "rotation": step.rotation(),
            "x": step.x(), "y": step.y(), "hold": step.hold(),
            "cleared_lines": step.cleared_lines(),
        })).collect::<Vec<_>>(),
    });
    let count_complete = result.bool_field("count_complete") == Some(true);
    let probability_complete = result.bool_field("probability_complete") == Some(true);
    report["count_complete"] = json!(count_complete);
    report["probability_complete"] = json!(probability_complete);
    report["complete"] = json!(count_complete && probability_complete);
    if !count_complete || !probability_complete {
        return Err(
            "native Build returned an incomplete result; see preserved product fields".into(),
        );
    }
    if result.usize_field("coverage_pattern_count") != Some(ORIGINAL_PATTERNS)
        || result.usize_field("source_sequence_length") != Some(SOURCE_DRAWS)
        || result.usize_field("workers_used") != Some(expected_workers)
        || result.bool_field("cpu_parallel_execution") != Some(expected_workers > 1)
        || words.len() != ORIGINAL_PATTERNS.div_ceil(64)
    {
        report["complete"] = json!(false);
        return Err(
            "completed result violates original source/automatic worker/coverage bitmap contract"
                .into(),
        );
    }
    Ok(())
}

#[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
fn register_native_host() -> Result<(), String> {
    clearra_app::register_system_native_build_probability_host()
        .map_err(|error| format!("native Build host registration: {error:?}"))
}
#[cfg(not(all(feature = "parallel", not(target_arch = "wasm32"))))]
fn register_native_host() -> Result<(), String> {
    Err("run requires native --features parallel; no serial fallback was used".into())
}

#[cfg(all(
    feature = "wasm-stage-profiling",
    feature = "local-search-ab",
    not(target_family = "wasm")
))]
type Profile = Option<ExecutorSearchProfileSession>;
#[cfg(not(all(
    feature = "wasm-stage-profiling",
    feature = "local-search-ab",
    not(target_family = "wasm")
)))]
type Profile = ();

fn start_profile(enabled: bool) -> Result<Profile, String> {
    #[cfg(all(
        feature = "wasm-stage-profiling",
        feature = "local-search-ab",
        not(target_family = "wasm")
    ))]
    {
        enabled
            .then(ExecutorSearchProfileSession::start)
            .transpose()
            .map_err(|error| format!("stage profile: {error:?}"))
    }
    #[cfg(not(all(
        feature = "wasm-stage-profiling",
        feature = "local-search-ab",
        not(target_family = "wasm")
    )))]
    {
        if enabled {
            Err("--profile requires native --features local-search-ab,wasm-stage-profiling".into())
        } else {
            Ok(())
        }
    }
}

fn finish_profile(profile: Profile) -> Value {
    #[cfg(all(
        feature = "wasm-stage-profiling",
        feature = "local-search-ab",
        not(target_family = "wasm")
    ))]
    {
        profile.map_or(Value::Null, |profile| {
            let (caller, workers) = profile.finish_partitioned();
            let rows = |stages: Vec<ExecutorSearchProfileStage>| -> Vec<Value> {
                stages.into_iter().filter(|stage| stage.invocation_count != 0).map(|stage| json!({
                    "name": stage.name, "duration_ns": stage.duration_ns,
                    "invocation_count": stage.invocation_count, "work_item_count": stage.work_item_count,
                })).collect()
            };
            json!({ "timing_contract": "accumulated overlapping/nested spans; not exclusive wall time",
                "caller": rows(caller), "pooled_workers": rows(workers) })
        })
    }
    #[cfg(not(all(
        feature = "wasm-stage-profiling",
        feature = "local-search-ab",
        not(target_family = "wasm")
    )))]
    {
        let () = profile;
        Value::Null
    }
}

struct DiagnosticProgress {
    last: Mutex<(&'static str, Instant)>,
}
impl DiagnosticProgress {
    fn new() -> Self {
        Self {
            last: Mutex::new(("", Instant::now())),
        }
    }
}
impl ProgressSink for DiagnosticProgress {
    fn report(&self, progress: ExecutionProgress) {
        if let Ok(mut last) = self.last.lock() {
            if last.0 != progress.stage || last.1.elapsed() >= Duration::from_secs(1) {
                eprintln!(
                    "oracle_ctk3_bench product_progress stage={} completed={} total={:?}",
                    progress.stage, progress.completed, progress.total
                );
                *last = (progress.stage, Instant::now());
            }
        }
    }
}

fn binary_sha256() -> Result<String, String> {
    let path = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut file = BufReader::new(File::open(path).map_err(|error| error.to_string())?);
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

fn clear_completed_rows(board: [u64; 4]) -> ([u64; 4], u32) {
    let mut output = [0_u64; 4];
    let mut cleared = 0_u32;
    let mut destination_y = 0;
    for y in 0..10 {
        let occupied = |x: usize| board[(y * 10 + x) / 64] & (1 << ((y * 10 + x) % 64)) != 0;
        if (0..10).all(occupied) {
            cleared |= 1 << y;
        } else {
            for x in 0..10 {
                if occupied(x) {
                    let cell = destination_y * 10 + x;
                    output[cell / 64] |= 1 << (cell % 64);
                }
            }
            destination_y += 1;
        }
    }
    (output, cleared)
}

fn count_cells(words: [u64; 4]) -> u32 {
    words.iter().map(|word| word.count_ones()).sum()
}
fn hex_words(words: [u64; 4]) -> [String; 4] {
    words.map(|word| format!("0x{word:016x}"))
}
const fn piece_index(piece: Ctk3Piece) -> usize {
    match piece {
        Ctk3Piece::I => 0,
        Ctk3Piece::O => 1,
        Ctk3Piece::T => 2,
        Ctk3Piece::S => 3,
        Ctk3Piece::Z => 4,
        Ctk3Piece::J => 5,
        Ctk3Piece::L => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_ctk3_is_general_fourteen_lock_build_without_color_identity_filter() {
        let mut report = json!({});
        let field = decode_fixture(&mut report).unwrap();
        assert_eq!(field.height(), 10);
        assert!(!field.is_compact());
        assert_eq!(field.base_words(), BASE);
        assert_eq!(field.target_words(), TARGET);
        assert_eq!(field.target_piece_count(), 14);
        assert_eq!(report["fixture_input"]["colored_identity_filter"], false);
        assert!(
            count_cells(clear_completed_rows(std::array::from_fn(|i| BASE[i] | TARGET[i])).0) > 0
        );
    }
    #[test]
    fn whole_two_bag_source_keeps_fourteen_draws_and_eof_hold_release() {
        let field = BuildProbabilityField::from_words(10, BASE, TARGET).unwrap();
        let query = build_query(field, EXPECTED_WORKERS);
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query()).unwrap();
        validate_compiled(&problem, &mut json!({})).unwrap();
    }
}
