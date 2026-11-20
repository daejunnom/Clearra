use clearra_build_coverage::{
    domain::slot_domain::SlotDomain,
    query::build_coverage_limits::BuildCoverageLimits,
    template::{BuildSlot, BuildSlotId, BuildTemplate, TemplateImport},
};
use clearra_core_domain::{
    board::cell::CellCoord,
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
    solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
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
    let native = run_app(AppCoreExecutorService::default(), query.clone());
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
fn legacy_cover_does_not_claim_zero_for_unproved_or_mismatched_queries() {
    let rust = AppCoreExecutorService::wasm_cpu();
    let five_slots = product_cover_template("area-can-fit", 5);
    let one_slot = product_cover_template("basic", 1);
    let different_template = product_cover_template("other-template", 1);
    for (problem, query) in [
        (compile(&five_slots), &five_slots),
        (compile(&one_slot), &different_template),
    ] {
        let error = rust
            .execute_build_coverage(&problem, query)
            .expect_err("unsupported coverage must not appear as a completed zero result");
        assert!(error.unsupported_reason().is_some());
    }
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
