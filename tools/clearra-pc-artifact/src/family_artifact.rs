//! Export exact normalized families through the Rust CPU solver, not the
//! legacy candidate-buffer numbering and phase timings of the C diagnostic.

use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    solution::{NormalizedTilingSolutionKey, StandardBoard64TilingIdentity},
};
use clearra_core_executor::{CoreExecutionResult, SolutionCoverage, WasmCpuSearchBackend};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_fumen::{ColoredSolutionFumenExporter, ColoredSolutionPage, ColoredSolutionPlacement};
use clearra_pc_graph::request::{
    GpuDeviceSelection, PcSolutionProbabilityPolicy, RequestedSearchBackend,
};
use clearra_problem::{compile::problem_compiler::ProblemCompiler, SearchProblem};
use serde::Serialize;

use crate::{
    args::{ArtifactArgs, ArtifactCountMode},
    scenario::ArtifactScenarioSpec,
};

const FUMEN_BATCH_PAGES: usize = 1_024;
const ENGINE: &str = "rust-cpu";

#[derive(Serialize)]
struct FamilySummary<'a> {
    schema: &'static str,
    execution_engine: &'static str,
    scenario: &'static str,
    rule_profile: &'static str,
    count_mode: &'static str,
    initial_board_mask: String,
    visible_height: u16,
    expected_unique_solution_count: Option<usize>,
    actual_unique_solution_count: usize,
    normalized_solution_set_hash: &'a str,
    pattern_universe_id: String,
    pattern_weight_model_id: String,
    materialized_pattern_count: usize,
    total_possible_pattern_count: String,
    supply_complete: bool,
    coverage_complete: bool,
    coverage_probability: Option<f64>,
    covered_pattern_count: Option<u32>,
    workers_requested: usize,
    workers_used: usize,
    search_wall_ns: u128,
    fumen_page_count: usize,
}

#[derive(Serialize)]
struct FamilyRow {
    normalized_solution_key: String,
    covered_pattern_count: Option<u32>,
    probability: Option<f64>,
    pattern_bitset_words: Option<Vec<String>>,
}

pub fn run(args: ArtifactArgs) -> Result<(), String> {
    if !matches!(
        args.backend,
        RequestedSearchBackend::Cpu | RequestedSearchBackend::Auto
    ) || args.gpu_device != GpuDeviceSelection::Auto
        || args.list_gpu_devices
        || args.probe_gpu_warmup
        || args.prewarm_gpu
        || args.profile_stages
        || args.sfinder_html.is_some()
    {
        return Err(
            "Rust family export accepts CPU/Auto only, without GPU, stage-profile or Sfinder options; the native C diagnostic retains those options"
                .to_owned(),
        );
    }
    for scenario in args.scenarios.iter().copied() {
        let mut case_args = args.clone();
        case_args.scenario = scenario;
        let output = if args.scenarios.len() == 1 {
            args.output_directory.clone()
        } else {
            args.output_directory.join(scenario.as_str())
        };
        run_and_write(&case_args, &output)?;
    }
    Ok(())
}

fn run_and_write(args: &ArtifactArgs, output: &Path) -> Result<(), String> {
    let scenario = ArtifactScenarioSpec::from_args(args);
    let coverage_requested = scenario.count_mode == ArtifactCountMode::All;
    let query = scenario.query(args);
    let policy = query
        .execution_policy()
        .clone()
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false);
    let query = query
        .with_execution_policy(policy)
        .with_retained_trace_limit(0)
        .with_solution_probability_policy(if coverage_requested {
            PcSolutionProbabilityPolicy::Include
        } else {
            PcSolutionProbabilityPolicy::Omit
        });
    let problem = ProblemCompiler::compile_scenario_pc(&query)
        .map_err(|error| format!("Rust family problem compilation failed: {error:?}"))?;
    let started_at = Instant::now();
    let result = WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
        .map_err(|error| format!("Rust family search failed: {error:?}"))?;
    let search_wall_ns = started_at.elapsed().as_nanos();
    let identities = check_family(&result)?;
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or("missing executed pattern universe")?;
    let pattern_count = universe.pattern_count();
    let initial_board_mask = problem.initial_board().occupied_mask();
    if identities.iter().any(|identity| {
        identity.initial_board_mask() != initial_board_mask
            || identity.placement_count() != scenario.piece_window
    }) {
        return Err("a Rust identity does not belong to the compiled scenario".to_owned());
    }
    let coverage_rows = coverage_requested.then(|| result.solution_coverages());
    let union = coverage_rows
        .map(|rows| check_coverage_rows(&result, identities, rows, pattern_count))
        .transpose()?;
    let coverage_probability = union
        .as_ref()
        .map(|bits| {
            let probability = universe
                .weights()
                .covered_weight(bits)
                .ok_or("coverage and pattern weights disagree")?
                .get();
            let reported = result
                .unique_field("coverage_probability")
                .and_then(|text| text.parse::<f64>().ok())
                .ok_or("missing Rust terminal coverage probability")?;
            if (probability - reported).abs() > 1e-12 || !reported.is_finite() {
                return Err("checked family union differs from terminal weighted probability");
            }
            Ok(probability)
        })
        .transpose()?;
    let coverage_complete = coverage_requested && universe.complete();
    if coverage_complete
        && (result.unique_field("probability_complete") != Some("true")
            || result.unique_field("solution_probability_complete") != Some("true"))
    {
        return Err("the Rust solver did not prove complete per-solution coverage".to_owned());
    }
    let expected = (universe.complete()
        && universe.total_possible_pattern_count() == pattern_count as u128)
        .then_some(scenario.expected_unique_solution_count)
        .flatten();
    if expected.is_some_and(|expected| expected != identities.len()) {
        return Err(format!(
            "Rust family count {} differs from fixture {expected:?}",
            identities.len()
        ));
    }

    let summary = FamilySummary {
        schema: "clearra.rust-pc-family-artifact.v1",
        execution_engine: ENGINE,
        scenario: scenario.id,
        rule_profile: args.rule.as_str(),
        count_mode: scenario.count_mode.as_str(),
        initial_board_mask: format!("{initial_board_mask:016x}"),
        visible_height: problem.visible_height(),
        expected_unique_solution_count: expected,
        actual_unique_solution_count: identities.len(),
        normalized_solution_set_hash: result
            .unique_field("normalized_solution_set_hash")
            .ok_or("missing Rust terminal family hash")?,
        pattern_universe_id: format!("{:016x}", universe.pattern_universe_id().get()),
        pattern_weight_model_id: format!("{:016x}", universe.pattern_weight_model_id().get()),
        materialized_pattern_count: pattern_count,
        total_possible_pattern_count: universe.total_possible_pattern_count().to_string(),
        supply_complete: universe.complete(),
        coverage_complete,
        coverage_probability,
        covered_pattern_count: union.as_ref().map(PatternBitSet::count_ones),
        workers_requested: args.workers,
        workers_used: result
            .unique_field("workers_used")
            .and_then(|workers| workers.parse().ok())
            .filter(|&workers| workers > 0 && workers <= args.workers)
            .ok_or("missing or invalid Rust terminal worker evidence")?,
        search_wall_ns,
        fumen_page_count: if args.emit_fumen { identities.len() } else { 0 },
    };

    fs::create_dir_all(output).map_err(|error| format!("{}: {error}", output.display()))?;
    write_family_rows(output, &problem, identities, coverage_rows)?;
    if summary.fumen_page_count != 0 {
        write_fumen_batches(output, &problem, identities)?;
    }
    // A summary is the last write; no partial row export is presented as a
    // completed family if encoding or I/O fails. Never replace prior evidence.
    let mut writer = create_new(output.join("family-summary.json"))?;
    serde_json::to_writer_pretty(&mut writer, &summary).map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())?;
    println!(
        "family artifact complete | engine={ENGINE} | output={} | solutions={}",
        output.display(),
        identities.len(),
    );
    Ok(())
}

fn check_family(result: &CoreExecutionResult) -> Result<&[StandardBoard64TilingIdentity], String> {
    if result.unique_field("actual_backend") != Some("wasm-cpu")
        || result.unique_field("resource_truncated") != Some("false")
        || result.unique_field("count_complete") != Some("true")
        || result.unique_field("solution_set_materialized") != Some("true")
    {
        return Err("Rust CPU did not finish a materialized family for this request".to_owned());
    }
    let identities = result.normalized_solution_identities();
    if result.usize_field("normalized_unique_solution_count") != Some(identities.len())
        || !identities.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err("Rust terminal identities are not canonical and complete".to_owned());
    }
    Ok(identities)
}

fn check_coverage_rows(
    result: &CoreExecutionResult,
    identities: &[StandardBoard64TilingIdentity],
    rows: &[SolutionCoverage],
    pattern_count: usize,
) -> Result<PatternBitSet, String> {
    if rows.len() != identities.len() {
        return Err("Rust terminal coverage does not have one row per identity".to_owned());
    }
    let mut union = PatternBitSet::new(pattern_count);
    for (row, &identity) in rows.iter().zip(identities) {
        if row.identity() != identity || row.covered_patterns().pattern_count() != pattern_count {
            return Err("Rust terminal coverage identity or pattern dimension differs".to_owned());
        }
        union
            .union_with(row.covered_patterns())
            .map_err(|error| format!("coverage union: {error:?}"))?;
    }
    if union.words() != result.coverage_pattern_words() {
        return Err("Rust terminal union differs from its per-identity coverage".to_owned());
    }
    Ok(union)
}

fn write_family_rows(
    output: &Path,
    problem: &SearchProblem,
    identities: &[StandardBoard64TilingIdentity],
    rows: Option<&[SolutionCoverage]>,
) -> Result<(), String> {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or("missing pattern universe")?;
    let mut writer = create_new(output.join("family-solutions.jsonl"))?;
    for (index, &identity) in identities.iter().enumerate() {
        let bits = rows.map(|rows| rows[index].covered_patterns());
        let row = FamilyRow {
            normalized_solution_key: NormalizedTilingSolutionKey::from_standard_board64_identity(
                identity,
            )
            .as_str()
            .to_owned(),
            covered_pattern_count: bits.map(PatternBitSet::count_ones),
            probability: bits
                .map(|bits| {
                    universe
                        .weights()
                        .covered_weight(bits)
                        .map(|weight| weight.get())
                        .ok_or("solution coverage and pattern weights disagree")
                })
                .transpose()?,
            pattern_bitset_words: bits.map(|bits| {
                bits.words()
                    .iter()
                    .map(|word| format!("{word:016x}"))
                    .collect()
            }),
        };
        serde_json::to_writer(&mut writer, &row).map_err(|error| error.to_string())?;
        writer.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn write_fumen_batches(
    output: &Path,
    problem: &SearchProblem,
    identities: &[StandardBoard64TilingIdentity],
) -> Result<(), String> {
    let height = u8::try_from(problem.visible_height()).map_err(|error| error.to_string())?;
    let mut writer = create_new(output.join("family-solutions.fumen"))?;
    for batch in identities.chunks(FUMEN_BATCH_PAGES) {
        let pages = batch
            .iter()
            .map(|identity| {
                let placements = (0..identity.placement_count())
                    .map(|index| {
                        let placement = identity
                            .placement(index)
                            .ok_or("missing canonical placement")?;
                        Ok(ColoredSolutionPlacement::new(
                            placement.piece(),
                            placement.cells_mask(),
                        ))
                    })
                    .collect::<Result<Vec<_>, &str>>()?;
                ColoredSolutionPage::new(10, height, identity.initial_board_mask(), placements)
                    .map_err(|error| format!("invalid canonical Fumen page: {error:?}"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let fumen = ColoredSolutionFumenExporter::encode(&pages)
            .map_err(|error| format!("Rust family Fumen round-trip failed: {error:?}"))?;
        writeln!(writer, "{fumen}").map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn create_new(path: PathBuf) -> Result<BufWriter<File>, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map(BufWriter::new)
        .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_pc_graph::request::{
        PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PieceWindow, SupplyWindowSize,
    };

    fn one_line_bag_case() -> SearchProblem {
        ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(1, 0x3f0),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(1),
            )
            .with_allow_hold(false)
            .with_exact_pieces(Some(1))
            .with_supply_window_size(SupplyWindowSize::new(1))
            .with_count_policy(PcCountPolicy::CountAll)
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
            .with_retained_trace_limit(0)
            .with_execution_policy(
                PcExecutionPolicy::mvp_default()
                    .with_requested_backend(RequestedSearchBackend::Cpu)
                    .with_workers(1),
            ),
        )
        .expect("one-line family test problem")
    }

    #[test]
    fn rust_family_exports_only_the_original_covered_bag_ordinals() {
        use clearra_core_domain::piece::piece_kind::PieceKind;

        let problem = one_line_bag_case();
        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("Rust family result");
        let identities = check_family(&result).expect("complete Rust family");
        assert_eq!(identities.len(), 1);
        let universe = problem
            .piece_source()
            .materialized_universe()
            .expect("pattern universe");
        assert_eq!(universe.pattern_count(), 7);
        let rows = result.solution_coverages();
        let union = check_coverage_rows(&result, identities, rows, universe.pattern_count())
            .expect("exact coverage rows");
        assert_eq!(union.count_ones(), 1);
        for ordinal in 0..universe.pattern_count() {
            assert_eq!(
                union.contains(PatternId::new(ordinal)),
                universe.sequence_at(ordinal).first() == Some(&PieceKind::I),
                "original bag ordinal {ordinal}",
            );
        }
    }

    #[cfg(feature = "native-c-reference")]
    #[test]
    fn rust_family_matches_the_independent_c_identity_and_pattern_words() {
        let problem = one_line_bag_case();
        let native = clearra_core_executor::CoreExecutor::execute_native_c_reference(&problem)
            .expect("independent C family");
        let rust =
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .expect("Rust family result");
        assert!(!native.normalized_solution_keys().is_empty());
        assert_eq!(
            native.normalized_solution_keys(),
            rust.normalized_solution_keys()
        );
        assert_eq!(native.solution_coverages(), rust.solution_coverages());
    }
}
