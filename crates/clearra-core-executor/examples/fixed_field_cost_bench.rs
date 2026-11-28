//! Bounded diagnostics for fixed-field PC route and representation boundaries.
//! Build with parallel,local-search-ab,wasm-stage-profiling, then run under the
//! repository's benchmark-search supervisor. Each process executes one fixed
//! all-I queue, with Hold off, and requests cooperative cancellation after at
//! most 20 seconds. The supervisor remains the hard time and memory boundary.
//! Preparation, execution and cleanup share one budgeted compute owner; the
//! requesting thread only admits the request, waits and prints its JSON.
//! JSON is diagnostic evidence, not a count/probability or timing-adoption proof.

#[cfg(not(all(
    feature = "parallel",
    feature = "local-search-ab",
    feature = "wasm-stage-profiling",
    not(target_family = "wasm")
)))]
fn main() {
    eprintln!(
        "fixed_field_cost_bench requires native features parallel,local-search-ab,wasm-stage-profiling"
    );
    std::process::exit(2);
}

#[cfg(all(
    feature = "parallel",
    feature = "local-search-ab",
    feature = "wasm-stage-profiling",
    not(target_family = "wasm")
))]
fn main() {
    if let Err(error) = run::main() {
        eprintln!("fixed_field_cost_bench: {error}");
        std::process::exit(1);
    }
}

#[cfg(all(
    feature = "parallel",
    feature = "local-search-ab",
    feature = "wasm-stage-profiling",
    not(target_family = "wasm")
))]
mod run {
    use std::{
        fs::File,
        io::Read,
        panic::{catch_unwind, AssertUnwindSafe},
        path::Path,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc, Mutex,
        },
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };

    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        parallel_work::{try_for_each_chunk, ParallelWorkError},
        piece::piece_kind::PieceKind,
        runtime_cpu_capacity::CpuCapacity,
    };
    use clearra_core_executor::{
        CoreExecutionResult, ExecutorSearchProfileSession, ExecutorSearchProfileStage,
        WasmCpuSearchBackend, WasmCpuSearchError,
    };
    use clearra_pc_graph::request::{
        PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, WorkerPolicy,
    };
    use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
    use clearra_rules::profile::builtin_rules::srs_plus;
    use clearra_supply::queue::fixed_sequence::FixedSequence;
    use serde_json::{json, Map, Value};
    use sha2::{Digest, Sha256};

    const MAX_CANCEL_MS: u64 = 20_000;
    const DEFAULT_CANCEL_MS: u64 = 5_000;
    const USAGE: &str = "fixed_field_cost_bench [--case h5-i10|h6-i11|h6-i12|h7-i14|h8-i15|h8-i16|h9-i18] [--cancel-after-ms 1..20000]\nDefaults: --case h5-i10 --cancel-after-ms 5000. Requires the benchmark-search supervisor.";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Fixture {
        H5I10,
        H6I11,
        H6I12,
        H7I14,
        H8I15,
        H8I16,
        H9I18,
    }

    impl Fixture {
        const ALL: [Self; 7] = [
            Self::H5I10,
            Self::H6I11,
            Self::H6I12,
            Self::H7I14,
            Self::H8I15,
            Self::H8I16,
            Self::H9I18,
        ];

        fn name(self) -> &'static str {
            match self {
                Self::H5I10 => "h5-i10",
                Self::H6I11 => "h6-i11",
                Self::H6I12 => "h6-i12",
                Self::H7I14 => "h7-i14",
                Self::H8I15 => "h8-i15",
                Self::H8I16 => "h8-i16",
                Self::H9I18 => "h9-i18",
            }
        }

        fn parse(value: &str) -> Result<Self, String> {
            Self::ALL
                .into_iter()
                .find(|fixture| fixture.name() == value)
                .ok_or_else(|| format!("unknown case {value:?}; {USAGE}"))
        }

        fn height(self) -> u16 {
            match self {
                Self::H5I10 => 5,
                Self::H6I11 | Self::H6I12 => 6,
                Self::H7I14 => 7,
                Self::H8I15 | Self::H8I16 => 8,
                Self::H9I18 => 9,
            }
        }

        fn pieces(self) -> usize {
            match self {
                Self::H5I10 => 10,
                Self::H6I11 => 11,
                Self::H6I12 => 12,
                Self::H7I14 => 14,
                Self::H8I15 => 15,
                Self::H8I16 => 16,
                Self::H9I18 => 18,
            }
        }

        fn top_extra(self) -> bool {
            matches!(self, Self::H6I11 | Self::H8I15)
        }

        fn masks(self) -> ([u64; 4], [u64; 4]) {
            let mut initial = [0; 4];
            let mut required = [0; 4];
            for row in 0..usize::from(self.height()) {
                for x in 0..10 {
                    let cell = row * 10 + x;
                    let occupied = x < 2
                        || (self.top_extra()
                            && row + 1 == usize::from(self.height())
                            && (2..6).contains(&x));
                    let words = if occupied {
                        &mut initial
                    } else {
                        &mut required
                    };
                    words[cell / 64] |= 1_u64 << (cell % 64);
                }
            }
            (initial, required)
        }

        fn validate(self) -> Result<(), String> {
            let (initial, required) = self.masks();
            let area = usize::from(self.height()) * 10;
            if required
                .iter()
                .map(|word| word.count_ones() as usize)
                .sum::<usize>()
                != self.pieces() * 4
            {
                return Err("fixture target area differs from exact all-I queue".into());
            }
            for cell in 0..256 {
                let bit = 1_u64 << (cell % 64);
                let occupied = initial[cell / 64] & bit != 0;
                let target = required[cell / 64] & bit != 0;
                if occupied && target || (occupied || target) != (cell < area) {
                    return Err("fixture masks do not partition its declared frame".into());
                }
            }
            for row in 0..usize::from(self.height()) {
                if (0..10).all(|x| {
                    let cell = row * 10 + x;
                    initial[cell / 64] & (1_u64 << (cell % 64)) != 0
                }) {
                    return Err("fixture contains an initially complete row".into());
                }
            }
            Ok(())
        }

        fn report(self) -> Value {
            let (initial, required) = self.masks();
            json!({
                "width": 10, "height": self.height(), "mino_count": self.pieces(),
                "initial_cell_count": initial.iter().map(|w| w.count_ones()).sum::<u32>(),
                "required_cell_count": self.pieces() * 4,
                "initial_words_hex_lsw_first": initial.map(|w| format!("{w:016x}")),
                "required_words_hex_lsw_first": required.map(|w| format!("{w:016x}")),
                "cell_index": "y * 10 + x; y=0 is bottom; x=0 is left",
                "initial_layout": "x=0,1 in every row; optionally x=2,3,4,5 in the top row",
                "top_row_extra_initial_cells": self.top_extra(),
                "goal": "clear-to-empty", "rule": "srs+", "hold": false,
                "queue": "I".repeat(self.pieces()), "queue_count": 1,
                "exact_pieces": self.pieces(), "count_policy": "unique",
                "output_policy": "summary", "solution_probability_policy": "omit",
                "retained_trace_limit": 0,
            })
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Options {
        fixture: Fixture,
        cancel_ms: u64,
    }

    fn parse_cancel_ms(value: &str) -> Result<u64, String> {
        (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse::<u64>().ok())
            .flatten()
            .filter(|millis| (1..=MAX_CANCEL_MS).contains(millis))
            .ok_or_else(|| {
                format!("--cancel-after-ms must be an integer from 1 to {MAX_CANCEL_MS}")
            })
    }

    fn parse_args(args: &[String]) -> Result<Option<Options>, String> {
        if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
            return Ok(None);
        }
        let mut fixture = None;
        let mut cancel_ms = None;
        let mut index = 0;
        while index < args.len() {
            let flag = args[index].as_str();
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("missing value for {flag}; {USAGE}"))?;
            match flag {
                "--case" if fixture.is_none() => fixture = Some(Fixture::parse(value)?),
                "--cancel-after-ms" if cancel_ms.is_none() => {
                    cancel_ms = Some(parse_cancel_ms(value)?);
                }
                _ => return Err(format!("unknown or duplicate option {flag:?}; {USAGE}")),
            }
            index += 2;
        }
        Ok(Some(Options {
            fixture: fixture.unwrap_or(Fixture::H5I10),
            cancel_ms: cancel_ms.unwrap_or(DEFAULT_CANCEL_MS),
        }))
    }

    // The local feature exposes experimental switches. This diagnostic admits
    // the ordinary product defaults only, and records the values it checked.
    fn product_configuration() -> Result<Value, String> {
        let mut values = Map::new();
        for (key, expected) in [
            ("CLEARRA_PLACEMENT_DEPENDENCY", "0"),
            ("CLEARRA_STRUCTURAL_ORDER", "0"),
            ("CLEARRA_STATIC_INFLUENCE", "0"),
            ("CLEARRA_TRANSLATION_INTERVAL", "0"),
            ("CLEARRA_MOVEMENT_CLEAR_CERTIFICATES", "0"),
            ("CLEARRA_INCREMENTAL_DOMAIN", "0"),
            ("CLEARRA_MINIMUM_DOMAIN_CUTOFF", "0"),
            ("CLEARRA_MINIMUM_DOMAIN_WORK", "0"),
            ("CLEARRA_ROW_STACK_GEOMETRY", "0"),
            ("CLEARRA_APDP_PARENT_RECHECK", "0"),
            ("CLEARRA_FIRST_KICK_INTERVAL", "0"),
            ("CLEARRA_ORDERED_KICK_INTERVAL", "0"),
            ("CLEARRA_ORDERED_KICK_GUARD_REUSE", "0"),
            ("CLEARRA_GEOMETRIC_EDGE_REUSE", "0"),
            ("CLEARRA_ILC_LAZY_FRAME", "0"),
            ("CLEARRA_NO_ENTRY_COMPONENTS", "0"),
            ("CLEARRA_SAME_TILE_PARENT_REUSE", "0"),
            ("CLEARRA_CONDITIONAL_ORDER", "0"),
            ("CLEARRA_TERMINAL_IDENTITY_MOVE", "0"),
            ("CLEARRA_STRICT_PREFIX", "1"),
            ("CLEARRA_REACHABILITY_VISITED_FIRST", "1"),
            ("CLEARRA_STANDARD_BAG_GLOBAL_SATURATION", "1"),
            ("CLEARRA_PHYSICAL_PREFIX_PRUNING", "1"),
            ("CLEARRA_CANDIDATE_ILC_PRUNING", "1"),
            ("CLEARRA_STANDARD_BAG_BOOLEAN_MEMBERSHIP", "1"),
            ("CLEARRA_STANDARD_BAG_SHARED_SATURATION", "1"),
        ] {
            let observed = match std::env::var(key) {
                Ok(value) if value == expected => Some(value),
                Ok(_) | Err(std::env::VarError::NotUnicode(_)) => {
                    return Err(format!(
                        "{key} must be absent or {expected} for this product diagnostic"
                    ));
                }
                Err(std::env::VarError::NotPresent) => None,
            };
            values.insert(
                key.into(),
                json!({ "override": observed, "product_value": expected }),
            );
        }
        for key in [
            "CLEARRA_STANDARD_BAG_MEMO",
            "CLEARRA_STANDARD_BAG_UNION_MEMO",
            "CLEARRA_EXTENDED_STANDARD_BAG_COVERAGE",
            "CLEARRA_EXTENDED_FULL_DRAW_MULTISET",
            "CLEARRA_SEARCH_STAGE_PROFILE",
            "CLEARRA_STRICT_CATALOG_GUARDS",
            "CLEARRA_STANDARD_BAG_ABSORB",
            "CLEARRA_STRICT_LANGUAGE_FRONTIER",
            "CLEARRA_STRICT_SUBSUMPTION",
        ] {
            values.insert(key.into(), json!({ "override": std::env::var(key).ok() }));
        }
        Ok(Value::Object(values))
    }

    pub(super) fn main() -> Result<(), String> {
        let args: Vec<_> = std::env::args().skip(1).collect();
        let Some(options) = parse_args(&args)? else {
            println!("{USAGE}");
            return Ok(());
        };
        if std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
            || std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search")
        {
            return Err(
                "fixed-field diagnostics require the supervised benchmark-search profile".into(),
            );
        }
        let capacity = CpuCapacity::current();
        let control =
            ExecutionControl::default().with_parallelism_if_unbound(capacity.hard_limit());
        let requester = thread::current().id();
        let response = Mutex::new(None);
        // Match the public AppContext's compute-budget ownership boundary.
        // This thread has no compute permit and never evaluates the chunk.
        // The owner keeps this same budget through compile, backend and cleanup.
        let handoff = try_for_each_chunk(
            &control,
            1,
            1,
            || Ok::<(), String>(()),
            |_, _| {
                if thread::current().id() == requester {
                    return Err("diagnostic compute owner ran on the requesting thread".into());
                }
                let completed = run_owned_diagnostic(options, capacity, &control, requester);
                *response
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(completed);
                Ok(())
            },
        );
        let completed = response
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ok_or_else(|| format!("diagnostic compute owner returned no report: {handoff:?}"))?;
        let (mut report, result) = completed?;
        let result = match handoff {
            Ok(()) => {
                report["execution_topology"]["handoff_status"] = json!("completed");
                result
            }
            Err(ParallelWorkError::Cancelled) => {
                // The owner has finished its profiler and joined its timer.
                // The helper rechecks cancellation after joining the owner;
                // retain that owner's completed/cancelled diagnostic report.
                report["execution_topology"]["handoff_status"] =
                    json!("cancelled-after-owner-return");
                result
            }
            Err(error) => {
                let error = format!("diagnostic compute-owner handoff failed: {error:?}");
                report["execution_topology"]["handoff_status"] = json!("failed");
                report["search_status"] = json!("failed");
                report["execution_error"] = json!(error);
                report["completed_backend_fields"] = Value::Null;
                Err(error)
            }
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
        result
    }

    fn run_owned_diagnostic(
        options: Options,
        capacity: CpuCapacity,
        control: &ExecutionControl,
        requester: thread::ThreadId,
    ) -> Result<(Value, Result<(), String>), String> {
        options.fixture.validate()?;
        let configuration = product_configuration()?;
        let binary = std::env::current_exe().map_err(|error| error.to_string())?;
        let binary_sha = file_sha(&binary)?;
        let owner = thread::current();
        let mut report = json!({
            "schema": "clearra.fixed-field-cost-diagnostic.v1",
            "diagnostic_only": true, "search_result_validated": false,
            "solution_parity_checked": false, "timing_adoption_authority": false,
            "search_status": "not-started", "execution_error": null,
            "case": options.fixture.name(), "fixture": options.fixture.report(),
            "binary_sha256": binary_sha,
            "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
            "supervisor_run_id": std::env::var("CLEARRA_RUNTIME_RUN_ID").ok(),
            "backend_contract": "native Rust WasmCpuSearchBackend; ordinary product defaults; no C core; no tablebase",
            "execution_topology": {
                "contract": "equivalent compute-budget owner handoff for backend diagnostic",
                "scope": "uses the public core-domain handoff helper; does not execute the complete AppContext API",
                "requester_role": "request admission, owner dispatch/wait and JSON output; no compile, search or result cleanup",
                "requester_thread_id": format!("{requester:?}"),
                "owner_thread_id": format!("{:?}", owner.id()),
                "owner_thread_name": owner.name(),
                "owner_is_requester": owner.id() == requester,
                "compute_budget_parallelism": control.parallelism(),
                "nested_work": "backend stages retain the same compute budget and existing worker reuse or coordinator permit suspension/reacquisition",
                "handoff_status": "pending",
            },
            "process_configuration": configuration,
            "cancel_after_ms": options.cancel_ms,
            "cancellation_requested": false,
            "cancellation_scope": "timer starts immediately before memory observation and backend execution; cooperative cancellation is not a hard deadline; the supervisor supplies the hard bound",
            "cpu": {
                "available_parallelism": std::thread::available_parallelism().map(usize::from).ok(),
                "recommended_parallelism": capacity.recommended_parallelism(),
                "affinity_limit": capacity.affinity_limit(), "expected_vcpus": capacity.expected_vcpus(),
                "hard_limit": capacity.hard_limit(), "worker_policy": "auto",
                "use_all_logical_processors": true,
            },
            "timing_ms": { "problem_compile": null, "search": null, "cleanup": null },
            "completed_backend_fields": null,
            "memory_scope": "Linux RSS is process-wide; logical owner snapshots cover only instrumented dispatch paths and may be stale; missing or zero logical-owner values do not imply zero allocations",
            "result_scope": "only a completed backend call exports selected summary fields, without checking their count, probability or coverage correctness; cancelled and failed calls export no partial result",
        });
        let result = execute_diagnostic(options, capacity, control, &mut report);
        if let Err(error) = &result {
            report["search_status"] = json!("failed");
            report["execution_error"] = json!(error);
            report["completed_backend_fields"] = Value::Null;
        }
        Ok((report, result))
    }

    fn execute_diagnostic(
        options: Options,
        capacity: CpuCapacity,
        control: &ExecutionControl,
        report: &mut Value,
    ) -> Result<(), String> {
        let fixture = options.fixture;
        let compile_started = Instant::now();
        let board = PcScenarioBoard::standard_10_from_words(fixture.height(), fixture.masks().0)
            .map_err(|error| format!("invalid diagnostic board: {error:?}"))?;
        let policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_worker_policy(WorkerPolicy::Auto)
            .with_worker_hardware_limit(capacity.hard_limit())
            .with_use_all_logical_processors(true)
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false)
            .with_tablebase_requested(false);
        let query = PcScenarioQuery::new(
            board,
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; fixture.pieces()])),
            PieceWindow::new(fixture.pieces()),
        )
        .with_rule(srs_plus())
        .with_allow_hold(false)
        .with_exact_pieces(Some(fixture.pieces()))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
        .with_retained_trace_limit(0)
        .with_execution_policy(policy);
        let compiled = ProblemCompiler::compile_scenario_pc(&query);
        report["timing_ms"]["problem_compile"] = json!(millis(compile_started.elapsed()));
        let problem = compiled
            .map_err(|error| format!("problem compilation failed: {error:?}"))?
            .with_output_policy(SearchOutputPolicy::Summary);
        let patterns = problem
            .piece_source()
            .materialized_universe()
            .map(|universe| universe.pattern_count());
        if patterns != Some(1) {
            return Err(format!(
                "fixed all-I fixture did not compile to exactly one queue: {patterns:?}"
            ));
        }
        report["cpu"]["compiled_requested_workers"] = json!(problem.backend_policy().workers());
        report["compiled_materialized_pattern_count"] = json!(patterns);
        report["dispatch_predicates"] = json!({
            "pc_placement_dependency_eligible": problem.pc_placement_dependency_eligible(),
            "height_fits_compact_pc": fixture.height() <= 6,
            "extended_geometry_required_cells_fit_u64": fixture.pieces() * 4 <= 64,
            "extended_physical_dense_subset_piece_limit_met": fixture.pieces() <= 15,
            "scope": "eligibility and storage thresholds only; a fixed-queue witness fast path can avoid graph work; actual execution is described by stages and completed backend fields",
        });
        let profile = ExecutorSearchProfileSession::start()
            .map_err(|error| format!("cannot start search stage profile: {error:?}"))?;
        let timer = match DiagnosticCancellation::arm(control, options.cancel_ms) {
            Ok(timer) => timer,
            Err(error) => {
                let (caller, workers) = profile.finish_partitioned();
                report["stage_profile"] = stage_report(caller, workers);
                return Err(error);
            }
        };
        let observation_started = Instant::now();
        let mut backend_started = None;
        let mut backend_finished = None;
        let mut outcome = None;
        let execute = || {
            backend_started = Some(Instant::now());
            // Finishing the observation and timer also on a backend panic keeps
            // this one-search diagnostic from abandoning its observer threads.
            outcome = Some(catch_unwind(AssertUnwindSafe(|| {
                WasmCpuSearchBackend::execute_with_control(&problem, control)
            })));
            backend_finished = Some(Instant::now());
        };
        #[cfg(target_os = "linux")]
        let memory = clearra_core_executor::backend::observe_native_pc_memory(execute)
            .map(|(_, memory)| memory)
            .map_err(|error| format!("native memory observation failed: {error}"));
        #[cfg(not(target_os = "linux"))]
        let memory: Result<Value, String> = {
            let mut execute = execute;
            execute();
            Ok(json!({ "available": false, "reason": "native memory observer requires Linux" }))
        };
        let timer_result = timer.finish();
        let (caller, workers) = profile.finish_partitioned();
        report["stage_profile"] = stage_report(caller, workers);
        if let (Some(started), Some(finished)) = (backend_started, backend_finished) {
            report["timing_ms"]["observer_startup"] =
                json!(millis(started.duration_since(observation_started)));
            report["timing_ms"]["search"] = json!(millis(finished.duration_since(started)));
        }
        let mut errors = Vec::new();
        match memory {
            Ok(memory) => report["product_memory_profile"] = memory,
            Err(error) => errors.push(error),
        }
        match timer_result {
            Ok(fired) => report["cancellation_requested"] = json!(fired),
            Err(error) => errors.push(error),
        }
        let backend_status = match &outcome {
            Some(Ok(Ok(_))) => "completed",
            Some(Ok(Err(WasmCpuSearchError::Cancelled))) => "cancelled",
            Some(Ok(Err(error))) => {
                errors.push(format!("backend execution failed: {error:?}"));
                "failed"
            }
            Some(Err(_)) => {
                errors.push("backend execution panicked; see stderr for panic details".into());
                "failed"
            }
            None => {
                errors.push("backend execution did not start".into());
                "not-started"
            }
        };
        report["backend_status"] = json!(backend_status);
        report["search_status"] = json!(backend_status);
        if errors.is_empty() {
            if let Some(Ok(Ok(result))) = &outcome {
                report["completed_backend_fields"] = summary_fields(result);
            }
        }
        drop(outcome);
        drop(problem);
        if let Some(finished) = backend_finished {
            report["timing_ms"]["cleanup"] = json!(millis(finished.elapsed()));
        }
        report["timing_ms"]["scope"] = json!("search is backend call wall time, including its worker joins; cleanup includes memory-observer shutdown, cancellation-timer join, profile finishing, selected summary extraction and result/problem drops; serialization is excluded");
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn summary_fields(result: &CoreExecutionResult) -> Value {
        let mut fields = Map::new();
        for key in [
            "backend_selected",
            "actual_backend",
            "board_storage",
            "board_height",
            "workers_requested",
            "workers_used",
            "cpu_parallel_execution",
            "cpu_parallel_decision_reason",
            "cpu_parallel_task_granularity",
            "cpu_parallel_active_workers",
            "cpu_parallel_minimum_worker_candidates",
            "cpu_parallel_maximum_worker_candidates",
            "geometry_skeleton_count",
            "concrete_realization_count",
            "instantiated_realization_count",
            "geometry_candidate_family_count",
            "geometry_current_retained_bytes",
            "geometry_parallel_family_dispatch_bytes",
            "packing_candidate_count",
            "packing_candidate_domain",
            "packing_candidate_digest",
            "packing_candidate_set_digest",
            "geometry_domain_pruned_states",
            "geometry_hall_pruned_states",
            "geometry_column_pruned_states",
            "geometry_component_compositions",
            "geometry_component_pruned_states",
            "physical_prefix_pruning_mode",
            "physical_prefix_dependency_checks",
            "physical_prefix_first_clear_checks",
            "physical_prefix_negative_prefixes",
            "physical_prefix_certified_candidate_count",
            "physical_prefix_skipped_candidate_count",
            "physical_prefix_emitted_candidate_count",
            "candidate_ilc_checks",
            "candidate_ilc_reused_prefix_certificates",
            "searched_nodes",
            "searched_geometry_nodes",
            "searched_build_nodes",
            "geometry_searched_nodes",
            "buildup_searched_nodes",
            "peak_frontier_states",
            "resource_peak_frontier_states",
            "peak_build_order_nodes",
            "total_build_order_nodes",
            "total_reachability_states",
            "coverage_product_edge_checks",
            "coverage_product_states",
            "coverage_product_words",
            "resource_peak_cpu_bytes",
            "resource_peak_cpu_bytes_kind",
            "resource_truncated",
            "resource_truncation_reason",
            "resource_node_budget_consumed",
            "count_complete",
            "probability_complete",
            "objective_complete",
            "objective_search_complete",
            "unique_solution_count",
            "normalized_unique_solution_count",
            "materialized_pattern_count",
            "covered_pattern_count",
            "coverage_pattern_count",
            "solution_keys_complete",
            "solution_keys_materialized_count",
            "build_variant_count",
            "build_variant_count_exact",
            "buildup_executed",
            "buildability_verified",
        ] {
            if let Some(value) = result.unique_field(key) {
                fields.insert(key.into(), json!(value));
            }
        }
        Value::Object(fields)
    }

    fn stage_report(
        caller: Vec<ExecutorSearchProfileStage>,
        workers: Vec<ExecutorSearchProfileStage>,
    ) -> Value {
        let stages: Vec<_> = caller
            .iter()
            .zip(&workers)
            .filter_map(|(caller, workers)| {
                if caller.invocation_count == 0 && workers.invocation_count == 0 {
                    return None;
                }
                Some(json!({
                    "stage": caller.name,
                    "caller_ns": caller.duration_ns, "pool_workers_ns": workers.duration_ns,
                    "caller_measured_ns": caller.measured_duration_ns,
                    "pool_workers_measured_ns": workers.measured_duration_ns,
                    "caller_measured_samples": caller.sample_count,
                    "pool_worker_measured_samples": workers.sample_count,
                    "caller_invocations": caller.invocation_count,
                    "pool_worker_invocations": workers.invocation_count,
                    "caller_work_items": caller.work_item_count,
                    "pool_worker_work_items": workers.work_item_count,
                }))
            })
            .collect();
        json!({
            "scope": "submitting-thread and pool-worker accumulated stage times; spans may nest; do not sum as wall time",
            "caller_identity": "caller_* is the budgeted compute owner that submitted the backend work, not the requesting thread; the profile starts and finishes inside that owner",
            "sampling": "*_ns and *_invocations include sample expansion; *_measured_ns and *_measured_samples contain only observed spans; neither is end-to-end wall time",
            "incomplete_call_scope": "on cancellation or failure, only spans published by the backend are present; an unfinished operation need not publish a full stage duration",
            "stages": stages,
        })
    }

    fn millis(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1_000.0
    }

    fn file_sha(path: &Path) -> Result<String, String> {
        let mut file = File::open(path).map_err(|error| error.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    struct DiagnosticCancellation {
        stop: Option<mpsc::Sender<()>>,
        worker: Option<JoinHandle<()>>,
        fired: Arc<AtomicBool>,
    }

    impl DiagnosticCancellation {
        fn arm(control: &ExecutionControl, millis: u64) -> Result<Self, String> {
            if !(1..=MAX_CANCEL_MS).contains(&millis) {
                return Err("diagnostic cancellation deadline is outside its finite range".into());
            }
            let handle = control.cancellation.handle();
            let fired = Arc::new(AtomicBool::new(false));
            let observed = Arc::clone(&fired);
            let (stop, receive) = mpsc::channel();
            let worker = thread::Builder::new()
                .name("clearra-fixed-field-cancel".into())
                .spawn(move || {
                    if matches!(
                        receive.recv_timeout(Duration::from_millis(millis)),
                        Err(mpsc::RecvTimeoutError::Timeout)
                    ) {
                        observed.store(true, Ordering::Release);
                        handle.cancel();
                    }
                })
                .map_err(|error| format!("cannot start cancellation timer: {error}"))?;
            Ok(Self {
                stop: Some(stop),
                worker: Some(worker),
                fired,
            })
        }

        fn finish(mut self) -> Result<bool, String> {
            self.stop_and_join()?;
            Ok(self.fired.load(Ordering::Acquire))
        }

        fn stop_and_join(&mut self) -> Result<(), String> {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            if let Some(worker) = self.worker.take() {
                worker
                    .join()
                    .map_err(|_| "cancellation timer panicked".to_owned())?;
            }
            Ok(())
        }
    }

    impl Drop for DiagnosticCancellation {
        fn drop(&mut self) {
            let _ = self.stop_and_join();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn fixtures_partition_nonfull_initial_rows_at_expected_boundaries() {
            let expected = [
                (5, 10, 10),
                (6, 11, 16),
                (6, 12, 12),
                (7, 14, 14),
                (8, 15, 20),
                (8, 16, 16),
                (9, 18, 18),
            ];
            for (fixture, (height, pieces, initial_count)) in Fixture::ALL.into_iter().zip(expected)
            {
                fixture.validate().expect("well-formed fixed-field fixture");
                assert_eq!(fixture.height(), height);
                assert_eq!(fixture.pieces(), pieces);
                let (initial, required) = fixture.masks();
                assert_eq!(
                    initial.iter().map(|word| word.count_ones()).sum::<u32>(),
                    initial_count
                );
                assert_eq!(
                    required
                        .iter()
                        .map(|word| word.count_ones() as usize)
                        .sum::<usize>(),
                    4 * pieces
                );
                // Independently specify each row rather than reusing masks().
                for row in 0..usize::from(height) {
                    let expected_row = if matches!(fixture, Fixture::H6I11 | Fixture::H8I15)
                        && row + 1 == usize::from(height)
                    {
                        0b11_1111
                    } else {
                        0b11
                    };
                    let actual = (0..10).fold(0, |mask, x| {
                        let cell = row * 10 + x;
                        mask | (((initial[cell / 64] >> (cell % 64)) & 1) << x)
                    });
                    assert_eq!(actual, expected_row);
                    assert_ne!(actual, 0b11_1111_1111);
                }
            }
            assert_ne!(Fixture::H7I14.masks().1[1], 0);
            assert_eq!(Fixture::H8I16.pieces() * 4, 64);
            assert!(Fixture::H9I18.pieces() * 4 > 64);
        }

        #[test]
        fn cancellation_deadline_is_always_finite_and_strictly_parsed() {
            assert_eq!(parse_cancel_ms("1"), Ok(1));
            assert_eq!(parse_cancel_ms("20000"), Ok(20_000));
            for value in [
                "",
                "0",
                "20001",
                "-1",
                "+1",
                "1.0",
                " 1",
                "1 ",
                "18446744073709551616",
            ] {
                assert!(parse_cancel_ms(value).is_err(), "accepted {value:?}");
            }
            let args = |values: &[&str]| {
                values
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                parse_args(&[]),
                Ok(Some(Options {
                    fixture: Fixture::H5I10,
                    cancel_ms: DEFAULT_CANCEL_MS
                }))
            );
            assert!(
                parse_args(&args(&["--cancel-after-ms", "1", "--cancel-after-ms", "2"])).is_err()
            );
            assert!(parse_args(&args(&["--case", "h6-i12", "--case", "h6-i11"])).is_err());
            assert!(parse_args(&args(&["--case", "p7p7"])).is_err());
        }
    }
}
