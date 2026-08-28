// SRP rationale: this module has one behavior-level change reason: deriving deterministic PC score matrices, summaries, and winner families from exact search results.

use std::{collections::BTreeMap, sync::Arc};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    solution::normalized_tiling_solution::{
        NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
    },
};
use clearra_core_executor::{CoreExecutionError, CoreExecutionResult, SolutionAverageScoreReport};
use clearra_objectives::policy::score_objective_policy::{
    ScoreObjectiveMode, ScoreObjectivePolicy, ScoreProfileSelection, SpinProfileSelection,
};
use clearra_postprocess::{
    checked_score_profile_memory_projection, score_profile_with_memory_guard, CandidateExecution,
    CandidateExecutionAggregate, ExactScoreCellMaterializationError, ExactScoredExecution,
    ExactScoringExecutionMaterializer, PcScoringMemoryGuardError, PcScoringPostProcessInput,
    PcScoringPostProcessor, ScoreCell, ScoreMatrix, ScoreMatrixMemoryGuardError,
    ScoreProfileMemoryGuardError,
};
use clearra_scoring::{
    builtin::{
        guideline_pc_score_with_spin_profile, jstris_ultra_pc_score_with_spin_profile,
        tetrio_pc_score_with_spin_profile,
    },
    profile::SpinProfileId,
};

use crate::pc_score_winner_result::PcScorePatternWinnerV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcScoreExecutionSource {
    NativeLegacyReplay,
    WasmExactBatch,
    DistributedPrecomputedCells,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PcScoreDerivation {
    source: PcScoreExecutionSource,
    execution_source_complete: bool,
    pattern_winners: Arc<Vec<PcScorePatternWinnerV1>>,
}

impl PcScoreDerivation {
    fn new(
        source: PcScoreExecutionSource,
        execution_source_complete: bool,
        fields: &[(String, String)],
        pattern_winners: Arc<Vec<PcScorePatternWinnerV1>>,
    ) -> Result<Self, CoreExecutionError> {
        let required = [
            "score_profile",
            "score_accuracy_level",
            "score_accuracy_reason",
            "score_profile_specific_exact",
            "score_evaluation_complete",
            "score_evaluation_basis",
            "score_evaluation_scope",
            "score_matrix_materialized",
            "score_matrix_complete",
            "score_matrix_cell_count",
            "score_matrix_pattern_count",
            "score_summary_complete",
            "score_summary_incomplete_reason",
            "score_all_universe_patterns_covered",
            "score_pattern_optimal_count",
            "score_failed_pc_pattern_count",
            "score_covered_probability",
            "score_unconditional_expected_score",
            "score_unconditional_expected_attack",
        ];
        if required
            .into_iter()
            .any(|key| unique_field_value(fields, key).is_none())
        {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_derivation_contract_incomplete",
            });
        }
        Ok(Self {
            source,
            execution_source_complete,
            pattern_winners,
        })
    }

    pub(crate) const fn source(&self) -> PcScoreExecutionSource {
        self.source
    }

    pub(crate) const fn execution_source_complete(&self) -> bool {
        self.execution_source_complete
    }

    pub(crate) fn pattern_winners(&self) -> &[PcScorePatternWinnerV1] {
        self.pattern_winners.as_slice()
    }

    pub(crate) fn pattern_winner_owner(&self) -> &Arc<Vec<PcScorePatternWinnerV1>> {
        &self.pattern_winners
    }
}

pub(crate) struct PcScorePostprocessOutput {
    result: CoreExecutionResult,
    derivation: Option<PcScoreDerivation>,
}

impl PcScorePostprocessOutput {
    pub(crate) fn into_result(self) -> CoreExecutionResult {
        self.result
    }

    pub(crate) fn into_parts(self) -> (CoreExecutionResult, Option<PcScoreDerivation>) {
        (self.result, self.derivation)
    }
}

pub(crate) fn apply_pc_postprocess(
    result: CoreExecutionResult,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    apply_pc_postprocess_internal(result, control, false).map(PcScorePostprocessOutput::into_result)
}

pub(crate) fn apply_pc_postprocess_with_derivation(
    result: CoreExecutionResult,
    control: &ExecutionControl,
) -> Result<PcScorePostprocessOutput, CoreExecutionError> {
    apply_pc_postprocess_internal(result, control, true)
}

/// Typed `pc.score` post-processing while the producer's shared execution
/// lease is still alive. Every count-proportional allocation is projected
/// before reserve and the caller's terminal memory authority sees the whole
/// live peak, including intermediates that are not yet owned by `result`.
pub(crate) fn apply_pc_postprocess_with_derivation_and_memory_guard(
    result: CoreExecutionResult,
    control: &ExecutionControl,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<PcScorePostprocessOutput, CoreExecutionError> {
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    if result.field("postprocess_scoring_requested") != Some("true") {
        return Ok(PcScorePostprocessOutput {
            result,
            derivation: None,
        });
    }
    if result.postprocess_score_profile_id().is_some()
        || !result.postprocess_score_cells().is_empty()
    {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_typed_distributed_cells_not_authoritative",
        });
    }

    let score_policy = score_policy_from_result(&result);
    let profile_projection = checked_score_profile_memory_projection(score_policy).ok_or(
        CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_profile_memory_projection_overflow",
        },
    )?;
    memory_guard(&result, profile_projection.required_memory_bytes)?;
    let (profile, profile_report) = score_profile_with_memory_guard(score_policy, 0, u128::MAX)
        .map_err(map_score_profile_memory_error)?;
    memory_guard(&result, profile_report.retained_bytes)?;
    let profile_retained_bytes = profile_report.retained_bytes;
    let probability = result.field("coverage_probability").unwrap_or("0");
    let pattern_count = result.usize_field("coverage_pattern_count").unwrap_or(0);
    let search_objective_complete = result
        .bool_field("objective_search_complete")
        .unwrap_or(false);
    let retained_trace_count = result.usize_field("retained_trace_count").unwrap_or(0);

    let weight_count = result.postprocess_pattern_weights().len();
    let projected_weight_bytes = checked_score_product(
        weight_count,
        core::mem::size_of::<f64>(),
        "pc_score_pattern_weight_memory_projection_overflow",
    )?;
    memory_guard(
        &result,
        checked_score_add(
            profile_retained_bytes,
            projected_weight_bytes,
            "pc_score_pattern_weight_memory_projection_overflow",
        )?,
    )?;
    let mut pattern_weights = Vec::new();
    pattern_weights
        .try_reserve_exact(weight_count)
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_pattern_weight_allocation_failed",
        })?;
    for value in result.postprocess_pattern_weights() {
        let weight = value
            .parse::<f64>()
            .ok()
            .filter(|weight| weight.is_finite() && *weight >= 0.0)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_pattern_weight_invalid",
            })?;
        pattern_weights.push(weight);
    }
    let pattern_weight_bytes = checked_score_product(
        pattern_weights.capacity(),
        core::mem::size_of::<f64>(),
        "pc_score_pattern_weight_memory_projection_overflow",
    )?;
    memory_guard(
        &result,
        checked_score_add(
            profile_retained_bytes,
            pattern_weight_bytes,
            "pc_score_pattern_weight_memory_projection_overflow",
        )?,
    )?;
    let weights_complete = pattern_count > 0
        && pattern_weights.len() == pattern_count
        && (pattern_weights.iter().sum::<f64>() - 1.0).abs() <= 1.0e-8;
    if !weights_complete {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_pattern_weight_model_incomplete",
        });
    }

    let batch =
        result
            .exact_scoring_execution_batch()
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_exact_batch_missing",
            })?;
    let materialization_projection =
        ExactScoringExecutionMaterializer::checked_score_cell_memory_projection_with_profile_bytes(
            batch,
            profile_retained_bytes,
        )
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_cell_memory_projection_overflow",
        })?;
    memory_guard(
        &result,
        checked_score_sum(
            &[
                pattern_weight_bytes,
                materialization_projection.required_peak_bytes,
            ],
            "pc_score_cell_memory_projection_overflow",
        )?,
    )?;
    let (materialized, materialization_report) =
        ExactScoringExecutionMaterializer::materialize_score_cells_with_profile_and_memory_limit(
            batch,
            score_policy,
            &profile,
            profile_retained_bytes,
            control,
            pattern_weight_bytes,
            u128::MAX,
        )
        .map_err(map_score_cell_materialization_error)?;
    let execution_source_complete = materialized.complete() && search_objective_complete;
    if !execution_source_complete {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_cell_materialization_incomplete",
        });
    }
    let live_after_materialization = checked_score_sum(
        &[
            profile_retained_bytes,
            pattern_weight_bytes,
            materialization_report.retained_bytes,
        ],
        "pc_score_cell_memory_projection_overflow",
    )?;
    memory_guard(&result, live_after_materialization)?;

    let identity_count = result.normalized_solution_identities().len();
    let projected_identity_bytes = checked_score_product(
        identity_count,
        core::mem::size_of::<StandardBoard64TilingIdentity>(),
        "pc_score_identity_memory_projection_overflow",
    )?;
    memory_guard(
        &result,
        checked_score_add(
            live_after_materialization,
            projected_identity_bytes,
            "pc_score_identity_memory_projection_overflow",
        )?,
    )?;
    let mut identities = Vec::new();
    identities.try_reserve_exact(identity_count).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_identity_allocation_failed",
        }
    })?;
    identities.extend_from_slice(result.normalized_solution_identities());
    identities.sort_unstable();
    identities.dedup();
    let identity_bytes = checked_score_product(
        identities.capacity(),
        core::mem::size_of::<StandardBoard64TilingIdentity>(),
        "pc_score_identity_memory_projection_overflow",
    )?;

    let scored_execution_count = materialized.scored_executions().len();
    let projected_cell_outer_bytes = checked_score_product(
        scored_execution_count,
        core::mem::size_of::<ScoreCell>(),
        "pc_score_matrix_memory_projection_overflow",
    )?;
    memory_guard(
        &result,
        checked_score_sum(
            &[
                profile_retained_bytes,
                pattern_weight_bytes,
                materialization_report.retained_bytes,
                identity_bytes,
                projected_cell_outer_bytes,
            ],
            "pc_score_matrix_memory_projection_overflow",
        )?,
    )?;
    let mut cells = Vec::new();
    cells
        .try_reserve_exact(scored_execution_count)
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_matrix_cell_allocation_failed",
        })?;
    let mut trace_identity_bytes = 0_u128;
    for execution in materialized.into_scored_executions() {
        let (candidate_identity, pattern_id, trace_identity, score, attack) =
            execution.into_parts();
        let candidate_index = identities.binary_search(&candidate_identity).map_err(|_| {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_matrix_candidate_identity_missing",
            }
        })?;
        trace_identity_bytes = trace_identity_bytes
            .checked_add(trace_identity.capacity() as u128)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_matrix_memory_projection_overflow",
            })?;
        cells.push(ScoreCell::new_with_static_accuracy(
            (candidate_index + 1) as u64,
            pattern_id,
            trace_identity,
            score,
            attack,
            profile.accuracy_level().as_str(),
        ));
    }
    let cell_outer_bytes = checked_score_product(
        cells.capacity(),
        core::mem::size_of::<ScoreCell>(),
        "pc_score_matrix_memory_projection_overflow",
    )?;
    let cell_retained_bytes = checked_score_add(
        cell_outer_bytes,
        trace_identity_bytes,
        "pc_score_matrix_memory_projection_overflow",
    )?;
    let matrix_projection = ScoreMatrix::checked_materialized_cells_memory_projection(
        &cells,
        cells.capacity(),
        &profile,
    )
    .ok_or(CoreExecutionError::RuntimeUnavailable {
        component: "pc_score_matrix_memory_projection_overflow",
    })?;
    debug_assert_eq!(
        matrix_projection.cell_outer_storage_bytes + matrix_projection.cell_string_storage_bytes,
        cell_retained_bytes
    );
    let matrix_external_retained_bytes = checked_score_sum(
        &[profile_retained_bytes, pattern_weight_bytes, identity_bytes],
        "pc_score_matrix_memory_projection_overflow",
    )?;
    memory_guard(
        &result,
        checked_score_add(
            matrix_external_retained_bytes,
            matrix_projection.required_peak_bytes,
            "pc_score_matrix_memory_projection_overflow",
        )?,
    )?;
    let (matrix, matrix_report) = ScoreMatrix::from_materialized_cells_with_memory_guard(
        cells,
        &profile,
        pattern_count,
        execution_source_complete && weights_complete,
        matrix_external_retained_bytes,
        u128::MAX,
    )
    .map_err(map_score_matrix_memory_error)?;
    let matrix_retained_bytes = matrix_report.retained_bytes;
    memory_guard(
        &result,
        checked_score_sum(
            &[
                profile_retained_bytes,
                pattern_weight_bytes,
                identity_bytes,
                matrix_retained_bytes,
            ],
            "pc_score_matrix_memory_projection_overflow",
        )?,
    )?;

    let projected_pattern_winner_count = checked_pattern_winner_count(&matrix)?;
    let projected_pattern_winner_retained_bytes = checked_score_add(
        checked_score_product(
            projected_pattern_winner_count,
            core::mem::size_of::<PcScorePatternWinnerV1>(),
            "pc_score_pattern_winner_memory_projection_overflow",
        )?,
        core::mem::size_of::<Vec<PcScorePatternWinnerV1>>() as u128,
        "pc_score_pattern_winner_memory_projection_overflow",
    )?;
    memory_guard(
        &result,
        checked_score_sum(
            &[
                profile_retained_bytes,
                pattern_weight_bytes,
                identity_bytes,
                matrix_retained_bytes,
                projected_pattern_winner_retained_bytes,
            ],
            "pc_score_pattern_winner_memory_projection_overflow",
        )?,
    )?;
    let pattern_winners = Arc::new(try_materialize_pattern_winners(&matrix, &identities)?);
    let pattern_winner_retained_bytes = checked_score_add(
        checked_score_product(
            pattern_winners.capacity(),
            core::mem::size_of::<PcScorePatternWinnerV1>(),
            "pc_score_pattern_winner_memory_projection_overflow",
        )?,
        core::mem::size_of::<Vec<PcScorePatternWinnerV1>>() as u128,
        "pc_score_pattern_winner_memory_projection_overflow",
    )?;
    if pattern_winners.len() != projected_pattern_winner_count {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_pattern_winner_count_mismatch",
        });
    }
    drop(identities);

    let input = PcScoringPostProcessInput::new(
        result.postprocess_replay_trace(),
        &[],
        &pattern_weights,
        pattern_count,
        execution_source_complete && weights_complete,
        score_policy,
        search_objective_complete,
        probability,
        retained_trace_count,
    );
    let processor_projection = PcScoringPostProcessor::checked_materialized_memory_projection(
        input,
        &profile,
        profile_retained_bytes,
        &matrix,
    )
    .ok_or(CoreExecutionError::RuntimeUnavailable {
        component: "pc_score_summary_memory_projection_overflow",
    })?;
    let external_retained_bytes = checked_score_add(
        pattern_weight_bytes,
        pattern_winner_retained_bytes,
        "pc_score_pattern_winner_memory_projection_overflow",
    )?;
    memory_guard(
        &result,
        checked_score_add(
            external_retained_bytes,
            processor_projection.required_peak_bytes,
            "pc_score_summary_memory_projection_overflow",
        )?,
    )?;
    let (postprocess, postprocess_report) =
        PcScoringPostProcessor::process_materialized_with_memory_guard(
            input,
            &profile,
            profile_retained_bytes,
            matrix,
            control,
            external_retained_bytes,
            u128::MAX,
        )
        .map_err(map_score_summary_memory_error)?;
    memory_guard(
        &result,
        checked_score_sum(
            &[
                external_retained_bytes,
                profile_retained_bytes,
                postprocess_report.result_retained_bytes,
            ],
            "pc_score_summary_memory_projection_overflow",
        )?,
    )?;

    let mut fields = postprocess.fields();
    let appended_field_projection = checked_score_sum(
        &[
            postprocess_report.result_retained_bytes,
            checked_score_product(
                fields
                    .len()
                    .checked_add(4)
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "pc_score_summary_memory_projection_overflow",
                    })?,
                core::mem::size_of::<(String, String)>(),
                "pc_score_summary_memory_projection_overflow",
            )?,
            ("score_execution_distribution".len()
                + "coordinator".len()
                + "score_distributed_cell_count".len()
                + 1
                + "score_equality_basis".len()
                + "score-only".len()
                + "informational_attack_basis".len()
                + "canonical-equal-score-trace".len()) as u128,
        ],
        "pc_score_summary_memory_projection_overflow",
    )?;
    memory_guard(
        &result,
        checked_score_sum(
            &[
                external_retained_bytes,
                profile_retained_bytes,
                appended_field_projection,
            ],
            "pc_score_summary_memory_projection_overflow",
        )?,
    )?;
    fields
        .try_reserve_exact(4)
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_summary_field_allocation_failed",
        })?;
    fields.push((
        try_score_string("score_execution_distribution")?,
        try_score_string("coordinator")?,
    ));
    fields.push((
        try_score_string("score_distributed_cell_count")?,
        try_score_string("0")?,
    ));
    fields.push((
        try_score_string("score_equality_basis")?,
        try_score_string("score-only")?,
    ));
    fields.push((
        try_score_string("informational_attack_basis")?,
        try_score_string("canonical-equal-score-trace")?,
    ));
    let derivation = PcScoreDerivation::new(
        PcScoreExecutionSource::WasmExactBatch,
        execution_source_complete && weights_complete,
        &fields,
        pattern_winners,
    )?;
    drop(pattern_weights);
    drop(profile);

    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    let result = result
        .try_with_replaced_fields_with_memory_guard(fields, |live, future| {
            memory_guard(
                live,
                checked_score_add(
                    future,
                    pattern_winner_retained_bytes,
                    "pc_score_pattern_winner_memory_projection_overflow",
                )?,
            )
        })
        .map_err(map_score_field_replacement_error)?;
    memory_guard(&result, pattern_winner_retained_bytes)?;
    Ok(PcScorePostprocessOutput {
        result,
        derivation: Some(derivation),
    })
}

fn for_each_candidate_score_maximum(matrix: &ScoreMatrix, mut visit: impl FnMut(&ScoreCell)) {
    let cells = matrix.cells();
    let mut index = 0;
    while index < cells.len() {
        let pattern_id = cells[index].pattern_id();
        let candidate_id = cells[index].candidate_id();
        let mut selected = &cells[index];
        index += 1;
        while index < cells.len()
            && cells[index].pattern_id() == pattern_id
            && cells[index].candidate_id() == candidate_id
        {
            let cell = &cells[index];
            if cell.score() > selected.score()
                || cell.score() == selected.score()
                    && cell.trace_identity() < selected.trace_identity()
            {
                selected = cell;
            }
            index += 1;
        }
        visit(selected);
    }
}

fn checked_pattern_winner_count(matrix: &ScoreMatrix) -> Result<usize, CoreExecutionError> {
    let mut active_pattern = None;
    let mut active_maximum_score = 0_u64;
    let mut active_winner_count = 0_usize;
    let mut total = 0_usize;
    let mut overflow = false;

    for_each_candidate_score_maximum(matrix, |cell| {
        if overflow {
            return;
        }
        if active_pattern != Some(cell.pattern_id()) {
            total = match total.checked_add(active_winner_count) {
                Some(total) => total,
                None => {
                    overflow = true;
                    return;
                }
            };
            active_pattern = Some(cell.pattern_id());
            active_maximum_score = cell.score();
            active_winner_count = 1;
        } else if cell.score() > active_maximum_score {
            active_maximum_score = cell.score();
            active_winner_count = 1;
        } else if cell.score() == active_maximum_score {
            active_winner_count = match active_winner_count.checked_add(1) {
                Some(count) => count,
                None => {
                    overflow = true;
                    return;
                }
            };
        }
    });
    if overflow {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_pattern_winner_count_overflow",
        });
    }
    total
        .checked_add(active_winner_count)
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_pattern_winner_count_overflow",
        })
}

fn try_materialize_pattern_winners(
    matrix: &ScoreMatrix,
    identities: &[StandardBoard64TilingIdentity],
) -> Result<Vec<PcScorePatternWinnerV1>, CoreExecutionError> {
    let winner_count = checked_pattern_winner_count(matrix)?;
    let mut winners = Vec::new();
    winners.try_reserve_exact(winner_count).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_pattern_winner_allocation_failed",
        }
    })?;

    let mut active_pattern = None;
    let mut active_maximum_score = 0_u64;
    let mut active_pattern_start = 0_usize;
    let mut error = None;
    for_each_candidate_score_maximum(matrix, |cell| {
        if error.is_some() {
            return;
        }
        if active_pattern != Some(cell.pattern_id()) {
            active_pattern = Some(cell.pattern_id());
            active_maximum_score = cell.score();
            active_pattern_start = winners.len();
        } else if cell.score() > active_maximum_score {
            active_maximum_score = cell.score();
            winners.truncate(active_pattern_start);
        } else if cell.score() < active_maximum_score {
            return;
        }

        let Some(identity_index) = cell
            .candidate_id()
            .checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
        else {
            error = Some(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_pattern_winner_candidate_id_invalid",
            });
            return;
        };
        let Some(solution_identity) = identities.get(identity_index).copied() else {
            error = Some(CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_pattern_winner_candidate_identity_missing",
            });
            return;
        };
        winners.push(PcScorePatternWinnerV1::new(
            cell.pattern_id(),
            cell.candidate_id(),
            solution_identity,
            cell.score(),
            cell.attack(),
        ));
    });
    if let Some(error) = error {
        return Err(error);
    }
    if winners.len() != winner_count {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_pattern_winner_count_mismatch",
        });
    }
    Ok(winners)
}

fn apply_pc_postprocess_internal(
    result: CoreExecutionResult,
    control: &ExecutionControl,
    capture_derivation: bool,
) -> Result<PcScorePostprocessOutput, CoreExecutionError> {
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    if result.field("postprocess_scoring_requested") != Some("true") {
        return Ok(PcScorePostprocessOutput {
            result,
            derivation: None,
        });
    }

    let probability = result
        .field("coverage_probability")
        .unwrap_or("0")
        .to_owned();
    let score_policy = score_policy_from_result(&result);
    let pattern_weights = result
        .postprocess_pattern_weights()
        .iter()
        .map(|value| value.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    let pattern_count = result.usize_field("coverage_pattern_count").unwrap_or(0);
    let weights_complete = pattern_weights.len() == pattern_count && pattern_count > 0;
    let search_objective_complete = result
        .bool_field("objective_search_complete")
        .unwrap_or(false);
    let retained_trace_count = result.usize_field("retained_trace_count").unwrap_or(0);
    let distributed_score_available = result.postprocess_score_profile_id().is_some();
    let mut exact_materialization_present = false;
    let (postprocess, solution_average_scores, execution_source_complete) =
        if distributed_score_available {
            let profile = score_profile_for_policy(score_policy);
            let mut identities = result.normalized_solution_identities().to_vec();
            identities.sort_unstable();
            identities.dedup();
            let profile_matches = result.postprocess_score_profile_id() == Some(profile.id());
            let mut identities_complete = true;
            let cells = result
                .postprocess_score_cells()
                .iter()
                .filter_map(
                    |cell| match identities.binary_search(&cell.candidate_identity()) {
                        Ok(index) => Some(ScoreCell::new(
                            (index + 1) as u64,
                            cell.pattern_id(),
                            cell.trace_identity(),
                            cell.score(),
                            cell.attack(),
                            profile.accuracy_level().as_str(),
                        )),
                        Err(_) => {
                            identities_complete = false;
                            None
                        }
                    },
                )
                .collect::<Vec<_>>();
            let execution_source_complete = result.postprocess_score_cells_complete()
                && profile_matches
                && identities_complete
                && search_objective_complete;
            let matrix = ScoreMatrix::from_materialized_cells(
                cells,
                &profile,
                pattern_count,
                execution_source_complete && weights_complete,
            );
            let solution_average_scores = solution_average_score_reports(
                &identities,
                &matrix,
                &pattern_weights,
                pattern_count,
            );
            (
                PcScoringPostProcessor::process_materialized_with_control(
                    PcScoringPostProcessInput::new(
                        result.postprocess_replay_trace(),
                        &[],
                        &pattern_weights,
                        pattern_count,
                        execution_source_complete && weights_complete,
                        score_policy,
                        search_objective_complete,
                        &probability,
                        retained_trace_count,
                    ),
                    matrix,
                    control,
                ),
                solution_average_scores,
                execution_source_complete && weights_complete,
            )
        } else {
            let legacy_candidate_executions = candidate_execution_aggregates(&result);
            let exact_materialization = result
                .exact_scoring_execution_batch()
                .map(|batch| {
                    ExactScoringExecutionMaterializer::materialize(batch, score_policy, control)
                })
                .transpose()
                .map_err(|_| CoreExecutionError::Cancelled)?;
            exact_materialization_present = exact_materialization.is_some();
            let candidate_executions = exact_materialization
                .as_ref()
                .map_or(legacy_candidate_executions.as_slice(), |materialized| {
                    materialized.aggregates()
                });
            let execution_source_complete = exact_materialization
                .as_ref()
                .map_or(result.postprocess_execution_complete(), |materialized| {
                    materialized.complete()
                })
                && search_objective_complete;
            let solution_average_scores =
                exact_materialization
                    .as_ref()
                    .map_or_else(Vec::new, |materialized| {
                        let profile = score_profile_for_policy(score_policy);
                        let mut identities = result.normalized_solution_identities().to_vec();
                        identities.sort_unstable();
                        identities.dedup();
                        let matrix = score_matrix_for_exact_solutions(
                            &identities,
                            materialized.scored_executions(),
                            &profile,
                            pattern_count,
                            execution_source_complete && weights_complete,
                        );
                        solution_average_score_reports(
                            &identities,
                            &matrix,
                            &pattern_weights,
                            pattern_count,
                        )
                    });
            (
                PcScoringPostProcessor::process_with_control(
                    PcScoringPostProcessInput::new(
                        result.postprocess_replay_trace(),
                        candidate_executions,
                        &pattern_weights,
                        pattern_count,
                        execution_source_complete && weights_complete,
                        score_policy,
                        search_objective_complete,
                        &probability,
                        retained_trace_count,
                    ),
                    control,
                ),
                solution_average_scores,
                execution_source_complete && weights_complete,
            )
        };
    let postprocess = postprocess.map_err(|_| CoreExecutionError::Cancelled)?;

    let mut fields = postprocess.fields();
    let derivation = if capture_derivation {
        let source = if distributed_score_available {
            PcScoreExecutionSource::DistributedPrecomputedCells
        } else if exact_materialization_present {
            PcScoreExecutionSource::WasmExactBatch
        } else {
            PcScoreExecutionSource::NativeLegacyReplay
        };
        Some(PcScoreDerivation::new(
            source,
            execution_source_complete,
            &fields,
            Arc::new(Vec::new()),
        )?)
    } else {
        None
    };
    fields.push((
        "score_execution_distribution".to_owned(),
        if distributed_score_available {
            "worker-partitions"
        } else {
            "coordinator"
        }
        .to_owned(),
    ));
    fields.push((
        "score_distributed_cell_count".to_owned(),
        result.postprocess_score_cells().len().to_string(),
    ));
    fields.push(("score_equality_basis".to_owned(), "score-only".to_owned()));
    fields.push((
        "informational_attack_basis".to_owned(),
        "canonical-equal-score-trace".to_owned(),
    ));

    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    Ok(PcScorePostprocessOutput {
        result: result
            .with_solution_average_scores(solution_average_scores)
            .with_replaced_fields(fields),
        derivation,
    })
}

fn checked_score_product(
    count: usize,
    element_size: usize,
    component: &'static str,
) -> Result<u128, CoreExecutionError> {
    (count as u128)
        .checked_mul(element_size as u128)
        .ok_or(CoreExecutionError::RuntimeUnavailable { component })
}

fn checked_score_add(
    left: u128,
    right: u128,
    component: &'static str,
) -> Result<u128, CoreExecutionError> {
    left.checked_add(right)
        .ok_or(CoreExecutionError::RuntimeUnavailable { component })
}

fn checked_score_sum(values: &[u128], component: &'static str) -> Result<u128, CoreExecutionError> {
    values.iter().try_fold(0_u128, |total, value| {
        checked_score_add(total, *value, component)
    })
}

fn try_score_string(value: &str) -> Result<String, CoreExecutionError> {
    let mut owned = String::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_summary_field_allocation_failed",
        })?;
    owned.push_str(value);
    Ok(owned)
}

fn map_score_profile_memory_error(error: ScoreProfileMemoryGuardError) -> CoreExecutionError {
    match error {
        ScoreProfileMemoryGuardError::ProjectionOverflow => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_profile_memory_projection_overflow",
            }
        }
        ScoreProfileMemoryGuardError::LimitExceeded { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_profile_memory_limit_exceeded",
            }
        }
        ScoreProfileMemoryGuardError::AllocationFailed => CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_profile_allocation_failed",
        },
    }
}

fn map_score_cell_materialization_error(
    error: ExactScoreCellMaterializationError,
) -> CoreExecutionError {
    match error {
        ExactScoreCellMaterializationError::Cancelled => CoreExecutionError::Cancelled,
        ExactScoreCellMaterializationError::ProfileMismatch => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_cell_profile_mismatch",
            }
        }
        ExactScoreCellMaterializationError::ProjectionOverflow => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_cell_memory_projection_overflow",
            }
        }
        ExactScoreCellMaterializationError::LimitExceeded { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_cell_memory_limit_exceeded",
            }
        }
        ExactScoreCellMaterializationError::AllocationFailed => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_cell_allocation_failed",
            }
        }
    }
}

fn map_score_matrix_memory_error(error: ScoreMatrixMemoryGuardError) -> CoreExecutionError {
    match error {
        ScoreMatrixMemoryGuardError::ProjectionOverflow => CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_matrix_memory_projection_overflow",
        },
        ScoreMatrixMemoryGuardError::LimitExceeded { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_matrix_memory_limit_exceeded",
            }
        }
        ScoreMatrixMemoryGuardError::AllocationFailed => CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_matrix_allocation_failed",
        },
    }
}

fn map_score_summary_memory_error(error: PcScoringMemoryGuardError) -> CoreExecutionError {
    match error {
        PcScoringMemoryGuardError::Cancelled => CoreExecutionError::Cancelled,
        PcScoringMemoryGuardError::ProfileMismatch => CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_summary_profile_mismatch",
        },
        PcScoringMemoryGuardError::ProjectionOverflow => CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_summary_memory_projection_overflow",
        },
        PcScoringMemoryGuardError::LimitExceeded { .. } => CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_summary_memory_limit_exceeded",
        },
        PcScoringMemoryGuardError::AllocationFailed => CoreExecutionError::RuntimeUnavailable {
            component: "pc_score_summary_allocation_failed",
        },
    }
}

fn map_score_field_replacement_error(
    error: clearra_core_executor::core_execution_result::CoreResultFieldReplacementError<
        CoreExecutionError,
    >,
) -> CoreExecutionError {
    match error {
        clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::ProjectionOverflow => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_result_field_memory_projection_overflow",
            }
        }
        clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::AllocationFailed { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "pc_score_result_field_allocation_failed",
            }
        }
        clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::MemoryGuard(error) => error,
    }
}

fn score_matrix_for_exact_solutions(
    identities: &[StandardBoard64TilingIdentity],
    executions: &[ExactScoredExecution],
    profile: &clearra_scoring::profile::ScoreProfile,
    pattern_count: usize,
    source_complete: bool,
) -> ScoreMatrix {
    let mut identities_complete = true;
    let cells = executions
        .iter()
        .filter_map(
            |execution| match identities.binary_search(&execution.candidate_identity()) {
                Ok(index) => Some(ScoreCell::new(
                    (index + 1) as u64,
                    execution.pattern_id(),
                    execution.trace_identity(),
                    execution.score(),
                    execution.attack(),
                    profile.accuracy_level().as_str(),
                )),
                Err(_) => {
                    identities_complete = false;
                    None
                }
            },
        )
        .collect();
    ScoreMatrix::from_materialized_cells(
        cells,
        profile,
        pattern_count,
        source_complete && identities_complete,
    )
}

fn solution_average_score_reports(
    identities: &[StandardBoard64TilingIdentity],
    matrix: &ScoreMatrix,
    pattern_weights: &[f64],
    pattern_count: usize,
) -> Vec<SolutionAverageScoreReport> {
    let weights_valid = pattern_count > 0
        && pattern_weights.len() == pattern_count
        && pattern_weights
            .iter()
            .all(|weight| weight.is_finite() && *weight >= 0.0)
        && (pattern_weights.iter().sum::<f64>() - 1.0).abs() <= 1.0e-8;
    if identities.is_empty() || !weights_valid {
        return Vec::new();
    }

    let mut expected_scores = vec![0.0_f64; identities.len()];
    let mut covered_patterns = vec![0_usize; identities.len()];
    let mut identities_valid = true;
    for cell in matrix.highest_legal_cells_by_candidate_pattern() {
        let Some(candidate_index) = usize::try_from(cell.candidate_id())
            .ok()
            .and_then(|candidate_id| candidate_id.checked_sub(1))
            .filter(|index| *index < identities.len())
        else {
            identities_valid = false;
            continue;
        };
        let Some(weight) = pattern_weights.get(cell.pattern_id()) else {
            identities_valid = false;
            continue;
        };
        expected_scores[candidate_index] += *weight * cell.score() as f64;
        covered_patterns[candidate_index] = covered_patterns[candidate_index].saturating_add(1);
    }
    let score_complete = matrix.complete() && identities_valid;

    identities
        .iter()
        .copied()
        .enumerate()
        .map(|(index, identity)| {
            let average_score = expected_scores[index];
            SolutionAverageScoreReport::new(
                NormalizedTilingSolutionKey::from_standard_board64_identity(identity).to_string(),
                if average_score == 0.0 {
                    "0".to_owned()
                } else {
                    average_score.to_string()
                },
                covered_patterns[index],
                pattern_count,
                score_complete,
            )
        })
        .collect()
}

pub(crate) fn score_policy_from_result(result: &CoreExecutionResult) -> ScoreObjectivePolicy {
    let mode = match result.field("score_objective_mode") {
        Some("summary") => ScoreObjectiveMode::Summary,
        _ => ScoreObjectiveMode::Disabled,
    };
    let profile = result
        .field("score_profile_requested")
        .and_then(ScoreProfileSelection::parse)
        .unwrap_or_default();
    let spin_profile = result
        .field("score_spin_profile_requested")
        .or_else(|| result.field("spin_profile_requested"))
        .and_then(SpinProfileSelection::parse)
        .unwrap_or_default();
    ScoreObjectivePolicy::new(mode)
        .with_profile(profile)
        .with_spin_profile(spin_profile)
        .with_initial_b2b(
            result
                .field("score_initial_b2b")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
        )
}

pub(crate) fn score_profile_for_policy(
    policy: ScoreObjectivePolicy,
) -> clearra_scoring::profile::ScoreProfile {
    let spin_profile = score_spin_profile_id(policy.spin_profile());
    match policy.profile() {
        ScoreProfileSelection::Guideline => guideline_pc_score_with_spin_profile(spin_profile),
        ScoreProfileSelection::JstrisUltra => jstris_ultra_pc_score_with_spin_profile(spin_profile),
        ScoreProfileSelection::Tetrio => tetrio_pc_score_with_spin_profile(spin_profile),
    }
}

fn score_spin_profile_id(selection: SpinProfileSelection) -> SpinProfileId {
    match selection {
        SpinProfileSelection::TSpins => SpinProfileId::TSpins,
        SpinProfileSelection::TSpinsPlus => SpinProfileId::TSpinsPlus,
        SpinProfileSelection::AllSpin => SpinProfileId::AllSpin,
        SpinProfileSelection::AllSpinPlus => SpinProfileId::AllSpinPlus,
        SpinProfileSelection::AllMini => SpinProfileId::AllMini,
        SpinProfileSelection::AllMiniPlus => SpinProfileId::AllMiniPlus,
    }
}

fn candidate_execution_aggregates(
    result: &CoreExecutionResult,
) -> Vec<CandidateExecutionAggregate> {
    let mut by_candidate = BTreeMap::<u64, Vec<CandidateExecution>>::new();
    for execution in result.postprocess_executions() {
        by_candidate
            .entry(execution.candidate_id())
            .or_default()
            .push(CandidateExecution::new(
                execution.pattern_id(),
                execution.trace_identity(),
                execution.replay_trace().clone(),
            ));
    }
    by_candidate
        .into_iter()
        .map(|(candidate_id, mut executions)| {
            executions.sort_by(|left, right| {
                (left.pattern_id(), left.trace_identity())
                    .cmp(&(right.pattern_id(), right.trace_identity()))
            });
            CandidateExecutionAggregate::new(candidate_id, executions)
        })
        .collect()
}

fn unique_field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    let mut values = fields
        .iter()
        .filter_map(|(field_key, value)| (field_key == key).then_some(value.as_str()));
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
    use clearra_objectives::policy::score_objective_policy::ScoreObjectivePolicy;
    use clearra_postprocess::{ScoreCell, ScoreMatrix};

    use super::{
        checked_pattern_winner_count, score_profile_for_policy, solution_average_score_reports,
        try_materialize_pattern_winners,
    };

    #[test]
    fn pattern_winner_family_preserves_every_score_tie_without_consulting_attack() {
        let identities = vec![
            StandardBoard64TilingIdentity::from_placements(0, std::iter::empty()).unwrap(),
            StandardBoard64TilingIdentity::from_placements(1, std::iter::empty()).unwrap(),
            StandardBoard64TilingIdentity::from_placements(2, std::iter::empty()).unwrap(),
        ];
        let profile = score_profile_for_policy(ScoreObjectivePolicy::default());
        let accuracy = profile.accuracy_level().as_str();
        let matrix = ScoreMatrix::from_materialized_cells(
            vec![
                ScoreCell::new(1, 0, "z-low-attack", 100, 0, accuracy),
                ScoreCell::new(1, 0, "a-high-attack", 100, 99, accuracy),
                ScoreCell::new(2, 0, "candidate-two", 100, 1, accuracy),
                ScoreCell::new(3, 0, "lower-score", 99, 500, accuracy),
                ScoreCell::new(2, 1, "second-pattern", 50, 7, accuracy),
            ],
            &profile,
            2,
            true,
        );

        assert_eq!(checked_pattern_winner_count(&matrix).unwrap(), 3);
        let winners = try_materialize_pattern_winners(&matrix, &identities).unwrap();
        assert_eq!(
            winners
                .iter()
                .map(|winner| (
                    winner.pattern_id(),
                    winner.candidate_id(),
                    winner.score(),
                    winner.informational_attack(),
                ))
                .collect::<Vec<_>>(),
            vec![(0, 1, 100, 99), (0, 2, 100, 1), (1, 2, 50, 7)]
        );
        assert!(winners.iter().all(|winner| {
            winner.informational_attack_basis() == "canonical-equal-score-trace"
        }));
    }

    #[test]
    fn averages_each_solution_over_the_whole_weighted_pattern_universe() {
        let identities = vec![
            StandardBoard64TilingIdentity::from_placements(0, std::iter::empty()).unwrap(),
            StandardBoard64TilingIdentity::from_placements(1, std::iter::empty()).unwrap(),
        ];
        let profile = score_profile_for_policy(ScoreObjectivePolicy::default());
        let accuracy = profile.accuracy_level().as_str();
        let matrix = ScoreMatrix::from_materialized_cells(
            vec![
                ScoreCell::new(1, 0, "lower", 100, 0, accuracy),
                ScoreCell::new(1, 0, "best", 150, 0, accuracy),
                ScoreCell::new(1, 1, "only", 50, 0, accuracy),
                ScoreCell::new(2, 0, "only", 40, 0, accuracy),
            ],
            &profile,
            2,
            true,
        );

        let reports = solution_average_score_reports(&identities, &matrix, &[0.25, 0.75], 2);

        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].average_score(), "75");
        assert_eq!(reports[0].covered_pattern_count(), 2);
        assert!(reports[0].score_complete());
        assert_eq!(reports[1].average_score(), "10");
        assert_eq!(reports[1].covered_pattern_count(), 1);
        assert!(reports[1].score_complete());
    }
}
