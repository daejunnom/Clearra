//! End-to-end preservation of placement identities, supply coverage and path
//! multiplicity when collision-footprint relations are reused.

use std::sync::{Mutex, MutexGuard};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::placement_dependency::{with_mode, PlacementDependencyMetrics};
use crate::{CoreExecutionResult, WasmCpuSearchBackend};

// The mode and counters are thread-local. These fixtures also share the
// process-wide terminal resource authority, so serialize this test family.
static PRODUCT_TEST_LOCK: Mutex<()> = Mutex::new(());

fn product_test_guard() -> MutexGuard<'static, ()> {
    PRODUCT_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn execution_policy() -> PcExecutionPolicy {
    // Keep both A/B executions on the thread that owns the scoped test mode.
    // This is fixture configuration, not a change to product worker policy.
    PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(1)
        .with_max_memory_mib(Some(256))
}

fn fixed_query(
    height: u16,
    board: u64,
    pieces: Vec<PieceKind>,
    count: PcCountPolicy,
) -> PcScenarioQuery {
    let piece_count = pieces.len();
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(height, board),
        PcQueueInput::fixed_sequence(FixedSequence::new(pieces)),
        PieceWindow::new(piece_count),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(piece_count))
    .with_count_policy(count)
    .with_retained_trace_limit(1)
    .with_execution_policy(execution_policy())
}

fn compare_modes(
    label: &str,
    problem: &SearchProblem,
) -> (
    CoreExecutionResult,
    CoreExecutionResult,
    PlacementDependencyMetrics,
) {
    let run = || {
        WasmCpuSearchBackend::execute_with_control(problem, &ExecutionControl::default())
            .unwrap_or_else(|error| panic!("{label}: exact search failed: {error:?}"))
    };
    let (baseline, disabled_metrics) = with_mode(false, run);
    let (reused, enabled_metrics) = with_mode(true, run);

    assert_eq!(disabled_metrics.reused_relations, 0, "{label}: mode off");
    assert_eq!(disabled_metrics.cross_board_reuses, 0, "{label}: mode off");
    assert_exact_result_parity(label, &baseline, &reused);

    // Hard-drop successes deliberately bypass the new relation query. The
    // four-piece bag fixture and exact-lock tests require real reuse; simpler
    // fixtures must not require a different search strategy just to get hits.
    eprintln!(
        "{label}: queries={}, generated={}, reverse_relations={}, full_relations={}, reused={}, cross_board={}, independent_edges={}, frame_misses={}, static_masks={}, static_independent_cells={}, catalogs={}, row_conditions={}, clear_domains={}",
        enabled_metrics.queries,
        enabled_metrics.generated_relations,
        enabled_metrics.reverse_relations,
        enabled_metrics.full_relations,
        enabled_metrics.reused_relations,
        enabled_metrics.cross_board_reuses,
        enabled_metrics.independent_placement_edges,
        enabled_metrics.frame_misses,
        enabled_metrics.static_masks_compiled,
        enabled_metrics.static_independent_cells,
        enabled_metrics.catalogs_compiled,
        enabled_metrics.row_conditions_specialized,
        enabled_metrics.shared_clear_domains,
    );
    eprintln!(
        "{label}: catalog_placements={}, unique_clear_domains={}, row_condition_groups={}, atomic_row_groups={}",
        enabled_metrics.catalog_placements,
        enabled_metrics.unique_clear_domains,
        enabled_metrics.row_condition_groups,
        enabled_metrics.atomic_row_groups,
    );
    for (mode, result, metrics) in [
        ("baseline", &baseline, &disabled_metrics),
        ("dependency", &reused, &enabled_metrics),
    ] {
        eprintln!(
            "{label} {mode}: normalized={}, build_variants={}, covered_patterns={}/{}, probability={}, total_reachability_states={}, peak_reachability_states={}, primitive_states={}",
            result.field("normalized_unique_solution_count").unwrap(),
            result.field("build_variant_count").unwrap(),
            result.field("covered_pattern_count").unwrap(),
            result.field("coverage_pattern_count").unwrap(),
            result.field("coverage_probability").unwrap(),
            result.field("total_reachability_states").unwrap(),
            result.field("peak_reachability_states").unwrap(),
            metrics.primitive_states,
        );
    }
    (baseline, reused, enabled_metrics)
}

fn assert_exact_result_parity(
    label: &str,
    baseline: &CoreExecutionResult,
    reused: &CoreExecutionResult,
) {
    assert!(
        baseline.solution_found(),
        "{label}: nonempty oracle fixture"
    );
    assert!(
        !baseline.normalized_solution_identities().is_empty(),
        "{label}: compare full placement identities"
    );
    assert!(
        baseline
            .coverage_pattern_words()
            .iter()
            .any(|&word| word != 0),
        "{label}: compare nonempty supply coverage"
    );
    assert_eq!(
        reused.normalized_solution_identities(),
        baseline.normalized_solution_identities(),
        "{label}: placement identities"
    );
    assert_eq!(
        reused.normalized_solution_keys(),
        baseline.normalized_solution_keys(),
        "{label}: placement keys"
    );
    assert_eq!(
        reused.coverage_pattern_words(),
        baseline.coverage_pattern_words(),
        "{label}: supply coverage bits"
    );
    assert_eq!(
        reused.solution_coverages(),
        baseline.solution_coverages(),
        "{label}: per-solution coverage"
    );
    assert_eq!(
        reused.normalized_solution_coverages(),
        baseline.normalized_solution_coverages(),
        "{label}: canonical per-solution coverage"
    );
    assert_eq!(
        reused.solution_probabilities(),
        baseline.solution_probabilities(),
        "{label}: solution probabilities"
    );
    assert_eq!(
        reused.path_steps(),
        baseline.path_steps(),
        "{label}: replay"
    );
    assert_eq!(
        reused.exact_scoring_execution_batches(),
        baseline.exact_scoring_execution_batches(),
        "{label}: concrete scoring executions"
    );
    assert_eq!(
        reused.postprocess_pattern_weights(),
        baseline.postprocess_pattern_weights(),
        "{label}: scoring pattern weights"
    );
    for field in [
        "unique_solution_count",
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
        "postprocess_scoring_requested",
        "postprocess_execution_complete",
    ] {
        assert_eq!(
            reused.field(field),
            baseline.field(field),
            "{label}: {field}"
        );
    }
    for result in [baseline, reused] {
        assert_eq!(result.bool_field("count_complete"), Some(true), "{label}");
        assert_eq!(
            result.bool_field("probability_complete"),
            Some(true),
            "{label}"
        );
        assert_eq!(
            result.bool_field("resource_truncated"),
            Some(false),
            "{label}"
        );
    }
}

#[test]
fn placement_dependency_preserves_repeated_piece_unique_and_all() {
    let _guard = product_test_guard();
    for count in [PcCountPolicy::CountUnique, PcCountPolicy::CountAll] {
        let query = fixed_query(
            2,
            0,
            vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ],
            count,
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("IIOOO problem");
        // Enabling scoring would silently turn CountUnique into CountAll.
        assert_eq!(problem.count_policy(), count);
        let label = format!("IIOOO {count:?}");
        compare_modes(&label, &problem);
    }
}

#[test]
fn placement_dependency_preserves_same_current_and_held_piece_multiplicity() {
    let _guard = product_test_guard();
    let direct_query = fixed_query(1, 0x3f0, vec![PieceKind::I], PcCountPolicy::CountAll);
    let held_query = direct_query
        .clone()
        .with_allow_hold(true)
        .with_hold_piece(Some(PieceKind::I));
    let direct_problem =
        ProblemCompiler::compile_scenario_pc(&direct_query).expect("current I problem");
    let held_problem =
        ProblemCompiler::compile_scenario_pc(&held_query).expect("current I and held I problem");
    let (_, direct, _) = compare_modes("current I, hold disabled", &direct_problem);
    let (_, held, _) = compare_modes("current I, held I", &held_problem);

    assert_eq!(
        held.normalized_solution_identities(),
        direct.normalized_solution_identities()
    );
    assert_eq!(
        held.coverage_pattern_words(),
        direct.coverage_pattern_words()
    );
    let count = |result: &CoreExecutionResult| {
        result
            .field("build_variant_count")
            .unwrap()
            .parse::<u128>()
            .unwrap()
    };
    assert!(count(&direct) > 0);
    // Using current I and swapping identical held I are distinct supply
    // actions even though they reach the same placement and future state.
    assert_eq!(count(&held), count(&direct) * 2);
}

#[test]
fn placement_dependency_preserves_repeated_piece_after_a_line_clear() {
    let _guard = product_test_guard();
    // Both rows have their left four cells empty. Each horizontal I clears a
    // row; the second operation therefore uses a different deleted-row frame.
    let query = fixed_query(
        2,
        0xfc3f0,
        vec![PieceKind::I, PieceKind::I],
        PcCountPolicy::CountAll,
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("two-I clear problem");
    let (_, result, _) = compare_modes("II with an intermediate line clear", &problem);
    assert_eq!(result.path_steps().len(), 2);
    assert!(result
        .path_steps()
        .iter()
        .all(|step| step.cleared_lines() == 1));
}

#[test]
fn placement_dependency_preserves_standard_bag_count_and_probability() {
    let _guard = product_test_guard();
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0xfc3f_0fc3_f0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(4),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(4))
    .with_supply_window_size(SupplyWindowSize::new(4))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(execution_policy());
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("four-piece bag problem");
    let (baseline, result, metrics) = compare_modes("four-piece standard bag", &problem);
    assert!(result.usize_field("coverage_pattern_count").unwrap() > 1);
    assert_eq!(result.bool_field("build_variant_count_exact"), Some(true));
    assert!(
        metrics.reused_relations > 0,
        "four-piece product search must exercise relation reuse"
    );
    assert!(
        metrics.cross_board_reuses > 0,
        "four-piece product search must reuse across different boards"
    );
    assert!(
        metrics.catalogs_compiled > 0,
        "four-piece product search must compile placement conditions"
    );
    assert!(
        metrics.row_conditions_specialized > 0,
        "four-piece product search must specialize selected row conditions"
    );
    let baseline_states = baseline.usize_field("total_reachability_states").unwrap();
    let reused_states = result.usize_field("total_reachability_states").unwrap();
    assert!(
        reused_states <= baseline_states,
        "four-piece reuse must not expand primitive search: baseline={baseline_states}, dependency={reused_states}"
    );
}

#[test]
fn placement_dependency_preserves_scoring_and_retained_replay() {
    let _guard = product_test_guard();
    let query = fixed_query(1, 0x3f0, vec![PieceKind::I], PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::all().with_score_summary());
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("scored one-I problem");
    let (_, result, _) = compare_modes("scored I", &problem);
    assert!(!result.path_steps().is_empty());
    assert!(result.exact_scoring_execution_batch().is_some());
    assert_eq!(
        result.bool_field("postprocess_scoring_requested"),
        Some(true)
    );
}
