use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_coverage::{
    row::coverage_row_kind::CoverageRowKind,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_objectives::reducer::objective_reducer::{
    ObjectiveCountInput, ObjectiveCoverageIdentity, ObjectiveReducer, ObjectiveReducerError,
};
#[cfg(test)]
use clearra_objectives::{
    max_score::max_score_selection::MaxScoreCoverPolicy,
    reducer::objective_reducer::ObjectiveCandidate,
};
use clearra_supply::piece_source::PieceSource;

#[cfg(test)]
use crate::buildup::candidate_execution_aggregate::CandidateExecutionAggregate;
use crate::{
    buildup::{
        buildup_coverage_bridge::CoverageUniverseIdentity, buildup_error::BuildUpRunnerError,
        objective_pattern_input_materializer::materialize_objective_pattern_inputs,
        objective_pattern_materialization::ObjectivePatternMaterialization,
        objective_reduction_outcome::ObjectiveReductionOutcome,
    },
    packing::scenario_packing_witness::ScenarioPackingWitness,
};

#[cfg(test)]
pub(crate) fn objective_stable_key(aggregate: &CandidateExecutionAggregate) -> &str {
    aggregate.stable_key()
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn reduce_objectives(
    piece_source: &PieceSource,
    aggregates: &[CandidateExecutionAggregate],
    pattern_count: usize,
    identity: CoverageUniverseIdentity,
    witness: ScenarioPackingWitness,
    retained_trace_count: usize,
    count_complete: bool,
) -> Result<ObjectiveReductionOutcome, BuildUpRunnerError> {
    reduce_objectives_internal(
        piece_source,
        aggregates,
        pattern_count,
        identity,
        witness,
        retained_trace_count,
        count_complete,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reduce_coverage_rows_for_policy(
    piece_source: &PieceSource,
    rows: &[clearra_coverage::row::coverage_row::CoverageRow],
    pattern_count: usize,
    identity: CoverageUniverseIdentity,
    witness: ScenarioPackingWitness,
    retained_trace_count: usize,
    count_complete: bool,
    objective_kind: ObjectiveKind,
) -> Result<ObjectiveReductionOutcome, BuildUpRunnerError> {
    let inputs = match materialize_objective_pattern_inputs(piece_source, pattern_count)? {
        ObjectivePatternMaterialization::Ready(inputs) => inputs,
        ObjectivePatternMaterialization::Incomplete(reason) => {
            return Ok(ObjectiveReductionOutcome::incomplete(reason));
        }
    };
    if rows.is_empty() {
        return Ok(ObjectiveReductionOutcome::complete(None));
    }
    let result = ObjectiveReducer::reduce_canonical_unscored_rows_requested(
        rows,
        inputs.required_patterns(),
        inputs.weights(),
        ObjectiveCountInput::new(
            witness.total_solution_count,
            retained_trace_count,
            count_complete,
            retained_trace_count < witness.total_solution_count && witness.solution_found,
        ),
        ObjectiveCoverageIdentity::new(
            CoverageRowKind::Build,
            identity.piece_source_id,
            PatternUniverseId::new(identity.pattern_universe_id),
            PatternWeightModelId::new(identity.pattern_weight_model_id),
        ),
        objective_kind,
    )
    .map_err(|_error: ObjectiveReducerError| BuildUpRunnerError::Objective)?;
    // Search owns the exact coverage matrix. Scoring consumes accepted
    // executions later and cannot change this coverage result.
    Ok(ObjectiveReductionOutcome::complete(Some(result)))
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn reduce_objectives_internal(
    piece_source: &PieceSource,
    aggregates: &[CandidateExecutionAggregate],
    pattern_count: usize,
    identity: CoverageUniverseIdentity,
    witness: ScenarioPackingWitness,
    retained_trace_count: usize,
    count_complete: bool,
    objective_kind: Option<ObjectiveKind>,
) -> Result<ObjectiveReductionOutcome, BuildUpRunnerError> {
    let inputs = match materialize_objective_pattern_inputs(piece_source, pattern_count)? {
        ObjectivePatternMaterialization::Ready(inputs) => inputs,
        ObjectivePatternMaterialization::Incomplete(reason) => {
            return Ok(ObjectiveReductionOutcome::incomplete(reason));
        }
    };
    if aggregates.is_empty() {
        return Ok(ObjectiveReductionOutcome::complete(None));
    }

    let candidates = aggregates
        .iter()
        .map(|aggregate| {
            let candidate_id = usize::try_from(aggregate.candidate_id())
                .map_err(|_| BuildUpRunnerError::Objective)?;
            Ok(ObjectiveCandidate::unscored(
                candidate_id,
                objective_stable_key(aggregate).to_owned(),
                aggregate.coverage_row().coverage_bits().clone(),
            ))
        })
        .collect::<Result<Vec<_>, BuildUpRunnerError>>()?;

    let counts = ObjectiveCountInput::new(
        witness.total_solution_count,
        retained_trace_count,
        count_complete,
        retained_trace_count < witness.total_solution_count && witness.solution_found,
    );
    let coverage_identity = ObjectiveCoverageIdentity::new(
        CoverageRowKind::Build,
        identity.piece_source_id,
        PatternUniverseId::new(identity.pattern_universe_id),
        PatternWeightModelId::new(identity.pattern_weight_model_id),
    );
    let result = match objective_kind {
        Some(kind) => ObjectiveReducer::reduce_requested(
            &candidates,
            inputs.required_patterns(),
            inputs.weights(),
            counts,
            coverage_identity,
            MaxScoreCoverPolicy::default(),
            kind,
        ),
        None => ObjectiveReducer::reduce(
            &candidates,
            inputs.required_patterns(),
            inputs.weights(),
            counts,
            coverage_identity,
            MaxScoreCoverPolicy::default(),
        ),
    };
    result
        .map(|result| ObjectiveReductionOutcome::complete(Some(result)))
        .map_err(|_error: ObjectiveReducerError| BuildUpRunnerError::Objective)
}
