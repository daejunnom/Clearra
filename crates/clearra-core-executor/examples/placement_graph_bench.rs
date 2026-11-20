//! Finite same-binary A/B entrypoint for the placement-condition experiment.
//! Build with --release --features parallel,local-search-ab. Large runs require
//! the repository supervisor; A/B selection is fixed by the process environment.

#[cfg(not(all(feature = "parallel", feature = "local-search-ab")))]
fn main() {
    eprintln!("placement_graph_bench requires features parallel,local-search-ab");
    std::process::exit(2);
}

#[cfg(all(feature = "parallel", feature = "local-search-ab"))]
fn main() {
    // The public CLI uses this stack contract too. Keeping the example small
    // avoids linking App/UI while executing the same Rust CPU core.
    let result = std::thread::Builder::new()
        .name("placement-graph-benchmark".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(run::main)
        .expect("benchmark product thread")
        .join()
        .expect("benchmark product thread completed");
    if let Err(error) = result {
        eprintln!("placement_graph_bench: {error}");
        std::process::exit(1);
    }
}

#[cfg(all(feature = "parallel", feature = "local-search-ab"))]
#[path = "placement_graph_bench/solution_coverage_compare.rs"]
mod solution_coverage_compare;
#[cfg(all(feature = "parallel", feature = "local-search-ab"))]
#[path = "placement_graph_bench/solution_coverage_spool.rs"]
mod solution_coverage_spool;

#[cfg(all(feature = "parallel", feature = "local-search-ab"))]
mod run {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs::{self, File, OpenOptions},
        io::{self, BufReader, BufWriter, Read, Write},
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
        time::Instant,
    };

    use super::solution_coverage_compare::{
        self, CoverageComparison, CoverageUniverse, CoverageWriter,
    };
    use super::solution_coverage_spool::{CoverageFragment, CoverageSpool, CoverageSpoolLimits};

    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        pc::pc_target::PcTarget,
        piece::piece_kind::PieceKind,
        runtime_cpu_capacity::CpuCapacity,
        solution::normalized_tiling_solution::{
            NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity,
        },
    };
    use clearra_core_executor::{
        backend::{CapturedSolutionCoverage, GeometryDomainExperimentTotals},
        CoreExecutionResult, WasmCpuSearchBackend,
    };
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
        PcScenarioBoard, PcScenarioQuery, PieceWindow, RequestedSearchBackend, WorkerPolicy,
    };
    use clearra_problem::{ProblemCompiler, SearchProblem};
    use clearra_rules::profile::builtin_rules::jstris_180;
    use clearra_supply::queue::fixed_sequence::FixedSequence;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};

    const LARGE_COUNT: usize = 456_459;
    const LARGE_HASH: &str = "cts1:2fcbc950acdffa31";
    const EIGHT_I_INITIAL: u64 = 0xc030_0c03;
    const EIGHT_I_CAPTURE_ERROR: &str =
        "--nonempty-eight-i supports global-only output; --coverage-slot is unavailable";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Fixture {
        OneI,
        NonemptyEightI,
        P7P4,
    }

    impl Fixture {
        const fn name(self) -> &'static str {
            match self {
                Self::OneI => "one-I-preflight",
                Self::NonemptyEightI => "nonempty-4L-eight-I-jstris180",
                Self::P7P4 => "empty-4L-P7P4-jstris180",
            }
        }

        const fn height(self) -> u16 {
            match self {
                Self::OneI => 1,
                Self::NonemptyEightI | Self::P7P4 => 4,
            }
        }

        const fn placed_pieces(self) -> usize {
            match self {
                Self::OneI => 1,
                Self::NonemptyEightI => 8,
                Self::P7P4 => 10,
            }
        }

        const fn fixed_board(self) -> Option<u64> {
            match self {
                Self::OneI => Some(0x3f0),
                Self::NonemptyEightI => Some(EIGHT_I_INITIAL),
                Self::P7P4 => None,
            }
        }

        fn input(self) -> Value {
            let (board, supply, supplied_pieces, original_supply_count, hold) = match self {
                Self::OneI => ("0x3f0", "I", 1, 1, "disabled"),
                Self::NonemptyEightI => ("0xc0300c03", "IIIIIIII", 8, 1, "disabled"),
                Self::P7P4 => ("0", "P7P4", 11, 4_233_600, "enabled-empty"),
            };
            json!({
                "board": board, "height": self.height(), "placed_pieces": self.placed_pieces(),
                "supply": supply, "supplied_pieces": supplied_pieces,
                "original_supply_count": original_supply_count, "hold": hold,
                "rule": "jstris-180", "count": "unique", "queue_knowledge": "full-future-oracle"
            })
        }

        fn capture_universe(self) -> Result<CoverageUniverse, String> {
            match self {
                Self::OneI => Ok(CoverageUniverse::SmallI),
                Self::P7P4 => Ok(CoverageUniverse::P7P4),
                Self::NonemptyEightI => Err(EIGHT_I_CAPTURE_ERROR.into()),
            }
        }
    }

    fn parse_fixture(args: &[String]) -> Result<(Fixture, Option<&str>), String> {
        match args {
            [] => Ok((Fixture::P7P4, None)),
            [flag] if flag == "--small" => Ok((Fixture::OneI, None)),
            [flag] if flag == "--nonempty-eight-i" => Ok((Fixture::NonemptyEightI, None)),
            [flag, slot] if flag == "--coverage-slot" => Ok((Fixture::P7P4, Some(slot.as_str()))),
            [small, flag, slot] if small == "--small" && flag == "--coverage-slot" => {
                Ok((Fixture::OneI, Some(slot.as_str())))
            }
            [fixture, flag, _] if fixture == "--nonempty-eight-i" && flag == "--coverage-slot" => {
                Err(EIGHT_I_CAPTURE_ERROR.into())
            }
            _ => Err(
                "usage: placement_graph_bench [[--small] [--coverage-slot SLOT] | --nonempty-eight-i | --scalar-demo | --compare FILE FILE ... | --compare-coverage FILE FILE]"
                    .into(),
            ),
        }
    }

    pub fn main() -> Result<(), String> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.first().is_some_and(|arg| arg == "--compare") {
            return compare(&args[1..]);
        }
        if let [flag, reference, candidate] = args.as_slice() {
            if flag == "--compare-coverage" {
                return compare_coverage(Path::new(reference), Path::new(candidate));
            }
        }
        if args.as_slice() == ["--scalar-demo"] {
            return scalar_demo();
        }
        let (fixture, coverage_slot) = parse_fixture(&args)?;
        if (fixture != Fixture::OneI || coverage_slot.is_some())
            && (std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
                || std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search"))
        {
            return Err(
                "P7P4, nonempty eight-I and coverage capture require the supervised benchmark-search profile".into(),
            );
        }
        let mode = std::env::var("CLEARRA_PLACEMENT_DEPENDENCY").unwrap_or_else(|_| "0".into());
        if !matches!(mode.as_str(), "0" | "1") {
            return Err("CLEARRA_PLACEMENT_DEPENDENCY must be 0 or 1 at process start".into());
        }
        let structural_mode =
            std::env::var("CLEARRA_STRUCTURAL_ORDER").unwrap_or_else(|_| "0".into());
        if !matches!(structural_mode.as_str(), "0" | "1") {
            return Err("CLEARRA_STRUCTURAL_ORDER must be 0 or 1 at process start".into());
        }
        let static_influence_mode =
            std::env::var("CLEARRA_STATIC_INFLUENCE").unwrap_or_else(|_| "0".into());
        if !matches!(static_influence_mode.as_str(), "0" | "1") {
            return Err("CLEARRA_STATIC_INFLUENCE must be 0 or 1 at process start".into());
        }
        let translation_interval_mode =
            std::env::var("CLEARRA_TRANSLATION_INTERVAL").unwrap_or_else(|_| "0".into());
        if !matches!(translation_interval_mode.as_str(), "0" | "1") {
            return Err("CLEARRA_TRANSLATION_INTERVAL must be 0 or 1 at process start".into());
        }
        let movement_clear_certificates_mode =
            experiment_mode("CLEARRA_MOVEMENT_CLEAR_CERTIFICATES")?;
        let incremental_domain_mode = experiment_mode("CLEARRA_INCREMENTAL_DOMAIN")?;
        let minimum_domain_cutoff_mode = experiment_mode("CLEARRA_MINIMUM_DOMAIN_CUTOFF")?;
        let minimum_domain_work_mode = experiment_mode("CLEARRA_MINIMUM_DOMAIN_WORK")?;
        let first_kick_interval_mode = experiment_mode("CLEARRA_FIRST_KICK_INTERVAL")?;
        let ordered_kick_interval_mode = experiment_mode("CLEARRA_ORDERED_KICK_INTERVAL")?;
        let ilc_lazy_frame_mode = experiment_mode("CLEARRA_ILC_LAZY_FRAME")?;
        let no_entry_components_mode = experiment_mode("CLEARRA_NO_ENTRY_COMPONENTS")?;
        let same_tile_parent_reuse_mode = experiment_mode("CLEARRA_SAME_TILE_PARENT_REUSE")?;
        let conditional_order_mode = experiment_mode("CLEARRA_CONDITIONAL_ORDER")?;
        let terminal_identity_move_mode = experiment_mode("CLEARRA_TERMINAL_IDENTITY_MOVE")?;
        let binary_sha = file_sha(&std::env::current_exe().map_err(|e| e.to_string())?)?;
        let capacity = CpuCapacity::current();
        let policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_worker_policy(WorkerPolicy::Auto)
            .with_worker_hardware_limit(capacity.hard_limit())
            .with_use_all_logical_processors(true)
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false)
            .with_tablebase_requested(false);
        let compile_started = Instant::now();
        let problem = compile_problem(fixture, policy)?;
        if problem.count_policy() != PcCountPolicy::CountUnique {
            return Err("fixture must retain CountUnique".into());
        }
        let compile_ms = compile_started.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "placement_graph_bench starting: case={} mode={} structural_mode={} static_influence_mode={} translation_interval_mode={} auto_cpu_limit={}",
            fixture.name(),
            mode,
            structural_mode,
            static_influence_mode,
            translation_interval_mode,
            capacity.hard_limit(),
        );
        let capture = coverage_slot
            .map(|slot| -> Result<_, String> {
                let universe = fixture.capture_universe()?;
                Ok((new_capture_directory(slot)?, universe))
            })
            .transpose()?;
        let search_started = Instant::now();
        let control = ExecutionControl::default();
        let (result, spool) = if let Some((dir, universe)) = &capture {
            let universe = *universe;
            let spool = Arc::new(Mutex::new(
                CoverageSpool::create(&dir.join("runs"), universe, CoverageSpoolLimits::default())
                    .map_err(|e| format!("coverage spool: {e}"))?,
            ));
            let owner = Arc::clone(&spool);
            let failure = Arc::new(Mutex::new(None::<String>));
            let capture_failure = Arc::clone(&failure);
            let result = WasmCpuSearchBackend::execute_with_solution_coverage_capture(
                &problem,
                &control,
                move |identity, evidence| {
                    let save = || -> Result<(), String> {
                        let fragment = match evidence {
                            CapturedSolutionCoverage::Bitmap(bits) => {
                                CoverageFragment::from_bitset(universe, bits)
                                    .map_err(|e| e.to_string())?
                            }
                            CapturedSolutionCoverage::P7P4(blocks) => {
                                CoverageFragment::from_p7p4_blocks(blocks)
                            }
                        };
                        let key =
                            NormalizedTilingSolutionKey::from_standard_board64_identity(identity);
                        owner
                            .lock()
                            .map_err(|_| "spool lock poisoned".to_owned())?
                            .push_fragment(key.as_str(), fragment)
                            .map_err(|e| e.to_string())
                    };
                    save().map_err(|error| {
                        if let Ok(mut first) = capture_failure.lock() {
                            if first.is_none() {
                                *first = Some(error);
                            }
                        }
                        "solution_coverage_spool_write_failed"
                    })
                },
            )
            .map_err(|e| {
                let cause = failure.lock().ok().and_then(|mut first| first.take());
                let stats = spool.lock().ok().map(|owner| owner.stats());
                format!(
                    "exact capture execution failed: {e:?}; spool diagnostic: {cause:?}; {stats:?}"
                )
            })?;
            let spool = Arc::try_unwrap(spool)
                .map_err(|_| "coverage workers still hold the spool")?
                .into_inner()
                .map_err(|_| "coverage spool lock poisoned")?;
            (result, Some(spool))
        } else {
            (
                WasmCpuSearchBackend::execute_with_control(&problem, &control)
                    .map_err(|e| format!("exact core execution failed: {e:?}"))?,
                None,
            )
        };
        let search_ms = search_started.elapsed().as_secs_f64() * 1000.0;
        let structural_totals =
            clearra_core_executor::backend::structural_order_experiment_totals();
        let geometry_domains = clearra_core_executor::backend::geometry_domain_experiment_totals();
        let evidence_started = Instant::now();
        let errors = validate(
            &result,
            fixture,
            capacity.hard_limit(),
            geometry_domains,
            incremental_domain_mode == "1",
            same_tile_parent_reuse_mode == "1",
        );
        let solution_coverage = match (capture, spool) {
            (Some((dir, universe)), Some(spool)) if errors.is_empty() => {
                finish_capture(dir, spool, universe, &result)?
            }
            _ => Value::Null,
        };
        let fields: Vec<_> = result.summary_field_entries().collect();
        let summary: BTreeMap<_, _> = fields.iter().copied().collect();
        let words = result.coverage_pattern_words();
        let mut coverage_sha = Sha256::new();
        coverage_sha.update(b"clearra-coverage-words-le-v1\0");
        coverage_sha.update((words.len() as u64).to_le_bytes());
        for word in words {
            coverage_sha.update(word.to_le_bytes());
        }
        let identities = result.normalized_solution_identities();
        let mut identity_sha = Sha256::new();
        identity_sha.update(b"clearra-materialized-ordered-ctk1-v1\0");
        identity_sha.update((identities.len() as u64).to_le_bytes());
        for identity in identities {
            // The public key API owns the canonical CTK1 encoding; the
            // identity's private formatter is not part of this contract.
            let canonical = NormalizedTilingSolutionKey::from_standard_board64_identity(*identity);
            let bytes = canonical.as_str().as_bytes();
            identity_sha.update((bytes.len() as u64).to_le_bytes());
            identity_sha.update(bytes);
        }
        let total_count = result.usize_field("normalized_unique_solution_count");
        let mut covered_ordinal_sample = Vec::new();
        for (index, &word) in words.iter().enumerate() {
            let mut pending = word;
            while pending != 0 && covered_ordinal_sample.len() < 16 {
                covered_ordinal_sample.push(index * 64 + pending.trailing_zeros() as usize);
                pending &= pending - 1;
            }
            if covered_ordinal_sample.len() == 16 {
                break;
            }
        }
        // Keep report sections separate so json!'s token recursion is
        // bounded independently of the full published report's field count.
        let metadata = json!({
            "schema": "clearra.placement-graph-benchmark.v1",
            "case": fixture.name(),
            "binary_sha256": binary_sha,
            "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
            "supervisor_run_id": std::env::var("CLEARRA_RUNTIME_RUN_ID").ok(),
            "output_mode": if coverage_slot.is_some() { "solution-coverage" } else { "global-only" },
            "solution_coverage": solution_coverage,
            "mode": mode,
            "structural_mode": structural_mode,
            "static_influence_mode": static_influence_mode,
            "translation_interval_mode": translation_interval_mode,
            "movement_clear_certificates_mode": movement_clear_certificates_mode,
            "incremental_domain_mode": incremental_domain_mode,
            "minimum_domain_cutoff_mode": minimum_domain_cutoff_mode,
            "minimum_domain_work_mode": minimum_domain_work_mode,
            "first_kick_interval_mode": first_kick_interval_mode,
            "ordered_kick_interval_mode": ordered_kick_interval_mode,
            "ilc_lazy_frame_mode": ilc_lazy_frame_mode,
            "no_entry_components_mode": no_entry_components_mode,
            "same_tile_parent_reuse_mode": same_tile_parent_reuse_mode,
            "conditional_order_mode": conditional_order_mode,
            "terminal_identity_move_mode": terminal_identity_move_mode,
            "variant": format!("{},movement_clear={movement_clear_certificates_mode},incremental_domain={incremental_domain_mode},first_kick={first_kick_interval_mode},ordered_kick={ordered_kick_interval_mode},ilc_lazy_frame={ilc_lazy_frame_mode},no_entry_components={no_entry_components_mode},same_tile_parent_reuse={same_tile_parent_reuse_mode},conditional_order={conditional_order_mode},terminal_identity_move={terminal_identity_move_mode},minimum_domain_cutoff={minimum_domain_cutoff_mode},minimum_domain_work={minimum_domain_work_mode}",
                variant(&mode, &structural_mode, &static_influence_mode, &translation_interval_mode)),
        });
        let structural_metrics = json!({
            "static_influence": {
                "scope": "completed search, workspace counters aggregated after worker joins; null means metric not exposed",
                "queries": result.usize_field("reachability_static_influence_queries"),
                "reachable": result.usize_field("reachability_static_influence_reachable"),
                "unreachable": result.usize_field("reachability_static_influence_unreachable"),
                "row_steps": result.usize_field("reachability_static_influence_row_steps"),
                "cache_hits": result.usize_field("reachability_static_influence_cache_hits"),
                "masked_queries": result.usize_field("reachability_static_influence_masked_queries"),
                "ignored_cells": result.usize_field("reachability_static_influence_ignored_cells"),
                "mask_compilations": result.usize_field("reachability_static_influence_mask_compilations")
            },
            "structural_order": {
                "scope": "process lifetime, one completed search; sampled after worker joins",
                "skipped_build_graphs": structural_totals.skipped_build_graphs,
                "language_residual_states": structural_totals.language_residual_states,
                "exchangeable_operations_removed": structural_totals.exchangeable_operations_removed,
                "cached_interval_guards": structural_totals.cached_interval_guards,
                "algebraic_interval_attempts": structural_totals.algebraic_interval_attempts,
                "algebraic_interval_guards": structural_totals.algebraic_interval_guards,
                "algebraic_interval_row_steps": structural_totals.algebraic_interval_row_steps,
                "translation_interval_attempts": structural_totals.translation_interval_attempts,
                "translation_interval_guards": structural_totals.translation_interval_guards,
                "translation_interval_row_steps": structural_totals.translation_interval_row_steps
            },
            "movement_clear": {
                "frame_pose_attempts": structural_totals.movement_clear.frame_pose_attempts,
                "frame_pose_proofs": structural_totals.movement_clear.frame_pose_proofs,
                "reused_positive_locks": structural_totals.movement_clear.reused_positive_locks,
                "reused_closed_locks": structural_totals.movement_clear.reused_closed_locks,
                "point_pose_attempts": structural_totals.movement_clear.point_pose_attempts,
                "point_pose_proofs": structural_totals.movement_clear.point_pose_proofs
            },
            "first_kick_interval": {
                "attempts": structural_totals.first_kick_interval.attempts,
                "proofs": structural_totals.first_kick_interval.proofs,
                "selected_locks": structural_totals.first_kick_interval.selected_locks
            },
            "ordered_kick_interval": {
                "attempts": structural_totals.ordered_kick_interval.attempts,
                "proofs": structural_totals.ordered_kick_interval.proofs,
                "selected_locks": structural_totals.ordered_kick_interval.selected_locks
            },
            "conditional_order": {
                "attempted_candidates": structural_totals.conditional_order.attempted_candidates,
                "certified_candidates": structural_totals.conditional_order.certified_candidates,
                "empty_certificates": structural_totals.conditional_order.empty_certificates,
                "unknown_candidates": structural_totals.conditional_order.unknown_candidates,
                "budget_candidates": structural_totals.conditional_order.budget_candidates,
                "checked_transitions": structural_totals.conditional_order.checked_transitions,
                "residual_states": structural_totals.conditional_order.residual_states,
                "graph_bypasses": structural_totals.conditional_order.graph_bypasses,
                "reused_pose_proofs": structural_totals.conditional_order.reused_pose_proofs
            },
        });
        let domains = json!({
            "geometry_domain": {
                "scope": "completed compact family roots; not memo/column exits, partial roots or extended",
                "completed_families": geometry_domains.completed_families,
                "minimum_compiles": geometry_domains.minimum_compiles,
                "minimum_resource_authoritative": geometry_domains.minimum_resource_authoritative,
                "minimum_below_seven_pieces": geometry_domains.minimum_below_seven_pieces,
                "minimum_empty_origin": geometry_domains.minimum_empty_origin,
                "minimum_profiled_compiles": geometry_domains.minimum_profiled_compiles,
                "minimum_incidence_rows_scanned": geometry_domains.minimum_incidence_rows_scanned,
                "minimum_incidence_rows_skipped": geometry_domains.minimum_incidence_rows_skipped,
                "minimum_cells_cut_off": geometry_domains.minimum_cells_cut_off,
                "advanced_full_compiles": geometry_domains.advanced_full_compiles,
                "advanced_incremental_compiles": geometry_domains.advanced_incremental_compiles,
                "support_cells_reused": geometry_domains.support_cells_reused,
                "support_cells_recomputed": geometry_domains.support_cells_recomputed,
                "same_tile_parent_scans": geometry_domains.same_tile_parent_scans,
                "same_tile_parent_reuses": geometry_domains.same_tile_parent_reuses,
                "apdp_static_checks": geometry_domains.apdp_static_checks,
                "apdp_static_complete": geometry_domains.apdp_static_complete,
                "apdp_pivot_tightenings": geometry_domains.apdp_pivot_tightenings
            },
        });
        let memory_lifetimes = json!({
            "core_memory_lifetimes": {
                "scope": result.field("parallel_memory_lifetime_scope"),
                "interpretation": "selected logical byte estimates at named checkpoints; checkpoints overlap, do not sum or interpret as allocator or RSS peaks; null means not exposed",
                "worker_exit": {
                    "samples": result.usize_field("parallel_worker_exit_snapshot_count"),
                    "scratch_bytes_sum": result.usize_field("parallel_worker_exit_scratch_bytes_sum"),
                    "scratch_bytes_max": result.usize_field("parallel_worker_exit_scratch_bytes_max"),
                    "outbound_payload_bytes_sum": result.usize_field("parallel_worker_exit_outbound_payload_bytes_sum"),
                    "branch_slots_bytes_sum": result.usize_field("parallel_worker_exit_branch_slots_bytes_sum")
                },
                "coordinator_handoff": {
                    "catalog_payload_bytes": result.usize_field("parallel_handoff_catalog_payload_bytes"),
                    "result_slots_bytes": result.usize_field("parallel_handoff_result_slots_bytes"),
                    "global_coverage_storage_bytes": result.usize_field("parallel_handoff_global_coverage_storage_bytes")
                },
                "terminal_assembly": {
                    "result_slots_bytes": result.usize_field("parallel_assembly_result_slots_bytes")
                },
                "shared_request": {
                    "standard_bag_retained_bytes": result.usize_field("shared_standard_bag_request_retained_bytes"),
                    "reachability_template_retained_bytes": result.usize_field("shared_reachability_template_retained_bytes")
                }
            }
        });
        let evidence = json!({
            "other_process_configuration": {
                "CLEARRA_STANDARD_BAG_MEMO": std::env::var("CLEARRA_STANDARD_BAG_MEMO").ok(),
                "CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT": std::env::var("CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT").ok()
            },
            "backend_contract": "native-host Rust WasmCpuSearchBackend; no C core; no tablebase",
            "input": fixture.input(),
            "cpu": { "available_parallelism": std::thread::available_parallelism().map(usize::from).ok(),
                "recommended_parallelism": capacity.recommended_parallelism(),
                "affinity_limit": capacity.affinity_limit(), "expected_vcpus": capacity.expected_vcpus(),
                "hard_limit": capacity.hard_limit(), "worker_policy": "auto", "use_all_logical_processors": true },
            "timing_ms": { "problem_compile": compile_ms, "search": search_ms,
                "evidence_hashing": evidence_started.elapsed().as_secs_f64() * 1000.0 },
            "coverage": { "word_count": words.len(),
                "set_bit_count": words.iter().map(|word| u64::from(word.count_ones())).sum::<u64>(),
                "sha256_le_v1": format!("{:x}", coverage_sha.finalize()),
                "covered_ordinal_sample": covered_ordinal_sample,
                "ordinal_sample_scope": "at most 16 original-universe bit indices; not a full queue export" },
            "materialized_identities": { "count": identities.len(), "reported_total_count": total_count,
                "covers_reported_family": total_count == Some(identities.len()),
                "sha256_ordered_ctk1_v1": format!("{:x}", identity_sha.finalize()),
                "scope": "only the identities actually exposed by this result; a partial page is never a full-set proof" },
            "materialized_solution_coverage_count": result.normalized_solution_coverages().len(),
            "materialized_solution_probability_count": result.solution_probabilities().len(),
            "summary": summary, "summary_field_entries": fields,
            "valid": errors.is_empty(), "validation_errors": errors,
        });
        let output = join_report_sections([
            metadata,
            structural_metrics,
            domains,
            memory_lifetimes,
            evidence,
        ])?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
        );
        if output["valid"] != true {
            return Err("completed result failed benchmark validation; see JSON evidence".into());
        }
        Ok(())
    }

    fn join_report_sections(sections: impl IntoIterator<Item = Value>) -> Result<Value, String> {
        let mut report = serde_json::Map::new();
        for section in sections {
            let Value::Object(fields) = section else {
                return Err("benchmark report section must be an object".into());
            };
            for (key, value) in fields {
                match report.entry(key) {
                    serde_json::map::Entry::Occupied(entry) => {
                        return Err(format!("duplicate benchmark report key: {}", entry.key()));
                    }
                    serde_json::map::Entry::Vacant(entry) => {
                        entry.insert(value);
                    }
                }
            }
        }
        Ok(Value::Object(report))
    }

    fn experiment_mode(name: &str) -> Result<String, String> {
        let value = std::env::var(name).unwrap_or_else(|_| "0".into());
        if matches!(value.as_str(), "0" | "1") {
            Ok(value)
        } else {
            Err(format!("{name} must be 0 or 1 at process start"))
        }
    }

    fn new_capture_directory(slot: &str) -> Result<PathBuf, String> {
        if slot.is_empty()
            || slot.len() > 64
            || !slot
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("coverage slot must be an ASCII alphanumeric/hyphen leaf".into());
        }
        let mut path = std::env::current_dir()
            .and_then(|path| path.canonicalize())
            .map_err(|e| e.to_string())?;
        if !path.join("config/clearra-management.v1.json").is_file() {
            return Err("run coverage capture from the Clearra repository root".into());
        }
        for component in [
            "_local",
            "artifacts",
            "benchmarks",
            "placement-graph-coverage",
        ] {
            path.push(component);
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(format!("coverage output root: {error}")),
            }
            let resolved = path.canonicalize().map_err(|e| e.to_string())?;
            if resolved != path || !path.is_dir() {
                return Err("coverage output root cannot follow symlinks or junctions".into());
            }
        }
        path.push(slot);
        fs::create_dir(&path).map_err(|e| format!("coverage slot must be new: {e}"))?;
        Ok(path)
    }

    fn finish_capture(
        dir: PathBuf,
        spool: CoverageSpool,
        universe: CoverageUniverse,
        result: &CoreExecutionResult,
    ) -> Result<Value, String> {
        let path = dir.join("solution-coverage.clrcov");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let buffer = BufWriter::new(file);
        let output = CoverageWriter::new(
            LimitedOutput {
                inner: buffer,
                remaining: 8 * 1024 * 1024 * 1024,
            },
            universe,
        )
        .map_err(|e| e.to_string())?;
        let mut result_keys: Vec<_> = result
            .normalized_solution_identities()
            .iter()
            .copied()
            .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
            .collect();
        result_keys.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        let mut expected = result_keys.iter();
        let (summary, spool_stats) = spool.finish_checked(output, |key| {
            if expected.next().is_some_and(|next| next.as_str() == key) {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "captured solution identity differs from exact product result",
                ))
            }
        });
        let summary = summary.map_err(|e| format!("coverage evidence: {e}; {spool_stats:?}"))?;
        if expected.next().is_some() {
            return Err("captured solution family did not cover the exact product result".into());
        }
        drop(result_keys);
        Ok(json!({
            "sha256": file_sha(&path)?, "solution_count": summary.solutions,
            "total_weight": universe.queue_count(), "covered_weight": summary.covered_queues,
            "covered_pairs": summary.covered_pairs, "complete": true,
            "output_bytes": fs::metadata(&path).map_err(|e| e.to_string())?.len(),
            "spool": {
                "fragments_fed": spool_stats.fragments_fed,
                "spill_bytes_written": spool_stats.spill_bytes_written,
                "peak_live_spill_bytes": spool_stats.peak_live_spill_bytes,
                "peak_live_run_count": spool_stats.peak_live_run_count
            }
        }))
    }

    struct LimitedOutput<W> {
        inner: W,
        remaining: u64,
    }

    impl<W: Write> Write for LimitedOutput<W> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() as u64 > self.remaining {
                return Err(io::Error::other("coverage output exceeded its 8 GiB quota"));
            }
            let written = self.inner.write(bytes)?;
            self.remaining -= written as u64;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    struct HashingReader<R> {
        inner: R,
        sha: Sha256,
    }

    impl<R: Read> Read for HashingReader<R> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let count = self.inner.read(bytes)?;
            self.sha.update(&bytes[..count]);
            Ok(count)
        }
    }

    fn compare_coverage(reference: &Path, candidate: &Path) -> Result<(), String> {
        let open = |path: &Path| {
            File::open(path)
                .map(BufReader::new)
                .map(|inner| HashingReader {
                    inner,
                    sha: Sha256::new(),
                })
                .map_err(|e| e.to_string())
        };
        let mut reference = open(reference)?;
        let mut candidate = open(candidate)?;
        let comparison = solution_coverage_compare::compare(&mut reference, &mut candidate)
            .map_err(|e| format!("coverage comparison: {e}"))?;
        let mut report = json!({
            "schema": "clearra.placement-graph-solution-coverage-comparison.v1",
        });
        match comparison {
            CoverageComparison::Equal(summary) => {
                report["left_sha256"] = json!(format!("{:x}", reference.sha.finalize()));
                report["right_sha256"] = json!(format!("{:x}", candidate.sha.finalize()));
                report["equivalent"] = json!(true);
                report["complete"] = json!(true);
                report["universe"] = json!(match summary.universe {
                    CoverageUniverse::P7P4 => "p7p4",
                    CoverageUniverse::SmallI => "small-i",
                    #[cfg(test)]
                    CoverageUniverse::Tiny => "synthetic",
                });
                report["solution_count"] = json!(summary.solutions);
                report["total_weight"] = json!(summary.universe.queue_count());
                report["covered_weight"] = json!(summary.covered_queues);
                report["covered_pairs"] = json!(summary.covered_pairs);
            }
            CoverageComparison::Different(diff) => {
                report["equivalent"] = json!(false);
                report["complete"] = json!(false);
                report["difference"] = json!({
                    "reference_identity": diff.reference_identity,
                    "candidate_identity": diff.candidate_identity,
                    "first_missing_queue": diff.first_missing_queue,
                    "first_added_queue": diff.first_added_queue,
                    "reference_numerator": diff.reference_numerator,
                    "candidate_numerator": diff.candidate_numerator,
                    "denominator": diff.denominator,
                });
            }
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        if report["equivalent"] == true {
            Ok(())
        } else {
            Err("per-solution coverage differs".into())
        }
    }

    fn compile_problem(
        fixture: Fixture,
        policy: PcExecutionPolicy,
    ) -> Result<SearchProblem, String> {
        if let Some(board) = fixture.fixed_board() {
            let pieces = fixture.placed_pieces();
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(fixture.height(), board),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
                PieceWindow::new(pieces),
            )
            .with_rule(jstris_180())
            .with_allow_hold(false)
            .with_exact_pieces(Some(pieces))
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_execution_policy(policy);
            ProblemCompiler::compile_scenario_pc(&query)
                .map_err(|e| format!("{} compile: {e:?}", fixture.name()))
        } else {
            let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
                .with_rule(jstris_180())
                .with_queue(PcQueueInput::standard_7_bag())
                .with_hold_policy(PcHoldPolicy::EnabledEmpty)
                .with_objective(ObjectivePolicy::unique())
                .with_execution_policy(policy);
            ProblemCompiler::compile_opening_pc(&query).map_err(|e| format!("P7P4 compile: {e:?}"))
        }
    }

    /// Independent tiling enumeration on the 4x8 rectangle behind the two
    /// occupied leftmost columns. Its left edge is either one vertical I
    /// column or a four-column slab of four horizontal I pieces. Any tiling
    /// is a sequence of those blocks, all achievable by placing the bottom
    /// rows of the horizontal slabs in order while vertical pieces are down.
    fn eight_i_identity_oracle() -> BTreeSet<StandardBoard64TilingIdentity> {
        fn visit(
            col: u8,
            placed: &mut Vec<PiecePlacementMask>,
            identities: &mut BTreeSet<StandardBoard64TilingIdentity>,
        ) {
            if col == 10 {
                identities.insert(
                    StandardBoard64TilingIdentity::from_placements(
                        EIGHT_I_INITIAL,
                        placed.iter().copied(),
                    )
                    .expect("oracle's placements are disjoint"),
                );
                return;
            }
            let vertical = (0..4).fold(0_u64, |mask, row| mask | (1_u64 << (row * 10 + col)));
            placed.push(PiecePlacementMask::new(PieceKind::I, vertical));
            visit(col + 1, placed, identities);
            placed.pop();
            if col <= 6 {
                let checkpoint = placed.len();
                for row in 0..4 {
                    placed.push(PiecePlacementMask::new(
                        PieceKind::I,
                        0xf_u64 << (row * 10 + col),
                    ));
                }
                visit(col + 4, placed, identities);
                placed.truncate(checkpoint);
            }
        }
        let mut identities = BTreeSet::new();
        visit(2, &mut Vec::with_capacity(8), &mut identities);
        identities
    }

    fn validate(
        result: &CoreExecutionResult,
        fixture: Fixture,
        workers: usize,
        geometry: GeometryDomainExperimentTotals,
        incremental: bool,
        same_tile_reuse: bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        for key in ["count_complete", "probability_complete"] {
            if result.bool_field(key) != Some(true) {
                errors.push(format!("{key} must be true"));
            }
        }
        if result.bool_field("resource_truncated") != Some(false) {
            errors.push("resource_truncated must be false".into());
        }
        let expected_count = match fixture {
            Fixture::OneI => 1,
            Fixture::NonemptyEightI => 7,
            Fixture::P7P4 => LARGE_COUNT,
        };
        if result.usize_field("normalized_unique_solution_count") != Some(expected_count) {
            errors.push("completed solution count differs from fixture oracle".into());
        }
        if fixture == Fixture::P7P4 {
            if result.field("normalized_solution_set_hash") != Some(LARGE_HASH) {
                errors.push("completed solution-set hash differs from Jstris180 oracle".into());
            }
            if result.usize_field("workers_used") != Some(workers) {
                errors.push(format!(
                    "workers_used must equal detected auto limit {workers}"
                ));
            }
            if workers > 1 && result.bool_field("cpu_parallel_execution") != Some(true) {
                errors.push("large run must use the detected parallel topology".into());
            }
            if workers > 1 && result.usize_field("parallel_active_workers") != Some(workers) {
                errors.push(format!(
                    "parallel_active_workers must equal detected auto limit {workers}"
                ));
            }
        } else if fixture == Fixture::NonemptyEightI {
            let actual_identities = result.normalized_solution_identities();
            if actual_identities.len() != expected_count
                || actual_identities.iter().copied().collect::<BTreeSet<_>>()
                    != eight_i_identity_oracle()
            {
                errors.push("eight-I canonical identities differ from the rectangle oracle".into());
            }
            // This compact CPU result exposes coverage_calculated and
            // probability_complete (checked above), not coverage_complete.
            // Its exact universe and bitmap prove completion of coverage.
            let calculated = result.bool_field("coverage_calculated");
            let patterns = result.usize_field("coverage_pattern_count");
            let covered = result.usize_field("covered_pattern_count");
            let probability = result
                .field("coverage_probability")
                .and_then(|value| value.parse::<f64>().ok());
            let words = result.coverage_pattern_words();
            if calculated != Some(true)
                || patterns != Some(1)
                || covered != Some(1)
                || probability != Some(1.0)
                || words != [1]
            {
                errors.push(format!(
                    "eight-I must completely cover its only fixed queue: calculated={calculated:?}, patterns={patterns:?}, covered={covered:?}, probability={probability:?}, words={}, first word={:?}",
                    words.len(),
                    words.first(),
                ));
            }
            // The compact fixture permits automatic topology admission to
            // use fewer workers than P7P4. --compare checks actual topology.
            if !result
                .usize_field("workers_used")
                .is_some_and(|used| (1..=workers).contains(&used))
            {
                errors
                    .push("eight-I must report an actual worker count within the Auto cap".into());
            }
            let (expected, excluded) = if incremental {
                (
                    geometry.advanced_incremental_compiles,
                    geometry.advanced_full_compiles,
                )
            } else {
                (
                    geometry.advanced_full_compiles,
                    geometry.advanced_incremental_compiles,
                )
            };
            if geometry.completed_families == 0 || expected == 0 || excluded != 0 {
                errors.push("eight-I must actually execute its selected advanced route".into());
            }
            if incremental != (geometry.support_cells_reused > 0) {
                errors.push("eight-I support-cell reuse did not match the incremental arm".into());
            }
            if same_tile_reuse != (geometry.same_tile_parent_reuses > 0) {
                errors.push("eight-I parent reuse did not match the SameTile arm".into());
            }
            if !same_tile_reuse && geometry.same_tile_parent_scans == 0 {
                errors
                    .push("eight-I reference arm must perform actual SameTile parent scans".into());
            }
        }
        errors
    }

    fn file_sha(path: &Path) -> Result<String, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    fn scalar_demo() -> Result<(), String> {
        let pieces = PieceKind::STANDARD_TETROMINOES;
        let cases: [(&str, &[PieceKind], &[u16], u128, u128, bool); 4] = [
            (
                "seven independent pieces",
                &pieces,
                &[0; 7],
                5040,
                5040,
                false,
            ),
            (
                "seven-piece chain",
                &pieces,
                &[0, 1, 2, 4, 8, 16, 32],
                1,
                5040,
                false,
            ),
            (
                "three-piece chain and four independent pieces",
                &pieces,
                &[0, 1, 2, 0, 0, 0, 0],
                840,
                5040,
                false,
            ),
            (
                "four-piece irreducible N order",
                &pieces[..4],
                &[0, 0, 3, 2],
                5,
                840,
                true,
            ),
        ];
        let mut rows = Vec::new();
        let mut errors = Vec::new();
        for (name, kinds, predecessors, expected_count, expected_total, requires_ideals) in cases {
            let count = clearra_core_executor::backend::count_distinct_no_hold_one_bag_poset(
                kinds,
                predecessors,
            )
            .ok_or_else(|| format!("{name}: supported distinct-kind poset was rejected"))?;
            if count.accepted_queue_count != expected_count
                || count.total_queue_count != expected_total
            {
                errors.push(format!(
                    "{name}: exact queue counts differ from the fixture oracle"
                ));
            }
            if (count.ideal_states > 0) != requires_ideals {
                errors.push(format!("{name}: unexpected decomposition/ideal-count path"));
            }
            rows.push(json!({
                "case": name,
                "pieces_in_operation_order": kinds.iter().map(|piece| piece.as_ascii().to_string()).collect::<Vec<_>>(),
                "predecessor_masks": predecessors,
                "mask_convention": "bit i names concrete operation i in pieces_in_operation_order",
                "accepted_queue_count": count.accepted_queue_count,
                "total_queue_count": count.total_queue_count,
                "exact_probability": {
                    "numerator": count.accepted_queue_count,
                    "denominator": count.total_queue_count,
                    "fraction": format!("{}/{}", count.accepted_queue_count, count.total_queue_count)
                },
                "probability_approximate": count.accepted_queue_count as f64 / count.total_queue_count as f64,
                "decomposed_subproblems": count.decomposed_subproblems,
                "ideal_states": count.ideal_states,
                "oracle": { "accepted_queue_count": expected_count, "total_queue_count": expected_total }
            }));
        }
        let binary_sha = file_sha(&std::env::current_exe().map_err(|e| e.to_string())?)?;
        println!("{}", serde_json::to_string_pretty(&json!({
            "schema": "clearra.poset-scalar-demonstration.v1",
            "binary_sha256": binary_sha,
            "scope": "explicit fixed posets supplied directly; no field or placement certificate is claimed",
            "assumptions": {
                "hold": "disabled",
                "piece_kinds": "distinct, at most seven",
                "source": "fresh standard seven-piece bag",
                "draw_count": "exactly k draws, where k is the operation count",
                "denominator": "7Pk; every target piece must occur among those first k draws",
                "four_piece_N_probability": "5/840 unconditional; 5/24 would condition on drawing precisely its four piece kinds"
            },
            "field_search_performed": false,
            "placement_certificate_generated": false,
            "materialized_queue_count": 0,
            "cases": rows,
            "valid": errors.is_empty(),
            "validation_errors": errors
        })).map_err(|e| e.to_string())?);
        if !errors.is_empty() {
            return Err("scalar demonstration failed its exact-count validation".into());
        }
        Ok(())
    }

    fn compare(paths: &[String]) -> Result<(), String> {
        if paths.len() < 2 {
            return Err("--compare requires at least two result files".into());
        }
        let reports: Vec<Value> = paths
            .iter()
            .map(|path| {
                let file = File::open(path).map_err(|e| format!("{path}: {e}"))?;
                serde_json::from_reader(file).map_err(|e| format!("{path}: {e}"))
            })
            .collect::<Result<_, String>>()?;
        let first = &reports[0];
        let eight_i = first["case"] == Fixture::NonemptyEightI.name();
        let mut differences = Vec::new();
        let summary_keys = [
            "normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "build_variant_count",
            "build_variant_count_exact",
            "coverage_pattern_count",
            "covered_pattern_count",
            "coverage_probability",
            "materialized_probability_mass",
            "count_complete",
            "coverage_complete",
            "probability_complete",
            "solution_probability_complete",
            "resource_truncated",
            "packing_candidate_set_digest",
        ];
        for (index, report) in reports.iter().enumerate() {
            if report["valid"] != true {
                differences.push(format!("{}: invalid source result", paths[index]));
            }
            for key in [
                "schema",
                "case",
                "binary_sha256",
                "input",
                "cpu",
                "other_process_configuration",
                "output_mode",
                "coverage",
                "materialized_identities",
            ] {
                if report[key] != first[key] {
                    differences.push(format!("{}: {key} mismatch", paths[index]));
                }
            }
            for key in summary_keys {
                if report["summary"][key] != first["summary"][key] {
                    differences.push(format!("{}: summary.{key} mismatch", paths[index]));
                }
            }
            if eight_i {
                if report["output_mode"] != "global-only" {
                    differences.push(format!("{}: eight-I must be global-only", paths[index]));
                }
                for key in ["workers_used", "cpu_parallel_execution"] {
                    if report["summary"][key].is_null()
                        || report["summary"][key] != first["summary"][key]
                    {
                        differences
                            .push(format!("{}: actual topology {key} mismatch", paths[index]));
                    }
                }
            }
        }
        // Missing experimental axes in old reports mean disabled. Keep their
        // original grouping when every report predates that axis. A binary
        // mismatch remains a parity error, even for otherwise compatible JSON.
        let has_static_influence_mode = reports
            .iter()
            .any(|report| report.get("static_influence_mode").is_some());
        let has_translation_interval_mode = reports
            .iter()
            .any(|report| report.get("translation_interval_mode").is_some());
        let legacy_modes = reports
            .iter()
            .all(|report| report.get("structural_mode").is_none())
            && !has_static_influence_mode
            && !has_translation_interval_mode;
        let report_variant = |dependency: &str,
                              structural: &str,
                              static_influence: &str,
                              translation_interval: &str| {
            if legacy_modes {
                dependency.to_owned()
            } else if has_translation_interval_mode {
                variant(
                    dependency,
                    structural,
                    static_influence,
                    translation_interval,
                )
            } else if has_static_influence_mode {
                format!("dependency={dependency},structural={structural},static_influence={static_influence}")
            } else {
                format!("dependency={dependency},structural={structural}")
            }
        };
        let mut times: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut all_axes_times: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut domain_work: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for report in &reports {
            let dependency_mode = report["mode"].as_str().ok_or("mode missing")?;
            let structural_mode = match report.get("structural_mode") {
                None => "0",
                Some(value) => value.as_str().ok_or("structural_mode must be a string")?,
            };
            let static_influence_mode = match report.get("static_influence_mode") {
                None => "0",
                Some(value) => value
                    .as_str()
                    .ok_or("static_influence_mode must be a string")?,
            };
            let translation_interval_mode = match report.get("translation_interval_mode") {
                None => "0",
                Some(value) => value
                    .as_str()
                    .ok_or("translation_interval_mode must be a string")?,
            };
            if !matches!(dependency_mode, "0" | "1")
                || !matches!(structural_mode, "0" | "1")
                || !matches!(static_influence_mode, "0" | "1")
                || !matches!(translation_interval_mode, "0" | "1")
            {
                return Err("result mode flags must be 0 or 1".into());
            }
            let mode = report_variant(
                dependency_mode,
                structural_mode,
                static_influence_mode,
                translation_interval_mode,
            );
            let ms = report["timing_ms"]["search"]
                .as_f64()
                .ok_or("search time missing")?;
            if !ms.is_finite() || ms <= 0.0 {
                return Err("search time must be finite and positive".into());
            }
            let extra_mode = |key: &str| match report.get(key) {
                None => Ok("0"),
                Some(value) => value
                    .as_str()
                    .filter(|v| matches!(*v, "0" | "1"))
                    .ok_or("extra result mode flags must be 0 or 1"),
            };
            let movement = extra_mode("movement_clear_certificates_mode")?;
            let incremental = extra_mode("incremental_domain_mode")?;
            let minimum_domain_cutoff = extra_mode("minimum_domain_cutoff_mode")?;
            let minimum_domain_work = extra_mode("minimum_domain_work_mode")?;
            let first_kick = extra_mode("first_kick_interval_mode")?;
            let ordered_kick = extra_mode("ordered_kick_interval_mode")?;
            let ilc_lazy_frame = extra_mode("ilc_lazy_frame_mode")?;
            let no_entry_components = extra_mode("no_entry_components_mode")?;
            let same_tile_parent_reuse = extra_mode("same_tile_parent_reuse_mode")?;
            let conditional = extra_mode("conditional_order_mode")?;
            let terminal_move = extra_mode("terminal_identity_move_mode")?;
            let all_axes = format!(
                "{mode},movement_clear={movement},incremental_domain={incremental},first_kick={first_kick},ordered_kick={ordered_kick},ilc_lazy_frame={ilc_lazy_frame},no_entry_components={no_entry_components},same_tile_parent_reuse={same_tile_parent_reuse},conditional_order={conditional},terminal_identity_move={terminal_move},minimum_domain_cutoff={minimum_domain_cutoff},minimum_domain_work={minimum_domain_work}"
            );
            if eight_i {
                let work = report
                    .get("geometry_domain")
                    .filter(|work| work.is_object())
                    .ok_or("eight-I result is missing geometry_domain work counters")?;
                domain_work.entry(all_axes.clone()).or_default().push(work);
            }
            all_axes_times.entry(all_axes).or_default().push(ms);
            if movement == "0"
                && incremental == "0"
                && minimum_domain_cutoff == "0"
                && minimum_domain_work == "0"
                && first_kick == "0"
                && ordered_kick == "0"
                && ilc_lazy_frame == "0"
                && no_entry_components == "0"
                && same_tile_parent_reuse == "0"
                && conditional == "0"
                && terminal_move == "0"
            {
                times.entry(mode).or_default().push(ms);
            }
        }
        let means: BTreeMap<String, f64> = times
            .iter()
            .map(|(mode, values)| {
                (
                    mode.clone(),
                    values.iter().sum::<f64>() / values.len() as f64,
                )
            })
            .collect();
        let dependency_baseline = report_variant("0", "0", "0", "0");
        let dependency_treatment = report_variant("1", "0", "0", "0");
        let structural_treatment = report_variant("1", "1", "0", "0");
        let static_influence_treatment = report_variant("1", "0", "1", "0");
        let static_structural_treatment = report_variant("1", "1", "1", "0");
        let translation_treatment = report_variant("1", "1", "0", "1");
        let all_experiments_treatment = report_variant("1", "1", "1", "1");
        let time_change = |baseline: &str, treatment: &str| {
            means
                .get(baseline)
                .zip(means.get(treatment))
                .map(|(a, b)| (b / a - 1.0) * 100.0)
        };
        let dependency_change = time_change(&dependency_baseline, &dependency_treatment);
        let structural_change = (!legacy_modes)
            .then(|| time_change(&dependency_treatment, &structural_treatment))
            .flatten();
        let combined_change = (!legacy_modes)
            .then(|| time_change(&dependency_baseline, &structural_treatment))
            .flatten();
        let static_influence_change = has_static_influence_mode
            .then(|| time_change(&dependency_treatment, &static_influence_treatment))
            .flatten();
        let static_influence_over_structural_change = has_static_influence_mode
            .then(|| time_change(&structural_treatment, &static_structural_treatment))
            .flatten();
        let translation_change = has_translation_interval_mode
            .then(|| time_change(&structural_treatment, &translation_treatment))
            .flatten();
        let translation_over_static_change = has_translation_interval_mode
            .then(|| time_change(&static_structural_treatment, &all_experiments_treatment))
            .flatten();
        let all_experiments_change = (has_static_influence_mode || has_translation_interval_mode)
            .then(|| time_change(&dependency_baseline, &all_experiments_treatment))
            .flatten();
        let (primary_baseline, primary_treatment, change) = if translation_change.is_some() {
            (
                &structural_treatment,
                &translation_treatment,
                translation_change,
            )
        } else if translation_over_static_change.is_some() {
            (
                &static_structural_treatment,
                &all_experiments_treatment,
                translation_over_static_change,
            )
        } else if static_influence_change.is_some() {
            (
                &dependency_treatment,
                &static_influence_treatment,
                static_influence_change,
            )
        } else if static_influence_over_structural_change.is_some() {
            (
                &structural_treatment,
                &static_structural_treatment,
                static_influence_over_structural_change,
            )
        } else if structural_change.is_some() {
            (
                &dependency_treatment,
                &structural_treatment,
                structural_change,
            )
        } else {
            (
                &dependency_baseline,
                &dependency_treatment,
                dependency_change,
            )
        };
        let mut comparison = json!({
            "schema": "clearra.placement-graph-comparison.v1", "files": paths,
            "equivalent": differences.is_empty(), "differences": differences,
            "mode_grouping": if legacy_modes { "dependency-only (legacy reports)" }
                else if has_translation_interval_mode { "dependency-structural-static-influence-translation-interval" }
                else if has_static_influence_mode { "dependency-structural-static-influence" }
                else { "dependency-and-structural" },
            "search_times_ms_by_mode": times, "mean_search_ms_by_mode": means,
            "search_times_ms_by_all_axes": all_axes_times,
            "treatment_time_change_percent": change,
            "primary_comparison": { "baseline": primary_baseline, "treatment": primary_treatment },
            "time_change_percent": {
                "dependency_vs_disabled": dependency_change,
                "structural_vs_dependency": structural_change,
                "combined_vs_disabled": combined_change,
                "static_influence_vs_dependency": static_influence_change,
                "static_influence_vs_structural": static_influence_over_structural_change,
                "translation_interval_vs_structural": translation_change,
                "translation_interval_vs_static_structural": translation_over_static_change,
                "all_experiments_vs_disabled": all_experiments_change
            },
            "scope": "same binary/input/CPU, exact reported full-set hash and global coverage; no p95 claim"
        });
        if eight_i {
            comparison["geometry_domain_work_samples_by_all_axes"] = json!(domain_work);
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&comparison).map_err(|e| e.to_string())?
        );
        if !differences.is_empty() {
            return Err("benchmark result parity failed".into());
        }
        Ok(())
    }

    fn variant(
        dependency_mode: &str,
        structural_mode: &str,
        static_influence_mode: &str,
        translation_interval_mode: &str,
    ) -> String {
        format!(
            "dependency={dependency_mode},structural={structural_mode},static_influence={static_influence_mode},translation_interval={translation_interval_mode}"
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn report_sections_preserve_flat_keys_and_reject_collisions() {
            let report = join_report_sections([
                json!({"schema": "fixture"}),
                json!({"geometry_domain": {"completed_families": 1}}),
                json!({"valid": true}),
            ])
            .unwrap();
            assert_eq!(
                report,
                json!({"schema": "fixture", "geometry_domain": {"completed_families": 1}, "valid": true})
            );
            assert_eq!(
                join_report_sections([json!({"case": "a"}), json!({"case": "b"})]).unwrap_err(),
                "duplicate benchmark report key: case"
            );
        }

        #[test]
        fn fixture_selection_keeps_capture_universes_separate() {
            let parse = |arguments: &[&str]| {
                let arguments: Vec<String> =
                    arguments.iter().map(|value| (*value).to_owned()).collect();
                parse_fixture(&arguments).map(|(fixture, slot)| (fixture, slot.map(str::to_owned)))
            };
            assert_eq!(parse(&[]).unwrap(), (Fixture::P7P4, None));
            assert_eq!(parse(&["--small"]).unwrap(), (Fixture::OneI, None));
            assert_eq!(
                parse(&["--nonempty-eight-i"]).unwrap(),
                (Fixture::NonemptyEightI, None)
            );
            assert_eq!(
                parse(&["--small", "--coverage-slot", "sample"]).unwrap(),
                (Fixture::OneI, Some("sample".to_owned()))
            );
            assert_eq!(
                parse(&["--nonempty-eight-i", "--coverage-slot", "sample"]).unwrap_err(),
                EIGHT_I_CAPTURE_ERROR
            );
            assert!(Fixture::NonemptyEightI.capture_universe().is_err());
        }

        #[test]
        fn eight_i_rectangle_oracle_derives_all_seven_placements() {
            assert_eq!(EIGHT_I_INITIAL.count_ones(), 8);
            assert_eq!(eight_i_identity_oracle().len(), 7);
        }
    }
}
