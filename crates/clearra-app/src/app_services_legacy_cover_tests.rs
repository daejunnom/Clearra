use clearra_build_coverage::{
    domain::slot_domain::SlotDomain,
    query::build_coverage_limits::BuildCoverageLimits,
    template::{BuildSlot, BuildSlotId, BuildTemplate},
};
use clearra_core_domain::{
    board::cell::CellCoord, execution_cancellation::ExecutionCancellationToken,
    piece::piece_kind::PieceKind,
};
use clearra_problem::{BuildProblemLimits, BuildQuery, BuildTemplateBridge, ProblemCompiler};

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
