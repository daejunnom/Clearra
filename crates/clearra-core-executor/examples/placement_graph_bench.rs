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
mod run {
    use std::{collections::BTreeMap, fs::File, io::Read, path::Path, time::Instant};

    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, pc::pc_target::PcTarget,
        piece::piece_kind::PieceKind, runtime_cpu_capacity::CpuCapacity,
        solution::normalized_tiling_solution::NormalizedTilingSolutionKey,
    };
    use clearra_core_executor::{CoreExecutionResult, WasmCpuSearchBackend};
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
        if args.as_slice() == ["--scalar-demo"] {
            return scalar_demo();
        }
        let small = match args.as_slice() {
            [] => false,
            [flag] if flag == "--small" => true,
            _ => return Err(
                "usage: placement_graph_bench [--small | --scalar-demo | --compare FILE FILE ...]"
                    .into(),
            ),
        };
        if !small && std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1") {
            return Err("large P7P4 must run under clearra-manage runtime run --producer benchmark --profile benchmark-search".into());
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
            "placement_graph_bench starting: case={} mode={} structural_mode={} static_influence_mode={} auto_cpu_limit={}",
            if small {
                "one-I-preflight"
            } else {
                "empty-4L-P7P4-jstris180"
            },
            mode,
            structural_mode,
            static_influence_mode,
            capacity.hard_limit(),
        );
        let search_started = Instant::now();
        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .map_err(|e| format!("exact core execution failed: {e:?}"))?;
        let search_ms = search_started.elapsed().as_secs_f64() * 1000.0;
        let structural_totals =
            clearra_core_executor::backend::structural_order_experiment_totals();
        let evidence_started = Instant::now();
        let errors = validate(&result, small, capacity.hard_limit());
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
            "mode": mode,
            "structural_mode": structural_mode,
            "static_influence_mode": static_influence_mode,
            "variant": variant(&mode, &structural_mode, &static_influence_mode),
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
                "algebraic_interval_row_steps": structural_totals.algebraic_interval_row_steps
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
        let legacy_modes = reports
            .iter()
            .all(|report| report.get("structural_mode").is_none())
            && !has_static_influence_mode;
        let report_variant = |dependency: &str, structural: &str, static_influence: &str| {
            if legacy_modes {
                dependency.to_owned()
            } else if has_static_influence_mode {
                variant(dependency, structural, static_influence)
            } else {
                format!("dependency={dependency},structural={structural}")
            }
        };
        let mut times: BTreeMap<String, Vec<f64>> = BTreeMap::new();
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
            if !matches!(dependency_mode, "0" | "1")
                || !matches!(structural_mode, "0" | "1")
                || !matches!(static_influence_mode, "0" | "1")
            {
                return Err("result mode flags must be 0 or 1".into());
            }
            let mode = report_variant(dependency_mode, structural_mode, static_influence_mode);
            let ms = report["timing_ms"]["search"]
                .as_f64()
                .ok_or("search time missing")?;
            if !ms.is_finite() || ms <= 0.0 {
                return Err("search time must be finite and positive".into());
            }
            times.entry(mode).or_default().push(ms);
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
        let dependency_baseline = report_variant("0", "0", "0");
        let dependency_treatment = report_variant("1", "0", "0");
        let structural_treatment = report_variant("1", "1", "0");
        let static_influence_treatment = report_variant("1", "0", "1");
        let all_experiments_treatment = report_variant("1", "1", "1");
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
            .then(|| time_change(&structural_treatment, &all_experiments_treatment))
            .flatten();
        let all_experiments_change = has_static_influence_mode
            .then(|| time_change(&dependency_baseline, &all_experiments_treatment))
            .flatten();
        let (primary_baseline, primary_treatment, change) = if static_influence_change.is_some() {
            (
                &dependency_treatment,
                &static_influence_treatment,
                static_influence_change,
            )
        } else if static_influence_over_structural_change.is_some() {
            (
                &structural_treatment,
                &all_experiments_treatment,
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
                else if has_static_influence_mode { "dependency-structural-static-influence" }
                else { "dependency-and-structural" },
            "search_times_ms_by_mode": times, "mean_search_ms_by_mode": means,
            "treatment_time_change_percent": change,
            "primary_comparison": { "baseline": primary_baseline, "treatment": primary_treatment },
            "time_change_percent": {
                "dependency_vs_disabled": dependency_change,
                "structural_vs_dependency": structural_change,
                "combined_vs_disabled": combined_change,
                "static_influence_vs_dependency": static_influence_change,
                "static_influence_vs_structural": static_influence_over_structural_change,
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
    ) -> String {
        format!(
            "dependency={dependency_mode},structural={structural_mode},static_influence={static_influence_mode}"
        )
    }
}
