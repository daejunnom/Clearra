//! Exact candidate-local placement proof composed with the existing supply
//! owner. This Boolean producer never enumerates the geometric family.

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;

use super::super::{
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    geometry::{GeometryCandidate, TargetGroup},
    realization_feasibility::RealizationFeasibilityWorkspace,
    symbolic_prefix_orders::{
        with_product_prefix_order, SymbolicPrefixOrders, SymbolicPrefixRefusal,
    },
    WasmExactSearchError,
};
use super::{
    certified_language_result, infeasible_candidate_result, BuildUpWorkspace, CandidateBuildResult,
    CandidateWitnessMode,
};
use crate::performance::{ExecutorSearchStage, SearchStageSpan};

mod witness;

#[cfg(test)]
#[path = "candidate_ilc_pruning_tests.rs"]
mod candidate_ilc_pruning_tests;

#[cfg(test)]
thread_local! {
    static CANDIDATE_ILC_OVERRIDE: core::cell::Cell<Option<bool>> = const { core::cell::Cell::new(None) };
}

fn candidate_ilc_pruning_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = CANDIDATE_ILC_OVERRIDE.get() {
        return enabled;
    }
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    {
        // The local benchmark validates 0/1 before starting the request. Read
        // once per process, never query the environment on each candidate.
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED
            .get_or_init(|| std::env::var("CLEARRA_CANDIDATE_ILC_PRUNING").as_deref() != Ok("0"))
    }
    #[cfg(not(all(feature = "local-search-ab", not(target_family = "wasm"))))]
    true
}

fn candidate_ilc_rejects(
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    workspace: &mut BuildUpWorkspace,
) -> bool {
    if !candidate_ilc_pruning_enabled() {
        return false;
    }
    if candidate.ilc_dependency_checked() {
        workspace.candidate_ilc_work.reused_prefix_certificates = workspace
            .candidate_ilc_work
            .reused_prefix_certificates
            .saturating_add(1);
        return false;
    }
    workspace.candidate_ilc_work.checks = workspace.candidate_ilc_work.checks.saturating_add(1);
    let rejected =
        RealizationFeasibilityWorkspace::relaxed_ilc_order_is_impossible(catalog, candidate);
    workspace.candidate_ilc_work.rejected_candidates = workspace
        .candidate_ilc_work
        .rejected_candidates
        .saturating_add(usize::from(rejected));
    rejected
}

pub(super) fn supports_candidate(catalog: &GeometryCatalog, candidate: &GeometryCandidate) -> bool {
    catalog.width() == 10
        && (1..=6).contains(&catalog.height())
        && (1..=11).contains(&candidate.row_ids().len())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_product(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    target: &TargetGroup,
    workspace: &mut BuildUpWorkspace,
    evaluator: &mut CoverageProductEvaluator,
    witness_mode: CandidateWitnessMode,
    retain_trace: bool,
    supply_first: bool,
    control: &ExecutionControl,
    profile_scale: u64,
) -> Result<CandidateBuildResult, WasmExactSearchError> {
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    #[cfg(test)]
    super::record_product_verifier(true);
    let span =
        SearchStageSpan::begin_scaled(ExecutorSearchStage::PlacementDependencyPc, profile_scale);
    let result = with_product_prefix_order(|work| {
        work.candidates_attempted += 1;
        // The automatic dependency route returns before the old concrete
        // verifier's optional prepass. Check completed candidates here, before
        // physical relation and full source coverage construction.
        if candidate_ilc_rejects(catalog, candidate, workspace) {
            work.candidates_certified += 1;
            return Ok(infeasible_candidate_result(0));
        }
        let source_constraint =
            super::super::symbolic_prefix_orders::SourcePrefixConstraint::for_problem(problem);
        let orders = SymbolicPrefixOrders::compile_product_completion_with_source(
            catalog,
            candidate,
            problem.kick_profile().profile_id(),
            control,
            &mut workspace.reachability,
            work,
            profile_scale,
            0,
            source_constraint,
        )
        .map_err(map_prefix_refusal)?;
        let language_span = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::PlacementDependencyPcLanguage,
            profile_scale,
        );
        let language =
            orders.compile_product_language(&mut workspace.piece_order_languages, work)?;
        language_span.finish(1);
        let mut result = if let Some(root) = language {
            // Count kinds only when the consumer can use existence alone.
            // Full coverage, empty languages and retained witnesses avoid
            // this additional O(candidate locks) check entirely.
            let source_membership_certified = witness_mode == CandidateWitnessMode::MembershipOnly
                && !retain_trace
                && problem.output_policy() == clearra_problem::SearchOutputPolicy::Summary
                && workspace
                    .standard_bag_coverage
                    .as_ref()
                    .is_some_and(|coverage| coverage.boolean_membership_enabled())
                && source_constraint.is_some_and(|constraint| {
                    constraint.certifies_every_word(
                        candidate.row_ids().len(),
                        candidate
                            .row_ids()
                            .iter()
                            .map(|&row| catalog.skeleton(row).piece),
                    )
                });
            let product_span = SearchStageSpan::begin_scaled(
                ExecutorSearchStage::WasmCoverageLanguageProduct,
                profile_scale,
            );
            let result = workspace.cover_piece_language(
                problem,
                target,
                root,
                witness_mode,
                retain_trace,
                // `root` is nonempty and was compiled from this candidate
                // using the exact constraint which issued the certificate.
                source_membership_certified,
                evaluator,
                control,
            )?;
            product_span.finish(result.coverage_product_edge_checks as u64);
            result
        } else {
            certified_language_result(None)
        };
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        work.candidates_certified += 1;
        if retain_trace && result.buildable {
            let pattern = result
                .witness_pattern_id
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_placement_dependency_witness_pattern_missing",
                ))?;
            result.representative_path = witness::materialize(
                problem,
                catalog,
                candidate,
                &orders,
                pattern,
                supply_first,
                &mut workspace.reachability,
                control,
                &mut work.pose,
            )?;
        }
        Ok(result)
    });
    span.finish(u64::from(result.is_ok()));
    result
}

fn map_prefix_refusal(reason: SymbolicPrefixRefusal) -> WasmExactSearchError {
    // Refusal is neither a physical negative nor permission to silently
    // replace a bounded request with an unrequested expensive traversal.
    match reason {
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
    }
}
