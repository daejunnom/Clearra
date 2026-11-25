use clearra_build_coverage::{
    domain::slot_domain::SlotDomain,
    query::build_coverage_limits::BuildCoverageLimits,
    template::{BuildSlot, BuildSlotId, BuildTemplate, TemplateImport},
};
use clearra_core_domain::{
    board::{board_size::BoardSize, cell::CellCoord},
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
    solution::{NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_core_executor::WasmCpuSearchBackend;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow,
};
use clearra_problem::{BuildProblemLimits, BuildQuery, BuildTemplateBridge, ProblemCompiler};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::{
    AppCoreExecutorService, AppServices, BuildCoverageQuery, CoreExecutionError,
    CoreExecutionResult, SearchProblem,
};
use crate::{
    app_command::AppCommand, app_context::AppContext, app_request::AppRequest,
    app_response::AppStatus, commands::cover_app_command::CoverAppCommand, render::AppRenderModel,
};

fn product_cover_template(id: &str, slot_count: u16) -> BuildCoverageQuery {
    let slots = (0..slot_count)
        .map(|x| {
            BuildSlot::new(
                BuildSlotId::new(u32::from(x)),
                vec![CellCoord::new_unchecked(x, 0)],
            )
        })
        .collect::<Vec<_>>();
    let domains = slots
        .iter()
        .map(|slot| SlotDomain::new(slot.id(), vec![PieceKind::I]))
        .collect();
    BuildCoverageQuery::new(
        BuildTemplate::new(id, slots),
        domains,
        Vec::new(),
        1,
        BuildCoverageLimits::default(),
    )
}

fn compile(query: &BuildCoverageQuery) -> SearchProblem {
    let template = query.template();
    ProblemCompiler::compile_build(&BuildQuery::coverage_bridge(
        BuildTemplateBridge::new(template.id(), template.board_size(), template.slots().len()),
        query.pattern_count(),
        BuildProblemLimits::new(
            query.limits().max_assignments(),
            query.limits().max_patterns(),
        ),
    ))
    .expect("canonical cover problem")
}

fn run_app(backend: AppCoreExecutorService, query: BuildCoverageQuery) -> CoreExecutionResult {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let app = AppContext::new(AppServices::new().with_core_executor(backend));
    let response = app.run(AppRequest::new(AppCommand::Cover(CoverAppCommand::new(
        query,
    ))));
    assert_eq!(response.status(), AppStatus::Success);
    let Some(AppRenderModel::Cover(result)) = response.render_model() else {
        panic!("expected Cover result");
    };
    result.clone()
}

fn horizontal_i_import() -> BuildCoverageQuery {
    let template = TemplateImport::from_json(
        "positive legacy cover seam fixture",
        r#"{
            "schema_version": 2,
            "id": "legacy-one-I",
            "board": { "width": 4, "height": 1 },
            "slots": [{
                "id": 0,
                "cells": [{ "x": 0, "y": 0 }, { "x": 1, "y": 0 },
                          { "x": 2, "y": 0 }, { "x": 3, "y": 0 }],
                "allowed_pieces": ["I"],
                "required_piece": "I"
            }]
        }"#,
    )
    .expect("native cover template must accept a horizontal I in a 4×1 board")
    .into_template();
    let slot = &template.slots()[0];
    let domain = SlotDomain::new(slot.id(), slot.allowed_pieces().to_vec());
    let required = clearra_build_coverage::domain::slot_constraint::SlotConstraint::required(
        slot.id(),
        PieceKind::I,
    );
    BuildCoverageQuery::new(
        template,
        vec![domain],
        vec![required],
        1,
        BuildCoverageLimits::default(),
    )
}

#[test]
fn positive_legacy_cover_ordinal_zero_is_solved_by_the_explicit_i_rust_kernel() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let coverage_query = horizontal_i_import();
    assert!(
        !clearra_validation::validators::build_query_validator::validate_build_coverage_query(
            &coverage_query,
        )
        .has_errors()
    );
    let legacy_problem = compile(&coverage_query);
    let original_universe = legacy_problem
        .piece_source()
        .materialized_universe()
        .expect("legacy observed universe");
    assert_eq!(coverage_query.pattern_count(), 1);
    assert_eq!(original_universe.sequence_at(0).as_ref(), &[PieceKind::I]);

    // Rebuild the one admitted ordinal as an explicit source. Executing all
    // legacy observed ordinals through the Rust PC API instead would silently
    // change the request's denominator.
    let rust_problem = ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::new(4, 1, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_rule(legacy_problem.rule_profile_value())
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1)),
    )
    .expect("explicit I queue on a 4×1 board");
    let rust =
        WasmCpuSearchBackend::execute_with_control(&rust_problem, &ExecutionControl::default())
            .expect("Rust compact PC kernel, without the product cover adapter");
    let exact_identity = StandardBoard64TilingIdentity::from_placements(
        0,
        [PiecePlacementMask::new(PieceKind::I, 0xf)],
    )
    .expect("the single colored placement must tile the row");
    assert_eq!(rust.normalized_solution_identities(), &[exact_identity]);
    assert_eq!(rust.usize_field("coverage_pattern_count"), Some(1));
    assert_eq!(rust.coverage_pattern_words(), &[1]);
    assert_eq!(rust.bool_field("probability_complete"), Some(true));
    assert_eq!(rust.bool_field("build_variant_count_exact"), Some(true));
}

#[cfg(feature = "native-c-core")]
#[test]
fn native_legacy_cover_rejects_the_positive_four_column_fixture() {
    use clearra_core_executor::packing::{PackingRunner, PackingRunnerError};

    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let legacy_problem = compile(&horizontal_i_import());
    // Native BuildUp rejects this request during Packing's streamed candidate
    // admission (observed ABI status 2, INVALID_PROBLEM), before a coverage
    // row can exist. This is not a completed result with zero coverage and
    // does not establish a general rule for all nonstandard widths.
    let error = PackingRunner::run(&legacy_problem)
        .expect_err("native packing must reject this specific legacy request");
    assert!(matches!(error, PackingRunnerError::Native(_)), "{error:?}");
}

// Original-row masks of the independently verified hard-drop witness.
// The ten locks clear original rows [1], [3], and [0, 2].
const LEGACY_FOUR_LINE_WITNESS: [(PieceKind, u64); 10] = [
    (PieceKind::I, 0x78),
    (PieceKind::O, 0xc0300),
    (PieceKind::T, 0x100e000),
    (PieceKind::S, 0x1803),
    (PieceKind::Z, 0x6030000),
    (PieceKind::J, 0xc0100400),
    (PieceKind::L, 0x3008000080),
    (PieceKind::I, 0xf00000000),
    (PieceKind::O, 0xc030000000),
    (PieceKind::T, 0xe00004),
];

fn four_line_legacy_problem_and_exact_identity() -> (SearchProblem, StandardBoard64TilingIdentity) {
    let mut domains = Vec::new();
    let mut slots = Vec::new();
    for (index, (piece, mask)) in LEGACY_FOUR_LINE_WITNESS.iter().copied().enumerate() {
        let id = BuildSlotId::new(u32::try_from(index).expect("fixture slot fits u32"));
        let cells = (0..40_u16)
            .filter(|&cell| mask & (1_u64 << cell) != 0)
            .map(|cell| CellCoord::new_unchecked(cell % 10, cell / 10))
            .collect();
        slots.push(BuildSlot::new(id, cells).with_allowed_pieces(vec![piece]));
        domains.push(SlotDomain::new(id, vec![piece]));
    }
    let query = BuildCoverageQuery::new(
        BuildTemplate::new("legacy-four-line-ordinal-zero", slots)
            .with_board_size(BoardSize::new(10, 4).expect("standard four-line board")),
        domains,
        Vec::new(),
        1,
        // This witness deliberately retains the old bounded source prefix.
        BuildCoverageLimits::new(1024, 4096),
    );
    assert!(
        !clearra_validation::validators::build_query_validator::validate_build_coverage_query(
            &query,
        )
        .has_errors()
    );
    let problem = compile(&query);
    let identity = StandardBoard64TilingIdentity::from_placements(
        0,
        LEGACY_FOUR_LINE_WITNESS.map(|(piece, mask)| PiecePlacementMask::new(piece, mask)),
    )
    .expect("known four-line witness");
    (problem, identity)
}

fn fixed_ordinal_pc_problem(
    board: PcScenarioBoard,
    pieces: Vec<PieceKind>,
    rule: clearra_rules::profile::rule_profile::RuleProfile,
) -> SearchProblem {
    let count = pieces.len();
    ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            board,
            PcQueueInput::fixed_sequence(FixedSequence::new(pieces)),
            PieceWindow::new(count),
        )
        .with_rule(rule)
        .with_allow_hold(false)
        .with_exact_pieces(Some(count))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1)),
    )
    .expect("explicit ordinal PC problem")
}

fn rust_covered_ordinal_word(
    problem: &SearchProblem,
    identity: StandardBoard64TilingIdentity,
) -> u64 {
    let result = WasmCpuSearchBackend::execute_with_control(problem, &ExecutionControl::default())
        .expect("existing Rust PC provider must execute this fixed ordinal");
    assert_eq!(result.bool_field("count_complete"), Some(true));
    assert_eq!(
        result.bool_field("solution_probability_complete"),
        Some(true)
    );
    assert_eq!(result.usize_field("coverage_pattern_count"), Some(1));
    assert!(result.normalized_solution_identities().contains(&identity));
    let key = NormalizedTilingSolutionKey::from_standard_board64_identity(identity);
    let coverage = result
        .normalized_solution_coverages()
        .iter()
        .find(|coverage| coverage.solution_key() == key.as_str())
        .expect("Rust coverage row bound to the exact colored placement identity");
    assert_eq!(coverage.covered_patterns().pattern_count(), 1);
    coverage.covered_patterns().word_at(0)
}

fn four_line_fixed_ordinal_problem(legacy_problem: &SearchProblem) -> SearchProblem {
    let ordinal_zero = LEGACY_FOUR_LINE_WITNESS.map(|(piece, _)| piece);
    let universe = legacy_problem
        .piece_source()
        .materialized_universe()
        .unwrap();
    assert_eq!(
        universe.sequence_at(0).as_ref(),
        ordinal_zero.as_slice(),
        "legacy observed ordinal zero must match the independently replayed fixed queue"
    );
    assert_eq!(legacy_problem.build_query().unwrap().pattern_count(), 1);
    fixed_ordinal_pc_problem(
        PcScenarioBoard::standard_10(4, 0),
        ordinal_zero.to_vec(),
        legacy_problem.rule_profile_value(),
    )
}

#[test]
fn positive_four_line_legacy_ordinal_zero_is_solved_by_the_rust_kernel() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let (legacy_problem, identity) = four_line_legacy_problem_and_exact_identity();
    let fixed = four_line_fixed_ordinal_problem(&legacy_problem);
    assert_eq!(rust_covered_ordinal_word(&fixed, identity), 1);
}

#[cfg(feature = "native-c-core")]
fn native_covered_ordinal_word(
    problem: &SearchProblem,
    identity: StandardBoard64TilingIdentity,
) -> u64 {
    use clearra_core_executor::{packing::PackingRunner, BuildUpRunner};

    let packing = PackingRunner::run(problem).expect("native packing must admit the request");
    let initial = problem.initial_board().occupied_mask();
    let matching_ids: std::collections::BTreeSet<_> = (0..packing.candidate_count())
        .filter_map(|index| {
            let candidate = packing.candidate_view_at(index).expect("stored candidate");
            let key = candidate
                .standard_board64_tiling_identity(initial)
                .expect("native candidate identity");
            (key == identity).then(|| candidate.candidate_id())
        })
        .collect();
    assert!(
        !matching_ids.is_empty(),
        "native packing omitted the known witness"
    );
    let buildup = BuildUpRunner::run_for_coverage(problem, &packing)
        .expect("native BuildUp must produce raw coverage rows");
    assert!(buildup.materialized_coverage_complete());
    let mut coverage_word = 0;
    let mut found = false;
    for row in buildup.coverage_rows() {
        if matching_ids.contains(&row.candidate_id()) {
            assert_eq!(row.pattern_count(), 1);
            coverage_word |= row.coverage_bits().word_at(0);
            found = true;
        }
    }
    assert!(
        found,
        "native BuildUp omitted the known candidate's coverage row"
    );
    coverage_word
}

#[cfg(feature = "native-c-core")]
#[test]
fn positive_native_and_rust_fixed_queue_control_join_coverage_by_candidate_identity() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let fixed = fixed_ordinal_pc_problem(
        PcScenarioBoard::standard_10(1, 0x3f0),
        vec![PieceKind::I],
        compile(&horizontal_i_import()).rule_profile_value(),
    );
    let identity = StandardBoard64TilingIdentity::from_placements(
        0x3f0,
        [PiecePlacementMask::new(PieceKind::I, 0xf)],
    )
    .expect("horizontal I covers the empty cells in the initial row");
    let rust_word = rust_covered_ordinal_word(&fixed, identity);
    assert_eq!(rust_word, 1);
    assert_eq!(native_covered_ordinal_word(&fixed, identity), rust_word);
}

#[cfg(feature = "native-c-core")]
#[test]
#[ignore = "run explicitly to qualify native Legacy Cover admission"]
fn positive_four_line_legacy_native_ordinal_join_qualification() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let (legacy_problem, identity) = four_line_legacy_problem_and_exact_identity();
    let fixed = four_line_fixed_ordinal_problem(&legacy_problem);
    let rust_word = rust_covered_ordinal_word(&fixed, identity);
    assert_eq!(rust_word, 1);
    // Check only the known ordinal and geometry, not the completeness of the
    // entire truncated observed source or its positional assignment matrix.
    assert_eq!(
        native_covered_ordinal_word(&legacy_problem, identity),
        rust_word
    );
}

#[test]
fn single_slot_legacy_cover_runs_rust_assignment_and_proves_zero_coverage() {
    let result = run_app(
        AppCoreExecutorService::wasm_cpu(),
        product_cover_template("basic", 1),
    );
    assert_eq!(result.usize_field("assignment_count"), Some(1));
    assert_eq!(result.usize_field("union_covered_patterns"), Some(0));
    assert_eq!(result.field("build_coverage_probability"), Some("0.0"));
    assert_eq!(result.bool_field("probability_complete"), Some(true));
}

#[cfg(feature = "native-c-core")]
#[test]
fn single_slot_legacy_cover_numeric_result_matches_c_reference() {
    let query = product_cover_template("basic", 1);
    let native = run_app(AppCoreExecutorService::native_c_reference(), query.clone());
    let rust = run_app(AppCoreExecutorService::wasm_cpu(), query);
    for key in [
        "assignment_count",
        "assignment_exact_cover_complete",
        "packing_candidate_count",
        "core_buildup_variant_count",
        "unique_trace_count",
        "coverage_matrix_row_count",
        "union_covered_patterns",
        "build_coverage_probability",
        "probability_complete",
        "count_complete",
        "resource_truncated",
    ] {
        assert!(native.field(key).is_some(), "missing C baseline key {key}");
        assert_eq!(rust.field(key), native.field(key), "{key}");
    }
}

#[test]
fn legacy_cover_rejects_mismatched_queries() {
    let rust = AppCoreExecutorService::wasm_cpu();
    let one_slot = product_cover_template("basic", 1);
    let different_template = product_cover_template("other-template", 1);
    let error = rust
        .execute_build_coverage(&compile(&one_slot), &different_template)
        .expect_err("a different template must not appear as a completed zero result");
    assert!(error.unsupported_reason().is_some());
}

#[test]
fn legacy_cover_product_admits_positive_original_observed_ordinal() {
    let result = run_app(AppCoreExecutorService::wasm_cpu(), horizontal_i_import());
    assert_eq!(result.coverage_pattern_words(), &[1]);
    assert_eq!(result.usize_field("pattern_count"), Some(1));
    assert_eq!(result.usize_field("union_covered_patterns"), Some(1));
    assert_eq!(result.field("build_coverage_probability"), Some("1.0"));
    assert_eq!(result.bool_field("probability_complete"), Some(true));
}

#[test]
fn cancelled_legacy_cover_preempts_an_exact_empty_result() {
    let query = product_cover_template("basic", 1);
    let cancellation = ExecutionCancellationToken::new();
    cancellation.handle().cancel();
    let result = AppCoreExecutorService::wasm_cpu().execute_build_coverage_with_cancellation(
        &compile(&query),
        &query,
        &cancellation,
    );
    assert_eq!(
        result.expect_err("cancelled"),
        CoreExecutionError::Cancelled
    );
}
