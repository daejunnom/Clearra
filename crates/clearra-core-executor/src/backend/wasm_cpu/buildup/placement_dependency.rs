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
mod witness;

#[cfg(feature = "placement-dependency-pc")]
#[allow(clippy::too_many_arguments)]
pub(super) fn verify_product(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    workspace: &mut BuildUpWorkspace,
    retain_trace: bool,
    control: &ExecutionControl,
    profile_scale: u64,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    use super::super::symbolic_prefix_orders::with_product_prefix_order;

    #[cfg(test)]
    super::record_product_verifier(true);
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
            PrefixEvaluation::Product { retain_trace },
        )
    });
    span.finish(u64::from(result.is_ok()));
    // A bounded refusal is neither a negative result nor permission to
    // start the unrequested BuildUp verifier.
    result.map_err(map_prefix_error)
}

#[cfg(feature = "placement-dependency-pc")]
pub(super) fn validate_candidate_scope(
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
) -> Result<(), WasmExactSearchError> {
    if catalog.width() == 10
        && (1..=4).contains(&catalog.height())
        && (1..=10).contains(&candidate.row_ids().len())
        && target.pattern_index.is_none()
    {
        Ok(())
    } else {
        Err(WasmExactSearchError::InvalidProblem(
            "wasm_placement_dependency_candidate_unsupported",
        ))
    }
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
    use super::super::symbolic_prefix_orders::record_strict_prefix_work;

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
    .map_err(map_prefix_error)
}

fn map_prefix_error(error: PrefixEvaluationError) -> WasmExactSearchError {
    use super::super::symbolic_prefix_orders::SymbolicPrefixRefusal;
    match error {
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
    }
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
    Product { retain_trace: bool },
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
        PrefixEvaluation::Product { .. } => SymbolicPrefixOrders::compile_product_with_workspace,
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
        PrefixEvaluation::Product { .. } => SymbolicPrefixOrders::compile_product_language,
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
        PrefixEvaluation::Product { .. } => ExecutorSearchStage::PlacementDependencyPcLanguage,
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
    let result = certified_language_result(coverage);
    #[cfg(feature = "placement-dependency-pc")]
    if matches!(evaluation, PrefixEvaluation::Product { .. })
        && result.buildable
        && problem
            .queue_observation_policy()
            .requires_observation_policy()
    {
        // An oracle-empty candidate cannot contribute to an observation
        // policy for this exact universe. Reuse that completed composition
        // before retaining its physical action language.
        orders
            .compile_observation_language(&mut workspace.dependency_observation, control)
            .map_err(PrefixEvaluationError::Execution)?;
    }
    #[cfg(feature = "placement-dependency-pc")]
    let result = match evaluation {
        PrefixEvaluation::Product { retain_trace: true } if result.buildable => {
            let mut result = result;
            let pattern_id = result
                .witness_pattern_id
                .ok_or(PrefixEvaluationError::Execution(
                    WasmExactSearchError::InvalidProblem(
                        "wasm_placement_dependency_witness_pattern_missing",
                    ),
                ))?;
            result.representative_path = witness::materialize(
                problem,
                catalog,
                candidate,
                &orders,
                pattern_id,
                &mut workspace.reachability,
                control,
                &mut work.pose,
            )
            .map_err(PrefixEvaluationError::Execution)?;
            result
        }
        _ => result,
    };
    Ok(result)
}
