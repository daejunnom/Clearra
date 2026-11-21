//! Candidate-local placement dependency proof and exact supply composition.

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;

#[cfg(feature = "placement-dependency-pc")]
use super::super::geometry::TargetGroup;
use super::super::{
    catalog::GeometryCatalog, geometry::GeometryCandidate,
    standard_bag_coverage::StandardBagCoverage, WasmExactSearchError,
};
use super::{certified_language_result, BuildUpWorkspace, CandidateBuildResult};
use crate::performance::{ExecutorSearchStage, SearchStageSpan};

#[cfg(feature = "placement-dependency-pc")]
pub(super) fn verify_product(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    workspace: &mut BuildUpWorkspace,
    control: &ExecutionControl,
    profile_scale: u64,
) -> Result<Option<CandidateBuildResult>, WasmExactSearchError> {
    use super::super::symbolic_prefix_orders::{with_product_prefix_order, SymbolicPrefixRefusal};

    let span =
        SearchStageSpan::begin_scaled(ExecutorSearchStage::PlacementDependencyPc, profile_scale);
    let result = with_product_prefix_order(|work| {
        evaluate_prefix_order(
            problem,
            catalog,
            candidate,
            workspace,
            control,
            profile_scale,
            work,
            PrefixEvaluation::Product,
        )
    });
    span.finish(u64::from(result.is_ok()));
    match result {
        Ok(result) => Ok(Some(result)),
        Err(PrefixEvaluationError::Refused(SymbolicPrefixRefusal::Cancelled)) => {
            Err(WasmExactSearchError::Cancelled)
        }
        Err(PrefixEvaluationError::Execution(error)) => Err(error),
        // A refusal is not a reachability conclusion. Use the concrete
        // verifier when this path cannot certify a candidate.
        Err(PrefixEvaluationError::UnsupportedSupply | PrefixEvaluationError::Refused(_)) => {
            Ok(None)
        }
    }
}

#[cfg(feature = "placement-dependency-pc")]
pub(super) fn supports_product(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
) -> bool {
    problem.pc_placement_dependency_requested()
        && catalog.width() == 10
        && (1..=4).contains(&catalog.height())
        && (1..=10).contains(&candidate.row_ids().len())
        && target.pattern_index.is_none()
}

#[cfg(any(
    test,
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
pub(super) fn verify_strict(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    workspace: &mut BuildUpWorkspace,
    control: &ExecutionControl,
    profile_scale: u64,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    use super::super::symbolic_prefix_orders::{record_strict_prefix_work, SymbolicPrefixRefusal};

    record_strict_prefix_work(|work| {
        evaluate_prefix_order(
            problem,
            catalog,
            candidate,
            workspace,
            control,
            profile_scale,
            work,
            PrefixEvaluation::NativeExperiment,
        )
    })
    .map_err(|error| match error {
        PrefixEvaluationError::UnsupportedSupply => {
            WasmExactSearchError::InvalidProblem("wasm_symbolic_prefix_unsupported_supply")
        }
        PrefixEvaluationError::Execution(error) => error,
        PrefixEvaluationError::Refused(reason) => match reason {
            SymbolicPrefixRefusal::Cancelled => WasmExactSearchError::Cancelled,
            SymbolicPrefixRefusal::Unsupported => {
                WasmExactSearchError::InvalidProblem("wasm_symbolic_prefix_unsupported_candidate")
            }
            SymbolicPrefixRefusal::RelationLimit => {
                WasmExactSearchError::InvalidProblem("wasm_symbolic_prefix_relation_limit")
            }
            SymbolicPrefixRefusal::InconsistentProjection => {
                WasmExactSearchError::InvalidProblem("wasm_symbolic_prefix_inconsistent_projection")
            }
        },
    })
}

#[cfg(any(
    feature = "placement-dependency-pc",
    test,
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
#[derive(Clone, Copy)]
enum PrefixEvaluation {
    #[cfg(feature = "placement-dependency-pc")]
    Product,
    #[cfg(any(
        test,
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    NativeExperiment,
}

#[cfg(any(
    feature = "placement-dependency-pc",
    test,
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
enum PrefixEvaluationError {
    UnsupportedSupply,
    Refused(super::super::symbolic_prefix_orders::SymbolicPrefixRefusal),
    Execution(WasmExactSearchError),
}

#[cfg(any(
    feature = "placement-dependency-pc",
    test,
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
#[allow(clippy::too_many_arguments)]
fn evaluate_prefix_order(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    workspace: &mut BuildUpWorkspace,
    control: &ExecutionControl,
    profile_scale: u64,
    work: &mut super::super::symbolic_prefix_orders::SymbolicPrefixWork,
    evaluation: PrefixEvaluation,
) -> Result<CandidateBuildResult, PrefixEvaluationError> {
    use super::super::symbolic_prefix_orders::SymbolicPrefixOrders;

    if !problem
        .piece_source()
        .materialized_universe()
        .is_some_and(|universe| StandardBagCoverage::supports(universe, problem.initial_hold()))
    {
        return Err(PrefixEvaluationError::UnsupportedSupply);
    }
    work.candidates_attempted += 1;
    let compile_orders = match evaluation {
        #[cfg(feature = "placement-dependency-pc")]
        PrefixEvaluation::Product => SymbolicPrefixOrders::compile_product_with_workspace,
        #[cfg(any(
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        PrefixEvaluation::NativeExperiment => SymbolicPrefixOrders::compile_with_workspace,
    };
    let orders = compile_orders(
        catalog,
        candidate,
        problem.kick_profile().profile_id(),
        control,
        &mut workspace.reachability,
        work,
        profile_scale,
    )
    .map_err(PrefixEvaluationError::Refused)?;
    let compile_language = match evaluation {
        #[cfg(feature = "placement-dependency-pc")]
        PrefixEvaluation::Product => SymbolicPrefixOrders::compile_product_language,
        #[cfg(any(
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        PrefixEvaluation::NativeExperiment => SymbolicPrefixOrders::compile_language,
    };
    let language_stage = match evaluation {
        #[cfg(feature = "placement-dependency-pc")]
        PrefixEvaluation::Product => ExecutorSearchStage::PlacementDependencyPcLanguage,
        #[cfg(any(
            test,
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        PrefixEvaluation::NativeExperiment => ExecutorSearchStage::StrictPrefixLanguage,
    };
    let language_span = SearchStageSpan::begin_scaled(language_stage, profile_scale);
    let language = compile_language(&orders, &mut workspace.piece_order_languages, work)
        .map_err(PrefixEvaluationError::Execution)?;
    language_span.finish(1);
    let coverage = match language {
        Some(root) => {
            let product_span = SearchStageSpan::begin_scaled(
                ExecutorSearchStage::WasmCoverageLanguageProduct,
                profile_scale,
            );
            #[cfg(all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            ))]
            let bag_product_span = SearchStageSpan::begin_scaled(
                ExecutorSearchStage::StrictStandardBagProduct,
                profile_scale,
            );
            let coverage = workspace
                .cover_standard_bag_language(problem, root, control)
                .map_err(PrefixEvaluationError::Execution)?
                .ok_or(PrefixEvaluationError::UnsupportedSupply)?;
            #[cfg(all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            ))]
            bag_product_span.finish(coverage.edge_checks as u64);
            product_span.finish(coverage.edge_checks as u64);
            Some(coverage)
        }
        None => None,
    };
    if control.is_cancelled() {
        return Err(PrefixEvaluationError::Execution(
            WasmExactSearchError::Cancelled,
        ));
    }
    work.candidates_certified += 1;
    Ok(certified_language_result(coverage))
}
