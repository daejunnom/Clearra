//! Explicit native entry point for the direct symbolic family research path.
//!
//! `compile` prepares and reports the request without calling the solver.
//! `run` calls only `solve_symbolic_family`; there is no candidate fallback.
//! No elapsed-time deadline is installed. Large or unbounded runs belong inside
//! the repository's managed runtime, just like other search diagnostics.

use std::io;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use clearra_core_domain::execution_cancellation::{
    ExecutionCancellationToken, ExecutionControl, ExecutionProgress, ProgressSink,
};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::runtime_cpu_capacity::CpuCapacity;
use clearra_core_executor::{solve_symbolic_family, SymbolicFamilyLimits, SymbolicFamilyReport};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
    WorkerPolicy,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_rules::profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x};
use clearra_rules::profile::rule_profile::RuleProfile;
use clearra_supply::pattern_universe::materialized_pattern_universe::UniformCompactPatternSource;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;
use clearra_supply::QueueObservationPolicy;
use serde_json::{json, Value};

#[path = "support/symbolic_archive.rs"]
mod symbolic_archive;

const USAGE: &str = "usage: symbolic_family compile|run --height 1..6 --queue PATTERN [options]\n\
  --board HEX                  initial 10-column board; default 0\n\
  --rule NAME                  srs, srs-plus, srs-x, jstris-180, no-kick\n\
                               default srs-plus\n\
  --hold | --no-hold            default disabled unless --initial-hold is set\n\
  --initial-hold I|O|T|S|Z|J|L  enable Hold with this held piece\n\
  --source-draws N              explicit finite source window\n\
                               default full pattern length\n\
  --max-geometry-nodes N        explicit geometry bound; 0 means unlimited\n\
  --max-set-nodes N             explicit ZDD bound; 0 means unlimited\n\
  --max-predicate-nodes N       explicit Boolean bound; 0 means unlimited\n\
  --max-work N                  explicit work bound per arena; 0 is unlimited\n\
  --max-memo-entries N          explicit memo bound; 0 means unlimited\n\
  --page-size N                 extract this many identities after run; default 0\n\
  --page-offset N               skip counted identity branches; default 0\n\
  --archive PATH                save shared CTK4 before requested pages\n\
  --help                       show this text without compiling or running\n\
\n\
All representation bounds default to 0. There is no time-limit option.\n\
Use --queue standard7bag for the actual fresh standard-bag source; its default\n\
draw count is the required locks minus the initial held piece. Literal queues\n\
such as IOT and expressions such as P7P7, P1P7P7, OP7P7 use the pattern parser.\n\
The solver reports unsupported source representations instead of enumerating\n\
queues or falling back to candidate verification. Counts in JSON are strings.";

struct Options {
    run: bool,
    height: u16,
    board: u64,
    queue: String,
    rule: String,
    hold: bool,
    initial_hold: Option<PieceKind>,
    source_draws: Option<usize>,
    limits: SymbolicFamilyLimits,
    page_size: usize,
    page_offset: u128,
    archive: Option<PathBuf>,
}

fn main() {
    let status = thread::Builder::new()
        .name("symbolic-family-example".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(main_on_product_stack)
        .expect("symbolic family diagnostic thread")
        .join()
        .expect("symbolic family diagnostic thread completed");
    std::process::exit(status);
}

fn main_on_product_stack() -> i32 {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{USAGE}");
        return 0;
    }
    let started = Instant::now();
    let mut report = json!({
        "schema": "clearra.symbolic-family.v1",
        "phase": "arguments",
        "complete": false,
        "search_complete": false,
        "implicit_deadline_seconds": null,
        "candidate_fallback": false,
        "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
    });
    let outcome =
        parse_options(args.into_iter()).and_then(|options| execute(&options, &mut report));
    report["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    report["process_memory"] = process_memory();
    if let Err(error) = &outcome {
        report["status"] = json!("error-or-incomplete");
        report["complete"] = json!(false);
        report["error"] = json!(error);
    }
    if let Err(error) = serde_json::to_writer_pretty(io::stdout().lock(), &report) {
        eprintln!("symbolic_family JSON output failed: {error}");
        return 1;
    }
    println!();
    i32::from(outcome.is_err())
}

fn parse_options(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let run = match args.next().as_deref() {
        Some("compile") => false,
        Some("run") => true,
        _ => return Err(USAGE.into()),
    };
    let mut options = Options {
        run,
        height: 0,
        board: 0,
        queue: String::new(),
        rule: "srs-plus".into(),
        hold: false,
        initial_hold: None,
        source_draws: None,
        limits: SymbolicFamilyLimits::default(),
        page_size: 0,
        page_offset: 0,
        archive: None,
    };
    let mut hold_selection = None;
    while let Some(option) = args.next() {
        match option.as_str() {
            "--height" => options.height = number(args.next(), &option)?,
            "--board" => {
                let value = argument(args.next(), &option)?.replace('_', "");
                let digits = value
                    .strip_prefix("0x")
                    .or_else(|| value.strip_prefix("0X"))
                    .unwrap_or(&value);
                options.board = u64::from_str_radix(digits, 16)
                    .map_err(|_| "--board requires a hexadecimal u64".to_owned())?;
            }
            "--queue" => options.queue = argument(args.next(), &option)?,
            "--rule" => options.rule = argument(args.next(), &option)?.to_ascii_lowercase(),
            "--hold" => hold_selection = Some(true),
            "--no-hold" => hold_selection = Some(false),
            "--initial-hold" => {
                let value = argument(args.next(), &option)?;
                let mut chars = value.chars();
                let piece = chars
                    .next()
                    .filter(|_| chars.next().is_none())
                    .ok_or("--initial-hold requires one tetromino kind")?;
                options.initial_hold = Some(
                    PieceKind::from_ascii(piece)
                        .map_err(|_| "--initial-hold requires I, O, T, S, Z, J or L")?,
                );
            }
            "--source-draws" => options.source_draws = Some(number(args.next(), &option)?),
            "--max-geometry-nodes" => {
                options.limits.max_geometry_nodes = number(args.next(), &option)?;
            }
            "--max-set-nodes" => {
                options.limits.max_set_nodes = number(args.next(), &option)?;
            }
            "--max-predicate-nodes" => {
                options.limits.max_predicate_nodes = number(args.next(), &option)?;
            }
            "--max-work" => options.limits.max_work_per_arena = number(args.next(), &option)?,
            "--max-memo-entries" => {
                options.limits.max_memo_entries = number(args.next(), &option)?;
            }
            "--page-size" => options.page_size = number(args.next(), &option)?,
            "--page-offset" => options.page_offset = number(args.next(), &option)?,
            "--archive" if options.archive.is_none() => {
                options.archive = Some(symbolic_archive::checked_output(&PathBuf::from(
                    argument(args.next(), &option)?,
                ))?);
            }
            _ => return Err(format!("unknown option {option}\n{USAGE}")),
        }
    }
    if !(1..=6).contains(&options.height) || options.queue.is_empty() {
        return Err("--height 1..6 and a nonempty --queue are required".into());
    }
    rule(&options.rule)?;
    if options.archive.is_some() && !options.run {
        return Err("--archive requires run mode".into());
    }
    if options.initial_hold.is_some() && hold_selection == Some(false) {
        return Err("--initial-hold conflicts with --no-hold".into());
    }
    options.hold = hold_selection.unwrap_or(options.initial_hold.is_some());
    if options.page_offset != 0 && options.page_size == 0 {
        return Err("--page-offset requires a positive --page-size".into());
    }
    if !options.run && (options.page_size != 0 || options.page_offset != 0) {
        return Err("identity pages require the explicit run command".into());
    }
    Ok(options)
}

fn argument(value: Option<String>, option: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("missing value for {option}"))
}

fn number<T: FromStr>(value: Option<String>, option: &str) -> Result<T, String> {
    argument(value, option)?
        .parse()
        .map_err(|_| format!("invalid nonnegative integer for {option}"))
}

fn rule(name: &str) -> Result<RuleProfile, String> {
    match name {
        "srs" => Ok(srs()),
        "srs-plus" | "srs+" => Ok(srs_plus()),
        "srs-x" => Ok(srs_x()),
        "jstris-180" => Ok(jstris_180()),
        "no-kick" => Ok(no_kick()),
        _ => Err(format!(
            "unsupported rule {name}; use one of the five documented profiles"
        )),
    }
}

fn query(options: &Options, hardware_limit: usize) -> Result<PcScenarioQuery, String> {
    let target = PcScenarioBoard::standard_10(options.height, options.board)
        .to_standard_target_frame(options.height as u8)
        .map_err(|error| format!("target board: {error:?}"))?;
    let locks = target.required_pieces();
    let (queue, draws) = if matches!(
        options.queue.to_ascii_lowercase().as_str(),
        "standard7bag" | "standard-7-bag"
    ) {
        let draws = options
            .source_draws
            .unwrap_or(locks.saturating_sub(usize::from(options.initial_hold.is_some())));
        (PcQueueInput::Standard7Bag, draws)
    } else {
        let expression = QueuePatternExpression::parse(&options.queue, 0)
            .map_err(|error| format!("queue expression: {error:?}"))?;
        let draws = options.source_draws.unwrap_or(expression.sequence_len());
        (PcQueueInput::pattern_expression(expression), draws)
    };
    let execution = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_worker_policy(WorkerPolicy::Auto)
        .with_worker_hardware_limit(hardware_limit)
        .with_use_all_logical_processors(true)
        .with_cpu_warmup(false)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false)
        .with_max_patterns(0)
        .with_max_nodes(0)
        .with_max_candidates(0)
        .with_max_frontier_states(0)
        .with_max_memory_mib(None);
    Ok(PcScenarioQuery::new(
        target.normalized_board().clone(),
        queue,
        PieceWindow::new(locks),
    )
    .with_rule(rule(&options.rule)?)
    .with_exact_pieces(Some(locks))
    .with_supply_window_size(SupplyWindowSize::new(draws))
    .with_min_remaining_queue(0)
    .with_hold_piece(options.initial_hold)
    .with_allow_hold(options.hold)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::unique())
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_retained_trace_limit(0)
    .with_execution_policy(execution))
}

fn execute(options: &Options, output: &mut Value) -> Result<(), String> {
    let capacity = CpuCapacity::current();
    output["mode"] = json!(if options.run { "run" } else { "compile" });
    output["request"] = json!({
        "height": options.height,
        "board_hex": format!("0x{:x}", options.board),
        "queue": options.queue,
        "rule": options.rule,
        "hold_enabled": options.hold,
        "initial_hold": options.initial_hold.map(PieceKind::as_ascii),
        "source_draws_override": options.source_draws,
    });
    output["limits"] = json!({
        "max_geometry_nodes": options.limits.max_geometry_nodes.to_string(),
        "max_set_nodes": options.limits.max_set_nodes.to_string(),
        "max_predicate_nodes": options.limits.max_predicate_nodes.to_string(),
        "max_work_per_arena": options.limits.max_work_per_arena.to_string(),
        "max_memo_entries": options.limits.max_memo_entries.to_string(),
        "zero_means": "unlimited",
    });
    output["cpu"] = json!({
        "request_policy": "auto-all-logical",
        "hardware_limit": capacity.hard_limit(),
        "recommended_parallelism": capacity.recommended_parallelism(),
        "affinity_limit": capacity.affinity_limit(),
        "expected_vcpus": capacity.expected_vcpus(),
        "parallel_feature": cfg!(feature = "parallel"),
        "scope": "request configuration; no claim of parallel symbolic execution",
    });
    output["supervision"] = json!({
        "supervised": std::env::var("CLEARRA_RUNTIME_SUPERVISED").ok(),
        "profile": std::env::var("CLEARRA_RUNTIME_PROFILE").ok(),
        "run_id": std::env::var("CLEARRA_RUNTIME_RUN_ID").ok(),
    });
    output["phase"] = json!("problem-compilation");
    let compilation_started = Instant::now();
    let request = query(options, capacity.hard_limit())?;
    let problem = ProblemCompiler::compile_scenario_pc(&request)
        .map_err(|error| format!("problem compilation: {error:?}"))?;
    output["compilation_seconds"] = json!(compilation_started.elapsed().as_secs_f64());
    output["compiled"] = describe_problem(&problem)?;
    eprintln!("symbolic_family compiled={}", output["compiled"]);
    if !options.run {
        output["phase"] = json!("compiled");
        output["status"] = json!("compiled-without-search");
        return Ok(());
    }

    output["phase"] = json!("symbolic-search");
    let search_started = Instant::now();
    let progress = Arc::new(DiagnosticProgress {
        started: search_started,
        last: Mutex::new(("", search_started)),
    });
    let control =
        ExecutionControl::new(ExecutionCancellationToken::new()).with_progress_sink(progress);
    let outcome = solve_symbolic_family(&problem, options.limits, &control);
    output["search_seconds"] = json!(search_started.elapsed().as_secs_f64());
    let mut result = outcome.map_err(|error| error.to_string())?;
    output["search_complete"] = json!(true);
    output["result"] = describe_result(result.report());
    if let Some(path) = &options.archive {
        output["phase"] = json!("ctk4-archive");
        output["archive"] = symbolic_archive::persist(
            &mut result,
            path,
            &json!({"request": output["request"], "compiled": output["compiled"]}),
            &control,
        )?;
    }
    if options.page_size != 0 {
        output["phase"] = json!("identity-page");
        let started = Instant::now();
        let identities = result
            .identity_page(options.page_offset, options.page_size)
            .map_err(|error| format!("identity page: {error}"))?;
        let mut page = Vec::new();
        page.try_reserve_exact(identities.len())
            .map_err(|_| "identity output allocation failed")?;
        for identity in identities {
            let source_count = result
                .source_count_for_identity(&identity)
                .map_err(|error| format!("identity source count: {error}"))?;
            let placements = (0..identity.placement_count())
                .map(|index| {
                    let placement = identity
                        .placement(index)
                        .expect("identity placement bounds");
                    json!({
                        "piece": placement.piece().as_ascii(),
                        "original_cells_hex": format!("0x{:x}", placement.cells_mask()),
                    })
                })
                .collect::<Vec<_>>();
            page.push(json!({
                "initial_board_hex": format!("0x{:x}", identity.initial_board_mask()),
                "placements": placements,
                "original_source_count": source_count.to_string(),
            }));
        }
        output["page"] = json!({
            "offset": options.page_offset.to_string(),
            "requested": options.page_size,
            "order": "catalog ZDD order; not legacy normalized digest order",
            "identities": page,
        });
        output["page_seconds"] = json!(started.elapsed().as_secs_f64());
    }
    output["phase"] = json!("complete");
    output["status"] = json!("completed");
    output["complete"] = json!(true);
    Ok(())
}

fn describe_problem(problem: &SearchProblem) -> Result<Value, String> {
    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .ok_or("missing source universe")?;
    if !source.complete()
        || !universe.complete()
        || universe.truncation_reason().is_some()
        || universe.total_possible_pattern_count() != universe.pattern_count() as u128
        || problem.count_policy() != PcCountPolicy::CountUnique
        || problem.objective() != ObjectivePolicy::unique()
        || problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle
        || problem.budget().max_patterns() != 0
        || problem.budget().max_seconds() != 0
    {
        return Err("compiled source/unique/no-deadline contract is incomplete or changed".into());
    }
    let compact = match universe.uniform_compact_source() {
        Some(UniformCompactPatternSource::FactorizedExpression(shape)) => json!({
            "kind": "factorized-expression",
            "full_length": shape.full_sequence_len(),
            "visible_length": shape.visible_sequence_len(),
            "segments": shape.atoms().map(|atom| json!({
                "choices": atom.choices().iter().map(|piece| piece.as_ascii()).collect::<String>(),
                "draws": atom.draw_count(),
                "variants": atom.variant_count().to_string(),
            })).collect::<Vec<_>>(),
        }),
        Some(UniformCompactPatternSource::Standard7Bag { sequence_len, .. }) => json!({
            "kind": "standard-7-bag", "visible_length": sequence_len,
        }),
        None => json!({
            "kind": "single-source-or-unsupported-multiple-source",
            "solver_support_checked_on_run": true,
        }),
    };
    Ok(json!({
        "problem_id": problem.problem_id().as_str(),
        "height": problem.compact_pc_geometry_height(),
        "initial_board_words": problem.initial_board().occupied_words().map(|word| format!("0x{word:x}")),
        "exact_locks": problem.exact_pieces(),
        "rule": problem.rule_profile_value().id().as_str(),
        "goal": problem.search_goal().as_str(),
        "source_kind": source.kind().as_str(),
        "source_draws": problem.supply().source_sequence_length(),
        "original_source_count": universe.pattern_count().to_string(),
        "total_possible_source_count": universe.total_possible_pattern_count().to_string(),
        "source_complete": true,
        "source_representation": compact,
        "hold_enabled": problem.supply().hold_enabled(),
        "initial_hold": problem.initial_hold().hold_piece().map(PieceKind::as_ascii),
        "projects_unplaced_lookahead": problem.supply().projects_unplaced_lookahead(),
        "projects_standard_bag_lookahead": problem.supply().projects_standard_bag_lookahead(),
        "source_retained_bytes": source.checked_retained_capacity_bytes().map(|bytes| bytes.to_string()),
        "max_patterns": "0",
        "max_seconds": "0",
    }))
}

fn describe_result(report: &SymbolicFamilyReport) -> Value {
    let ranks = report.ranks.iter().map(|rank| json!({
        "placed_pieces": rank.placed_pieces,
        "supply_states": rank.supply_states.to_string(),
        "joint_members": rank.joint_members.to_string(),
        "set_nodes_before_compaction": rank.set_nodes_before_compaction.to_string(),
        "set_nodes_after_compaction": rank.set_nodes_after_compaction.to_string(),
        "set_compaction_peak_bytes_estimate": rank.set_compaction_peak_bytes_estimate.to_string(),
        "predicate_nodes": rank.predicate_nodes.to_string(),
        "derivative_nodes": rank.derivative_nodes.to_string(),
        "movement_poses": rank.movement_poses.to_string(),
        "guarded_movement_edges": rank.guarded_movement_edges.to_string(),
        "movement_propagations": rank.movement_propagations.to_string(),
        "rotation_candidate_checks": rank.rotation_candidate_checks.to_string(),
        "rotation_candidates_pruned": rank.rotation_candidates_pruned.to_string(),
        "rotation_guard_no_prior": rank.rotation_guard_no_prior.to_string(),
        "rotation_guard_single_bit": rank.rotation_guard_single_bit.to_string(),
        "rotation_guard_single_mask": rank.rotation_guard_single_mask.to_string(),
        "rotation_guard_ordered": rank.rotation_guard_ordered.to_string(),
        "rotation_ordered_prior_terms": rank.rotation_ordered_prior_terms.to_string(),
        "legal_row_queries": rank.legal_row_queries.to_string(),
        "geometry_filters": rank.geometry_filters.to_string(),
        "compatibility_cache_bytes_estimate": rank.compatibility_cache_bytes_estimate.to_string(),
        "compatibility_cache_peak_bytes_estimate": rank.compatibility_cache_peak_bytes_estimate.to_string(),
        "compatibility_cache_releases": rank.compatibility_cache_releases,
        "compatibility_filter_misses": rank.compatibility_filter_misses.to_string(),
        "compatibility_guard_misses": rank.compatibility_guard_misses.to_string(),
        "movement_parallel": symbolic_archive::movement_parallel(&rank.movement_parallel),
    })).collect::<Vec<_>>();
    json!({
        "unique_solution_count": report.unique_solution_count.to_string(),
        "covered_source_count": report.covered_source_count.to_string(),
        "original_source_count": report.source_count.to_string(),
        "solution_source_pairs": report.solution_source_pairs.to_string(),
        "catalog_rows": report.catalog_rows.to_string(),
        "geometry_nodes": report.geometry_nodes.to_string(),
        "geometry_bytes_estimate": report.geometry_bytes_estimate.to_string(),
        "geometry_released_construction_bytes": report.geometry_released_construction_bytes.to_string(),
        "geometry_expanded_nodes": report.geometry_expanded_nodes.to_string(),
        "geometry_tablebase_pruned_states": report.geometry_tablebase_pruned_states.to_string(),
        "source_residual_states": report.source_residual_states.to_string(),
        "set_nodes_created": report.set_nodes_created.to_string(),
        "set_apply_misses": report.set_apply_misses.to_string(),
        "set_work_steps": report.set_work_steps.to_string(),
        "set_compactions": report.set_compactions.to_string(),
        "predicate_peak_nodes": report.predicate_peak_nodes.to_string(),
        "predicate_work_steps": report.predicate_work_steps.to_string(),
        "bridge_work_steps": report.bridge_work_steps.to_string(),
        "derivative_work_steps": report.derivative_work_steps.to_string(),
        "derivative_cache_misses": report.derivative_cache_misses.to_string(),
        "compatibility_work_steps": report.compatibility_work_steps.to_string(),
        "compatibility_filter_misses": report.compatibility_filter_misses.to_string(),
        "compatibility_guard_misses": report.compatibility_guard_misses.to_string(),
        "compatibility_saturated_unions": report.compatibility_saturated_unions.to_string(),
        "compatibility_union_remainder_calls": report.compatibility_union_remainder_calls.to_string(),
        "compatibility_union_remainder_created_nodes": report.compatibility_union_remainder_created_nodes.to_string(),
        "compatibility_union_remainder_work_steps": report.compatibility_union_remainder_work_steps.to_string(),
        "retained_compatibility_bytes_estimate": report.retained_compatibility_bytes_estimate.to_string(),
        "retained_set_bytes_estimate": report.retained_set_bytes_estimate.to_string(),
        "retained_predicate_bytes_estimate": report.retained_predicate_bytes_estimate.to_string(),
        "retained_derivative_bytes_estimate": report.retained_derivative_bytes_estimate.to_string(),
        "result_compaction_peak_bytes_estimate": report.result_compaction_peak_bytes_estimate.to_string(),
        "estimate_scope": "retained capacities exclude operation scratch and allocator metadata; not peak RSS",
        "ranks": ranks,
    })
}

struct DiagnosticProgress {
    started: Instant,
    last: Mutex<(&'static str, Instant)>,
}

impl ProgressSink for DiagnosticProgress {
    fn report(&self, progress: ExecutionProgress) {
        // The comparison harness owns the detailed atomic-context sampler.
        // These markers can occur per row; this simple CLI keeps only its
        // existing geometry/rank reports instead of logging each transition.
        if progress.stage.starts_with("symbolic_phase_")
            || progress.stage.starts_with("symbolic_context_")
            || progress.stage.starts_with("symbolic_work_")
            || progress.stage.starts_with("symbolic_storage_")
            || progress.stage.starts_with("symbolic_movement_")
        {
            return;
        }
        let mut last = self.last.lock().expect("diagnostic progress state");
        if last.0 != progress.stage || last.1.elapsed() >= Duration::from_secs(1) {
            eprintln!(
                "symbolic_family progress={}",
                json!({
                    "stage": progress.stage,
                    "completed": progress.completed.to_string(),
                    "total": progress.total.map(|total| total.to_string()),
                    "seconds": self.started.elapsed().as_secs_f64(),
                    "complete": false,
                    "process_memory": process_memory(),
                })
            );
            *last = (progress.stage, Instant::now());
        }
    }
}

fn process_memory() -> Value {
    #[cfg(target_os = "linux")]
    {
        let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
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
        json!({
            "rss_bytes": bytes("VmRSS:"),
            "rss_high_water_bytes": bytes("VmHWM:"),
            "scope": "Linux kernel process RSS; not aggregate descendant memory",
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        json!({"available": false, "scope": "process RSS unavailable on this platform"})
    }
}
