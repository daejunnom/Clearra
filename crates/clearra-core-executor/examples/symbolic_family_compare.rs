//! Same-binary Full4L comparison with explicit, post-search evidence export.
//!
//! Run only inside the managed benchmark-search profile. Output prefixes must
//! be new and reside below the repository's `_local/artifacts` directory.
//! Identity and global original-source sets are checked; the per-identity
//! source relation is deliberately not claimed by this harness.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use clearra_core_domain::execution_cancellation::{
    ExecutionControl, ExecutionProgress, ProgressSink,
};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::piece::standard_bag_ordinals::StandardBagOrdinals;
use clearra_core_domain::runtime_cpu_capacity::CpuCapacity;
use clearra_core_domain::solution::normalized_tiling_solution::{
    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
    StandardBoard64TilingIdentity,
};
use clearra_core_executor::{
    solve_symbolic_family, CoreExecutionResult, SymbolicFamilyLimits, SymbolicFamilyReport,
    WasmCpuSearchBackend,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
    WorkerPolicy,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem};
use clearra_rules::profile::builtin_rules::jstris_180;
use clearra_supply::QueueObservationPolicy;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SOURCE_COUNT: usize = 4_233_600;
const SOURCE_LENGTH: usize = 11;
const PLACEMENTS: usize = 10;
const PAGE_SIZE: usize = 2_048;
const ABSENT: u64 = u64::MAX;
const SCHEMA: &str = "clearra.symbolic-family-comparison-run.v1";
const USAGE: &str = "usage:\n  symbolic_family_compare --backend legacy|symbolic --output-prefix PATH\n  symbolic_family_compare --compare LEGACY_PREFIX SYMBOLIC_PREFIX [--output-prefix PATH]\n\nThe fixed request is empty 4L, Jstris180, 10 locks, actual Standard7Bag 11 draws,\nenabled empty Hold, FullQueueOracle, CountUnique, Omit, Summary. No time limit\nor candidate fallback is installed. Runs require managed benchmark-search.\nPrefixes must be new and below _local/artifacts; parent directories must exist.\nThe comparison covers complete identity and global source sets, not their joint relation.";

#[derive(Clone, Copy)]
enum Backend {
    Legacy,
    Symbolic,
}

impl Backend {
    fn name(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Symbolic => "symbolic",
        }
    }
}

enum Command {
    Run {
        backend: Backend,
        prefix: PathBuf,
    },
    Compare {
        left: PathBuf,
        right: PathBuf,
        prefix: Option<PathBuf>,
    },
}

fn main() {
    let exit = thread::Builder::new()
        .name("symbolic-family-comparison".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(main_on_product_stack)
        .expect("comparison thread")
        .join()
        .expect("comparison thread completed");
    std::process::exit(exit);
}

fn main_on_product_stack() -> i32 {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return 0;
    }
    let outcome = parse(args.into_iter()).and_then(|command| match command {
        Command::Run { backend, prefix } => run(backend, &prefix),
        Command::Compare {
            left,
            right,
            prefix,
        } => compare(&left, &right, prefix.as_deref()),
    });
    if let Err(error) = outcome {
        eprintln!(
            "{}",
            json!({"event": "harness_error", "error": error, "joint_comparison_complete": false})
        );
        return 1;
    }
    0
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut backend = None;
    let mut prefix = None;
    let mut comparison = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--backend" if backend.is_none() => {
                backend = Some(match args.next().as_deref() {
                    Some("legacy") => Backend::Legacy,
                    Some("symbolic") => Backend::Symbolic,
                    _ => return Err("--backend requires legacy or symbolic".into()),
                });
            }
            "--output-prefix" if prefix.is_none() => {
                prefix = Some(PathBuf::from(args.next().ok_or("missing output prefix")?));
            }
            "--compare" if comparison.is_none() => {
                comparison = Some((
                    PathBuf::from(args.next().ok_or("missing legacy prefix")?),
                    PathBuf::from(args.next().ok_or("missing symbolic prefix")?),
                ));
            }
            _ => return Err(format!("unknown or repeated argument {flag}\n{USAGE}")),
        }
    }
    match (backend, comparison, prefix) {
        (Some(backend), None, Some(prefix)) => Ok(Command::Run { backend, prefix }),
        (None, Some((left, right)), prefix) => Ok(Command::Compare {
            left,
            right,
            prefix,
        }),
        _ => Err(USAGE.into()),
    }
}

fn run(backend: Backend, prefix: &Path) -> Result<(), String> {
    let started = Instant::now();
    let progress = Arc::new(ProgressState::new(started));
    let mut artifacts = Artifacts::create(prefix)?;
    let mut report = json!({
        "schema": SCHEMA, "backend": backend.name(), "fixture": "empty-4L-P7P4-jstris180",
        "complete": false, "search_complete": false, "joint_comparison_complete": false,
        "comparison_scope": "complete identity set and global original-source set only",
        "implicit_deadline_seconds": null, "coverage_capture": false,
        "candidate_fallback": false,
        "build": {
            "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
            "compiled_source_tree": option_env!("CLEARRA_SOURCE_TREE"),
            "profile_note": option_env!("CLEARRA_BUILD_PROFILE_NOTE"),
            "symbolic_family_feature": cfg!(feature = "symbolic-family"),
            "parallel_feature": cfg!(feature = "parallel"),
            "placement_dependency_feature": cfg!(feature = "placement-dependency-pc"),
        },
        "supervision": {
            "supervised": std::env::var("CLEARRA_RUNTIME_SUPERVISED").ok(),
            "profile": std::env::var("CLEARRA_RUNTIME_PROFILE").ok(),
            "run_id": std::env::var("CLEARRA_RUNTIME_RUN_ID").ok(),
        },
    });
    artifacts.checkpoint(&report)?;
    eprintln!("{}", json!({"event": "initial", "report": report}));
    let sampler = Sampler::start(backend.name(), Arc::clone(&progress))?;
    let outcome = execute(backend, &mut artifacts, &mut report, &progress);
    let sampler_outcome = sampler.stop();
    let outcome = outcome.and(sampler_outcome);
    report["total_seconds"] = json!(started.elapsed().as_secs_f64());
    report["last_progress"] = progress.snapshot();
    report["process_memory"] = process_memory();
    match &outcome {
        Ok(()) => {
            report["complete"] = json!(true);
            report["status"] = json!("completed-identity-and-global-coverage-export");
        }
        Err(error) => {
            report["complete"] = json!(false);
            report["status"] = json!("error-or-incomplete");
            report["error"] = json!(error);
        }
    }
    let saved = artifacts.checkpoint(&report);
    println!("{}", json!({"event": "summary", "report": report}));
    outcome.and(saved)
}

fn execute(
    backend: Backend,
    artifacts: &mut Artifacts,
    report: &mut Value,
    progress: &Arc<ProgressState>,
) -> Result<(), String> {
    if std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
        || std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search")
    {
        return Err("Full4L requires the supervised benchmark-search profile".into());
    }
    let binary = std::env::current_exe().map_err(|error| error.to_string())?;
    report["build"]["binary_sha256"] = json!(file_sha256(&binary)?);
    progress.phase.store(1, Ordering::Relaxed);
    let capacity = CpuCapacity::current();
    report["cpu"] = json!({
        "worker_policy": "auto", "use_all_logical_processors": true,
        "hardware_limit": capacity.hard_limit(),
        "recommended_parallelism": capacity.recommended_parallelism(),
        "affinity_limit": capacity.affinity_limit(), "expected_vcpus": capacity.expected_vcpus(),
        "symbolic_parallel_claim": false,
    });
    let compilation_started = Instant::now();
    let problem = compile_problem(capacity.hard_limit())?;
    let (source, request) = describe_problem(&problem)?;
    report["source_fingerprint_sha256"] = json!(json_sha256(&source)?);
    report["request_fingerprint_sha256"] = json!(json_sha256(&request)?);
    report["source"] = source;
    report["compiled_request"] = request;
    report["compile_seconds"] = json!(compilation_started.elapsed().as_secs_f64());
    artifacts.checkpoint(report)?;
    eprintln!(
        "{}",
        json!({"event": "compiled", "source": report["source"], "compiled_request": report["compiled_request"], "source_fingerprint_sha256": report["source_fingerprint_sha256"], "request_fingerprint_sha256": report["request_fingerprint_sha256"]})
    );
    let sink: Arc<dyn ProgressSink> = progress.clone();
    let control = ExecutionControl::default().with_progress_sink(sink);
    progress.phase.store(2, Ordering::Relaxed);
    progress
        .search_started_ns
        .store(progress.elapsed_ns(), Ordering::Relaxed);
    let search_started = Instant::now();
    match backend {
        Backend::Legacy => {
            let outcome = WasmCpuSearchBackend::execute_with_control(&problem, &control);
            report["search_seconds"] = json!(search_started.elapsed().as_secs_f64());
            let result = outcome.map_err(|error| format!("legacy execution: {error:?}"))?;
            report["core_summary"] = json!(result.summary_fields());
            validate_legacy(&result)?;
            report["unique_solution_count"] =
                json!(result.normalized_solution_identities().len().to_string());
            report["covered_source_count"] =
                json!(popcount(result.coverage_pattern_words()).to_string());
            search_checkpoint(artifacts, report, progress)?;
            let evidence_started = Instant::now();
            progress.phase.store(3, Ordering::Relaxed);
            let started = Instant::now();
            report["identities"] = write_identities(
                &artifacts.identities,
                result.normalized_solution_identities(),
            )?;
            report["identity_export_seconds"] = json!(started.elapsed().as_secs_f64());
            progress.phase.store(4, Ordering::Relaxed);
            let started = Instant::now();
            report["coverage"] =
                write_coverage(&artifacts.coverage, result.coverage_pattern_words())?;
            report["coverage"]["construction"] = json!("legacy complete original-ordinal bitmap");
            report["coverage"]["membership_queries"] = json!("0");
            report["coverage_export_seconds"] = json!(started.elapsed().as_secs_f64());
            report["evidence_seconds"] = json!(evidence_started.elapsed().as_secs_f64());
        }
        Backend::Symbolic => {
            let outcome =
                solve_symbolic_family(&problem, SymbolicFamilyLimits::default(), &control);
            report["search_seconds"] = json!(search_started.elapsed().as_secs_f64());
            let mut result = outcome.map_err(|error| error.to_string())?;
            report["symbolic_report"] = symbolic_report(result.report());
            if result.report().source_count != SOURCE_COUNT as u128
                || result.source_length() != SOURCE_LENGTH
            {
                return Err("symbolic result changed the original source contract".into());
            }
            report["unique_solution_count"] =
                json!(result.report().unique_solution_count.to_string());
            report["covered_source_count"] =
                json!(result.report().covered_source_count.to_string());
            search_checkpoint(artifacts, report, progress)?;
            let evidence_started = Instant::now();
            progress.phase.store(3, Ordering::Relaxed);
            let started = Instant::now();
            let count = usize::try_from(result.report().unique_solution_count)
                .map_err(|_| "identity count does not fit this host")?;
            let mut identities = Vec::new();
            identities
                .try_reserve_exact(count)
                .map_err(|_| "identity export allocation failed")?;
            while identities.len() < count {
                let wanted = PAGE_SIZE.min(count - identities.len());
                let page = result
                    .identity_page(identities.len() as u128, wanted)
                    .map_err(|error| format!("identity page: {error}"))?;
                if page.len() != wanted {
                    return Err("symbolic identity page ended before its exact count".into());
                }
                identities.extend(page);
            }
            if !result
                .identity_page(count as u128, 1)
                .map_err(|error| error.to_string())?
                .is_empty()
            {
                return Err("symbolic identity page exceeds its exact count".into());
            }
            identities.sort_unstable();
            report["identities"] = write_identities(&artifacts.identities, &identities)?;
            drop(identities);
            report["identity_export_seconds"] = json!(started.elapsed().as_secs_f64());
            progress.phase.store(4, Ordering::Relaxed);
            let started = Instant::now();
            let ordinals = StandardBagOrdinals::new(SOURCE_LENGTH).expect("fixed source length");
            let mut words = vec![0_u64; SOURCE_COUNT.div_ceil(64)];
            for ordinal in 0..SOURCE_COUNT {
                let (queue, length) = ordinals
                    .sequence_at(ordinal)
                    .expect("bounded original ordinal");
                if result
                    .covers_source(&queue[..length])
                    .map_err(|error| error.to_string())?
                {
                    words[ordinal / 64] |= 1_u64 << (ordinal % 64);
                }
                if ordinal & 0x3fff == 0 {
                    progress
                        .export_completed
                        .store(ordinal as u64, Ordering::Relaxed);
                }
            }
            progress
                .export_completed
                .store(SOURCE_COUNT as u64, Ordering::Relaxed);
            if u128::from(popcount(&words)) != result.report().covered_source_count {
                return Err(
                    "symbolic membership bitmap differs from its exact covered-source count".into(),
                );
            }
            report["coverage"] = write_coverage(&artifacts.coverage, &words)?;
            report["coverage"]["construction"] =
                json!("read-only covers_source for every original ordinal");
            report["coverage"]["membership_queries"] = json!(SOURCE_COUNT.to_string());
            report["coverage"]["full_count_shortcut"] = json!(false);
            report["coverage_export_seconds"] = json!(started.elapsed().as_secs_f64());
            report["evidence_seconds"] = json!(evidence_started.elapsed().as_secs_f64());
        }
    }
    report["checked_identity_count"] = report["identities"]["count"].clone();
    report["checked_original_source_count"] = json!(SOURCE_COUNT.to_string());
    report["identity_export_complete"] = json!(true);
    report["global_coverage_export_complete"] = json!(true);
    progress.phase.store(7, Ordering::Relaxed);
    Ok(())
}

fn compile_problem(hardware_limit: usize) -> Result<SearchProblem, String> {
    let execution = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_worker_policy(WorkerPolicy::Auto)
        .with_worker_hardware_limit(hardware_limit)
        .with_use_all_logical_processors(true)
        .with_cpu_warmup(false)
        .with_gpu_warmup(false)
        .with_runtime_webgpu_available(false)
        .with_tablebase_requested(false)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_precompute_build_dependencies(false)
        .with_max_patterns(0)
        .with_max_nodes(0)
        .with_max_candidates(0)
        .with_max_frontier_states(0)
        .with_max_memory_mib(None);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(PLACEMENTS),
    )
    .with_rule(jstris_180())
    .with_exact_pieces(Some(PLACEMENTS))
    .with_supply_window_size(SupplyWindowSize::new(SOURCE_LENGTH))
    .with_min_remaining_queue(0)
    .with_allow_hold(true)
    .with_hold_piece(None)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::unique())
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_retained_trace_limit(0)
    .with_execution_policy(execution);
    ProblemCompiler::compile_scenario_pc(&query)
        .map(|problem| problem.with_output_policy(SearchOutputPolicy::Summary))
        .map_err(|error| format!("common problem compilation: {error:?}"))
}

fn describe_problem(problem: &SearchProblem) -> Result<(Value, Value), String> {
    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .ok_or("missing original source")?;
    let ordinals = universe
        .complete_standard_bag_ordinals()
        .ok_or("source lacks actual complete standard-bag authority")?;
    let supply = problem.supply();
    let hold = problem.initial_hold();
    let policy = problem.backend_request();
    if ordinals.sequence_len() != SOURCE_LENGTH
        || ordinals.len() != SOURCE_COUNT
        || !source.complete()
        || !universe.complete()
        || universe.truncation_reason().is_some()
        || universe.total_possible_pattern_count() != SOURCE_COUNT as u128
        || problem.initial_board().occupied_words() != [0; 4]
        || problem.visible_height() != 4
        || problem.compact_pc_geometry_height() != 4
        || problem.exact_pieces() != Some(PLACEMENTS)
        || problem.rule_profile_value() != jstris_180()
        || problem.count_policy() != PcCountPolicy::CountUnique
        || problem.objective() != ObjectivePolicy::unique()
        || problem.output_policy() != SearchOutputPolicy::Summary
        || problem.solution_probability_policy() != PcSolutionProbabilityPolicy::Omit
        || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
        || supply.source_sequence_length() != SOURCE_LENGTH
        || !supply.hold_enabled()
        || hold.hold_piece().is_some()
        || hold.cursor() != 0
        || supply.projects_unplaced_lookahead()
        || supply.projects_standard_bag_lookahead()
        || problem.budget().max_seconds() != 0
        || problem.budget().max_patterns() != 0
        || policy.max_nodes() != 0
        || policy.max_candidates() != 0
        || policy.max_frontier_states() != 0
        || policy.max_memory_mib().is_some()
    {
        return Err("common request/source/no-deadline contract changed".into());
    }
    let source = json!({
        "piece_source_id": source.id().get().to_string(), "kind": source.kind().as_str(),
        "pattern_universe_id": universe.pattern_universe_id().get().to_string(),
        "pattern_weight_model_id": universe.pattern_weight_model_id().get().to_string(),
        "provenance": format!("{:?}", source.provenance()),
        "structure": format!("{:?}", universe.structure()),
        "original_source_count": SOURCE_COUNT.to_string(), "full_length": SOURCE_LENGTH,
        "visible_length": supply.source_sequence_length(), "complete": true,
        "ordinal_order": PieceKind::STANDARD_TETROMINOES.iter().map(|piece| piece.as_ascii()).collect::<String>(),
        "ordinal_contract": "fresh standard bag permutations, 7! x 7P4, unit original outcomes",
        "hold_source_id": hold.piece_source_id().get().to_string(),
        "hold_source_kind": hold.source_kind().as_str(), "hold_cursor": hold.cursor(),
        "hold_policy": format!("{:?}", hold.hold_policy()), "initial_hold": null,
        "bag_epoch": hold.bag_epoch(), "bag_remainder_key": hold.bag_remainder_key(),
        "hold_provenance": format!("{:?}", hold.provenance()),
        "hold_observation": format!("{:?}", hold.observation()),
        "projects_unplaced_lookahead": supply.projects_unplaced_lookahead(),
        "projects_standard_bag_lookahead": supply.projects_standard_bag_lookahead(),
    });
    let request = json!({
        "problem_id": problem.problem_id().as_str(), "preset": format!("{:?}", problem.preset()),
        "width": 10, "height": 4, "initial_board_words": problem.initial_board().occupied_words(),
        "exact_locks": PLACEMENTS, "goal": problem.search_goal().as_str(),
        "rule": problem.rule_profile_value().id().as_str(), "kick_profile": format!("{:?}", problem.kick_profile()),
        "objective": format!("{:?}", problem.objective()), "count_policy": "count-unique",
        "output_policy": problem.output_policy().as_str(), "solution_probability_policy": "omit",
        "queue_observation_policy": "full-queue-oracle", "hold_enabled": true,
        "workers": policy.workers(), "worker_policy": policy.worker_policy().as_str(),
        "hardware_limit": policy.worker_hardware_limit(), "use_all_logical_processors": policy.use_all_logical_processors(),
        "cpu_warmup": policy.cpu_warmup(), "gpu_warmup": policy.gpu_warmup(),
        "tablebase": policy.tablebase_requested(), "exact_legal_board": policy.exact_legal_board_enabled(),
        "conditioned_reachability": policy.conditioned_reachability_enabled(),
        "precompute_build_dependencies": policy.precompute_build_dependencies(),
        "backend_fallback": policy.allow_backend_fallback(), "max_patterns": policy.max_patterns(),
        "max_nodes": policy.max_nodes(), "max_candidates": policy.max_candidates(),
        "max_frontier_states": policy.max_frontier_states(), "max_memory_mib": policy.max_memory_mib(),
        "max_seconds": problem.budget().max_seconds(), "resource_budget": format!("{:?}", problem.resource_budget()),
    });
    Ok((source, request))
}

fn validate_legacy(result: &CoreExecutionResult) -> Result<(), String> {
    if result.unique_field("count_complete") != Some("true")
        || result.unique_field("packing_count_complete") != Some("true")
        || result.unique_field("probability_complete") != Some("true")
        || result.unique_field("resource_truncated") != Some("false")
        || result.bool_field("backend_fallback_used") == Some(true)
        || result.usize_field("normalized_unique_solution_count")
            != Some(result.normalized_solution_identities().len())
        || result.usize_field("coverage_pattern_count") != Some(SOURCE_COUNT)
        || result.usize_field("source_sequence_length") != Some(SOURCE_LENGTH)
        || result.coverage_pattern_words().len() != SOURCE_COUNT.div_ceil(64)
        || result.usize_field("covered_pattern_count")
            != Some(popcount(result.coverage_pattern_words()) as usize)
    {
        return Err("legacy returned incomplete or mismatched identity/source evidence".into());
    }
    Ok(())
}

fn search_checkpoint(
    artifacts: &mut Artifacts,
    report: &mut Value,
    progress: &ProgressState,
) -> Result<(), String> {
    report["search_complete"] = json!(true);
    report["search_completion_memory"] = process_memory();
    report["search_completion_progress"] = progress.snapshot();
    report["status"] = json!("search-complete-evidence-export-pending");
    artifacts.checkpoint(report)?;
    println!("{}", json!({"event": "search_complete", "report": report}));
    Ok(())
}

fn write_identities(
    path: &Path,
    identities: &[StandardBoard64TilingIdentity],
) -> Result<Value, String> {
    if identities.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("identity output is duplicated or not in typed canonical order".into());
    }
    let mut output = BufWriter::new(new_file(path)?);
    let mut hash = Sha256::new();
    let mut bytes = 0_u64;
    let mut key = String::with_capacity(256);
    let mut previous = String::with_capacity(256);
    for &identity in identities {
        if identity.initial_board_mask() != 0 || identity.placement_count() != PLACEMENTS {
            return Err("identity is outside the compiled Full4 frame".into());
        }
        key.clear();
        identity
            .write_canonical(&mut key)
            .map_err(|error| error.to_string())?;
        if !previous.is_empty() && key <= previous {
            return Err("CTK1 rows are not strictly lexically ordered".into());
        }
        output
            .write_all(key.as_bytes())
            .and_then(|()| output.write_all(b"\n"))
            .map_err(|error| format!("identity output: {error}"))?;
        hash.update(key.as_bytes());
        hash.update(b"\n");
        bytes = bytes
            .checked_add(key.len() as u64 + 1)
            .ok_or("identity byte count overflow")?;
        std::mem::swap(&mut key, &mut previous);
    }
    output.flush().map_err(|error| error.to_string())?;
    let cts =
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(identities);
    Ok(json!({
        "count": identities.len().to_string(), "duplicate_count": "0", "bytes": bytes.to_string(),
        "encoding": "strictly sorted canonical CTK1 UTF-8 rows, one LF per identity, no header",
        "sha256": format!("{:x}", hash.finalize()), "cts1": cts,
        "path": path.to_string_lossy(), "complete": true,
    }))
}

fn write_coverage(path: &Path, words: &[u64]) -> Result<Value, String> {
    if words.len() != SOURCE_COUNT.div_ceil(64) {
        return Err("global coverage bitmap has the wrong original-source length".into());
    }
    let mut output = BufWriter::new(new_file(path)?);
    let mut hash = Sha256::new();
    for word in words {
        let encoded = word.to_le_bytes();
        output
            .write_all(&encoded)
            .map_err(|error| error.to_string())?;
        hash.update(encoded);
    }
    output.flush().map_err(|error| error.to_string())?;
    Ok(json!({
        "word_count": words.len().to_string(), "bytes": (words.len() * 8).to_string(),
        "covered_source_count": popcount(words).to_string(), "original_source_count": SOURCE_COUNT.to_string(),
        "encoding": "u64 little-endian, original StandardBagOrdinals order, no header",
        "sha256": format!("{:x}", hash.finalize()), "path": path.to_string_lossy(), "complete": true,
    }))
}

fn popcount(words: &[u64]) -> u64 {
    words.iter().map(|word| u64::from(word.count_ones())).sum()
}

fn symbolic_report(report: &SymbolicFamilyReport) -> Value {
    json!({
        "unique_solution_count": report.unique_solution_count.to_string(),
        "covered_source_count": report.covered_source_count.to_string(),
        "source_count": report.source_count.to_string(), "solution_source_pairs": report.solution_source_pairs.to_string(),
        "catalog_rows": report.catalog_rows, "geometry_nodes": report.geometry_nodes,
        "geometry_bytes_estimate": report.geometry_bytes_estimate,
        "geometry_released_construction_bytes": report.geometry_released_construction_bytes,
        "geometry_expanded_nodes": report.geometry_expanded_nodes,
        "geometry_tablebase_pruned_states": report.geometry_tablebase_pruned_states,
        "source_residual_states": report.source_residual_states,
        "set_nodes_created": report.set_nodes_created.to_string(), "set_apply_misses": report.set_apply_misses.to_string(),
        "set_work_steps": report.set_work_steps.to_string(), "set_compactions": report.set_compactions.to_string(),
        "predicate_peak_nodes": report.predicate_peak_nodes, "predicate_work_steps": report.predicate_work_steps.to_string(),
        "bridge_work_steps": report.bridge_work_steps.to_string(), "derivative_work_steps": report.derivative_work_steps.to_string(),
        "derivative_cache_misses": report.derivative_cache_misses.to_string(),
        "retained_set_bytes_estimate": report.retained_set_bytes_estimate,
        "retained_predicate_bytes_estimate": report.retained_predicate_bytes_estimate,
        "retained_derivative_bytes_estimate": report.retained_derivative_bytes_estimate,
        "result_compaction_peak_bytes_estimate": report.result_compaction_peak_bytes_estimate,
        "ranks": report.ranks.iter().map(|rank| json!({
            "placed_pieces": rank.placed_pieces, "supply_states": rank.supply_states,
            "joint_members": rank.joint_members.to_string(),
            "set_nodes_before_compaction": rank.set_nodes_before_compaction,
            "set_nodes_after_compaction": rank.set_nodes_after_compaction,
            "set_compaction_peak_bytes_estimate": rank.set_compaction_peak_bytes_estimate,
            "predicate_nodes": rank.predicate_nodes, "derivative_nodes": rank.derivative_nodes,
            "movement_poses": rank.movement_poses, "guarded_movement_edges": rank.guarded_movement_edges,
            "movement_propagations": rank.movement_propagations.to_string(),
            "legal_row_queries": rank.legal_row_queries.to_string(), "geometry_filters": rank.geometry_filters.to_string(),
        })).collect::<Vec<_>>(),
    })
}

struct Artifacts {
    summary: File,
    identities: PathBuf,
    coverage: PathBuf,
}

impl Artifacts {
    fn create(prefix: &Path) -> Result<Self, String> {
        let prefix = checked_prefix(prefix)?;
        let identities = append(&prefix, ".identities.ctk1");
        let coverage = append(&prefix, ".coverage.u64le");
        for path in [&identities, &coverage] {
            if path.try_exists().map_err(|error| error.to_string())? {
                return Err(format!("output already exists: {}", path.display()));
            }
        }
        Ok(Self {
            summary: new_file(&append(&prefix, ".summary.json"))?,
            identities,
            coverage,
        })
    }

    fn checkpoint(&mut self, report: &Value) -> Result<(), String> {
        self.summary
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.summary.set_len(0))
            .map_err(|error| format!("summary checkpoint: {error}"))?;
        serde_json::to_writer_pretty(&mut self.summary, report)
            .map_err(|error| error.to_string())?;
        self.summary
            .write_all(b"\n")
            .and_then(|()| self.summary.flush())
            .and_then(|()| self.summary.sync_data())
            .map_err(|error| format!("summary checkpoint: {error}"))
    }
}

fn append(prefix: &Path, suffix: &str) -> PathBuf {
    let mut name = prefix.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn checked_prefix(prefix: &Path) -> Result<PathBuf, String> {
    if prefix
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("output prefixes cannot contain parent traversal".into());
    }
    let name = prefix.file_name().ok_or("output prefix has no file name")?;
    let parent = prefix
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|error| format!("output parent: {error}"))?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|error| format!("repository root: {error}"))?;
    let root = repository.join("_local/artifacts");
    if !parent.starts_with(&root) {
        return Err("output prefix must be inside the repository's _local/artifacts root".into());
    }
    Ok(parent.join(name))
}

fn new_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create {}: {error}", path.display()))
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut input = BufReader::new(File::open(path).map_err(|error| error.to_string())?);
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65_536];
    loop {
        let count = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn json_sha256(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn compare(left: &Path, right: &Path, output: Option<&Path>) -> Result<(), String> {
    let started = Instant::now();
    let left = checked_prefix(left)?;
    let right = checked_prefix(right)?;
    let read = |prefix: &Path| -> Result<Value, String> {
        let file =
            File::open(append(prefix, ".summary.json")).map_err(|error| error.to_string())?;
        serde_json::from_reader(BufReader::new(file)).map_err(|error| error.to_string())
    };
    let a = read(&left)?;
    let b = read(&right)?;
    if a["backend"] != "legacy" || b["backend"] != "symbolic" {
        return Err("comparison expects legacy then symbolic prefixes".into());
    }
    for report in [&a, &b] {
        if report["schema"] != SCHEMA
            || report["complete"] != true
            || report["search_complete"] != true
            || report["identity_export_complete"] != true
            || report["global_coverage_export_complete"] != true
            || report["checked_original_source_count"] != SOURCE_COUNT.to_string()
            || report["source_fingerprint_sha256"] != json_sha256(&report["source"])?
            || report["request_fingerprint_sha256"] != json_sha256(&report["compiled_request"])?
        {
            return Err("a run lacks complete or internally consistent projection evidence".into());
        }
    }
    if a["source"] != b["source"]
        || a["compiled_request"] != b["compiled_request"]
        || a["source_fingerprint_sha256"] != b["source_fingerprint_sha256"]
        || a["request_fingerprint_sha256"] != b["request_fingerprint_sha256"]
        || a["build"]["binary_sha256"] != b["build"]["binary_sha256"]
    {
        return Err("source/request/same-binary fingerprints differ".into());
    }
    let identities = compare_files(
        &append(&left, ".identities.ctk1"),
        &append(&right, ".identities.ctk1"),
    )?;
    let coverage = compare_files(
        &append(&left, ".coverage.u64le"),
        &append(&right, ".coverage.u64le"),
    )?;
    if identities["left_sha256"] != a["identities"]["sha256"]
        || identities["right_sha256"] != b["identities"]["sha256"]
        || coverage["left_sha256"] != a["coverage"]["sha256"]
        || coverage["right_sha256"] != b["coverage"]["sha256"]
    {
        return Err("artifact bytes no longer match their owning summaries".into());
    }
    let equal = identities["equal"] == true
        && coverage["equal"] == true
        && a["checked_identity_count"] == b["checked_identity_count"]
        && a["identities"]["cts1"] == b["identities"]["cts1"];
    let report = json!({
        "schema": "clearra.symbolic-family-projection-comparison.v1", "complete": true,
        "equivalent_in_checked_projection": equal, "joint_comparison_complete": false,
        "identity_comparison_complete": true, "global_coverage_comparison_complete": true,
        "checked_identity_count": a["checked_identity_count"],
        "checked_original_source_count": SOURCE_COUNT.to_string(),
        "source_fingerprint_sha256": a["source_fingerprint_sha256"],
        "request_fingerprint_sha256": a["request_fingerprint_sha256"],
        "binary_sha256": a["build"]["binary_sha256"],
        "identities": identities, "global_coverage": coverage,
        "left_prefix": left.to_string_lossy(), "right_prefix": right.to_string_lossy(),
        "elapsed_seconds": started.elapsed().as_secs_f64(),
        "scope": "exact bytes of complete sorted identities and global source bitmap; per-identity source sets unchecked",
    });
    let output = output
        .map(checked_prefix)
        .transpose()?
        .unwrap_or_else(|| append(&right, "-comparison"));
    let mut file = new_file(&append(&output, ".summary.json"))?;
    serde_json::to_writer_pretty(&mut file, &report).map_err(|error| error.to_string())?;
    file.write_all(b"\n")
        .and_then(|()| file.flush())
        .map_err(|error| error.to_string())?;
    println!("{}", json!({"event": "comparison", "report": report}));
    if equal {
        Ok(())
    } else {
        Err("identity or global coverage evidence differs".into())
    }
}

fn compare_files(left: &Path, right: &Path) -> Result<Value, String> {
    let mut a = BufReader::new(File::open(left).map_err(|error| error.to_string())?);
    let mut b = BufReader::new(File::open(right).map_err(|error| error.to_string())?);
    let mut ah = Sha256::new();
    let mut bh = Sha256::new();
    let mut ab = [0_u8; 65_536];
    let mut bb = [0_u8; 65_536];
    let mut offset = 0_u64;
    let mut first_difference = None;
    let mut a_bytes = 0_u64;
    let mut b_bytes = 0_u64;
    loop {
        let ac = read_block(&mut a, &mut ab).map_err(|error| error.to_string())?;
        let bc = read_block(&mut b, &mut bb).map_err(|error| error.to_string())?;
        if ac == 0 && bc == 0 {
            break;
        }
        ah.update(&ab[..ac]);
        bh.update(&bb[..bc]);
        a_bytes += ac as u64;
        b_bytes += bc as u64;
        if first_difference.is_none() {
            first_difference = ab[..ac]
                .iter()
                .zip(&bb[..bc])
                .position(|(a, b)| a != b)
                .or_else(|| (ac != bc).then_some(ac.min(bc)))
                .map(|index| offset + index as u64);
        }
        offset += ac.max(bc) as u64;
    }
    Ok(json!({
        "equal": first_difference.is_none(), "first_difference_byte": first_difference.map(|n| n.to_string()),
        "left_bytes": a_bytes.to_string(), "right_bytes": b_bytes.to_string(),
        "left_sha256": format!("{:x}", ah.finalize()), "right_sha256": format!("{:x}", bh.finalize()),
        "both_eof_checked": true,
    }))
}

fn read_block(input: &mut impl Read, output: &mut [u8]) -> io::Result<usize> {
    let mut used = 0;
    while used < output.len() {
        match input.read(&mut output[used..]) {
            Ok(0) => break,
            Ok(count) => used += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(used)
}

const PHASES: &[&str] = &[
    "starting",
    "compile",
    "search",
    "identity_export",
    "coverage_export",
    "artifact_write",
    "compare",
    "complete",
    "catalog",
    "source",
    "targets",
    "geometry",
    "occupancy",
    "templates",
    "initial_down",
    "rank_domain",
    "board",
    "supply",
    "movement",
    "rows",
    "row_legal",
    "row_bridge",
    "row_add",
    "row_down",
    "row_merge",
    "rank_count",
    "rank_compact",
    "predicate_rewind",
    "accepted",
    "identity_project",
    "source_project",
    "final_count",
    "final_compact",
    "result_release",
];

struct ProgressState {
    started: Instant,
    phase: AtomicU32,
    rank: AtomicU64,
    rank_total: AtomicU64,
    piece: AtomicU64,
    row: AtomicU64,
    row_total: AtomicU64,
    work_zdd: AtomicU64,
    work_boolean: AtomicU64,
    work_down: AtomicU64,
    work_bridge: AtomicU64,
    event_count: AtomicU64,
    export_completed: AtomicU64,
    search_started_ns: AtomicU64,
    geometry_seen: AtomicBool,
    geometry_finished_ns: AtomicU64,
    other: Mutex<Option<ExecutionProgress>>,
}

impl ProgressState {
    fn new(started: Instant) -> Self {
        Self {
            started,
            phase: AtomicU32::new(0),
            rank: AtomicU64::new(ABSENT),
            rank_total: AtomicU64::new(0),
            piece: AtomicU64::new(ABSENT),
            row: AtomicU64::new(ABSENT),
            row_total: AtomicU64::new(0),
            work_zdd: AtomicU64::new(0),
            work_boolean: AtomicU64::new(0),
            work_down: AtomicU64::new(0),
            work_bridge: AtomicU64::new(0),
            event_count: AtomicU64::new(0),
            export_completed: AtomicU64::new(0),
            search_started_ns: AtomicU64::new(0),
            geometry_seen: AtomicBool::new(false),
            geometry_finished_ns: AtomicU64::new(0),
            other: Mutex::new(None),
        }
    }

    fn elapsed_ns(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    fn snapshot(&self) -> Value {
        let optional = |value: &AtomicU64| {
            let value = value.load(Ordering::Relaxed);
            (value != ABSENT).then_some(value)
        };
        let geometry = self.geometry_finished_ns.load(Ordering::Relaxed);
        let search = self.search_started_ns.load(Ordering::Relaxed);
        let other = *self.other.lock().expect("progress event state");
        json!({
            "phase": PHASES[self.phase.load(Ordering::Relaxed) as usize],
            "rank": optional(&self.rank), "rank_total": self.rank_total.load(Ordering::Relaxed),
            "piece": optional(&self.piece).and_then(|piece| char::from_u32(piece as u32)),
            "row": optional(&self.row), "row_total": self.row_total.load(Ordering::Relaxed),
            "latest_arena_work": {
                "zdd": self.work_zdd.load(Ordering::Relaxed).to_string(),
                "boolean": self.work_boolean.load(Ordering::Relaxed).to_string(),
                "down": self.work_down.load(Ordering::Relaxed).to_string(),
                "bridge": self.work_bridge.load(Ordering::Relaxed).to_string(),
            },
            "event_count": self.event_count.load(Ordering::Relaxed).to_string(),
            "coverage_export_completed": self.export_completed.load(Ordering::Relaxed).to_string(),
            "geometry_completed_seconds_since_start": (geometry != 0).then_some(geometry as f64 / 1e9),
            "geometry_completed_seconds_since_search": (geometry != 0 && geometry >= search).then_some(geometry.saturating_sub(search) as f64 / 1e9),
            "last_other_event": other.map(|event| json!({"stage": event.stage, "completed": event.completed.to_string(), "total": event.total.map(|total| total.to_string())})),
            "snapshot_scope": "independent relaxed diagnostic fields; not an execution-completeness authority",
        })
    }
}

impl ProgressSink for ProgressState {
    fn report(&self, event: ExecutionProgress) {
        self.event_count.fetch_add(1, Ordering::Relaxed);
        // The one geometry boundary is the only clock read in this callback.
        // Work pulses and row/context markers never format, sample RSS or do I/O.
        if matches!(
            event.stage,
            "symbolic_phase_occupancy" | "geometry-candidate-family"
        ) && !self.geometry_seen.swap(true, Ordering::Relaxed)
        {
            self.geometry_finished_ns
                .store(self.elapsed_ns(), Ordering::Relaxed);
        }
        match event.stage {
            "symbolic_work_zdd" => self.work_zdd.store(event.completed, Ordering::Relaxed),
            "symbolic_work_boolean" => self.work_boolean.store(event.completed, Ordering::Relaxed),
            "symbolic_work_down" => self.work_down.store(event.completed, Ordering::Relaxed),
            "symbolic_work_bridge" => self.work_bridge.store(event.completed, Ordering::Relaxed),
            "symbolic_context_rank" => {
                self.rank.store(event.completed, Ordering::Relaxed);
                self.rank_total
                    .store(event.total.unwrap_or(0), Ordering::Relaxed);
                self.piece.store(ABSENT, Ordering::Relaxed);
                self.row.store(ABSENT, Ordering::Relaxed);
            }
            "symbolic_context_piece" => {
                self.piece.store(event.completed, Ordering::Relaxed);
                self.row.store(ABSENT, Ordering::Relaxed);
            }
            "symbolic_context_row" => {
                self.row.store(event.completed, Ordering::Relaxed);
                self.row_total
                    .store(event.total.unwrap_or(0), Ordering::Relaxed);
            }
            "symbolic_context_clear" => {
                self.rank.store(ABSENT, Ordering::Relaxed);
                self.piece.store(ABSENT, Ordering::Relaxed);
                self.row.store(ABSENT, Ordering::Relaxed);
            }
            _ => {
                if let Some(name) = event.stage.strip_prefix("symbolic_phase_") {
                    if let Some(index) = PHASES.iter().position(|phase| *phase == name) {
                        self.phase.store(index as u32, Ordering::Relaxed);
                        return;
                    }
                }
                *self.other.lock().expect("progress event state") = Some(event);
            }
        }
    }
}

struct Sampler {
    done: mpsc::Sender<()>,
    thread: thread::JoinHandle<()>,
}

impl Sampler {
    fn start(backend: &'static str, progress: Arc<ProgressState>) -> Result<Self, String> {
        let (done, wait) = mpsc::channel();
        let thread = thread::Builder::new().name("symbolic-family-progress".into()).spawn(move || {
            while matches!(wait.recv_timeout(Duration::from_secs(1)), Err(mpsc::RecvTimeoutError::Timeout)) {
                eprintln!("{}", json!({
                    "event": "progress", "backend": backend, "elapsed_seconds": progress.started.elapsed().as_secs_f64(),
                    "progress": progress.snapshot(), "process_memory": process_memory(), "complete": false,
                }));
            }
        }).map_err(|error| format!("progress sampler: {error}"))?;
        Ok(Self { done, thread })
    }

    fn stop(self) -> Result<(), String> {
        let _ = self.done.send(());
        self.thread
            .join()
            .map_err(|_| "progress sampler panicked".into())
    }
}

fn process_memory() -> Value {
    #[cfg(target_os = "linux")]
    {
        let Ok(status) = fs::read_to_string("/proc/self/status") else {
            return json!({"available": false});
        };
        let bytes = |label: &str| -> Option<String> {
            let mut fields = status
                .lines()
                .find_map(|line| line.strip_prefix(label))?
                .split_ascii_whitespace();
            let kib = fields.next()?.parse::<u64>().ok()?;
            if fields.next() != Some("kB") || fields.next().is_some() {
                return None;
            }
            kib.checked_mul(1024).map(|bytes| bytes.to_string())
        };
        json!({"rss_bytes": bytes("VmRSS:"), "rss_high_water_bytes": bytes("VmHWM:"), "scope": "kernel process RSS, not aggregate descendants"})
    }
    #[cfg(not(target_os = "linux"))]
    {
        json!({"available": false})
    }
}
