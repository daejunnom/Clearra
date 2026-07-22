use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::{NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity},
};
#[cfg(feature = "stage-profiling")]
use clearra_core_executor::ExecutorSearchProfileSession;
use clearra_core_executor::{BuildUpRunResult, BuildUpRunner, PackingRunResult, PackingRunner};
use clearra_core_ffi::CPackingCandidate;
#[cfg(feature = "stage-profiling")]
use clearra_core_ffi::NativeSearchProfileSession;
use clearra_coverage::{
    pattern::pattern_bitset::PatternBitSet, probability::union_probability::union_probability,
};
use clearra_fumen::{
    ColoredSolutionFumenExporter, ColoredSolutionPage, ColoredSolutionPlacement,
    SourceFumenColoredFieldSet,
};
use clearra_problem::compile::problem_compiler::ProblemCompiler;
use serde::Serialize;

use crate::{
    args::{ArtifactArgs, ArtifactCountMode, ArtifactRule},
    scenario::ArtifactScenarioSpec,
    sfinder_reference::{SfinderReference, SfinderReferenceReport},
};

pub struct ArtifactOutcome {
    pub output_directory: PathBuf,
    pub solution_count: usize,
    pub covered_pattern_count: usize,
    pub pattern_count: usize,
    pub coverage_probability: String,
}

#[derive(Serialize)]
struct ArtifactSummary {
    schema_version: u32,
    scenario: String,
    rule_profile: String,
    count_mode: String,
    initial_board_mask: String,
    visible_height: u16,
    expected_unique_solution_count: Option<usize>,
    actual_unique_solution_count: usize,
    normalized_solution_set_hash: String,
    backend: BackendSummary,
    supply: SupplySummary,
    search: SearchSummary,
    probability: ProbabilitySummary,
    fumen: FumenSummary,
    sfinder_reference: Option<SfinderReferenceReport>,
    timings: TimingSummary,
}

#[derive(Serialize)]
struct BackendSummary {
    requested: String,
    selected: String,
    selection_reason: String,
    workers_requested: usize,
    workers_used: usize,
    fallback_used: bool,
    fallback_reason: Option<String>,
    fallback_backend: Option<String>,
    gpu_failure_class: Option<String>,
    gpu_failure_stage: Option<String>,
    discarded_partial_gpu_result: bool,
    original_gpu_result_incomplete: bool,
    trust_state: String,
    cpu_confirmed: bool,
    exact_probability_authorized: bool,
    gpu_device_requested: String,
    gpu_device_selected_index: Option<u8>,
    gpu_device_selected_name: Option<String>,
    gpu_device_selected_type: Option<String>,
    gpu_device_selected_backend: Option<String>,
    gpu_device_selected_vendor: Option<u32>,
    gpu_device_selected_device: Option<u32>,
}

#[derive(Serialize)]
struct SupplySummary {
    piece_source_id: String,
    pattern_universe_id: String,
    pattern_weight_model_id: String,
    materialized_pattern_count: usize,
    total_possible_pattern_count: String,
    materialized_probability_mass: String,
    complete: bool,
    truncation_reason: Option<String>,
    renormalized: bool,
}

#[derive(Serialize)]
struct SearchSummary {
    packing_candidate_count: usize,
    accepted_candidate_count: usize,
    coverage_row_count: usize,
    build_variant_count: usize,
    pattern_verified_execution_count: usize,
    count_complete: bool,
    coverage_complete: bool,
    resource_truncated: bool,
    resource_truncation_reason: Option<String>,
    peak_frontier_states: usize,
    peak_candidate_rows: usize,
    peak_cpu_bytes: usize,
    peak_gpu_bytes: usize,
    retained_search_bytes: usize,
    buildup_workspace_bytes: usize,
}

#[derive(Serialize)]
struct ProbabilitySummary {
    requested: bool,
    source: String,
    pattern_count: usize,
    covered_pattern_count: usize,
    coverage_probability: String,
    probability_complete: bool,
    incomplete_reason: Option<String>,
    score_does_not_change_probability_union: bool,
    renormalized: bool,
}

#[derive(Serialize)]
struct FumenSummary {
    path: Option<String>,
    page_count: usize,
    generated_after_search: bool,
    roundtrip_verified: bool,
    normalized_identity_authoritative: bool,
    colored_field_identity_unique: bool,
    not_generated_reason: Option<String>,
}

#[derive(Clone, Serialize)]
struct TimingStage {
    name: String,
    duration_ns: u64,
    invocation_count: u64,
    work_item_count: u64,
}

#[derive(Clone, Serialize)]
struct TimingSummary {
    problem_compile_ns: u64,
    packing_wall_ns: u64,
    buildup_wall_ns: u64,
    probability_postprocess_ns: u64,
    fumen_postprocess_ns: u64,
    sfinder_validation_ns: u64,
    total_wall_ns: u64,
    executor_stages: Vec<TimingStage>,
    native_stages: Vec<TimingStage>,
}

#[derive(Clone, Serialize)]
struct SolutionProbabilityRecord {
    solution_index: usize,
    normalized_solution_key: String,
    colored_field_key: Option<String>,
    candidate_ids: Vec<String>,
    representative_candidate_id: String,
    operation_count: usize,
    cleared_lines: u8,
    covered_pattern_count: usize,
    coverage_probability: String,
    pattern_bitset_words: Vec<String>,
    fumen_page_index: Option<usize>,
}

struct SolutionAggregate {
    candidate_ids: Vec<u64>,
    representative: CPackingCandidate,
    coverage: PatternBitSet,
}

struct PartialSolutionAggregate {
    candidate_ids: Vec<u64>,
    representative_candidate_id: u64,
    representative_candidate_index: usize,
    coverage: PatternBitSet,
}

struct SolutionEntry {
    key: String,
    aggregate: SolutionAggregate,
}

const ARTIFACT_AGGREGATION_CHUNK_SIZE: usize = 4_096;

pub fn run_and_write(args: ArtifactArgs) -> Result<ArtifactOutcome, String> {
    let total_started = Instant::now();
    fs::create_dir_all(&args.output_directory).map_err(|error| {
        format!(
            "failed to create output directory {}: {error}",
            args.output_directory.display()
        )
    })?;

    let scenario = ArtifactScenarioSpec::from_args(&args);
    let compile_started = Instant::now();
    let problem = ProblemCompiler::compile_scenario_pc(&scenario.query(&args))
        .map_err(|error| format!("failed to compile SearchProblem: {error:?}"))?;
    let problem_compile_ns = elapsed_ns(compile_started);
    report_completed_stage(&args, "problem-compile", problem_compile_ns);
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or_else(|| "SearchProblem has no materialized pattern universe".to_owned())?;
    let pattern_count = universe.pattern_count();
    #[cfg(feature = "stage-profiling")]
    let executor_profile = args
        .profile_stages
        .then(ExecutorSearchProfileSession::start)
        .transpose()
        .map_err(|error| format!("executor profiler could not start: {error:?}"))?;
    #[cfg(feature = "stage-profiling")]
    let native_profile = args
        .profile_stages
        .then(NativeSearchProfileSession::start)
        .transpose()
        .map_err(|error| format!("native profiler could not start: {error:?}"))?;
    #[cfg(not(feature = "stage-profiling"))]
    if args.profile_stages {
        return Err(
            "--profile-stages requires a binary built with the stage-profiling feature".to_owned(),
        );
    }
    let packing_started = Instant::now();
    let packing = PackingRunner::run(&problem)
        .map_err(|error| format!("packing execution failed: {error:?}"))?;
    let packing_wall_ns = elapsed_ns(packing_started);
    report_completed_stage(&args, "packing", packing_wall_ns);
    let buildup_started = Instant::now();
    let buildup = BuildUpRunner::run(&problem, &packing)
        .map_err(|error| format!("BuildUp execution failed: {error:?}"))?;
    let buildup_wall_ns = elapsed_ns(buildup_started);
    report_completed_stage(&args, "buildup", buildup_wall_ns);
    #[cfg(feature = "stage-profiling")]
    let native_stages = native_profile
        .map(NativeSearchProfileSession::finish)
        .unwrap_or_default()
        .into_iter()
        .map(|stage| TimingStage {
            name: stage.name,
            duration_ns: stage.duration_ns,
            invocation_count: stage.invocation_count,
            work_item_count: stage.work_item_count,
        })
        .collect::<Vec<_>>();
    #[cfg(not(feature = "stage-profiling"))]
    let native_stages = Vec::new();
    #[cfg(feature = "stage-profiling")]
    let executor_stages = executor_profile
        .map(ExecutorSearchProfileSession::finish)
        .unwrap_or_default()
        .into_iter()
        .map(|stage| TimingStage {
            name: stage.name.to_owned(),
            duration_ns: stage.duration_ns,
            invocation_count: stage.invocation_count,
            work_item_count: stage.work_item_count,
        })
        .collect::<Vec<_>>();
    #[cfg(not(feature = "stage-profiling"))]
    let executor_stages = Vec::new();

    let probability_started = Instant::now();
    let candidate_coverage = coverage_by_candidate(&problem, &buildup)?;
    let postprocess_workers = packing.backend_report().workers_used().max(1);
    let (solutions, accepted_candidate_count) = aggregate_solutions_parallel(
        &problem,
        &packing,
        &buildup,
        &candidate_coverage,
        pattern_count,
        postprocess_workers,
    )?;
    if solutions.len() != buildup.normalized_unique_solution_count() {
        return Err(format!(
            "artifact solution aggregation produced {} keys but BuildUp reports {}",
            solutions.len(),
            buildup.normalized_unique_solution_count()
        ));
    }

    let mut overall_coverage = PatternBitSet::new(pattern_count);
    for solution in &solutions {
        overall_coverage
            .union_with(&solution.aggregate.coverage)
            .map_err(|error| format!("overall coverage union failed: {error:?}"))?;
    }
    let overall_probability = union_probability(&overall_coverage, universe.weights())
        .map_err(|error| format!("overall probability calculation failed: {error:?}"))?;
    let probability_postprocess_ns = elapsed_ns(probability_started);
    report_completed_stage(&args, "probability-postprocess", probability_postprocess_ns);

    let fumen_started = Instant::now();
    let mut pages = Vec::with_capacity(solutions.len());
    let mut records = Vec::with_capacity(solutions.len());
    let mut coverage_counts_by_colored_field = BTreeMap::new();
    let mut colored_field_identity_unique = true;
    let needs_colored_field = args.emit_fumen || args.sfinder_html.is_some();
    for (solution_index, solution) in solutions.iter().enumerate() {
        let key = &solution.key;
        let aggregate = &solution.aggregate;
        let placements = candidate_placements(&aggregate.representative)?;
        let probability = union_probability(&aggregate.coverage, universe.weights())
            .map_err(|error| format!("solution probability calculation failed: {error:?}"))?;
        let covered_pattern_count = aggregate.coverage.count_ones() as usize;
        let page = needs_colored_field
            .then(|| {
                ColoredSolutionPage::new(
                    problem.initial_board().width() as u8,
                    problem.visible_height() as u8,
                    problem.initial_board().occupied_mask(),
                    placements
                        .iter()
                        .map(|placement| {
                            ColoredSolutionPlacement::new(placement.piece(), placement.cells_mask())
                        })
                        .collect(),
                )
                .map(|page| {
                    page.with_comment(format!(
                        "Clearra {} solution {} coverage {}/{}",
                        scenario.id,
                        solution_index + 1,
                        covered_pattern_count,
                        pattern_count
                    ))
                })
            })
            .transpose()
            .map_err(|error| format!("failed to construct Fumen page: {error:?}"))?;
        let colored_field_key = page
            .as_ref()
            .map(|page| {
                let single_page_fumen =
                    ColoredSolutionFumenExporter::encode(std::slice::from_ref(page))
                        .map_err(|error| format!("failed to encode solution Fumen: {error:?}"))?;
                let colored = SourceFumenColoredFieldSet::decode(&single_page_fumen)
                    .map_err(|error| format!("failed to verify colored Fumen key: {error:?}"))?;
                colored
                    .keys()
                    .iter()
                    .next()
                    .cloned()
                    .ok_or_else(|| "encoded Fumen has no colored field key".to_owned())
            })
            .transpose()?;
        if let Some(colored_field_key) = colored_field_key.as_ref() {
            if coverage_counts_by_colored_field
                .insert(colored_field_key.clone(), covered_pattern_count)
                .is_some()
            {
                colored_field_identity_unique = false;
            }
        }
        let fumen_page_index = args.emit_fumen.then_some(pages.len());
        records.push(SolutionProbabilityRecord {
            solution_index: solution_index + 1,
            normalized_solution_key: key.to_owned(),
            colored_field_key,
            candidate_ids: aggregate
                .candidate_ids
                .iter()
                .map(|candidate_id| format!("{candidate_id:016x}"))
                .collect(),
            representative_candidate_id: format!("{:016x}", aggregate.representative.candidate_id),
            operation_count: usize::from(aggregate.representative.operation_count),
            cleared_lines: aggregate.representative.cleared_lines,
            covered_pattern_count,
            coverage_probability: format_probability(probability.get()),
            pattern_bitset_words: aggregate
                .coverage
                .words()
                .iter()
                .map(|word| format!("{word:016x}"))
                .collect(),
            fumen_page_index,
        });
        if args.emit_fumen {
            pages.push(page.expect("Fumen output requested a colored page"));
        }
    }
    let fumen_path = if !args.emit_fumen || pages.is_empty() {
        None
    } else {
        let combined_fumen = ColoredSolutionFumenExporter::encode(&pages)
            .map_err(|error| format!("failed to encode combined Fumen: {error:?}"))?;
        let path = args.output_directory.join("solutions.fumen");
        fs::write(&path, &combined_fumen)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        Some(path)
    };
    let fumen_postprocess_ns = elapsed_ns(fumen_started);
    report_completed_stage(&args, "fumen-postprocess", fumen_postprocess_ns);

    let sfinder_started = Instant::now();
    let sfinder_reference = args
        .sfinder_html
        .as_deref()
        .map(|path| {
            if !colored_field_identity_unique {
                return Err(
                    "Sfinder colored-field comparison is non-injective for this solution set"
                        .to_owned(),
                );
            }
            let reference = SfinderReference::read(path)?;
            if reference.input_sequence_count != pattern_count {
                return Err(format!(
                    "Sfinder input universe has {} sequences, Clearra materialized {pattern_count}",
                    reference.input_sequence_count
                ));
            }
            Ok(reference.compare(
                path,
                args.rule.as_str(),
                args.rule == ArtifactRule::SrsPlus,
                &coverage_counts_by_colored_field,
            ))
        })
        .transpose()?;
    let sfinder_validation_ns = elapsed_ns(sfinder_started);

    let full_pattern_universe_materialized =
        universe.complete() && universe.total_possible_pattern_count() == pattern_count as u128;
    let applicable_expected_solution_count = full_pattern_universe_materialized
        .then_some(scenario.expected_unique_solution_count)
        .flatten();
    let expected_matches =
        applicable_expected_solution_count.is_none_or(|expected| expected == solutions.len());
    let probability_requested = scenario.count_mode == ArtifactCountMode::All;
    let probability_complete = probability_requested
        && problem.piece_source().complete()
        && packing.resource_report().probability_complete
        && buildup.coverage_complete()
        && buildup.count_complete();
    let incomplete_reason = if !probability_requested {
        Some("count_unique_solution_set_only".to_owned())
    } else if !problem.piece_source().complete() {
        Some("piece_source_incomplete".to_owned())
    } else if !packing.resource_report().probability_complete {
        Some("packing_resource_truncated".to_owned())
    } else if !buildup.coverage_complete() {
        Some("buildup_coverage_incomplete".to_owned())
    } else {
        None
    };
    if probability_complete {
        let reported = buildup
            .coverage_probability()
            .parse::<f64>()
            .map_err(|_| "BuildUp returned a non-numeric coverage probability".to_owned())?;
        if (reported - overall_probability.get()).abs() > 1e-12 {
            return Err(format!(
                "postprocess union probability {} differs from BuildUp objective {}",
                overall_probability.get(),
                buildup.coverage_probability()
            ));
        }
    }

    let timings = TimingSummary {
        problem_compile_ns,
        packing_wall_ns,
        buildup_wall_ns,
        probability_postprocess_ns,
        fumen_postprocess_ns,
        sfinder_validation_ns,
        total_wall_ns: elapsed_ns(total_started),
        executor_stages,
        native_stages,
    };
    let report = packing.backend_report();
    let resource = packing.resource_report();
    let memory = packing.memory_report();
    let summary = ArtifactSummary {
        schema_version: 2,
        scenario: scenario.id.to_owned(),
        rule_profile: args.rule.as_str().to_owned(),
        count_mode: scenario.count_mode.as_str().to_owned(),
        initial_board_mask: format!("{:016x}", scenario.initial_board_mask),
        visible_height: scenario.visible_height,
        expected_unique_solution_count: applicable_expected_solution_count,
        actual_unique_solution_count: solutions.len(),
        normalized_solution_set_hash: buildup.normalized_solution_set_hash().to_owned(),
        backend: BackendSummary {
            requested: args.backend.as_str().to_owned(),
            selected: packing.actual_backend().as_str().to_owned(),
            selection_reason: report.selection_reason().as_str().to_owned(),
            workers_requested: args.workers,
            workers_used: report.workers_used(),
            fallback_used: report.backend_fallback_used(),
            fallback_reason: report
                .fallback_reason()
                .map(|reason| reason.as_str().to_owned()),
            fallback_backend: report
                .gpu_failure()
                .and_then(|failure| failure.fallback_backend())
                .map(|backend| backend.as_str().to_owned()),
            gpu_failure_class: report
                .gpu_failure()
                .map(|failure| failure.class().as_str().to_owned()),
            gpu_failure_stage: report
                .gpu_failure()
                .map(|failure| failure.stage().as_str().to_owned()),
            discarded_partial_gpu_result: report
                .gpu_failure()
                .is_some_and(|failure| failure.discarded_partial_gpu_result()),
            original_gpu_result_incomplete: report
                .gpu_failure()
                .is_some_and(|failure| failure.original_gpu_result_incomplete()),
            trust_state: packing.trust_report().state().as_str().to_owned(),
            cpu_confirmed: packing.trust_report().cpu_confirmed(),
            exact_probability_authorized: packing.trust_report().can_source_exact_probability(),
            gpu_device_requested: args.gpu_device.as_display_string(),
            gpu_device_selected_index: report
                .gpu_device()
                .and_then(|device| device.selected_index()),
            gpu_device_selected_name: report
                .gpu_device()
                .and_then(|device| device.selected_name())
                .map(str::to_owned),
            gpu_device_selected_type: report
                .gpu_device()
                .and_then(|device| device.selected_device_type())
                .map(str::to_owned),
            gpu_device_selected_backend: report
                .gpu_device()
                .and_then(|device| device.selected_backend())
                .map(str::to_owned),
            gpu_device_selected_vendor: report
                .gpu_device()
                .and_then(|device| device.selected_vendor()),
            gpu_device_selected_device: report
                .gpu_device()
                .and_then(|device| device.selected_device()),
        },
        supply: SupplySummary {
            piece_source_id: format!("{:016x}", problem.piece_source().id().get()),
            pattern_universe_id: format!("{:016x}", universe.pattern_universe_id().get()),
            pattern_weight_model_id: format!("{:016x}", universe.pattern_weight_model_id().get()),
            materialized_pattern_count: pattern_count,
            total_possible_pattern_count: universe.total_possible_pattern_count().to_string(),
            materialized_probability_mass: format_probability(
                universe.materialized_probability_mass().get(),
            ),
            complete: universe.complete(),
            truncation_reason: universe
                .truncation_reason()
                .map(|reason| reason.as_str().to_owned()),
            renormalized: false,
        },
        search: SearchSummary {
            packing_candidate_count: packing.candidate_count(),
            accepted_candidate_count,
            coverage_row_count: buildup.coverage_row_count(),
            build_variant_count: buildup.build_variant_count(),
            pattern_verified_execution_count: buildup.pattern_verified_execution_count(),
            count_complete: buildup.count_complete(),
            coverage_complete: buildup.coverage_complete(),
            resource_truncated: resource.truncated,
            resource_truncation_reason: resource
                .truncation_reason
                .map(|reason| reason.as_str().to_owned()),
            peak_frontier_states: resource.peak_frontier_states,
            peak_candidate_rows: resource.peak_candidate_rows,
            peak_cpu_bytes: resource
                .peak_cpu_bytes
                .saturating_add(buildup.peak_workspace_bytes()),
            peak_gpu_bytes: resource.peak_gpu_bytes,
            retained_search_bytes: memory
                .retained_search_bytes()
                .saturating_add(buildup.peak_workspace_bytes()),
            buildup_workspace_bytes: buildup.peak_workspace_bytes(),
        },
        probability: ProbabilitySummary {
            requested: probability_requested,
            source: "pattern-bitset-or-union".to_owned(),
            pattern_count,
            covered_pattern_count: overall_coverage.count_ones() as usize,
            coverage_probability: format_probability(overall_probability.get()),
            probability_complete,
            incomplete_reason,
            score_does_not_change_probability_union: true,
            renormalized: false,
        },
        fumen: FumenSummary {
            path: fumen_path.as_ref().map(|path| path.display().to_string()),
            page_count: pages.len(),
            generated_after_search: fumen_path.is_some(),
            roundtrip_verified: fumen_path.is_some(),
            normalized_identity_authoritative: true,
            colored_field_identity_unique,
            not_generated_reason: (!args.emit_fumen)
                .then(|| "disabled_by_request".to_owned())
                .or_else(|| {
                    pages
                        .is_empty()
                        .then(|| "no_solutions_to_encode".to_owned())
                }),
        },
        sfinder_reference,
        timings: timings.clone(),
    };

    write_json(&args.output_directory.join("summary.json"), &summary)?;
    write_json(&args.output_directory.join("timings.json"), &timings)?;
    write_json_lines(
        &args.output_directory.join("solution-probabilities.jsonl"),
        &records,
    )?;

    if !expected_matches {
        return Err(format!(
            "{} expected {} unique solutions but produced {}",
            scenario.id,
            applicable_expected_solution_count.unwrap_or(0),
            solutions.len()
        ));
    }
    if summary
        .sfinder_reference
        .as_ref()
        .is_some_and(|reference| !reference.validation_passed)
    {
        return Err(format!(
            "Sfinder comparison failed; inspect {}",
            args.output_directory.join("summary.json").display()
        ));
    }

    Ok(ArtifactOutcome {
        output_directory: args.output_directory,
        solution_count: solutions.len(),
        covered_pattern_count: overall_coverage.count_ones() as usize,
        pattern_count,
        coverage_probability: format_probability(overall_probability.get()),
    })
}

fn report_completed_stage(args: &ArtifactArgs, name: &str, duration_ns: u64) {
    if args.profile_stages {
        eprintln!("profile stage complete | name={name} | duration_ns={duration_ns}");
    }
}

fn aggregate_solutions_parallel(
    problem: &clearra_problem::SearchProblem,
    packing: &PackingRunResult,
    buildup: &BuildUpRunResult,
    candidate_coverage: &HashMap<u64, PatternBitSet>,
    pattern_count: usize,
    requested_workers: usize,
) -> Result<(Vec<SolutionEntry>, usize), String> {
    let result_count = buildup.candidate_result_count();
    if result_count != packing.candidate_count() {
        return Err(format!(
            "BuildUp result count {} differs from packing candidate count {}",
            result_count,
            packing.candidate_count()
        ));
    }
    if result_count == 0 {
        return Ok((Vec::new(), 0));
    }
    let worker_count = requested_workers.min(result_count).max(1);
    let next_index = AtomicUsize::new(0);
    let initial_board_mask = problem.initial_board().occupied_mask();
    let partials = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            handles.push(scope.spawn(|| {
                let mut solutions = HashMap::<
                    StandardBoard64TilingIdentity,
                    PartialSolutionAggregate,
                >::new();
                let mut accepted_count = 0_usize;
                loop {
                    let start =
                        next_index.fetch_add(ARTIFACT_AGGREGATION_CHUNK_SIZE, Ordering::Relaxed);
                    if start >= result_count {
                        break;
                    }
                    let end = start
                        .saturating_add(ARTIFACT_AGGREGATION_CHUNK_SIZE)
                        .min(result_count);
                    for candidate_index in start..end {
                        let candidate = packing.candidate_view_at(candidate_index).ok_or_else(|| {
                            format!("packing candidate {candidate_index} is unavailable")
                        })?;
                        let candidate_id = candidate.candidate_id();
                        let Some(accepted) =
                            buildup.candidate_succeeded(candidate_index, candidate_id)
                        else {
                            return Err(format!(
                                "packing candidate {} identity {:016x} differs from BuildUp acceptance",
                                candidate_index,
                                candidate_id,
                            ));
                        };
                        if !accepted {
                            continue;
                        }
                        let identity = candidate
                            .standard_board64_tiling_identity(initial_board_mask)
                            .map_err(|error| {
                                format!("candidate {candidate_id:016x} identity failed: {error:?}")
                            })?;
                        let coverage = candidate_coverage
                            .get(&candidate_id)
                            .cloned()
                            .unwrap_or_else(|| PatternBitSet::new(pattern_count));
                        match solutions.entry(identity) {
                            std::collections::hash_map::Entry::Occupied(mut entry) => {
                                let aggregate = entry.get_mut();
                                aggregate.candidate_ids.push(candidate_id);
                                if candidate_id < aggregate.representative_candidate_id {
                                    aggregate.representative_candidate_id = candidate_id;
                                    aggregate.representative_candidate_index = candidate_index;
                                }
                                aggregate.coverage.union_with(&coverage).map_err(|error| {
                                    format!("solution coverage union failed: {error:?}")
                                })?;
                            }
                            std::collections::hash_map::Entry::Vacant(entry) => {
                                entry.insert(PartialSolutionAggregate {
                                    candidate_ids: vec![candidate_id],
                                    representative_candidate_id: candidate_id,
                                    representative_candidate_index: candidate_index,
                                    coverage,
                                });
                            }
                        }
                        accepted_count = accepted_count.saturating_add(1);
                    }
                }
                Ok((solutions, accepted_count))
            }));
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| "artifact aggregation worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;

    let accepted_candidate_count = partials
        .iter()
        .map(|(_, accepted_count)| *accepted_count)
        .sum();
    let mut partial_maps = partials
        .into_iter()
        .map(|(solutions, _)| solutions)
        .collect::<Vec<_>>();
    partial_maps.sort_unstable_by_key(|solutions| std::cmp::Reverse(solutions.len()));
    let mut partial_maps = partial_maps.into_iter();
    let mut merged = partial_maps.next().unwrap_or_default();
    for partial in partial_maps {
        merged.reserve(partial.len());
        for (identity, incoming) in partial {
            match merged.entry(identity) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let aggregate = entry.get_mut();
                    aggregate.candidate_ids.extend(incoming.candidate_ids);
                    if incoming.representative_candidate_id < aggregate.representative_candidate_id
                    {
                        aggregate.representative_candidate_id =
                            incoming.representative_candidate_id;
                        aggregate.representative_candidate_index =
                            incoming.representative_candidate_index;
                    }
                    aggregate
                        .coverage
                        .union_with(&incoming.coverage)
                        .map_err(|error| format!("solution coverage merge failed: {error:?}"))?;
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(incoming);
                }
            }
        }
    }

    let materialize_workers = worker_count.min(merged.len().max(1));
    let mut buckets = (0..materialize_workers)
        .map(|_| Vec::with_capacity(merged.len().div_ceil(materialize_workers)))
        .collect::<Vec<_>>();
    for (index, item) in merged.into_iter().enumerate() {
        buckets[index % materialize_workers].push(item);
    }
    let partial_entries = std::thread::scope(|scope| {
        let handles = buckets
            .into_iter()
            .map(|bucket| {
                scope.spawn(move || {
                    bucket
                        .into_iter()
                        .map(|(identity, mut partial)| {
                            partial.candidate_ids.sort_unstable();
                            partial.candidate_ids.dedup();
                            let representative = packing
                                .candidate_at(partial.representative_candidate_index)
                                .ok_or_else(|| {
                                    format!(
                                        "representative candidate {} is unavailable",
                                        partial.representative_candidate_index
                                    )
                                })?;
                            Ok(SolutionEntry {
                                key: NormalizedTilingSolutionKey::from_standard_board64_identity(
                                    identity,
                                )
                                .to_string(),
                                aggregate: SolutionAggregate {
                                    candidate_ids: partial.candidate_ids,
                                    representative,
                                    coverage: partial.coverage,
                                },
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| "artifact materialization worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let mut solutions = Vec::with_capacity(partial_entries.iter().map(Vec::len).sum::<usize>());
    for mut entries in partial_entries {
        solutions.append(&mut entries);
    }
    solutions.sort_unstable_by(|left, right| left.key.cmp(&right.key));
    Ok((solutions, accepted_candidate_count))
}

fn coverage_by_candidate(
    problem: &clearra_problem::SearchProblem,
    buildup: &clearra_core_executor::BuildUpRunResult,
) -> Result<HashMap<u64, PatternBitSet>, String> {
    let source = problem.piece_source();
    let universe_id = source
        .pattern_universe_id()
        .ok_or_else(|| "piece source has no pattern universe identity".to_owned())?;
    let weight_model_id = source
        .pattern_weight_model_id()
        .ok_or_else(|| "piece source has no weight model identity".to_owned())?;
    let mut result = HashMap::<u64, PatternBitSet>::new();
    for row in buildup.coverage_rows() {
        if row.piece_source_id() != source.id().get()
            || row.pattern_universe_id() != universe_id
            || row.pattern_weight_model_id() != weight_model_id
        {
            return Err("coverage row identity differs from the SearchProblem source".to_owned());
        }
        match result.get_mut(&row.candidate_id()) {
            Some(bits) => bits
                .union_with(row.coverage_bits())
                .map_err(|error| format!("candidate coverage union failed: {error:?}"))?,
            None => {
                result.insert(row.candidate_id(), row.coverage_bits().clone());
            }
        }
    }
    Ok(result)
}

fn candidate_placements(candidate: &CPackingCandidate) -> Result<Vec<PiecePlacementMask>, String> {
    let operation_count = usize::from(candidate.operation_count);
    if operation_count > candidate.operations.len() {
        return Err("candidate operation count exceeds its operation storage".to_owned());
    }
    candidate.operations[..operation_count]
        .iter()
        .map(|operation| {
            piece_from_code(operation.piece)
                .map(|piece| PiecePlacementMask::new(piece, operation.mask))
                .ok_or_else(|| format!("unknown C piece code: {}", operation.piece))
        })
        .collect()
}

const fn piece_from_code(code: u8) -> Option<PieceKind> {
    match code {
        clearra_core_ffi::problem::C_PIECE_I => Some(PieceKind::I),
        clearra_core_ffi::problem::C_PIECE_O => Some(PieceKind::O),
        clearra_core_ffi::problem::C_PIECE_T => Some(PieceKind::T),
        clearra_core_ffi::problem::C_PIECE_S => Some(PieceKind::S),
        clearra_core_ffi::problem::C_PIECE_Z => Some(PieceKind::Z),
        clearra_core_ffi::problem::C_PIECE_J => Some(PieceKind::J),
        clearra_core_ffi::problem::C_PIECE_L => Some(PieceKind::L),
        _ => None,
    }
}

fn format_probability(value: f64) -> String {
    let formatted = format!("{value:.15}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

fn write_json(path: &std::path::Path, value: &impl Serialize) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    serde_json::to_writer_pretty(BufWriter::new(file), value)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn write_json_lines(
    path: &std::path::Path,
    records: &[SolutionProbabilityRecord],
) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    let mut writer = BufWriter::new(file);
    for record in records {
        serde_json::to_writer(&mut writer, record)
            .map_err(|error| format!("failed to serialize {}: {error}", path.display()))?;
        writer
            .write_all(b"\n")
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    }
    writer
        .flush()
        .map_err(|error| format!("failed to flush {}: {error}", path.display()))
}
