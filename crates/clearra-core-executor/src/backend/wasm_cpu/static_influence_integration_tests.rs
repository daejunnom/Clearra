//! Product-level differential tests for static movement influence.
//! The independent primitive/footprint oracles live beside their algorithms;
//! these tests ensure the shortcut preserves supply, identity and multiplicity.

use std::sync::{Mutex, MutexGuard};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem};
use clearra_rules::profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::{placement_dependency, reachability::static_influence, structural_order};
use crate::{CoreExecutionResult, WasmCpuSearchBackend};

static PRODUCT_LOCK: Mutex<()> = Mutex::new(());

fn product_guard() -> MutexGuard<'static, ()> {
    PRODUCT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn policy() -> PcExecutionPolicy {
    // The scoped mode is thread-local. Keep these small differential fixtures
    // on their owning thread; production/benchmark worker policy stays Auto.
    PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(1)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false)
        .with_max_memory_mib(Some(256))
}

fn run(problem: &SearchProblem, enabled: bool) -> CoreExecutionResult {
    run_with_structural(problem, enabled, false).0
}

fn run_with_structural(
    problem: &SearchProblem,
    enabled: bool,
    structural_enabled: bool,
) -> (
    CoreExecutionResult,
    structural_order::StructuralOrderMetrics,
) {
    run_with_modes(problem, enabled, structural_enabled, false)
}

fn run_with_modes(
    problem: &SearchProblem,
    static_enabled: bool,
    structural_enabled: bool,
    translation_enabled: bool,
) -> (
    CoreExecutionResult,
    structural_order::StructuralOrderMetrics,
) {
    placement_dependency::with_mode(true, || {
        structural_order::with_mode(structural_enabled, || {
            structural_order::with_translation_interval_mode(translation_enabled, || {
                static_influence::with_mode(static_enabled, || {
                    WasmCpuSearchBackend::execute_with_control(
                        problem,
                        &ExecutionControl::default(),
                    )
                    .expect("complete exact product fixture")
                })
            })
        })
    })
    .0
}

fn metric(result: &CoreExecutionResult, name: &str) -> usize {
    result
        .usize_field(name)
        .unwrap_or_else(|| panic!("static influence metric is missing: {name}"))
}

fn assert_parity(label: &str, a: &CoreExecutionResult, b: &CoreExecutionResult) {
    assert!(a.solution_found(), "{label}: nonempty fixture");
    assert!(
        !a.normalized_solution_identities().is_empty(),
        "{label}: full identity comparison must not be vacuous"
    );
    assert!(
        !a.solution_probabilities().is_empty(),
        "{label}: Include must expose actual per-solution probabilities"
    );
    assert_eq!(
        a.normalized_solution_identities(),
        b.normalized_solution_identities(),
        "{label}: concrete canonical identities"
    );
    assert_eq!(
        a.normalized_solution_keys(),
        b.normalized_solution_keys(),
        "{label}: keys"
    );
    assert_eq!(
        a.coverage_pattern_words(),
        b.coverage_pattern_words(),
        "{label}: global coverage"
    );
    assert_eq!(
        a.solution_coverages(),
        b.solution_coverages(),
        "{label}: each solution's coverage"
    );
    assert_eq!(
        a.normalized_solution_coverages(),
        b.normalized_solution_coverages(),
        "{label}: normalized coverage"
    );
    assert_eq!(
        a.solution_probabilities(),
        b.solution_probabilities(),
        "{label}: each probability"
    );
    assert_eq!(a.path_steps(), b.path_steps(), "{label}: retained replay");
    for field in [
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
    ] {
        assert_eq!(a.field(field), b.field(field), "{label}: {field}");
    }
    for result in [a, b] {
        for field in [
            "count_complete",
            "probability_complete",
            "solution_probability_complete",
        ] {
            assert_eq!(result.bool_field(field), Some(true), "{label}: {field}");
        }
        assert_eq!(
            result.bool_field("resource_truncated"),
            Some(false),
            "{label}"
        );
    }
    for field in [
        "reachability_static_influence_queries",
        "reachability_static_influence_reachable",
        "reachability_static_influence_unreachable",
        "reachability_static_influence_row_steps",
        "reachability_static_influence_cache_hits",
    ] {
        assert_eq!(metric(a, field), 0, "{label}: disabled path has no {field}");
    }
    assert_eq!(
        metric(b, "reachability_static_influence_queries"),
        metric(b, "reachability_static_influence_reachable")
            + metric(b, "reachability_static_influence_unreachable"),
        "{label}: exact answers are partitioned into positive and negative"
    );
    eprintln!(
        "static influence product: {label}; solutions={}, covered={}/{}, primitive_states={}->{}, answered={}, positive={}, negative={}, row_steps={}, cache_hits={}",
        a.normalized_solution_identities().len(),
        a.field("covered_pattern_count").unwrap(),
        a.field("coverage_pattern_count").unwrap(),
        a.field("total_reachability_states").unwrap(),
        b.field("total_reachability_states").unwrap(),
        metric(b, "reachability_static_influence_queries"),
        metric(b, "reachability_static_influence_reachable"),
        metric(b, "reachability_static_influence_unreachable"),
        metric(b, "reachability_static_influence_row_steps"),
        metric(b, "reachability_static_influence_cache_hits"),
    );
}

fn compare(label: &str, query: &PcScenarioQuery) -> (CoreExecutionResult, CoreExecutionResult) {
    let problem = ProblemCompiler::compile_scenario_pc(query).expect("valid product fixture");
    let off = run(&problem, false);
    let on = run(&problem, true);
    assert_parity(label, &off, &on);
    (off, on)
}

fn compare_exact_fallback_with_translation(
    label: &str,
    query: &PcScenarioQuery,
) -> (CoreExecutionResult, CoreExecutionResult) {
    let problem = ProblemCompiler::compile_scenario_pc(query).expect("valid fallback fixture");
    let baseline = run(&problem, false);
    let (treatment, metrics) = run_with_modes(&problem, true, true, true);
    assert_parity(label, &baseline, &treatment);
    assert_eq!(metrics.skipped_build_graphs, 0, "{label}: exact fallback");
    assert_eq!(metrics.translation_interval_attempts, 0);
    assert_eq!(metrics.translation_interval_guards, 0);
    assert_eq!(metrics.translation_interval_row_steps, 0);
    (baseline, treatment)
}

fn assert_summary_probability_matches_ordinary(
    label: &str,
    problem: &SearchProblem,
    ordinary: &CoreExecutionResult,
    summary: &CoreExecutionResult,
) {
    assert!(!ordinary.normalized_solution_identities().is_empty());
    assert!(!ordinary.solution_probabilities().is_empty());
    assert!(summary.normalized_solution_identities().is_empty());
    assert!(summary.solution_probabilities().is_empty());
    assert_eq!(
        summary.coverage_pattern_words(),
        ordinary.coverage_pattern_words(),
        "{label}: global coverage"
    );
    assert_eq!(
        summary.solution_coverages(),
        ordinary.solution_coverages(),
        "{label}: each concrete identity's coverage"
    );
    assert_eq!(
        summary.normalized_solution_coverages(),
        ordinary.normalized_solution_coverages(),
        "{label}: normalized identity coverage"
    );
    for field in [
        "count_complete",
        "probability_complete",
        "solution_probability_complete",
    ] {
        assert_eq!(summary.bool_field(field), Some(true), "{label}: {field}");
    }
    assert_eq!(summary.bool_field("resource_truncated"), Some(false));
    // CoverageSummary intentionally does not expose concrete identity reports.
    // Join its Include coverage to the ordinary run's actual identities, then
    // recompute every weighted probability rather than comparing only totals.
    let reports = crate::solution_probability::probability_reports(
        ordinary.normalized_solution_identities(),
        summary.solution_coverages(),
        problem
            .piece_source()
            .materialized_universe()
            .expect("materialized fixture universe")
            .weights(),
        summary.bool_field("solution_probability_complete") == Some(true),
    );
    assert!(!reports.is_empty(), "{label}: nonvacuous probability proof");
    assert_eq!(
        reports,
        ordinary.solution_probabilities(),
        "{label}: each probability"
    );
}

fn assert_summary_fields_equal(
    label: &str,
    baseline: &CoreExecutionResult,
    treatment: &CoreExecutionResult,
) {
    // Presentation fields are compared within one output policy. The separate
    // ordinary/summary check compares actual identities, coverage and weighted
    // probabilities even when a summary count or hash is not materialized.
    for field in [
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
    ] {
        assert_eq!(
            baseline.field(field),
            treatment.field(field),
            "{label}: summary {field}"
        );
    }
}

fn fixed_query(height: u16, board: u64, pieces: Vec<PieceKind>) -> PcScenarioQuery {
    let count = pieces.len();
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(height, board),
        PcQueueInput::fixed_sequence(FixedSequence::new(pieces)),
        PieceWindow::new(count),
    )
    .with_rule(jstris_180())
    .with_allow_hold(false)
    .with_exact_pieces(Some(count))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_retained_trace_limit(1)
    .with_execution_policy(policy())
}

fn mixed_bag_query(count: PcCountPolicy) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0xfc3f_0fc3_f0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(4),
    )
    .with_rule(srs_plus())
    .with_allow_hold(false)
    .with_exact_pieces(Some(4))
    .with_supply_window_size(SupplyWindowSize::new(4))
    .with_count_policy(count)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_retained_trace_limit(1)
    .with_execution_policy(policy())
}

#[test]
fn static_influence_preserves_every_solution_probability_across_five_kick_profiles() {
    let _guard = product_guard();
    let mut exact_answers = 0;
    let mut o_row_steps = 0;
    for (label, rule) in [
        ("SRS90", srs()),
        ("SRS+", srs_plus()),
        ("SRS-X", srs_x()),
        ("Jstris180", jstris_180()),
        ("NoKick", no_kick()),
    ] {
        let query = mixed_bag_query(PcCountPolicy::CountUnique).with_rule(rule);
        let (baseline, treatment) = compare(label, &query);
        assert_eq!(baseline.usize_field("coverage_pattern_count"), Some(840));
        if label == "SRS+" {
            assert_eq!(baseline.normalized_solution_identities().len(), 135);
            assert_eq!(baseline.solution_probabilities().len(), 135);
            assert_eq!(baseline.usize_field("covered_pattern_count"), Some(173));
        }
        exact_answers += metric(&treatment, "reachability_static_influence_queries");
        o_row_steps += metric(&treatment, "reachability_static_influence_row_steps");
    }
    assert!(
        exact_answers > 0,
        "the product must exercise exact static answers"
    );
    assert!(
        o_row_steps > 0,
        "the product must exercise the O row solver"
    );
}

#[test]
fn static_influence_preserves_count_all_same_current_and_held_kind() {
    let _guard = product_guard();
    // Exactly one 2x2 O hole; current and held O remain two distinct supply
    // actions although they produce the same concrete placement and kind word.
    let direct_query = fixed_query(2, 0xff3fc, vec![PieceKind::O]);
    let held_query = direct_query
        .clone()
        .with_allow_hold(true)
        .with_hold_piece(Some(PieceKind::O));
    let (_, direct) = compare_exact_fallback_with_translation("current O, no hold", &direct_query);
    let (_, held) = compare_exact_fallback_with_translation("current O, held O", &held_query);
    assert_eq!(
        held.normalized_solution_identities(),
        direct.normalized_solution_identities()
    );
    assert_eq!(
        held.coverage_pattern_words(),
        direct.coverage_pattern_words()
    );
    let variants = |result: &CoreExecutionResult| {
        result
            .field("build_variant_count")
            .unwrap()
            .parse::<u128>()
            .unwrap()
    };
    assert!(variants(&direct) > 0);
    assert_eq!(variants(&held), 2 * variants(&direct));
    assert_eq!(held.bool_field("build_variant_count_exact"), Some(true));

    // Repeated operations with interacting placements exercise CountAll beyond
    // the single-placement direct-harddrop multiplicity check above.
    let repeated = fixed_query(
        2,
        0,
        vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ],
    );
    let (_, result) = compare_exact_fallback_with_translation("IIOOO CountAll", &repeated);
    assert_eq!(result.bool_field("build_variant_count_exact"), Some(true));
}

#[test]
fn static_influence_preserves_repeated_o_across_original_row_clear_frames() {
    let _guard = product_guard();
    // The same two physical O anchors occur before/after deleting original
    // rows 0 and 1. Each placement clears two lines, leaving a shifted frame.
    let row = 0x3fc_u64;
    let board = row | (row << 10) | (row << 20) | (row << 30);
    let query = fixed_query(4, board, vec![PieceKind::O, PieceKind::O]);
    let (_, result) =
        compare_exact_fallback_with_translation("OO with an intermediate two-line clear", &query);
    assert_eq!(result.path_steps().len(), 2);
    assert!(result
        .path_steps()
        .iter()
        .all(|step| step.cleared_lines() == 2));
    assert_eq!(result.bool_field("build_variant_count_exact"), Some(true));
}

#[test]
fn static_influence_preserves_count_all_with_actual_algebraic_lock_answers() {
    let _guard = product_guard();
    let query = mixed_bag_query(PcCountPolicy::CountAll);
    let (baseline, treatment) =
        compare_exact_fallback_with_translation("mixed four-piece bag CountAll", &query);
    assert_eq!(
        treatment.bool_field("build_variant_count_exact"),
        Some(true)
    );
    assert!(
        metric(&treatment, "reachability_static_influence_row_steps") > 0,
        "CountAll must exercise algebraic O lock checks, not only hard-drop successes"
    );
    assert!(
        metric(&treatment, "reachability_static_influence_queries") > 0,
        "CountAll must receive exact static answers"
    );
    eprintln!(
        "static influence CountAll multiplicity: variants={}->{}, per_solution_coverages={}",
        baseline.field("build_variant_count").unwrap(),
        treatment.field("build_variant_count").unwrap(),
        treatment.normalized_solution_coverages().len(),
    );
}

#[test]
fn algebraic_intervals_preserve_the_structural_product_language_and_probability() {
    let _guard = product_guard();
    let query = mixed_bag_query(PcCountPolicy::CountUnique);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("mixed structural fixture");
    let (baseline, before) = run_with_structural(&problem, false, true);
    let (treatment, after) = run_with_structural(&problem, true, true);
    assert_parity(
        "structural producer plus static influence",
        &baseline,
        &treatment,
    );
    assert_eq!(baseline.normalized_solution_identities().len(), 135);
    assert_eq!(baseline.solution_probabilities().len(), 135);
    assert_eq!(before.algebraic_interval_attempts, 0);
    assert_eq!(before.algebraic_interval_guards, 0);
    assert_eq!(before.algebraic_interval_row_steps, 0);
    assert!(after.algebraic_interval_guards <= after.algebraic_interval_attempts);
    eprintln!(
        "static influence structural composition: bypasses={}->{}, graph_nodes={}->{}, residuals={}->{}, algebraic_interval_attempts={}, algebraic_interval_guards={}, algebraic_interval_rows={}, observed_guards={}->{}",
        before.skipped_build_graphs,
        after.skipped_build_graphs,
        baseline.field("total_build_order_nodes").unwrap(),
        treatment.field("total_build_order_nodes").unwrap(),
        before.language_residual_states,
        after.language_residual_states,
        after.algebraic_interval_attempts,
        after.algebraic_interval_guards,
        after.algebraic_interval_row_steps,
        before.cached_interval_guards,
        after.cached_interval_guards,
    );
}

#[test]
fn cold_translation_misses_preserve_exact_product_across_five_profiles() {
    let _guard = product_guard();
    // This fixture is a cost counterexample: analytic interval work is real,
    // but its non-O queries cannot obtain a positive witness. They must remain
    // Unknown and preserve the ordinary exact producer, every solution and
    // every weighted probability. A separate fixture verifies a positive
    // interval guard; whole-poset admission is measured independently of it.
    for (label, rule) in [
        ("SRS90", srs()),
        ("SRS+", srs_plus()),
        ("SRS-X", srs_x()),
        ("Jstris180", jstris_180()),
        ("NoKick", no_kick()),
    ] {
        let query = mixed_bag_query(PcCountPolicy::CountUnique)
            .with_rule(rule)
            .with_retained_trace_limit(0);
        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .expect("mixed translation fallback fixture");
        let ordinary = run(&problem, false);
        let (ordinary_treatment, _) = run_with_modes(&problem, true, true, true);
        assert_parity(label, &ordinary, &ordinary_treatment);
        assert_eq!(ordinary.usize_field("coverage_pattern_count"), Some(840));
        if label == "SRS+" {
            assert_eq!(ordinary.normalized_solution_identities().len(), 135);
            assert_eq!(ordinary.solution_probabilities().len(), 135);
            assert_eq!(ordinary.usize_field("covered_pattern_count"), Some(173));
        }

        let summary_problem = problem
            .clone()
            .with_output_policy(SearchOutputPolicy::CoverageSummary);
        assert!(!summary_problem
            .output_policy()
            .retains_representative_trace());
        // Each execution owns a fresh workspace. Keep the existing exact O
        // capability enabled in both arms: it answers every admitted O
        // interval before the translation-only callback is considered. Thus
        // the new translation counters below necessarily exercise non-O
        // shapes, rather than rediscovering the earlier O-only improvement.
        let (summary_baseline, before) = run_with_modes(&summary_problem, true, true, false);
        let (summary_treatment, after) = run_with_modes(&summary_problem, true, true, true);
        eprintln!(
            "cold translation cost counterexample: {label}; solutions={}, bypasses={}->{}, graph_nodes={}->{}, primitive_states={}->{}, translation_attempts={}, translation_guards={}, translation_rows={}",
            ordinary.normalized_solution_identities().len(),
            before.skipped_build_graphs,
            after.skipped_build_graphs,
            summary_baseline.field("total_build_order_nodes").unwrap(),
            summary_treatment.field("total_build_order_nodes").unwrap(),
            summary_baseline.field("total_reachability_states").unwrap(),
            summary_treatment.field("total_reachability_states").unwrap(),
            after.translation_interval_attempts,
            after.translation_interval_guards,
            after.translation_interval_row_steps,
        );
        assert_summary_probability_matches_ordinary(label, &problem, &ordinary, &summary_baseline);
        assert_summary_probability_matches_ordinary(label, &problem, &ordinary, &summary_treatment);
        assert_summary_fields_equal(label, &summary_baseline, &summary_treatment);
        assert_eq!(before.translation_interval_attempts, 0);
        assert_eq!(before.translation_interval_guards, 0);
        assert_eq!(before.translation_interval_row_steps, 0);
        assert!(
            after.translation_interval_attempts > 0,
            "{label}: the fallback must follow actual interval proof attempts"
        );
        assert!(after.translation_interval_row_steps > 0);
        assert_eq!(
            after.translation_interval_guards, 0,
            "{label}: this fixture exposes failed proof cost, not a positive certificate"
        );
        assert_eq!(
            after.skipped_build_graphs, before.skipped_build_graphs,
            "{label}: Unknown must not admit an additional poset language"
        );
        for field in ["total_build_order_nodes", "total_reachability_states"] {
            assert_eq!(
                summary_treatment.field(field),
                summary_baseline.field(field),
                "{label}: exact fallback retains {field}"
            );
        }
    }
}

#[test]
fn partial_translation_guard_preserves_the_t_l_product_across_five_profiles() {
    use clearra_core_domain::solution::normalized_tiling_solution::{
        PiecePlacementMask, StandardBoard64TilingIdentity,
    };

    let _guard = product_guard();
    // T Zero at (0,1), L Left at (2,0). L alone completes original row 0,
    // so D=0 excludes L from the interval maximum. T is supported by the
    // initial bottom row and can enter beside the roof, then slide left.
    // This is one positive guard. Other frames of the same candidate may
    // still fail the whole-poset certificate and retain the exact graph.
    let t_cells = (7_u64 << 10) | (1 << 21);
    let l_cells = (1_u64 << 3) | (1 << 13) | (1 << 23) | (1 << 22);
    let board = ((1_u64 << 30) - 1) & !(t_cells | l_cells);
    let intended = StandardBoard64TilingIdentity::from_placements(
        board,
        [
            PiecePlacementMask::new(PieceKind::T, t_cells),
            PiecePlacementMask::new(PieceKind::L, l_cells),
        ],
    )
    .expect("disjoint T/L partial-guard identity");
    for (label, rule) in [
        ("SRS90", srs()),
        ("SRS+", srs_plus()),
        ("SRS-X", srs_x()),
        ("Jstris180", jstris_180()),
        ("NoKick", no_kick()),
    ] {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(3, board),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(2),
        )
        .with_rule(rule)
        .with_allow_hold(false)
        .with_exact_pieces(Some(2))
        .with_supply_window_size(SupplyWindowSize::new(2))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_retained_trace_limit(0)
        .with_execution_policy(policy());
        let problem =
            ProblemCompiler::compile_scenario_pc(&query).expect("partial T/L interval fixture");
        let ordinary = run(&problem, false);
        let (ordinary_treatment, _) = run_with_modes(&problem, true, true, true);
        assert_parity(label, &ordinary, &ordinary_treatment);
        assert!(
            ordinary
                .normalized_solution_identities()
                .contains(&intended),
            "{label}: the exact T/L placement must occur in the complete result"
        );
        assert_eq!(ordinary.usize_field("coverage_pattern_count"), Some(42));

        let summary_problem = problem
            .clone()
            .with_output_policy(SearchOutputPolicy::CoverageSummary);
        assert!(!summary_problem
            .output_policy()
            .retains_representative_trace());
        // Static O admission stays enabled in both arms. The new positive
        // translation counters therefore belong to non-O operations.
        let (summary_baseline, before) = run_with_modes(&summary_problem, true, true, false);
        let (summary_treatment, after) = run_with_modes(&summary_problem, true, true, true);
        eprintln!(
            "partial T/L translation guard product: {label}; solutions={}, bypasses={}->{}, graph_nodes={}->{}, primitive_states={}->{}, translation_attempts={}, translation_guards={}, translation_rows={}",
            ordinary.normalized_solution_identities().len(),
            before.skipped_build_graphs,
            after.skipped_build_graphs,
            summary_baseline.field("total_build_order_nodes").unwrap(),
            summary_treatment.field("total_build_order_nodes").unwrap(),
            summary_baseline.field("total_reachability_states").unwrap(),
            summary_treatment.field("total_reachability_states").unwrap(),
            after.translation_interval_attempts,
            after.translation_interval_guards,
            after.translation_interval_row_steps,
        );
        assert_summary_probability_matches_ordinary(label, &problem, &ordinary, &summary_baseline);
        assert_summary_probability_matches_ordinary(label, &problem, &ordinary, &summary_treatment);
        assert_summary_fields_equal(label, &summary_baseline, &summary_treatment);
        assert_eq!(before.translation_interval_attempts, 0);
        assert_eq!(before.translation_interval_guards, 0);
        assert_eq!(before.translation_interval_row_steps, 0);
        assert!(
            after.translation_interval_guards > 0,
            "{label}: the cold product must obtain a non-O positive interval witness"
        );
        assert!(after.translation_interval_attempts >= after.translation_interval_guards);
        assert!(after.translation_interval_row_steps > 0);
        // A successful interval is not a complete order-language certificate.
        // Record whole-graph bypasses above without assuming they increased.
    }
}

#[test]
fn cold_o_interval_preserves_the_three_row_roof_product_and_probability() {
    use clearra_core_domain::solution::normalized_tiling_solution::{
        PiecePlacementMask, StandardBoard64TilingIdentity,
    };

    let _guard = product_guard();
    let o_cells = (1_u64 << 0) | (1 << 1) | (1 << 10) | (1 << 11);
    // L's registry rotation 3 translated to x=1: the vertical column is at
    // x=2 and its upper arm is at (1,2). This is L, not its J reflection.
    let l_cells = (1_u64 << 2) | (1 << 12) | (1 << 22) | (1 << 21);
    let board = ((1_u64 << 30) - 1) & !(o_cells | l_cells);
    // The roof at (0,2) prevents O's direct drop. With L absent, O descends
    // through x=1 and slides left at y=0. L alone completes original row 2;
    // the D=0 negative row-clear guard must therefore keep L out of max.
    // After L, that roof row is deleted and O can drop directly instead.
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(3, board),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(2),
    )
    .with_rule(srs_plus())
    .with_allow_hold(false)
    .with_exact_pieces(Some(2))
    .with_supply_window_size(SupplyWindowSize::new(2))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_retained_trace_limit(0)
    .with_execution_policy(policy());
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("three-row roof fixture");
    let (baseline, _) = run_with_structural(&problem, false, true);
    let (treatment, _) = run_with_structural(&problem, true, true);
    assert_parity("three-row O/L roof", &baseline, &treatment);
    let intended = StandardBoard64TilingIdentity::from_placements(
        board,
        [
            PiecePlacementMask::new(PieceKind::O, o_cells),
            PiecePlacementMask::new(PieceKind::L, l_cells),
        ],
    )
    .expect("disjoint two-piece identity");
    assert!(
        baseline
            .normalized_solution_identities()
            .contains(&intended),
        "the exact O/L roof placement must occur in the complete result"
    );

    // Ordinary output retains its first representative even with trace limit
    // zero. CoverageSummary enters the proof-only route on a cold workspace;
    // Include still retains the per-identity coverage needed below.
    let summary_problem = problem
        .clone()
        .with_output_policy(SearchOutputPolicy::CoverageSummary);
    assert!(!summary_problem
        .output_policy()
        .retains_representative_trace());
    let (summary_baseline, before) = run_with_structural(&summary_problem, false, true);
    let (summary_treatment, after) = run_with_structural(&summary_problem, true, true);
    for result in [&summary_baseline, &summary_treatment] {
        assert_eq!(
            result.coverage_pattern_words(),
            baseline.coverage_pattern_words()
        );
        assert_eq!(result.solution_coverages(), baseline.solution_coverages());
        assert_eq!(
            result.normalized_solution_coverages(),
            baseline.normalized_solution_coverages()
        );
        for field in [
            "normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "build_variant_count",
            "build_variant_count_exact",
            "coverage_pattern_count",
            "covered_pattern_count",
            "coverage_probability",
            "materialized_probability_mass",
            "count_complete",
            "probability_complete",
            "solution_probability_complete",
            "resource_truncated",
        ] {
            assert_eq!(
                result.field(field),
                summary_baseline.field(field),
                "roof summary off/on: {field}"
            );
        }
        for field in [
            "count_complete",
            "probability_complete",
            "solution_probability_complete",
        ] {
            assert_eq!(
                result.bool_field(field),
                Some(true),
                "roof summary: {field}"
            );
        }
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        let reports = crate::solution_probability::probability_reports(
            baseline.normalized_solution_identities(),
            result.solution_coverages(),
            problem
                .piece_source()
                .materialized_universe()
                .expect("standard bag universe")
                .weights(),
            result.bool_field("solution_probability_complete") == Some(true),
        );
        assert_eq!(reports, baseline.solution_probabilities());
    }
    assert_eq!(before.algebraic_interval_attempts, 0);
    assert_eq!(before.algebraic_interval_guards, 0);
    assert_eq!(before.algebraic_interval_row_steps, 0);
    eprintln!(
        "cold O interval roof product: solutions={}, bypasses={}->{}, graph_nodes={}->{}, primitive_states={}->{}, algebraic_interval_attempts={}, algebraic_interval_guards={}, algebraic_interval_rows={}, observed_guards={}->{}",
        baseline.normalized_solution_identities().len(),
        before.skipped_build_graphs,
        after.skipped_build_graphs,
        summary_baseline.field("total_build_order_nodes").unwrap(),
        summary_treatment.field("total_build_order_nodes").unwrap(),
        summary_baseline.field("total_reachability_states").unwrap(),
        summary_treatment.field("total_reachability_states").unwrap(),
        after.algebraic_interval_attempts,
        after.algebraic_interval_guards,
        after.algebraic_interval_row_steps,
        before.cached_interval_guards,
        after.cached_interval_guards,
    );
    assert!(
        after.algebraic_interval_guards > 0,
        "a cold O interval must certify the roof without an observed primitive path"
    );
    assert!(after.algebraic_interval_attempts >= after.algebraic_interval_guards);
    assert!(after.algebraic_interval_row_steps > 0);
    // Log graph reduction independently of certificate admission: this cavity
    // can contain other ILC tilings in addition to the intended O/L placement.
}
