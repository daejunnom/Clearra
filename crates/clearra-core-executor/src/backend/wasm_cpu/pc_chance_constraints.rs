//! Request-bound early B2B authority for the typed PC probability product.
//! Ordinary solution producers retain their existing raw-family checkpoints.

use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_pc_graph::request::PcCountPolicy;
use clearra_problem::{PcChanceEvidencePolicy, SearchOutputPolicy, SearchProblem};
use clearra_supply::QueueObservationPolicy;

use crate::{CoreExecutionResult, SolutionAuditCheckpoint};

use super::WasmExactSearchError;

pub(super) const FILTERED_PC_CHANCE_BASIS: &str = "filtered-pc-chance-order-language";

pub(super) fn filters_pc_chance_constraints(problem: &SearchProblem) -> bool {
    problem.pc_chance_evidence_policy() == PcChanceEvidencePolicy::PcProbabilityV2
        && problem.output_policy() == SearchOutputPolicy::CoverageSummary
        && problem.objective().kind() == ObjectiveKind::Unique
        && problem.count_policy() == PcCountPolicy::CountUnique
        && !problem.objective().score().requested()
        && !problem.solution_probability_policy().requested()
        && problem.queue_observation_policy() == QueueObservationPolicy::FullQueueOracle
        && problem
            .objective()
            .execution_constraints()
            .preserves_back_to_back()
}

pub(super) fn mark_filtered_pc_chance_constraints(
    result: CoreExecutionResult,
    problem: &SearchProblem,
) -> Result<CoreExecutionResult, WasmExactSearchError> {
    let invalid =
        || WasmExactSearchError::InvalidProblem("wasm_pc_chance_filtered_authority_invalid");
    if !filters_pc_chance_constraints(problem) {
        return Err(invalid());
    }
    let evidence = result.pc_chance_coverage_evidence().ok_or_else(invalid)?;
    if !evidence.problem().matches_search_problem(problem) {
        return Err(invalid());
    }
    let witnessed = evidence
        .rows()
        .iter()
        .try_fold(0_u128, |count, row| {
            count.checked_add(u128::from(row.coverage_bits().count_ones()))
        })
        .ok_or_else(invalid)?;
    let text = |name| result.unique_field(name).ok_or_else(invalid);
    let count_complete = text("count_complete")?;
    let probability_complete = text("probability_complete")?;
    if !matches!(count_complete, "true" | "false")
        || !matches!(probability_complete, "true" | "false")
    {
        return Err(invalid());
    }
    let complete = count_complete == "true" && probability_complete == "true";
    let fields = vec![
        field("execution_constraint_materialized", true),
        field(
            "b2b_preservation_evaluation_basis",
            FILTERED_PC_CHANCE_BASIS,
        ),
        field("b2b_preservation_selection", "existential"),
        field(
            "b2b_preservation_denominator_semantics",
            "original-materialized-queue",
        ),
        field(
            "b2b_preserving_pattern_count",
            text("covered_pattern_count")?,
        ),
        field(
            "b2b_preservation_pattern_universe_count",
            text("coverage_pattern_count")?,
        ),
        field(
            "b2b_preservation_probability",
            text("coverage_probability")?,
        ),
        field("b2b_preservation_count_complete", count_complete),
        field(
            "b2b_preservation_probability_complete",
            probability_complete,
        ),
        field("b2b_preserving_candidate_pattern_count", witnessed),
        field("b2b_preservation_path_multiplicity_counted", false),
        field("objective_complete", complete),
        field(
            "objective_incomplete_reason",
            if complete {
                "none"
            } else {
                "search-incomplete"
            },
        ),
    ];
    // Probability output does not count or retain a public solution family.
    // The only pre-filter family known here is unknown, never the filtered row count.
    let result = result
        .with_replaced_fields(fields)
        .with_pre_b2b_produced_solution_audit_checkpoint(SolutionAuditCheckpoint::unknown(
            "b2b-filtered-during-pc-chance-order-construction",
        ))
        .with_pre_b2b_solution_audit_checkpoint(SolutionAuditCheckpoint::unknown(
            "b2b-filtered-during-pc-chance-order-construction",
        ));
    validate_filtered_pc_chance_metadata(&result, problem)?;
    Ok(result)
}

/// A distributed worker cannot turn raw or differently filtered rows into the
/// coordinator's typed filtered proof by supplying only the same source IDs.
pub(super) fn validate_filtered_pc_chance_metadata(
    result: &CoreExecutionResult,
    problem: &SearchProblem,
) -> Result<(), WasmExactSearchError> {
    let invalid =
        || WasmExactSearchError::InvalidProblem("wasm_pc_chance_filtered_authority_invalid");
    if !filters_pc_chance_constraints(problem) {
        return Err(invalid());
    }
    for (name, expected) in [
        ("execution_constraint_preserve_b2b", "true"),
        ("execution_constraint_materialized", "true"),
        (
            "execution_constraint_spin_profile",
            problem
                .objective()
                .execution_constraints()
                .spin_profile()
                .as_str(),
        ),
        (
            "b2b_preservation_evaluation_basis",
            FILTERED_PC_CHANCE_BASIS,
        ),
        ("b2b_preservation_selection", "existential"),
        (
            "b2b_preservation_denominator_semantics",
            "original-materialized-queue",
        ),
        ("b2b_preservation_path_multiplicity_counted", "false"),
    ] {
        if result.unique_field(name) != Some(expected) {
            return Err(invalid());
        }
    }
    for (filtered, ordinary) in [
        ("b2b_preserving_pattern_count", "covered_pattern_count"),
        (
            "b2b_preservation_pattern_universe_count",
            "coverage_pattern_count",
        ),
        ("b2b_preservation_probability", "coverage_probability"),
        ("b2b_preservation_count_complete", "count_complete"),
        (
            "b2b_preservation_probability_complete",
            "probability_complete",
        ),
    ] {
        let expected = result.unique_field(ordinary).ok_or_else(invalid)?;
        if result.unique_field(filtered) != Some(expected) {
            return Err(invalid());
        }
    }
    Ok(())
}

fn field(name: &str, value: impl ToString) -> (String, String) {
    (name.to_owned(), value.to_string())
}
