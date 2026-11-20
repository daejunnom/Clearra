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
        collections::BTreeMap,
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
        execution_cancellation::ExecutionControl, pc::pc_target::PcTarget,
        piece::piece_kind::PieceKind, runtime_cpu_capacity::CpuCapacity,
        solution::normalized_tiling_solution::NormalizedTilingSolutionKey,
    };
    use clearra_core_executor::{
        backend::CapturedSolutionCoverage, CoreExecutionResult, WasmCpuSearchBackend,
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
        let (small, coverage_slot) = match args.as_slice() {
            [] => (false, None),
            [flag] if flag == "--small" => (true, None),
            [flag, slot] if flag == "--coverage-slot" => (false, Some(slot.as_str())),
            [small, flag, slot] if small == "--small" && flag == "--coverage-slot" => {
                (true, Some(slot.as_str()))
            }
            _ => return Err(
                "usage: placement_graph_bench [[--small] [--coverage-slot SLOT] | --scalar-demo | --compare FILE FILE ... | --compare-coverage FILE FILE]"
                    .into(),
            ),
        };
        if (!small || coverage_slot.is_some())
            && (std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
                || std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search"))
        {
            return Err(
                "P7P4 and coverage capture require the supervised benchmark-search profile".into(),
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
        let first_kick_interval_mode = experiment_mode("CLEARRA_FIRST_KICK_INTERVAL")?;
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
        let problem = compile_problem(small, policy)?;
        if problem.count_policy() != PcCountPolicy::CountUnique {
            return Err("fixture must retain CountUnique".into());
        }
        let compile_ms = compile_started.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "placement_graph_bench starting: case={} mode={} structural_mode={} static_influence_mode={} translation_interval_mode={} auto_cpu_limit={}",
            if small {
                "one-I-preflight"
            } else {
                "empty-4L-P7P4-jstris180"
            },
            mode,
            structural_mode,
            static_influence_mode,
            translation_interval_mode,
            capacity.hard_limit(),
        );
        let universe = if small {
            CoverageUniverse::SmallI
        } else {
            CoverageUniverse::P7P4
        };
        let capture_path = coverage_slot.map(new_capture_directory).transpose()?;
        let search_started = Instant::now();
        let control = ExecutionControl::default();
        let (result, spool) = if let Some(dir) = &capture_path {
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
        let errors = validate(&result, small, capacity.hard_limit());
        let solution_coverage = match (capture_path, spool) {
            (Some(dir), Some(spool)) if errors.is_empty() => {
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
        let output = json!({
            "schema": "clearra.placement-graph-benchmark.v1",
            "case": if small { "one-I-preflight" } else { "empty-4L-P7P4-jstris180" },
            "binary_sha256": binary_sha,
            "supervisor_run_id": std::env::var("CLEARRA_RUNTIME_RUN_ID").ok(),
            "output_mode": if coverage_slot.is_some() { "solution-coverage" } else { "global-only" },
            "solution_coverage": solution_coverage,
            "mode": mode,
            "structural_mode": structural_mode,
            "static_influence_mode": static_influence_mode,
            "translation_interval_mode": translation_interval_mode,
            "movement_clear_certificates_mode": movement_clear_certificates_mode,
            "incremental_domain_mode": incremental_domain_mode,
            "first_kick_interval_mode": first_kick_interval_mode,
            "conditional_order_mode": conditional_order_mode,
            "terminal_identity_move_mode": terminal_identity_move_mode,
            "variant": format!("{},movement_clear={movement_clear_certificates_mode},incremental_domain={incremental_domain_mode},first_kick={first_kick_interval_mode},conditional_order={conditional_order_mode},terminal_identity_move={terminal_identity_move_mode}",
                variant(&mode, &structural_mode, &static_influence_mode, &translation_interval_mode)),
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
            "geometry_domain": {
                "scope": "completed compact family roots; not memo/column exits, partial roots or extended",
                "completed_families": geometry_domains.completed_families,
                "minimum_compiles": geometry_domains.minimum_compiles,
                "minimum_resource_authoritative": geometry_domains.minimum_resource_authoritative,
                "minimum_below_seven_pieces": geometry_domains.minimum_below_seven_pieces,
                "minimum_empty_origin": geometry_domains.minimum_empty_origin,
                "advanced_full_compiles": geometry_domains.advanced_full_compiles,
                "advanced_incremental_compiles": geometry_domains.advanced_incremental_compiles,
                "support_cells_reused": geometry_domains.support_cells_reused,
                "support_cells_recomputed": geometry_domains.support_cells_recomputed,
                "apdp_static_checks": geometry_domains.apdp_static_checks,
                "apdp_static_complete": geometry_domains.apdp_static_complete,
                "apdp_pivot_tightenings": geometry_domains.apdp_pivot_tightenings
            },
            "other_process_configuration": {
                "CLEARRA_STANDARD_BAG_MEMO": std::env::var("CLEARRA_STANDARD_BAG_MEMO").ok(),
                "CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT": std::env::var("CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT").ok()
            },
            "backend_contract": "native-host Rust WasmCpuSearchBackend; no C core; no tablebase",
            "input": { "board": if small { "0x3f0" } else { "0" },
                "height": if small { 1 } else { 4 }, "placed_pieces": if small { 1 } else { 10 },
                "supply": if small { "I" } else { "P7P4" },
                "supplied_pieces": if small { 1 } else { 11 },
                "original_supply_count": if small { 1 } else { 4_233_600 },
                "hold": if small { "disabled" } else { "enabled-empty" },
                "rule": "jstris-180", "count": "unique", "queue_knowledge": "full-future-oracle" },
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
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
        );
        if output["valid"] != true {
            return Err("completed result failed benchmark validation; see JSON evidence".into());
        }
        Ok(())
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

    fn compile_problem(small: bool, policy: PcExecutionPolicy) -> Result<SearchProblem, String> {
        if small {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(1, 0x3f0),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
                PieceWindow::new(1),
            )
            .with_rule(jstris_180())
            .with_allow_hold(false)
            .with_exact_pieces(Some(1))
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_execution_policy(policy);
            ProblemCompiler::compile_scenario_pc(&query)
                .map_err(|e| format!("preflight compile: {e:?}"))
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

    fn validate(result: &CoreExecutionResult, small: bool, workers: usize) -> Vec<String> {
        let mut errors = Vec::new();
        for key in ["count_complete", "probability_complete"] {
            if result.bool_field(key) != Some(true) {
                errors.push(format!("{key} must be true"));
            }
        }
        if result.bool_field("resource_truncated") != Some(false) {
            errors.push("resource_truncated must be false".into());
        }
        if result.usize_field("normalized_unique_solution_count")
            != Some(if small { 1 } else { LARGE_COUNT })
        {
            errors.push("completed solution count differs from fixture oracle".into());
        }
        if !small {
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
            let first_kick = extra_mode("first_kick_interval_mode")?;
            let conditional = extra_mode("conditional_order_mode")?;
            let terminal_move = extra_mode("terminal_identity_move_mode")?;
            all_axes_times
                .entry(format!(
                    "{mode},movement_clear={movement},incremental_domain={incremental},first_kick={first_kick},conditional_order={conditional},terminal_identity_move={terminal_move}"
                ))
                .or_default()
                .push(ms);
            if movement == "0"
                && incremental == "0"
                && first_kick == "0"
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
        println!("{}", serde_json::to_string_pretty(&json!({
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
        })).map_err(|e| e.to_string())?);
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
}
