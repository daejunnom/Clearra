//! Reproducible product-path oracle/see-7 benchmark. Run large fixtures only
//! through clearra-manage runtime's benchmark-search profile.

#[cfg(not(all(feature = "parallel", feature = "placement-dependency-pc")))]
fn main() {
    eprintln!("requires parallel,placement-dependency-pc features");
    std::process::exit(2);
}

#[cfg(all(feature = "parallel", feature = "placement-dependency-pc"))]
fn main() {
    if let Err(error) = run::main() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(all(feature = "parallel", feature = "placement-dependency-pc"))]
mod run {
    use std::{collections::BTreeMap, time::Instant};

    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, pc::pc_target::PcTarget,
        runtime_cpu_capacity::CpuCapacity,
    };
    use clearra_core_executor::WasmCpuSearchBackend;
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
        PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow,
        RequestedSearchBackend, SupplyWindowSize, WorkerPolicy,
    };
    use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
    use clearra_rules::profile::builtin_rules::{jstris_180, srs_plus};
    use clearra_supply::QueueObservationPolicy;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    pub fn main() -> Result<(), String> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let [fixture, observation] = args.as_slice() else {
            return Err(
                "usage: observation_dependency_bench p7|eight|p7p4|eleven oracle|visible-7".into(),
            );
        };
        let observation = match observation.as_str() {
            "oracle" => QueueObservationPolicy::FullQueueOracle,
            "visible-7" => QueueObservationPolicy::VisibleSeven,
            _ => return Err("unknown observation contract".into()),
        };
        if std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
            || std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search")
        {
            return Err("requires supervised benchmark-search profile".into());
        }
        let pruning_mode = match std::env::var("CLEARRA_STRICT_GLOBAL_KICK_PRUNE") {
            Ok(mode)
                if cfg!(feature = "local-search-ab")
                    && matches!(mode.as_str(), "0" | "1" | "2") =>
            {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_STRICT_GLOBAL_KICK_PRUNE requires local-search-ab and mode 0|1|2"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "2".into(),
            Err(error) => return Err(error.to_string()),
        };
        let capacity = CpuCapacity::current();
        let board_mode = match std::env::var("CLEARRA_OBSERVATION_BOARD_QUOTIENT") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_BOARD_QUOTIENT requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "1".into(),
            Err(error) => return Err(error.to_string()),
        };
        let kind_union_mode = match std::env::var("CLEARRA_OBSERVATION_KIND_UNION") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_KIND_UNION requires local-search-ab and mode 0|1".into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "1".into(),
            Err(error) => return Err(error.to_string()),
        };
        let source_prefix_mode = match std::env::var("CLEARRA_OBSERVATION_SOURCE_PREFIX") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_SOURCE_PREFIX requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "1".into(),
            Err(error) => return Err(error.to_string()),
        };
        let boundary_suffix_mode = match std::env::var("CLEARRA_OBSERVATION_BOUNDARY_SUFFIX") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_BOUNDARY_SUFFIX requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "1".into(),
            Err(error) => return Err(error.to_string()),
        };
        let edge_closure_mode = match std::env::var("CLEARRA_OBSERVATION_EDGE_CLOSURE") {
            Ok(mode)
                if cfg!(feature = "local-search-ab")
                    && matches!(mode.as_str(), "0" | "1" | "2") =>
            {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_EDGE_CLOSURE requires local-search-ab and mode 0|1|2"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let collect_full_mode = match std::env::var("CLEARRA_OBSERVATION_COLLECT_FULL") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_COLLECT_FULL requires local-search-ab and mode 0|1".into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "1".into(),
            Err(error) => return Err(error.to_string()),
        };
        let supply_multiset_mode = match std::env::var("CLEARRA_OBSERVATION_SUPPLY_MULTISET") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_SUPPLY_MULTISET requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let prefix_mask_mode = match std::env::var("CLEARRA_OBSERVATION_PREFIX_MASK") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_PREFIX_MASK requires local-search-ab and mode 0|1".into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let suffix_import_mode = match std::env::var("CLEARRA_OBSERVATION_SUFFIX_IMPORT") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_SUFFIX_IMPORT requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let pre_reveal_memo_mode = match std::env::var("CLEARRA_OBSERVATION_PRE_REVEAL_MEMO") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_PRE_REVEAL_MEMO requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let source_existence_mode = match std::env::var("CLEARRA_OBSERVATION_SOURCE_EXISTENCE") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_SOURCE_EXISTENCE requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let source_filter_mode = match std::env::var("CLEARRA_OBSERVATION_SOURCE_FILTER") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_SOURCE_FILTER requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let multiset_groups_mode = match std::env::var("CLEARRA_OBSERVATION_MULTISET_GROUPS") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_MULTISET_GROUPS requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let branch_bound_mode = match std::env::var("CLEARRA_OBSERVATION_BRANCH_BOUND") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_BRANCH_BOUND requires local-search-ab and mode 0|1".into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "1".into(),
            Err(error) => return Err(error.to_string()),
        };
        let prefix_cones_mode =
            match std::env::var("CLEARRA_OBSERVATION_PREFIX_CONES") {
                Ok(mode)
                    if cfg!(feature = "local-search-ab")
                        && matches!(mode.as_str(), "0" | "1" | "2" | "3") =>
                {
                    mode
                }
                Ok(_) => return Err(
                    "CLEARRA_OBSERVATION_PREFIX_CONES requires local-search-ab and mode 0|1|2|3"
                        .into(),
                ),
                Err(std::env::VarError::NotPresent) => "0".into(),
                Err(error) => return Err(error.to_string()),
            };
        let boundary_classes_mode = match std::env::var("CLEARRA_OBSERVATION_BOUNDARY_CLASSES") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_BOUNDARY_CLASSES requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let word_bitmap_mode = match std::env::var("CLEARRA_OBSERVATION_WORD_BITMAP") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_WORD_BITMAP requires local-search-ab and mode 0|1".into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let shared_producer_mode = match std::env::var("CLEARRA_OBSERVATION_SHARED_PRODUCER") {
            Ok(mode) if cfg!(feature = "local-search-ab") && matches!(mode.as_str(), "0" | "1") => {
                mode
            }
            Ok(_) => {
                return Err(
                    "CLEARRA_OBSERVATION_SHARED_PRODUCER requires local-search-ab and mode 0|1"
                        .into(),
                )
            }
            Err(std::env::VarError::NotPresent) => "0".into(),
            Err(error) => return Err(error.to_string()),
        };
        let execution = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_worker_policy(WorkerPolicy::Auto)
            .with_worker_hardware_limit(capacity.hard_limit())
            .with_use_all_logical_processors(true)
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false)
            .with_tablebase_requested(false);
        let binary = std::env::current_exe().map_err(|error| error.to_string())?;
        let binary_sha = format!(
            "{:x}",
            Sha256::digest(std::fs::read(binary).map_err(|error| error.to_string())?)
        );
        let compile_start = Instant::now();
        let problem = match fixture.as_str() {
            "p7" | "eight" | "eleven" => {
                let (height, initial, locks, draws) = if fixture == "p7" {
                    (4, 0x3c0f_03c0f, 6, 7)
                } else if fixture == "eight" {
                    (4, 0xff, 8, 9)
                } else {
                    (5, 0x3f, 11, 12)
                };
                let query = PcScenarioQuery::new(
                    PcScenarioBoard::standard_10(height, initial),
                    PcQueueInput::standard_7_bag(),
                    PieceWindow::new(locks),
                )
                .with_rule(srs_plus())
                .with_allow_hold(true)
                .with_exact_pieces(Some(locks))
                .with_supply_window_size(SupplyWindowSize::new(draws))
                .with_count_policy(PcCountPolicy::CountUnique)
                .with_queue_observation_policy(observation)
                .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
                .with_retained_trace_limit(0)
                .with_execution_policy(execution);
                ProblemCompiler::compile_scenario_pc(&query)
            }
            "p7p4" => ProblemCompiler::compile_opening_pc(
                &OpeningPcSearchQuery::new(PcTarget::four_lines())
                    .with_rule(jstris_180())
                    .with_queue(PcQueueInput::standard_7_bag())
                    .with_hold_policy(PcHoldPolicy::EnabledEmpty)
                    .with_objective(ObjectivePolicy::unique())
                    .with_queue_observation_policy(observation)
                    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
                    .with_execution_policy(execution),
            ),
            _ => return Err("unknown fixture".into()),
        }
        .map_err(|error| format!("compile: {error:?}"))?
        .with_output_policy(SearchOutputPolicy::CoverageSummary);
        let compile_seconds = compile_start.elapsed().as_secs_f64();
        if !problem.pc_placement_dependency_eligible() {
            return Err("fixture is outside dependency admission".into());
        }
        let start = Instant::now();
        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default());
        let execution_seconds = start.elapsed().as_secs_f64();
        let mut report = json!({
            "schema": "clearra.observation-dependency-benchmark.v18",
            "fixture": fixture, "policy": observation.keyword(),
            "binary_sha256": binary_sha,
            "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
            "strict_global_kick_prune_mode": pruning_mode,
            "board_quotient_mode": board_mode,
            "kind_union_mode": kind_union_mode,
            "source_prefix_mode": source_prefix_mode,
            "boundary_suffix_mode": boundary_suffix_mode,
            "edge_closure_mode": edge_closure_mode,
            "collect_full_mode": collect_full_mode,
            "supply_multiset_mode": supply_multiset_mode,
            "prefix_mask_mode": prefix_mask_mode,
            "suffix_import_mode": suffix_import_mode,
            "pre_reveal_memo_mode": pre_reveal_memo_mode,
            "source_existence_mode": source_existence_mode,
            "source_filter_mode": source_filter_mode,
            "multiset_groups_mode": multiset_groups_mode,
            "branch_bound_mode": branch_bound_mode,
            "prefix_cones_mode": prefix_cones_mode,
            "boundary_classes_mode": boundary_classes_mode,
            "word_bitmap_mode": word_bitmap_mode,
            "shared_producer_mode": shared_producer_mode,
            "source_sequence_length": problem.supply().source_sequence_length(),
            "projects_unplaced_lookahead": problem.supply().projects_unplaced_lookahead(),
            "projects_standard_bag_lookahead": problem.supply().projects_standard_bag_lookahead(),
            "local_search_ab": cfg!(feature = "local-search-ab"),
            "automatic_logical_processors": capacity.hard_limit(),
            "compile_seconds": compile_seconds, "execution_seconds": execution_seconds,
        });
        match result {
            Ok(result) => {
                let fields: BTreeMap<_, _> = result.summary_fields().into_iter().collect();
                let mut hash = Sha256::new();
                for word in result.coverage_pattern_words() {
                    hash.update(word.to_le_bytes());
                }
                let contract_valid = fields.get("coverage_semantics").map(String::as_str)
                    == Some(observation.coverage_semantics());
                let build_up_absent = result.usize_field("total_build_order_nodes") == Some(0);
                let complete = result.bool_field("probability_complete") == Some(true)
                    && result.bool_field("packing_count_complete") == Some(true)
                    && contract_valid
                    && build_up_absent;
                report["complete"] = json!(complete);
                report["observation_contract_valid"] = json!(contract_valid);
                report["build_up_absent"] = json!(build_up_absent);
                report["coverage_words_sha256"] = json!(format!("{:x}", hash.finalize()));
                report["retained_solution_identities"] =
                    json!(result.normalized_solution_identities().len());
                report["fields"] = json!(fields);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
                );
                if !contract_valid {
                    return Err("product result changed the requested observation contract".into());
                }
                if !build_up_absent {
                    return Err("product fixture used BuildUp".into());
                }
                if !complete {
                    return Err("product result is incomplete".into());
                }
                Ok(())
            }
            Err(error) => {
                report["complete"] = json!(false);
                report["error"] = json!(format!("{error:?}"));
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
                );
                Err("product execution failed".into())
            }
        }
    }
}
