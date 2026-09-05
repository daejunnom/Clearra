//! SRP rationale: this module has one behavior-level change reason: validating and materializing requested execution constraints, including fail-closed evidence proofs.

use clearra_core_domain::{
    probability::probability_value::ProbabilityValue,
    solution::normalized_tiling_solution::{
        normalized_tiling_solution_key_set_hash_from_sorted_strings,
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
    },
};
use clearra_core_executor::{
    normalized_solution_probability_reports, solution_probability_pattern_weights,
    CoreExecutionError, CoreExecutionResult, NormalizedSolutionCoverage, SolutionAuditCheckpoint,
    SolutionCoverage, SolutionProbabilityReport,
};
use clearra_coverage::cover::{
    exact_minimum_cover::ExactMinimumCoverError,
    exact_minimum_cover_portfolios::{
        ExactMinimumCoverPortfolioEnumerator, ExactMinimumCoverPortfolioError,
    },
};
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, pattern_id::PatternId, weighted_pattern_set::WeightedPatternSet,
};
use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;
use clearra_postprocess::{
    BackToBackExecutionFilter, BackToBackFilterError, CandidatePatternCoverage,
    TSpinCoverageMaterializationError, TSpinCoverageOnlyMaterializer,
};
use clearra_problem::BuildSolutionProbabilityPolicy;
use clearra_replay::{
    ExactScoringExecutionBatch, ExactScoringExecutionGraph, ScoringExecutionEdge,
    ScoringExecutionNode, SpinCoverageExecutionBatch, SpinCoverageExecutionGraph,
};
use clearra_scoring::profile::SpinProfileId;

use clearra_core_domain::execution_cancellation::ExecutionControl;

use crate::build_solution_probability_result::{
    build_solution_probability_incomplete_reason, declared_build_solution_probability_policy,
    validate_build_solution_probability_reducer_input, validate_build_solution_probability_result,
    validate_build_solution_probability_worker_partial,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildExecutionConstraintInputAuthority {
    Final(BuildSolutionProbabilityPolicy),
    WorkerPartial(BuildSolutionProbabilityPolicy),
}

type CoverageTable = Vec<(String, PatternBitSet)>;

pub(crate) fn apply_execution_constraints_with_memory_guard(
    result: CoreExecutionResult,
    control: &ExecutionControl,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    let authority = if result.field("execution_constraint_preserve_b2b") == Some("true")
        && result.field("search_kind") == Some("build-probability")
    {
        Some(BuildExecutionConstraintInputAuthority::Final(
            declared_build_solution_probability_policy(&result).map_err(|error| {
                CoreExecutionError::RuntimeUnavailable {
                    component: error.input_component(),
                }
            })?,
        ))
    } else {
        None
    };
    apply_execution_constraints_inner(result, authority, control, memory_guard)
}

pub(crate) fn apply_build_execution_constraints_with_memory_guard(
    result: CoreExecutionResult,
    expected_policy: BuildSolutionProbabilityPolicy,
    control: &ExecutionControl,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    apply_execution_constraints_inner(
        result,
        Some(BuildExecutionConstraintInputAuthority::Final(
            expected_policy,
        )),
        control,
        memory_guard,
    )
}

pub(crate) fn apply_build_worker_execution_constraints_with_memory_guard(
    result: CoreExecutionResult,
    expected_policy: BuildSolutionProbabilityPolicy,
    control: &ExecutionControl,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    apply_execution_constraints_inner(
        result,
        Some(BuildExecutionConstraintInputAuthority::WorkerPartial(
            expected_policy,
        )),
        control,
        memory_guard,
    )
}

#[cfg(test)]
fn apply_execution_constraints(
    result: CoreExecutionResult,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    apply_execution_constraints_with_memory_guard(result, control, &mut |_, _| Ok(()))
}

#[cfg(test)]
fn apply_build_execution_constraints(
    result: CoreExecutionResult,
    expected_policy: BuildSolutionProbabilityPolicy,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    apply_build_execution_constraints_with_memory_guard(
        result,
        expected_policy,
        control,
        &mut |_, _| Ok(()),
    )
}

#[cfg(test)]
fn apply_build_worker_execution_constraints(
    result: CoreExecutionResult,
    expected_policy: BuildSolutionProbabilityPolicy,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    apply_build_worker_execution_constraints_with_memory_guard(
        result,
        expected_policy,
        control,
        &mut |_, _| Ok(()),
    )
}

fn apply_execution_constraints_inner(
    result: CoreExecutionResult,
    build_authority: Option<BuildExecutionConstraintInputAuthority>,
    control: &ExecutionControl,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    memory_guard(&result, 0)?;
    if result.field("execution_constraint_preserve_b2b") != Some("true") {
        return Ok(result);
    }
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    let worker_partial_authority = matches!(
        build_authority,
        Some(BuildExecutionConstraintInputAuthority::WorkerPartial(_))
    );
    let build_solution_probability_input = match build_authority {
        Some(BuildExecutionConstraintInputAuthority::Final(expected_policy)) => {
            validate_build_solution_probability_result(expected_policy, &result).map_err(
                |error| CoreExecutionError::RuntimeUnavailable {
                    component: error.input_component(),
                },
            )?;
            Some(
                validate_build_solution_probability_reducer_input(Some(expected_policy), &result)
                    .map_err(|error| CoreExecutionError::RuntimeUnavailable {
                    component: error.input_component(),
                })?,
            )
        }
        Some(BuildExecutionConstraintInputAuthority::WorkerPartial(expected_policy)) => Some(
            validate_build_solution_probability_worker_partial(expected_policy, &result).map_err(
                |error| CoreExecutionError::RuntimeUnavailable {
                    component: error.input_component(),
                },
            )?,
        ),
        None => None,
    };
    let (minimum_cover_requested, minimum_cover_blocking_reason) =
        minimum_cover_input_status(&result);
    let minimum_cover_deferred_to_named_product = minimum_cover_requested
        && minimum_cover_blocking_reason == Some("deferred-to-coordinator")
        && result.bool_field("minimum_cover_complete") == Some(false)
        && result.bool_field("minimum_cover_proven_minimum") == Some(false);
    // Distributed finalizers initialize this marker from the requested constraint and AND it
    // with every absorbed worker result. Once true, every partition is already materialized;
    // a coordinator-generated empty evidence wrapper must not trigger a second filtering pass.
    if result.field("execution_constraint_materialized") == Some("true") {
        return Ok(result);
    }
    if result.usize_field("target_piece_count") == Some(0) && execution_graphs_are_empty(&result) {
        if !vacuous_b2b_constraint_is_proven(&result) {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_vacuous_evidence_incomplete",
            });
        }
        let solution_count = result
            .usize_field("unique_solution_count")
            .expect("validated vacuous B2B result has a solution count");
        let score_requested = result
            .bool_field("postprocess_scoring_requested")
            .unwrap_or(false);
        let objective_complete = result
            .bool_field("objective_search_complete")
            .unwrap_or(false)
            && result.bool_field("count_complete").unwrap_or(false)
            && result.bool_field("probability_complete").unwrap_or(false)
            && !score_requested
            && (!minimum_cover_requested || minimum_cover_blocking_reason.is_none());
        let objective_incomplete_reason = if objective_complete {
            "none"
        } else if score_requested {
            "score_matrix_not_materialized"
        } else if !result.bool_field("probability_complete").unwrap_or(false) {
            "pattern_universe_incomplete"
        } else if minimum_cover_requested {
            minimum_cover_blocking_reason.unwrap_or("search_incomplete")
        } else {
            "search_incomplete"
        };
        let replacement_field_count = 19_usize
            .checked_add(usize::from(minimum_cover_requested).checked_mul(3).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        let replacement_projection = checked_replacement_field_construction_upper_bound(
            replacement_field_count,
            objective_incomplete_reason
                .len()
                .max(minimum_cover_blocking_reason.map_or(0, str::len)),
        )
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
        memory_guard(&result, replacement_projection)?;
        let mut replacements = vec![
            field("execution_constraint_materialized", true),
            field("b2b_preserving_solution_count", solution_count),
            field("b2b_preserving_candidate_pattern_count", 0),
            field("b2b_preservation_selection", "existential"),
            field(
                "b2b_preservation_denominator_semantics",
                "original-materialized-queue",
            ),
            field(
                "b2b_preservation_pattern_universe_count",
                result.usize_field("coverage_pattern_count").unwrap_or(0),
            ),
            field(
                "b2b_preserving_pattern_count",
                result.usize_field("covered_pattern_count").unwrap_or(0),
            ),
            field("b2b_preservation_probability", "not-calculated"),
            field("b2b_preservation_count_complete", true),
            field("b2b_preservation_probability_complete", false),
            field("b2b_preservation_witness_available", false),
            field("b2b_preservation_witness_kind", "none"),
            field(
                "b2b_preservation_witness_pattern_semantics",
                "original-queue-index",
            ),
            field("b2b_preservation_witness_candidate_key", ""),
            field("b2b_preservation_witness_pattern_index", ""),
            field(
                "b2b_preservation_evaluation_basis",
                "candidate-pattern-existence",
            ),
            field("b2b_preservation_path_multiplicity_counted", false),
            field("objective_complete", objective_complete),
            field("objective_incomplete_reason", objective_incomplete_reason),
        ];
        if minimum_cover_requested {
            let minimum_cover_complete = minimum_cover_blocking_reason.is_none();
            replacements.extend([
                field("minimum_cover_complete", minimum_cover_complete),
                field("minimum_cover_proven_minimum", minimum_cover_complete),
                field(
                    "minimum_cover_incomplete_reason",
                    minimum_cover_blocking_reason.unwrap_or("none"),
                ),
            ]);
        }
        let replacement_bytes = checked_owned_field_storage_bytes(&replacements).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?;
        memory_guard(&result, replacement_bytes)?;
        return result
            .try_with_replaced_fields_with_memory_guard(replacements, |live, future| {
                memory_guard(live, future)
            })
            .map_err(core_error_from_field_replacement);
    }
    if result.exact_scoring_execution_batches().is_empty()
        && result.spin_coverage_execution_batches().is_empty()
    {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_execution_evidence_missing",
        });
    }

    let profile = result
        .field("execution_constraint_spin_profile")
        .and_then(SpinProfileSelection::parse)
        .unwrap_or(SpinProfileSelection::TSpins);
    let profile_id = spin_profile_id(profile);
    let pattern_count = result.usize_field("coverage_pattern_count").unwrap_or(0);
    if pattern_count == 0 {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_pattern_universe_missing",
        });
    }
    let authoritative_coverage =
        authoritative_solution_coverages(&result, pattern_count, memory_guard)?;
    let authoritative_bytes = checked_coverage_table_retained_bytes(&authoritative_coverage)
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    memory_guard(&result, authoritative_bytes)?;
    validate_execution_graph_authority(&result, &authoritative_coverage)?;
    let checkpoint_future = authoritative_bytes
        .checked_add(
            checked_checkpoint_construction_projection(&result, &authoritative_coverage).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?,
        )
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    memory_guard(&result, checkpoint_future)?;
    let pre_b2b_produced_solution_audit_checkpoint =
        pre_b2b_produced_solution_audit_checkpoint(&result);
    let pre_b2b_solution_audit_checkpoint =
        pre_b2b_solution_audit_checkpoint(&result, &authoritative_coverage);

    let pass_count = result
        .exact_scoring_execution_batches()
        .len()
        .checked_add(result.spin_coverage_execution_batches().len())
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    let checkpoint_bytes = pre_b2b_produced_solution_audit_checkpoint
        .checked_nested_retained_bytes()
        .and_then(|bytes| {
            bytes.checked_add(pre_b2b_solution_audit_checkpoint.checked_nested_retained_bytes()?)
        })
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    let initial_collection_future = authoritative_bytes
        .checked_add(checkpoint_bytes)
        .and_then(|bytes| {
            bytes.checked_add(
                (authoritative_coverage.len() as u128)
                    .checked_mul(core::mem::size_of::<(String, PatternBitSet)>() as u128)?,
            )
        })
        .and_then(|bytes| {
            bytes.checked_add(
                (pass_count as u128)
                    .checked_mul(core::mem::size_of::<PassConstraintResult>() as u128)?,
            )
        })
        .and_then(|bytes| {
            bytes.checked_add(
                (result.exact_scoring_execution_batches().len() as u128)
                    .checked_mul(core::mem::size_of::<ExactScoringExecutionBatch>() as u128)?,
            )
        })
        .and_then(|bytes| {
            bytes.checked_add(
                (result.spin_coverage_execution_batches().len() as u128)
                    .checked_mul(core::mem::size_of::<SpinCoverageExecutionBatch>() as u128)?,
            )
        })
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    memory_guard(&result, initial_collection_future)?;
    let mut accepted = Vec::<(String, PatternBitSet)>::new();
    accepted
        .try_reserve_exact(authoritative_coverage.len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_coverage_allocation_failed",
        })?;
    let mut pass_results = Vec::<PassConstraintResult>::new();
    pass_results.try_reserve_exact(pass_count).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_coverage_allocation_failed",
        }
    })?;
    let mut witnessed_pattern_count = 0_u128;
    let mut complete = true;
    let mut filtered_scoring_batches = Vec::new();
    filtered_scoring_batches
        .try_reserve_exact(result.exact_scoring_execution_batches().len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_batch_allocation_failed",
        })?;
    let mut filtered_spin_batches = Vec::new();
    filtered_spin_batches
        .try_reserve_exact(result.spin_coverage_execution_batches().len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_batch_allocation_failed",
        })?;

    for batch in result.exact_scoring_execution_batches() {
        let scratch = checked_constraint_scratch_bytes(
            checkpoint_bytes,
            &authoritative_coverage,
            &accepted,
            &pass_results,
            &filtered_scoring_batches,
            &filtered_spin_batches,
        )?;
        let filter_projection =
            BackToBackExecutionFilter::checked_scoring_batch_memory_projection(batch, profile_id)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        let filter_cap = scratch
            .checked_add(filter_projection.output_retained_bytes)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(&result, filter_cap)?;
        let (filtered, filter_report) = BackToBackExecutionFilter::scoring_batch_with_memory_limit(
            batch, profile_id, scratch, filter_cap,
        )
        .map_err(core_error_from_b2b_filter)?;
        let filtered_live = scratch
            .checked_add(filter_report.output_retained_bytes)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(&result, filtered_live)?;
        let materialization_projection =
            TSpinCoverageOnlyMaterializer::checked_target_memory_projection(&filtered).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?;
        let materialization_cap = filtered_live
            .checked_add(materialization_projection.required_peak_bytes)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(&result, materialization_cap)?;
        let (materialized, materialization_report) =
            TSpinCoverageOnlyMaterializer::materialize_all_paths_with_memory_limit(
                &filtered,
                0..filtered.patterns().len(),
                control,
                filtered_live,
                materialization_cap,
            )
            .map_err(core_error_from_b2b_materialization)?;
        memory_guard(
            &result,
            filtered_live
                .checked_add(materialization_report.retained_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?,
        )?;
        complete &= materialized.complete();
        let pass = merge_candidate_coverages_with_memory_guard(
            &result,
            &mut accepted,
            materialized.candidate_coverages(),
            &authoritative_coverage,
            pattern_count,
            filtered_live,
            materialization_report.retained_bytes,
            memory_guard,
        )?;
        witnessed_pattern_count =
            witnessed_pattern_count.saturating_add(pass.witnessed_pattern_count);
        drop(materialized);
        pass_results.push(pass);
        filtered_scoring_batches.push(filtered);
        memory_guard(
            &result,
            checked_constraint_scratch_bytes(
                checkpoint_bytes,
                &authoritative_coverage,
                &accepted,
                &pass_results,
                &filtered_scoring_batches,
                &filtered_spin_batches,
            )?,
        )?;
    }
    for batch in result.spin_coverage_execution_batches() {
        let scratch = checked_constraint_scratch_bytes(
            checkpoint_bytes,
            &authoritative_coverage,
            &accepted,
            &pass_results,
            &filtered_scoring_batches,
            &filtered_spin_batches,
        )?;
        let filter_projection =
            BackToBackExecutionFilter::checked_spin_batch_memory_projection(batch, profile_id)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?;
        let filter_cap = scratch
            .checked_add(filter_projection.output_retained_bytes)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(&result, filter_cap)?;
        let (filtered, filter_report) = BackToBackExecutionFilter::spin_batch_with_memory_limit(
            batch, profile_id, scratch, filter_cap,
        )
        .map_err(core_error_from_b2b_filter)?;
        let filtered_live = scratch
            .checked_add(filter_report.output_retained_bytes)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(&result, filtered_live)?;
        let materialization_projection =
            TSpinCoverageOnlyMaterializer::checked_spin_batch_memory_projection(&filtered).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?;
        let materialization_cap = filtered_live
            .checked_add(materialization_projection.required_peak_bytes)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(&result, materialization_cap)?;
        let (materialized, materialization_report) =
            TSpinCoverageOnlyMaterializer::materialize_all_spin_paths_with_memory_limit(
                &filtered,
                0..filtered.patterns().len(),
                control,
                filtered_live,
                materialization_cap,
            )
            .map_err(core_error_from_b2b_materialization)?;
        memory_guard(
            &result,
            filtered_live
                .checked_add(materialization_report.retained_bytes)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?,
        )?;
        complete &= materialized.complete();
        let pass = merge_candidate_coverages_with_memory_guard(
            &result,
            &mut accepted,
            materialized.candidate_coverages(),
            &authoritative_coverage,
            pattern_count,
            filtered_live,
            materialization_report.retained_bytes,
            memory_guard,
        )?;
        witnessed_pattern_count =
            witnessed_pattern_count.saturating_add(pass.witnessed_pattern_count);
        drop(materialized);
        pass_results.push(pass);
        filtered_spin_batches.push(filtered);
        memory_guard(
            &result,
            checked_constraint_scratch_bytes(
                checkpoint_bytes,
                &authoritative_coverage,
                &accepted,
                &pass_results,
                &filtered_scoring_batches,
                &filtered_spin_batches,
            )?,
        )?;
    }
    if !complete {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_execution_evidence_incomplete",
        });
    }
    let filtered_scratch = checked_constraint_scratch_bytes(
        checkpoint_bytes,
        &authoritative_coverage,
        &accepted,
        &pass_results,
        &filtered_scoring_batches,
        &filtered_spin_batches,
    )?;
    let weight_projection = (pattern_count as u128)
        .checked_mul(core::mem::size_of::<ProbabilityValue>() as u128)
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    memory_guard(
        &result,
        filtered_scratch.checked_add(weight_projection).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?,
    )?;
    let weights = if build_solution_probability_input.is_some_and(|input| input.requested) {
        solution_probability_pattern_weights(&result).map_err(|error| {
            CoreExecutionError::RuntimeUnavailable {
                component: error.reason(),
            }
        })?
    } else {
        materialized_weights(&result, pattern_count)?
    };
    let weights_bytes =
        weights
            .checked_storage_retained_bytes()
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
    memory_guard(
        &result,
        filtered_scratch.checked_add(weights_bytes).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?,
    )?;
    let probability_complete = build_solution_probability_input.map_or_else(
        || result.bool_field("probability_complete").unwrap_or(false),
        |input| input.probability_complete,
    );
    let count_complete = build_solution_probability_input.map_or_else(
        || result.bool_field("count_complete").unwrap_or(false),
        |input| input.count_complete,
    );
    let minimum_cover_source_solution_count = accepted.len();
    let mut minimum_cover_complete = false;
    let mut minimum_cover_proven = false;
    let minimum_cover_reason_source = if let Some(reason) = minimum_cover_blocking_reason {
        reason
    } else if minimum_cover_requested && !probability_complete {
        "pattern_universe_incomplete"
    } else if minimum_cover_requested {
        "search_incomplete"
    } else {
        "not_requested"
    };
    let minimum_base_without_reason = filtered_scratch.checked_add(weights_bytes).ok_or(
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        },
    )?;
    let mut minimum_cover_reason = try_owned_string_with_memory_guard(
        &result,
        minimum_base_without_reason,
        minimum_cover_reason_source,
        memory_guard,
    )?;
    let minimum_base_live = minimum_base_without_reason
        .checked_add(minimum_cover_reason.capacity() as u128)
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    if minimum_cover_requested
        && !minimum_cover_deferred_to_named_product
        && minimum_cover_blocking_reason.is_none()
        && count_complete
        && probability_complete
    {
        let required_projection = PatternBitSet::checked_shared_construction_upper_bound(
            pattern_count,
            1,
            pattern_count as u128,
        )
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
        memory_guard(
            &result,
            minimum_base_live.checked_add(required_projection).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?,
        )?;
        let required =
            coverage_union(accepted.iter().map(|(_, coverage)| coverage), pattern_count)?;
        let required_bytes = required.checked_storage_retained_bytes().ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?;
        memory_guard(
            &result,
            minimum_base_live.checked_add(required_bytes).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?,
        )?;
        let row_projection = (accepted.len() as u128)
            .checked_mul(core::mem::size_of::<PatternBitSet>() as u128)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(
            &result,
            minimum_base_live
                .checked_add(required_bytes)
                .and_then(|bytes| bytes.checked_add(row_projection))
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?,
        )?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(accepted.len()).map_err(|_| {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_coverage_allocation_failed",
            }
        })?;
        rows.extend(accepted.iter().map(|(_, coverage)| coverage.clone()));
        let rows_bytes = (rows.capacity() as u128)
            .checked_mul(core::mem::size_of::<PatternBitSet>() as u128)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        let minimum_solver_external = minimum_base_live
            .checked_add(required_bytes)
            .and_then(|bytes| bytes.checked_add(rows_bytes))
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(&result, minimum_solver_external)?;
        let mut guard_error = None;
        let enumerator = ExactMinimumCoverPortfolioEnumerator::new_with_memory_guard(
            &required,
            &rows,
            &mut |solver_owned_bytes| {
                let future_bytes = minimum_solver_external
                    .checked_add(solver_owned_bytes)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                match memory_guard(&result, future_bytes) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        guard_error = Some(error);
                        Err(ExactMinimumCoverError::MemoryGuardRejected)
                    }
                }
            },
        );
        if let Some(error) = guard_error {
            return Err(error);
        }
        let canonical = enumerator
            .map_err(core_error_from_exact_minimum_cover_portfolio)?
            .into_canonical_portfolio()
            .map_err(core_error_from_exact_minimum_cover_portfolio)?
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_minimum_cover_incomplete",
            })?;
        let canonical_bytes = canonical.checked_retained_capacity_bytes().ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?;
        let selected_projection = (accepted.len() as u128)
            .checked_mul(core::mem::size_of::<bool>() as u128)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(
            &result,
            minimum_solver_external
                .checked_add(canonical_bytes)
                .and_then(|bytes| bytes.checked_add(selected_projection))
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?,
        )?;
        let mut selected = Vec::<bool>::new();
        selected.try_reserve_exact(accepted.len()).map_err(|_| {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_coverage_allocation_failed",
            }
        })?;
        selected.resize(accepted.len(), false);
        for row_index in canonical.row_indices() {
            selected[*row_index] = true;
        }
        let selected_bytes = (selected.capacity() as u128)
            .checked_mul(core::mem::size_of::<bool>() as u128)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        memory_guard(
            &result,
            minimum_solver_external
                .checked_add(canonical_bytes)
                .and_then(|bytes| bytes.checked_add(selected_bytes))
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?,
        )?;
        let mut accepted_index = 0_usize;
        accepted.retain(|_| {
            let keep = selected[accepted_index];
            accepted_index += 1;
            keep
        });
        drop(selected);
        drop(canonical);
        drop(rows);
        drop(required);
        minimum_cover_complete = true;
        minimum_cover_proven = true;
        minimum_cover_reason.clear();
        minimum_cover_reason.push_str("none");
    }
    let pre_final_scratch = checked_constraint_scratch_bytes(
        checkpoint_bytes,
        &authoritative_coverage,
        &accepted,
        &pass_results,
        &filtered_scoring_batches,
        &filtered_spin_batches,
    )?
    .checked_add(weights_bytes)
    .and_then(|bytes| bytes.checked_add(minimum_cover_reason.capacity() as u128))
    .ok_or(CoreExecutionError::RuntimeUnavailable {
        component: "b2b_preservation_memory_projection_overflow",
    })?;
    let final_projection = checked_final_constraint_output_projection(
        &accepted,
        &filtered_scoring_batches,
        &filtered_spin_batches,
        pattern_count,
        build_solution_probability_input.map_or_else(
            || {
                result
                    .bool_field("solution_probabilities_requested")
                    .unwrap_or(false)
            },
            |input| input.requested,
        ),
        minimum_cover_reason
            .len()
            .max(
                result
                    .field("resource_truncation_reason")
                    .map_or(0, str::len),
            )
            .max(accepted.iter().map(|(key, _)| key.len()).max().unwrap_or(0)),
    )
    .ok_or(CoreExecutionError::RuntimeUnavailable {
        component: "b2b_preservation_memory_projection_overflow",
    })?;
    memory_guard(
        &result,
        pre_final_scratch.checked_add(final_projection).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?,
    )?;
    filtered_scoring_batches =
        retain_accepted_scoring_batches(filtered_scoring_batches, &accepted)?;
    filtered_spin_batches = retain_accepted_spin_batches(filtered_spin_batches, &accepted)?;
    let union = coverage_union(accepted.iter().map(|(_, coverage)| coverage), pattern_count)?;

    let mut identity_by_key = Vec::<(String, StandardBoard64TilingIdentity)>::new();
    let identity_capacity = filtered_scoring_batches
        .iter()
        .map(|batch| batch.graphs().len())
        .try_fold(0_usize, usize::checked_add)
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    identity_by_key
        .try_reserve_exact(identity_capacity)
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_identity_allocation_failed",
        })?;
    for batch in &filtered_scoring_batches {
        for graph in batch.graphs() {
            let key = NormalizedTilingSolutionKey::from_standard_board64_identity(graph.identity());
            match identity_by_key
                .binary_search_by(|(candidate, _)| candidate.as_str().cmp(key.as_str()))
            {
                Ok(index) => identity_by_key[index].1 = graph.identity(),
                Err(index) => identity_by_key
                    .insert(index, (try_owned_string(key.as_str())?, graph.identity())),
            }
        }
    }
    let had_board64_identities = !filtered_scoring_batches.is_empty();
    let mut normalized_keys = Vec::new();
    normalized_keys
        .try_reserve_exact(accepted.len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_string_allocation_failed",
        })?;
    for (key, _) in &accepted {
        normalized_keys.push(try_owned_string(key)?);
    }
    let mut identities = Vec::new();
    identities
        .try_reserve_exact(normalized_keys.len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_identity_allocation_failed",
        })?;
    identities.extend(normalized_keys.iter().filter_map(|key| {
        identity_by_key
            .binary_search_by(|(candidate, _)| candidate.cmp(key))
            .ok()
            .map(|index| identity_by_key[index].1)
    }));
    identities.sort_unstable();
    identities.dedup();
    if had_board64_identities && identities.len() != normalized_keys.len() {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_candidate_identity_mismatch",
        });
    }

    let solution_coverages = board64_solution_coverages(&identities, &accepted)?;
    let mut normalized_solution_coverages = Vec::new();
    normalized_solution_coverages
        .try_reserve_exact(accepted.len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_coverage_allocation_failed",
        })?;
    for (key, coverage) in &accepted {
        normalized_solution_coverages.push(NormalizedSolutionCoverage::new(
            try_owned_string(key)?,
            coverage.clone(),
        ));
    }
    let solution_probabilities_requested = build_solution_probability_input.map_or_else(
        || {
            result
                .bool_field("solution_probabilities_requested")
                .unwrap_or(false)
        },
        |input| input.requested,
    );
    let source_solution_keys_complete = build_solution_probability_input.map_or_else(
        || result.bool_field("solution_keys_complete") == Some(true),
        |input| input.solution_keys_complete,
    );
    let filtered_solution_keys_complete = count_complete && source_solution_keys_complete;
    let resource_truncated = build_solution_probability_input.map_or_else(
        || result.bool_field("resource_truncated") != Some(false),
        |input| input.resource_truncated,
    );
    let solution_probability_complete = !solution_probabilities_requested
        || (probability_complete
            && count_complete
            && filtered_solution_keys_complete
            && !resource_truncated);
    // Worker partitions may use probability authority while filtering, but the
    // merger is the sole owner of the final per-solution probability surface.
    let solution_probabilities = if solution_probabilities_requested && !worker_partial_authority {
        normalized_solution_probability_reports(
            &normalized_keys,
            &normalized_solution_coverages,
            &weights,
            solution_probability_complete,
        )
        .map_err(|error| CoreExecutionError::RuntimeUnavailable {
            component: error.reason(),
        })?
    } else {
        Vec::new()
    };
    let probability = weights
        .covered_weight(&union)
        .expect("materialized weights and filtered coverage share one universe")
        .get();
    let solution_count = normalized_keys.len();
    let witness = accepted.iter().find_map(|(candidate_key, coverage)| {
        coverage
            .first_pattern()
            .map(|pattern| (candidate_key.as_str(), pattern.index()))
    });
    let solution_hash = if had_board64_identities {
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(&identities)
    } else {
        normalized_tiling_solution_key_set_hash_from_sorted_strings(&normalized_keys)
    };
    let coverage_summary = result.field("search_output_policy") == Some("coverage-summary");
    let search_output_policy = result
        .field("search_output_policy")
        .filter(|policy| matches!(*policy, "summary" | "trace" | "coverage-rows"))
        .unwrap_or("summary")
        .to_owned();
    let search_complete = result
        .bool_field("objective_search_complete")
        .unwrap_or(false);
    let score_requested = result
        .bool_field("postprocess_scoring_requested")
        .unwrap_or(false);
    let objective_complete = search_complete
        && count_complete
        && probability_complete
        && !score_requested
        && (!minimum_cover_requested || minimum_cover_complete);
    let objective_incomplete_reason = if score_requested {
        "score_matrix_not_materialized".to_owned()
    } else if !count_complete {
        result
            .field("resource_truncation_reason")
            .filter(|reason| *reason != "none")
            .unwrap_or("search_incomplete")
            .to_owned()
    } else if !probability_complete {
        "pattern_universe_incomplete".to_owned()
    } else if minimum_cover_requested && !minimum_cover_complete {
        minimum_cover_reason.clone()
    } else {
        "none".to_owned()
    };
    let mut replacements = vec![
        field("solution_found", solution_count != 0),
        field("coverage_row_count", solution_count),
        field("covered_pattern_count", union.count_ones()),
        field("coverage_probability", canonical_probability(probability)),
        field("count_complete", count_complete),
        field("probability_complete", probability_complete),
        field(
            "solution_probabilities_requested",
            solution_probabilities_requested,
        ),
    ];
    if !worker_partial_authority {
        replacements.extend([
            field("solution_probability_count", solution_probabilities.len()),
            field(
                "solution_probability_complete",
                solution_probability_complete,
            ),
            field(
                "solution_probability_basis",
                if solution_probabilities_requested {
                    "normalized-solution-pattern-bitset-or-union"
                } else {
                    "not-requested"
                },
            ),
            field(
                "solution_probability_incomplete_reason",
                build_solution_probability_incomplete_reason(
                    solution_probabilities_requested,
                    solution_probability_complete,
                    resource_truncated,
                    count_complete,
                    filtered_solution_keys_complete,
                ),
            ),
        ]);
    }
    replacements.extend([
        field("build_variant_count", 0),
        field("build_variant_count_exact", false),
        field("pattern_verified_execution_count", witnessed_pattern_count),
        field("execution_constraint_materialized", true),
        field("b2b_preserving_solution_count", solution_count),
        field("b2b_preservation_selection", "existential"),
        field(
            "b2b_preservation_denominator_semantics",
            "original-materialized-queue",
        ),
        field("b2b_preservation_pattern_universe_count", pattern_count),
        field("b2b_preserving_pattern_count", union.count_ones()),
        field(
            "b2b_preservation_probability",
            canonical_probability(probability),
        ),
        field("b2b_preservation_count_complete", count_complete),
        field(
            "b2b_preservation_probability_complete",
            probability_complete,
        ),
        field("b2b_preservation_witness_available", witness.is_some()),
        field(
            "b2b_preservation_witness_kind",
            if witness.is_some() {
                "candidate-pattern"
            } else {
                "none"
            },
        ),
        field(
            "b2b_preservation_witness_pattern_semantics",
            "original-queue-index",
        ),
        field(
            "b2b_preservation_witness_candidate_key",
            witness.map_or("", |(candidate_key, _)| candidate_key),
        ),
        field(
            "b2b_preservation_witness_pattern_index",
            witness.map_or_else(String::new, |(_, pattern_index)| pattern_index.to_string()),
        ),
        field(
            "b2b_preserving_candidate_pattern_count",
            witnessed_pattern_count,
        ),
        field(
            "b2b_preservation_evaluation_basis",
            "candidate-pattern-existence",
        ),
        field("b2b_preservation_path_multiplicity_counted", false),
        field("objective_complete", objective_complete),
        field("objective_incomplete_reason", objective_incomplete_reason),
        field("sample_trace_available", false),
        field("retained_trace_count", 0),
        field("trace_steps", 0),
        field("representative_candidate_id", ""),
        field("representative_candidate_ordinal", ""),
        field("representative_pattern_id", ""),
    ]);
    if coverage_summary {
        replacements.extend([
            field("unique_solution_count", "not-calculated"),
            field("normalized_unique_solution_count", "not-calculated"),
            field("actual_normalized_unique_solution_count", "not-calculated"),
            field("solution_count_calculated", false),
            field("solution_set_materialized", false),
            field("solution_keys_materialized_count", 0),
            field("solution_keys_complete", false),
            field("solution_page_available", false),
            field("normalized_solution_set_hash", "not-calculated"),
            field("actual_normalized_solution_set_hash", "not-calculated"),
        ]);
        if result.field("total_solution_count").is_some() {
            replacements.push(field("total_solution_count", "not-calculated"));
        }
    } else {
        replacements.extend([
            field("search_output_policy", search_output_policy),
            field("unique_solution_count", solution_count),
            field("normalized_unique_solution_count", solution_count),
            field("actual_normalized_unique_solution_count", solution_count),
            field("solution_count_calculated", true),
            field("solution_set_materialized", true),
            field("solution_keys_materialized_count", solution_count),
            field("solution_keys_complete", filtered_solution_keys_complete),
            field("solution_page_available", false),
            field("normalized_solution_set_hash", &solution_hash),
            field("actual_normalized_solution_set_hash", &solution_hash),
        ]);
        if result.field("total_solution_count").is_some() {
            replacements.push(field("total_solution_count", solution_count));
        }
    }
    if minimum_cover_requested {
        replacements.extend([
            field(
                "minimum_cover_source_solution_count",
                minimum_cover_source_solution_count,
            ),
            field("minimum_cover_selected_solution_count", solution_count),
            field("minimum_cover_required_pattern_count", union.count_ones()),
            field("minimum_cover_complete", minimum_cover_complete),
            field("minimum_cover_proven_minimum", minimum_cover_proven),
            field("minimum_cover_incomplete_reason", &minimum_cover_reason),
        ]);
    }
    append_build_symmetry_fields(&mut replacements, &pass_results, &weights);
    let mut coverage_pattern_words = Vec::new();
    coverage_pattern_words
        .try_reserve_exact(union.word_count())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_coverage_allocation_failed",
        })?;
    for word_index in 0..union.word_count() {
        coverage_pattern_words.push(union.word_at(word_index));
    }
    let output_bytes = checked_final_materialized_output_retained_bytes(
        &normalized_keys,
        &identities,
        &coverage_pattern_words,
        &solution_coverages,
        &normalized_solution_coverages,
        &solution_probabilities,
        &filtered_scoring_batches,
        &filtered_spin_batches,
        &pre_b2b_produced_solution_audit_checkpoint,
        &pre_b2b_solution_audit_checkpoint,
    )
    .ok_or(CoreExecutionError::RuntimeUnavailable {
        component: "b2b_preservation_memory_projection_overflow",
    })?;
    let replacement_bytes = checked_owned_field_storage_bytes(&replacements).ok_or(
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        },
    )?;
    memory_guard(
        &result,
        output_bytes.checked_add(replacement_bytes).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?,
    )?;

    drop(authoritative_coverage);
    drop(accepted);
    drop(pass_results);
    drop(identity_by_key);
    drop(weights);
    drop(minimum_cover_reason);
    drop(union);

    let result = result
        .with_packing_candidate_keys(Vec::new())
        .with_path_steps(Vec::new())
        .with_representative_solution_identity(None)
        .with_normalized_solution_keys(normalized_keys)
        .with_normalized_solution_identities(identities)
        .with_coverage_pattern_words(coverage_pattern_words)
        .with_solution_coverages(solution_coverages)
        .with_normalized_solution_coverages(normalized_solution_coverages)
        .with_solution_probabilities(solution_probabilities)
        .with_solution_average_scores(Vec::new())
        .with_exact_scoring_execution_batches(filtered_scoring_batches)
        .with_spin_coverage_execution_batches(filtered_spin_batches)
        .with_pre_b2b_produced_solution_audit_checkpoint(pre_b2b_produced_solution_audit_checkpoint)
        .with_pre_b2b_solution_audit_checkpoint(pre_b2b_solution_audit_checkpoint)
        .without_finesse_search_report()
        .without_tiling_solution_page_store();
    memory_guard(&result, replacement_bytes)?;
    result
        .try_with_replaced_fields_with_memory_guard(replacements, |live, future| {
            memory_guard(live, future)
        })
        .map_err(|error| match error {
            clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::ProjectionOverflow => {
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                }
            }
            clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::AllocationFailed { .. } => {
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_field_allocation_failed",
                }
            }
            clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::MemoryGuard(error) => error,
        })
}

fn minimum_cover_input_status(result: &CoreExecutionResult) -> (bool, Option<&str>) {
    let requested = result.field("objective") == Some("minimum-cover");
    if !requested {
        return (false, None);
    }

    let blocking_reason = match result.field("minimum_cover_incomplete_reason") {
        Some("none")
            if result.bool_field("minimum_cover_complete") == Some(true)
                && result.bool_field("minimum_cover_proven_minimum") == Some(true) =>
        {
            None
        }
        Some("none") | None => Some("minimum-cover-status-missing"),
        Some(reason) => Some(reason),
    };
    (true, blocking_reason)
}

fn checked_authoritative_coverage_projection(
    result: &CoreExecutionResult,
    pattern_count: usize,
) -> Option<u128> {
    let capacity = result
        .normalized_solution_coverages()
        .len()
        .checked_add(result.solution_coverages().len())?;
    let mut bytes =
        (capacity as u128).checked_mul(core::mem::size_of::<(String, PatternBitSet)>() as u128)?;
    for coverage in result.normalized_solution_coverages() {
        bytes = bytes.checked_add(coverage.solution_key().len() as u128)?;
    }
    for coverage in result.solution_coverages() {
        let canonical_capacity =
            42_usize.checked_add(coverage.identity().placement_count().checked_mul(20)?)?;
        bytes = bytes.checked_add(canonical_capacity as u128)?;
    }
    bytes.checked_add(
        PatternBitSet::checked_external_words_materialize_union_future_bytes(pattern_count)?,
    )
}

fn checked_coverage_table_retained_bytes(table: &CoverageTable) -> Option<u128> {
    let mut bytes = (table.capacity() as u128)
        .checked_mul(core::mem::size_of::<(String, PatternBitSet)>() as u128)?;
    for (key, coverage) in table {
        bytes = bytes
            .checked_add(key.capacity() as u128)?
            .checked_add(coverage.checked_storage_retained_bytes()?)?;
    }
    Some(bytes)
}

fn checked_checkpoint_construction_projection(
    result: &CoreExecutionResult,
    authoritative: &CoverageTable,
) -> Option<u128> {
    let normalized_slots = (result.normalized_solution_keys().len() as u128)
        .checked_mul(core::mem::size_of::<String>() as u128)?;
    let normalized_keys = result
        .normalized_solution_keys()
        .iter()
        .try_fold(normalized_slots, |bytes, key| {
            bytes.checked_add(key.len() as u128)
        })?;
    let authoritative_slots =
        (authoritative.len() as u128).checked_mul(core::mem::size_of::<String>() as u128)?;
    let authoritative_keys = authoritative
        .iter()
        .try_fold(authoritative_slots, |bytes, (key, _)| {
            bytes.checked_add(key.len() as u128)
        })?;
    const PRODUCED_REASONS: &[&str] = &[
        "pre-b2b-produced-solution-count-incomplete",
        "pre-b2b-produced-explicit-availability-contract-missing",
        "pre-b2b-produced-availability-contract-invalid",
        "pre-b2b-produced-solution-count-not-calculated",
        "pre-b2b-produced-solution-set-not-materialized",
        "pre-b2b-produced-solution-keys-incomplete",
        "pre-b2b-produced-solution-key-count-mismatch",
        "pre-b2b-produced-solution-keys-not-unique",
    ];
    const VALIDATED_REASONS: &[&str] = &[
        "pre-b2b-solution-count-incomplete",
        "pre-b2b-explicit-solution-availability-contract-missing",
        "pre-b2b-solution-availability-contract-invalid",
        "pre-b2b-solution-count-not-calculated",
        "pre-b2b-solution-set-not-materialized",
        "pre-b2b-solution-keys-incomplete",
        "pre-b2b-solution-key-count-mismatch",
        "pre-b2b-solution-keys-not-unique",
        "pre-b2b-authoritative-coverage-key-mismatch",
        "pre-b2b-execution-batches-incomplete",
    ];
    let reason_slots = (PRODUCED_REASONS
        .len()
        .checked_add(VALIDATED_REASONS.len())? as u128)
        .checked_mul(core::mem::size_of::<String>() as u128)?;
    let reason_strings = PRODUCED_REASONS
        .iter()
        .chain(VALIDATED_REASONS)
        .try_fold(0_u128, |bytes, reason| {
            bytes.checked_add(reason.len() as u128)
        })?;
    normalized_keys
        .max(authoritative_keys)
        .checked_add(reason_slots)?
        .checked_add(reason_strings)?
        // Both `ctks1:` identities have exactly sixteen lowercase hex digits.
        .checked_add(2 * "ctks1:0000000000000000".len() as u128)
}

fn checked_constraint_scratch_bytes(
    checkpoint_bytes: u128,
    authoritative: &CoverageTable,
    accepted: &CoverageTable,
    pass_results: &Vec<PassConstraintResult>,
    scoring_batches: &Vec<ExactScoringExecutionBatch>,
    spin_batches: &Vec<SpinCoverageExecutionBatch>,
) -> Result<u128, CoreExecutionError> {
    let mut bytes = checkpoint_bytes
        .checked_add(checked_coverage_table_retained_bytes(authoritative).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?)
        .and_then(|bytes| bytes.checked_add(checked_coverage_table_retained_bytes(accepted)?))
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    bytes = bytes
        .checked_add(
            (pass_results.capacity() as u128)
                .checked_mul(core::mem::size_of::<PassConstraintResult>() as u128)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?,
        )
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    for pass in pass_results {
        bytes = bytes
            .checked_add(pass.coverage.checked_storage_retained_bytes().ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
    }
    bytes = bytes
        .checked_add(
            (scoring_batches.capacity() as u128)
                .checked_mul(core::mem::size_of::<ExactScoringExecutionBatch>() as u128)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?,
        )
        .and_then(|bytes| {
            bytes.checked_add(
                (spin_batches.capacity() as u128)
                    .checked_mul(core::mem::size_of::<SpinCoverageExecutionBatch>() as u128)?,
            )
        })
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    for batch in scoring_batches {
        bytes = bytes
            .checked_add(batch.checked_nested_retained_bytes().ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
    }
    for batch in spin_batches {
        bytes = bytes
            .checked_add(batch.checked_nested_retained_bytes().ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
    }
    Ok(bytes)
}

fn checked_owned_field_storage_bytes(fields: &Vec<(String, String)>) -> Option<u128> {
    let mut bytes =
        (fields.capacity() as u128).checked_mul(core::mem::size_of::<(String, String)>() as u128)?;
    for (key, value) in fields {
        bytes = bytes
            .checked_add(key.capacity() as u128)?
            .checked_add(value.capacity() as u128)?;
    }
    Some(bytes)
}

fn checked_replacement_field_construction_upper_bound(
    field_count: usize,
    dynamic_value_bytes: usize,
) -> Option<u128> {
    let field_count = field_count as u128;
    let max_value_bytes = (dynamic_value_bytes as u128)
        .max("normalized-solution-pattern-bitset-or-union".len() as u128)
        .max(u128::from(u128::MAX.ilog10()) + 1);
    field_count
        .checked_mul(core::mem::size_of::<(String, String)>() as u128)?
        .checked_add(
            field_count.checked_mul("b2b_preservation_path_multiplicity_counted".len() as u128)?,
        )?
        .checked_add(field_count.checked_mul(max_value_bytes)?)
}

#[allow(clippy::too_many_arguments)]
fn checked_final_materialized_output_retained_bytes(
    normalized_keys: &Vec<String>,
    identities: &Vec<StandardBoard64TilingIdentity>,
    coverage_pattern_words: &Vec<u64>,
    solution_coverages: &Vec<SolutionCoverage>,
    normalized_solution_coverages: &Vec<NormalizedSolutionCoverage>,
    solution_probabilities: &Vec<SolutionProbabilityReport>,
    scoring_batches: &Vec<ExactScoringExecutionBatch>,
    spin_batches: &Vec<SpinCoverageExecutionBatch>,
    produced_checkpoint: &SolutionAuditCheckpoint,
    validated_checkpoint: &SolutionAuditCheckpoint,
) -> Option<u128> {
    let mut bytes =
        (normalized_keys.capacity() as u128).checked_mul(core::mem::size_of::<String>() as u128)?;
    for key in normalized_keys {
        bytes = bytes.checked_add(key.capacity() as u128)?;
    }
    bytes = bytes.checked_add(
        (identities.capacity() as u128)
            .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)?,
    )?;
    bytes = bytes.checked_add(
        (coverage_pattern_words.capacity() as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)?,
    )?;
    bytes = bytes.checked_add(
        (solution_coverages.capacity() as u128)
            .checked_mul(core::mem::size_of::<SolutionCoverage>() as u128)?,
    )?;
    for coverage in solution_coverages {
        bytes = bytes.checked_add(coverage.checked_nested_retained_bytes()?)?;
    }
    bytes = bytes.checked_add(
        (normalized_solution_coverages.capacity() as u128)
            .checked_mul(core::mem::size_of::<NormalizedSolutionCoverage>() as u128)?,
    )?;
    for coverage in normalized_solution_coverages {
        bytes = bytes.checked_add(coverage.checked_nested_retained_bytes()?)?;
    }
    bytes = bytes.checked_add(
        (solution_probabilities.capacity() as u128)
            .checked_mul(core::mem::size_of::<SolutionProbabilityReport>() as u128)?,
    )?;
    for probability in solution_probabilities {
        bytes = bytes.checked_add(probability.checked_nested_retained_bytes()?)?;
    }
    bytes = bytes.checked_add(
        (scoring_batches.capacity() as u128)
            .checked_mul(core::mem::size_of::<ExactScoringExecutionBatch>() as u128)?,
    )?;
    for batch in scoring_batches {
        bytes = bytes.checked_add(batch.checked_nested_retained_bytes()?)?;
    }
    bytes = bytes.checked_add(
        (spin_batches.capacity() as u128)
            .checked_mul(core::mem::size_of::<SpinCoverageExecutionBatch>() as u128)?,
    )?;
    for batch in spin_batches {
        bytes = bytes.checked_add(batch.checked_nested_retained_bytes()?)?;
    }
    bytes = bytes
        .checked_add(produced_checkpoint.checked_nested_retained_bytes()?)?
        .checked_add(validated_checkpoint.checked_nested_retained_bytes()?)?;
    Some(bytes)
}

fn checked_final_constraint_output_projection(
    accepted: &CoverageTable,
    scoring_batches: &[ExactScoringExecutionBatch],
    spin_batches: &[SpinCoverageExecutionBatch],
    pattern_count: usize,
    probabilities_requested: bool,
    dynamic_field_value_bytes: usize,
) -> Option<u128> {
    let candidate_count = accepted.len();
    let candidate_count_u128 = candidate_count as u128;
    let key_bytes = accepted.iter().try_fold(0_u128, |bytes, (key, _)| {
        bytes.checked_add(key.len() as u128)
    })?;
    let mut bytes = (scoring_batches.len() as u128)
        .checked_mul(core::mem::size_of::<ExactScoringExecutionBatch>() as u128)?
        .checked_add(
            (spin_batches.len() as u128)
                .checked_mul(core::mem::size_of::<SpinCoverageExecutionBatch>() as u128)?,
        )?;
    for batch in scoring_batches {
        bytes = bytes.checked_add(batch.checked_clone_nested_bytes()?)?;
    }
    for batch in spin_batches {
        bytes = bytes.checked_add(batch.checked_clone_nested_bytes()?)?;
    }
    let scoring_graph_count = scoring_batches
        .iter()
        .map(|batch| batch.graphs().len())
        .try_fold(0_usize, usize::checked_add)?;
    let identity_key_bytes = scoring_batches.iter().try_fold(0_u128, |bytes, batch| {
        batch.graphs().iter().try_fold(bytes, |bytes, graph| {
            let capacity =
                42_usize.checked_add(graph.identity().placement_count().checked_mul(20)?)?;
            bytes.checked_add(capacity as u128)
        })
    })?;
    bytes = bytes
        .checked_add(
            (scoring_graph_count as u128)
                .checked_mul(
                    core::mem::size_of::<(String, StandardBoard64TilingIdentity)>() as u128,
                )?,
        )?
        .checked_add(identity_key_bytes)?;
    bytes = bytes
        .checked_add(candidate_count_u128.checked_mul(core::mem::size_of::<String>() as u128)?)?
        .checked_add(key_bytes)?
        .checked_add(
            candidate_count_u128
                .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)?,
        )?
        .checked_add(
            candidate_count_u128.checked_mul(core::mem::size_of::<SolutionCoverage>() as u128)?,
        )?
        .checked_add(
            candidate_count_u128
                .checked_mul(core::mem::size_of::<NormalizedSolutionCoverage>() as u128)?,
        )?
        .checked_add(key_bytes)?;
    if probabilities_requested {
        bytes = bytes
            .checked_add(candidate_count_u128.checked_mul(core::mem::size_of::<
                clearra_core_executor::SolutionProbabilityReport,
            >() as u128)?)?
            .checked_add(key_bytes)?
            // Rust's shortest round-trippable finite-f64 representation is at
            // most 24 bytes; probabilities are a subset of that domain.
            .checked_add(candidate_count_u128.checked_mul(24)?)?;
    }
    let pattern_owner_count = candidate_count_u128.checked_mul(2)?.checked_add(2)?;
    bytes = bytes.checked_add(PatternBitSet::checked_shared_construction_upper_bound(
        pattern_count,
        pattern_owner_count,
        pattern_owner_count.checked_mul(pattern_count as u128)?,
    )?)?;
    bytes = bytes.checked_add(
        (pattern_count.div_ceil(u64::BITS as usize) as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)?,
    )?;
    // Set hash, objective reason, and output-policy owners. The reason is
    // copied from an existing result field or one of the fixed literals.
    bytes = bytes
        .checked_add("ctks1:0000000000000000".len() as u128)?
        .checked_add(24)?
        .checked_add("pattern_universe_incomplete".len() as u128)?;

    // Thirty-nine common fields, at most twelve policy fields (including the
    // optional total), six minimum-cover fields, and six two-pass symmetry
    // fields. The longest key is one of the two explicit 42-byte B2B keys.
    // Every value is bounded by either the caller-observed dynamic maximum,
    // the longest fixed value, or a u128 decimal representation.
    let field_count = 39_u128.checked_add(12)?.checked_add(6)?.checked_add(6)?;
    let max_value_bytes = (dynamic_field_value_bytes as u128)
        .max("normalized-solution-pattern-bitset-or-union".len() as u128)
        .max(u128::from(u128::MAX.ilog10()) + 1);
    bytes
        .checked_add(field_count.checked_mul(core::mem::size_of::<(String, String)>() as u128)?)?
        .checked_add(
            field_count.checked_mul("b2b_preservation_path_multiplicity_counted".len() as u128)?,
        )?
        .checked_add(field_count.checked_mul(max_value_bytes)?)
}

fn core_error_from_b2b_filter(error: BackToBackFilterError) -> CoreExecutionError {
    match error {
        BackToBackFilterError::ProjectionOverflow => CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        },
        BackToBackFilterError::MemoryCapacityExceeded { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_capacity_exceeded",
            }
        }
        BackToBackFilterError::AllocationFailed { .. } => CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_batch_allocation_failed",
        },
    }
}

fn core_error_from_exact_minimum_cover(error: ExactMinimumCoverError) -> CoreExecutionError {
    let component = match error {
        ExactMinimumCoverError::RowPatternCountMismatch { .. } => {
            "b2b_preservation_minimum_cover_universe_mismatch"
        }
        ExactMinimumCoverError::ProjectionOverflow => "b2b_preservation_memory_projection_overflow",
        ExactMinimumCoverError::MemoryCapacityExceeded { .. }
        | ExactMinimumCoverError::MemoryGuardRejected => {
            "b2b_preservation_memory_capacity_exceeded"
        }
        ExactMinimumCoverError::AllocationFailed { .. } => {
            "b2b_preservation_minimum_cover_allocation_failed"
        }
    };
    CoreExecutionError::RuntimeUnavailable { component }
}

fn core_error_from_exact_minimum_cover_portfolio(
    error: ExactMinimumCoverPortfolioError,
) -> CoreExecutionError {
    match error {
        ExactMinimumCoverPortfolioError::MinimumCover(error) => {
            core_error_from_exact_minimum_cover(error)
        }
        ExactMinimumCoverPortfolioError::RequiredPatternsNotCoverable { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_minimum_cover_incomplete",
            }
        }
        ExactMinimumCoverPortfolioError::AllocationFailed { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_minimum_cover_allocation_failed",
            }
        }
        ExactMinimumCoverPortfolioError::InvalidMinimumCoverProof
        | ExactMinimumCoverPortfolioError::PageSizeMustBePositive
        | ExactMinimumCoverPortfolioError::InvalidRestart => {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_minimum_cover_proof_invalid",
            }
        }
    }
}

fn core_error_from_field_replacement(
    error: clearra_core_executor::core_execution_result::CoreResultFieldReplacementError<
        CoreExecutionError,
    >,
) -> CoreExecutionError {
    match error {
        clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::ProjectionOverflow => {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            }
        }
        clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::AllocationFailed { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_field_allocation_failed",
            }
        }
        clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::MemoryGuard(error) => error,
    }
}

fn core_error_from_b2b_materialization(
    error: TSpinCoverageMaterializationError,
) -> CoreExecutionError {
    match error {
        TSpinCoverageMaterializationError::Cancelled => CoreExecutionError::Cancelled,
        TSpinCoverageMaterializationError::ProjectionOverflow => {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            }
        }
        TSpinCoverageMaterializationError::MemoryCapacityExceeded { .. } => {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_capacity_exceeded",
            }
        }
    }
}

fn coverage_union<'a>(
    coverages: impl IntoIterator<Item = &'a PatternBitSet>,
    pattern_count: usize,
) -> Result<PatternBitSet, CoreExecutionError> {
    let mut union = PatternBitSet::new(pattern_count);
    for coverage in coverages {
        union
            .union_with(coverage)
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_coverage_universe_mismatch",
            })?;
    }
    Ok(union)
}

fn authoritative_solution_coverages(
    result: &CoreExecutionResult,
    pattern_count: usize,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<CoverageTable, CoreExecutionError> {
    let projection = checked_authoritative_coverage_projection(result, pattern_count).ok_or(
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        },
    )?;
    memory_guard(result, projection)?;
    let union_future = PatternBitSet::checked_external_words_materialize_union_future_bytes(
        pattern_count,
    )
    .ok_or(CoreExecutionError::RuntimeUnavailable {
        component: "b2b_preservation_memory_projection_overflow",
    })?;
    let capacity = result
        .normalized_solution_coverages()
        .len()
        .checked_add(result.solution_coverages().len())
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    let mut authoritative = Vec::new();
    authoritative.try_reserve_exact(capacity).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_coverage_allocation_failed",
        }
    })?;
    memory_guard(
        result,
        checked_coverage_table_retained_bytes(&authoritative).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?,
    )?;
    for coverage in result.normalized_solution_coverages() {
        merge_authoritative_coverage(
            result,
            &mut authoritative,
            coverage.solution_key(),
            coverage.covered_patterns(),
            pattern_count,
            union_future,
            memory_guard,
        )?;
    }
    for coverage in result.solution_coverages() {
        let key = NormalizedTilingSolutionKey::from_standard_board64_identity(coverage.identity());
        merge_authoritative_coverage(
            result,
            &mut authoritative,
            key.as_str(),
            coverage.covered_patterns(),
            pattern_count,
            union_future,
            memory_guard,
        )?;
    }
    if authoritative.is_empty() && !is_proven_empty_candidate_partition(result, pattern_count) {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_authoritative_coverage_missing",
        });
    }
    Ok(authoritative)
}

fn pre_b2b_produced_solution_audit_checkpoint(
    result: &CoreExecutionResult,
) -> SolutionAuditCheckpoint {
    let mut solution_keys = result.normalized_solution_keys().to_vec();
    solution_keys.sort_unstable();
    let materialized_key_count = solution_keys.len();
    solution_keys.dedup();
    let identity_hash = normalized_tiling_solution_key_set_hash_from_sorted_strings(&solution_keys);

    let availability = result.execution_report().solution_set_availability();
    let mut incomplete_reasons = Vec::new();
    if result.bool_field("count_complete") != Some(true) {
        incomplete_reasons.push("pre-b2b-produced-solution-count-incomplete");
    }
    if !availability.uses_explicit_contract() {
        incomplete_reasons.push("pre-b2b-produced-explicit-availability-contract-missing");
    } else if !availability.contract_valid() {
        incomplete_reasons.push("pre-b2b-produced-availability-contract-invalid");
    } else {
        if !availability.solution_count_calculated() {
            incomplete_reasons.push("pre-b2b-produced-solution-count-not-calculated");
        }
        if !availability.solution_set_materialized() {
            incomplete_reasons.push("pre-b2b-produced-solution-set-not-materialized");
        }
        if !availability.solution_keys_complete() {
            incomplete_reasons.push("pre-b2b-produced-solution-keys-incomplete");
        }
        if !availability.materialized_key_count_matches(materialized_key_count) {
            incomplete_reasons.push("pre-b2b-produced-solution-key-count-mismatch");
        }
    }
    if materialized_key_count != solution_keys.len() {
        incomplete_reasons.push("pre-b2b-produced-solution-keys-not-unique");
    }

    SolutionAuditCheckpoint::new(
        Some(solution_keys.len()),
        incomplete_reasons.is_empty(),
        Some(identity_hash),
        incomplete_reasons,
    )
}

fn pre_b2b_solution_audit_checkpoint(
    result: &CoreExecutionResult,
    authoritative: &CoverageTable,
) -> SolutionAuditCheckpoint {
    let authoritative_keys = authoritative
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    let identity_hash =
        normalized_tiling_solution_key_set_hash_from_sorted_strings(&authoritative_keys);
    let mut materialized_keys = result.normalized_solution_keys().to_vec();
    materialized_keys.sort_unstable();
    let materialized_key_count = materialized_keys.len();
    materialized_keys.dedup();

    let availability = result.execution_report().solution_set_availability();
    let mut incomplete_reasons = Vec::new();
    if result.bool_field("count_complete") != Some(true) {
        incomplete_reasons.push("pre-b2b-solution-count-incomplete");
    }
    if !availability.uses_explicit_contract() {
        incomplete_reasons.push("pre-b2b-explicit-solution-availability-contract-missing");
    } else if !availability.contract_valid() {
        incomplete_reasons.push("pre-b2b-solution-availability-contract-invalid");
    } else {
        if !availability.solution_count_calculated() {
            incomplete_reasons.push("pre-b2b-solution-count-not-calculated");
        }
        if !availability.solution_set_materialized() {
            incomplete_reasons.push("pre-b2b-solution-set-not-materialized");
        }
        if !availability.solution_keys_complete() {
            incomplete_reasons.push("pre-b2b-solution-keys-incomplete");
        }
        if !availability.materialized_key_count_matches(materialized_key_count) {
            incomplete_reasons.push("pre-b2b-solution-key-count-mismatch");
        }
    }
    if materialized_key_count != materialized_keys.len() {
        incomplete_reasons.push("pre-b2b-solution-keys-not-unique");
    }
    if materialized_keys != authoritative_keys {
        incomplete_reasons.push("pre-b2b-authoritative-coverage-key-mismatch");
    }
    if !execution_batches_are_complete(result) {
        incomplete_reasons.push("pre-b2b-execution-batches-incomplete");
    }

    SolutionAuditCheckpoint::new(
        Some(authoritative_keys.len()),
        incomplete_reasons.is_empty(),
        Some(identity_hash),
        incomplete_reasons,
    )
}

fn execution_graphs_are_empty(result: &CoreExecutionResult) -> bool {
    result
        .exact_scoring_execution_batches()
        .iter()
        .all(|batch| batch.graphs().is_empty())
        && result
            .spin_coverage_execution_batches()
            .iter()
            .all(|batch| batch.graphs().is_empty())
}

fn execution_batches_are_complete(result: &CoreExecutionResult) -> bool {
    result
        .exact_scoring_execution_batches()
        .iter()
        .all(ExactScoringExecutionBatch::complete)
        && result
            .spin_coverage_execution_batches()
            .iter()
            .all(SpinCoverageExecutionBatch::complete)
}

fn vacuous_b2b_constraint_is_proven(result: &CoreExecutionResult) -> bool {
    let Some(solution_count) = result.usize_field("unique_solution_count") else {
        return false;
    };
    has_execution_batch(result)
        && execution_batches_are_complete(result)
        && result.bool_field("solution_found") == Some(solution_count != 0)
        && result.bool_field("objective_search_complete") == Some(true)
        && result.bool_field("count_complete") == Some(true)
}

fn is_proven_empty_candidate_partition(result: &CoreExecutionResult, pattern_count: usize) -> bool {
    has_execution_batch(result)
        && execution_graphs_are_empty(result)
        && execution_batches_are_complete(result)
        && solution_set_is_proven_empty(result)
        && result.bool_field("solution_found") == Some(false)
        && result.usize_field("covered_pattern_count") == Some(0)
        && result.normalized_solution_keys().is_empty()
        && result.normalized_solution_identities().is_empty()
        && result.representative_solution_identity().is_none()
        && result.path_steps().is_empty()
        && result.solution_coverages().is_empty()
        && result.normalized_solution_coverages().is_empty()
        && result.coverage_pattern_words().len() == pattern_count.div_ceil(u64::BITS as usize)
        && result
            .coverage_pattern_words()
            .iter()
            .all(|word| *word == 0)
}

fn has_execution_batch(result: &CoreExecutionResult) -> bool {
    !result.exact_scoring_execution_batches().is_empty()
        || !result.spin_coverage_execution_batches().is_empty()
}

fn solution_set_is_proven_empty(result: &CoreExecutionResult) -> bool {
    let materialized_zero = result.usize_field("unique_solution_count") == Some(0)
        && result.field("search_output_policy") != Some("coverage-summary")
        && !matches!(result.bool_field("solution_count_calculated"), Some(false))
        && !matches!(result.bool_field("solution_set_materialized"), Some(false));
    let coverage_summary_not_materialized = result.field("search_output_policy")
        == Some("coverage-summary")
        && result.field("unique_solution_count") == Some("not-calculated")
        && result.field("normalized_unique_solution_count") == Some("not-calculated")
        && result.bool_field("solution_count_calculated") == Some(false)
        && result.bool_field("solution_set_materialized") == Some(false)
        && result.usize_field("solution_keys_materialized_count") == Some(0)
        && result.field("normalized_solution_set_hash") == Some("not-calculated")
        && result.field("actual_normalized_solution_set_hash") == Some("not-calculated");
    materialized_zero || coverage_summary_not_materialized
}

fn merge_authoritative_coverage(
    result: &CoreExecutionResult,
    authoritative: &mut CoverageTable,
    candidate_key: &str,
    coverage: &PatternBitSet,
    pattern_count: usize,
    union_future: u128,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<(), CoreExecutionError> {
    if coverage.pattern_count() != pattern_count {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_authoritative_coverage_mismatch",
        });
    }
    match authoritative.binary_search_by(|(key, _)| key.as_str().cmp(candidate_key)) {
        Ok(index) => {
            let current_bytes = checked_coverage_table_retained_bytes(authoritative).ok_or(
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                },
            )?;
            memory_guard(
                result,
                current_bytes.checked_add(union_future).ok_or(
                    CoreExecutionError::RuntimeUnavailable {
                        component: "b2b_preservation_memory_projection_overflow",
                    },
                )?,
            )?;
            authoritative[index].1.union_with(coverage).map_err(|_| {
                CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_authoritative_coverage_mismatch",
                }
            })?;
            memory_guard(
                result,
                checked_coverage_table_retained_bytes(authoritative).ok_or(
                    CoreExecutionError::RuntimeUnavailable {
                        component: "b2b_preservation_memory_projection_overflow",
                    },
                )?,
            )
        }
        Err(index) => {
            let key = try_owned_string(candidate_key)?;
            authoritative.insert(index, (key, coverage.clone()));
            memory_guard(
                result,
                checked_coverage_table_retained_bytes(authoritative).ok_or(
                    CoreExecutionError::RuntimeUnavailable {
                        component: "b2b_preservation_memory_projection_overflow",
                    },
                )?,
            )?;
            Ok(())
        }
    }
}

fn validate_execution_graph_authority(
    result: &CoreExecutionResult,
    authoritative: &CoverageTable,
) -> Result<(), CoreExecutionError> {
    let scoring_complete = result
        .exact_scoring_execution_batches()
        .iter()
        .flat_map(|batch| batch.graphs())
        .all(|graph| {
            let key = NormalizedTilingSolutionKey::from_standard_board64_identity(graph.identity());
            coverage_table_get(authoritative, key.as_str()).is_some()
        });
    let spin_complete = result
        .spin_coverage_execution_batches()
        .iter()
        .flat_map(|batch| batch.graphs())
        .all(|graph| coverage_table_get(authoritative, graph.candidate_key()).is_some());
    if scoring_complete && spin_complete {
        Ok(())
    } else {
        Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_candidate_coverage_missing",
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn merge_candidate_coverages_with_memory_guard(
    result: &CoreExecutionResult,
    accepted: &mut CoverageTable,
    coverages: &[CandidatePatternCoverage],
    authoritative: &CoverageTable,
    pattern_count: usize,
    filtered_live_bytes: u128,
    materialized_retained_bytes: u128,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<PassConstraintResult, CoreExecutionError> {
    let accepted_before_bytes = checked_coverage_table_retained_bytes(accepted).ok_or(
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        },
    )?;
    let merge_base_bytes = filtered_live_bytes
        .checked_sub(accepted_before_bytes)
        .and_then(|bytes| bytes.checked_add(materialized_retained_bytes))
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    let merge_projection =
        checked_merge_candidate_coverages_projection(accepted, coverages, pattern_count).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?;
    memory_guard(
        result,
        merge_base_bytes.checked_add(merge_projection).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?,
    )?;
    merge_candidate_coverages(
        accepted,
        coverages,
        authoritative,
        pattern_count,
        |actual_merge_bytes| {
            memory_guard(
                result,
                merge_base_bytes.checked_add(actual_merge_bytes).ok_or(
                    CoreExecutionError::RuntimeUnavailable {
                        component: "b2b_preservation_memory_projection_overflow",
                    },
                )?,
            )
        },
    )
}

fn checked_merge_candidate_coverages_projection(
    accepted: &CoverageTable,
    coverages: &[CandidatePatternCoverage],
    pattern_count: usize,
) -> Option<u128> {
    let bitset = PatternBitSet::checked_all_projection(pattern_count)?;
    let candidate_count = coverages.len() as u128;
    let candidate_key_bytes = coverages.iter().try_fold(0_u128, |bytes, coverage| {
        bytes.checked_add(coverage.candidate_key().len() as u128)
    })?;
    checked_coverage_table_retained_bytes(accepted)?
        .checked_add(bitset.constructor_peak_bytes)?
        .checked_add(candidate_count.checked_mul(core::mem::size_of::<&str>() as u128)?)?
        .checked_add(candidate_key_bytes)?
        .checked_add(candidate_count.checked_mul(bitset.storage_retained_bytes)?)?
        // One accepted bitset may own its requested construction transient
        // while all earlier accepted entries and the pass bitset remain live.
        .checked_add(bitset.constructor_peak_bytes)
}

fn merge_candidate_coverages(
    accepted: &mut CoverageTable,
    coverages: &[CandidatePatternCoverage],
    authoritative: &CoverageTable,
    pattern_count: usize,
    mut reauthorize_actual: impl FnMut(u128) -> Result<(), CoreExecutionError>,
) -> Result<PassConstraintResult, CoreExecutionError> {
    let mut pass_coverage = PatternBitSet::new(pattern_count);
    let mut pass_solutions = Vec::<&str>::new();
    reauthorize_merge_candidate_state(
        accepted,
        &pass_coverage,
        &pass_solutions,
        0,
        &mut reauthorize_actual,
    )?;
    pass_solutions
        .try_reserve_exact(coverages.len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_coverage_allocation_failed",
        })?;
    reauthorize_merge_candidate_state(
        accepted,
        &pass_coverage,
        &pass_solutions,
        0,
        &mut reauthorize_actual,
    )?;
    let mut witnessed_pattern_count = 0_u128;
    for coverage in coverages {
        let allowed = coverage_table_get(authoritative, coverage.candidate_key()).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_candidate_coverage_missing",
            },
        )?;
        let covered = coverage.covered_patterns();
        if covered.pattern_count() != pattern_count || allowed.pattern_count() != pattern_count {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_candidate_coverage_mismatch",
            });
        }
        let mut constrained_count = 0_u32;
        for word_index in 0..covered.word_count() {
            constrained_count = constrained_count.saturating_add(
                (covered.word_at(word_index) & allowed.word_at(word_index)).count_ones(),
            );
        }
        if constrained_count == 0 {
            continue;
        }

        let accepted_index = match accepted
            .binary_search_by(|(candidate, _)| candidate.as_str().cmp(coverage.candidate_key()))
        {
            Ok(index) => index,
            Err(index) => {
                let key = try_owned_string(coverage.candidate_key())?;
                reauthorize_merge_candidate_state(
                    accepted,
                    &pass_coverage,
                    &pass_solutions,
                    key.capacity() as u128,
                    &mut reauthorize_actual,
                )?;
                let entry = PatternBitSet::new(pattern_count);
                let external_entry_bytes = (key.capacity() as u128)
                    .checked_add(entry.checked_storage_retained_bytes().ok_or(
                        CoreExecutionError::RuntimeUnavailable {
                            component: "b2b_preservation_memory_projection_overflow",
                        },
                    )?)
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "b2b_preservation_memory_projection_overflow",
                    })?;
                reauthorize_merge_candidate_state(
                    accepted,
                    &pass_coverage,
                    &pass_solutions,
                    external_entry_bytes,
                    &mut reauthorize_actual,
                )?;
                if accepted.len() == accepted.capacity() {
                    return Err(CoreExecutionError::RuntimeUnavailable {
                        component: "b2b_preservation_coverage_allocation_failed",
                    });
                }
                accepted.insert(index, (key, entry));
                reauthorize_merge_candidate_state(
                    accepted,
                    &pass_coverage,
                    &pass_solutions,
                    0,
                    &mut reauthorize_actual,
                )?;
                index
            }
        };
        for word_index in 0..covered.word_count() {
            let mut word = covered.word_at(word_index) & allowed.word_at(word_index);
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                let pattern = PatternId::new(word_index * u64::BITS as usize + bit);
                accepted[accepted_index].1.insert(pattern).map_err(|_| {
                    CoreExecutionError::RuntimeUnavailable {
                        component: "b2b_preservation_candidate_coverage_mismatch",
                    }
                })?;
                pass_coverage.insert(pattern).map_err(|_| {
                    CoreExecutionError::RuntimeUnavailable {
                        component: "b2b_preservation_candidate_coverage_mismatch",
                    }
                })?;
            }
        }
        match pass_solutions.binary_search(&coverage.candidate_key()) {
            Ok(_) => {}
            Err(index) => {
                if pass_solutions.len() == pass_solutions.capacity() {
                    return Err(CoreExecutionError::RuntimeUnavailable {
                        component: "b2b_preservation_coverage_allocation_failed",
                    });
                }
                pass_solutions.insert(index, coverage.candidate_key());
            }
        }
        witnessed_pattern_count =
            witnessed_pattern_count.saturating_add(u128::from(constrained_count));
    }
    reauthorize_merge_candidate_state(
        accepted,
        &pass_coverage,
        &pass_solutions,
        0,
        &mut reauthorize_actual,
    )?;
    Ok(PassConstraintResult {
        coverage: pass_coverage,
        solution_count: pass_solutions.len(),
        witnessed_pattern_count,
    })
}

fn reauthorize_merge_candidate_state(
    accepted: &CoverageTable,
    pass_coverage: &PatternBitSet,
    pass_solutions: &Vec<&str>,
    external_bytes: u128,
    reauthorize_actual: &mut impl FnMut(u128) -> Result<(), CoreExecutionError>,
) -> Result<(), CoreExecutionError> {
    let actual_bytes = checked_coverage_table_retained_bytes(accepted)
        .and_then(|bytes| bytes.checked_add(pass_coverage.checked_storage_retained_bytes()?))
        .and_then(|bytes| {
            bytes.checked_add(
                (pass_solutions.capacity() as u128)
                    .checked_mul(core::mem::size_of::<&str>() as u128)?,
            )
        })
        .and_then(|bytes| bytes.checked_add(external_bytes))
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    reauthorize_actual(actual_bytes)
}

fn materialized_weights(
    result: &CoreExecutionResult,
    pattern_count: usize,
) -> Result<WeightedPatternSet, CoreExecutionError> {
    if result.postprocess_pattern_weights().len() != pattern_count {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_pattern_weights_not_materialized",
        });
    }
    let mut weights = Vec::new();
    weights.try_reserve_exact(pattern_count).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_weight_allocation_failed",
        }
    })?;
    for value in result.postprocess_pattern_weights() {
        let parsed = value
            .parse::<f64>()
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_pattern_weight_invalid",
            })?;
        weights.push(ProbabilityValue::new(parsed).map_err(|_| {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_pattern_weight_invalid",
            }
        })?);
    }
    WeightedPatternSet::new(weights).map_err(|_| CoreExecutionError::RuntimeUnavailable {
        component: "b2b_preservation_pattern_weight_invalid",
    })
}

fn board64_solution_coverages(
    identities: &[StandardBoard64TilingIdentity],
    accepted: &CoverageTable,
) -> Result<Vec<SolutionCoverage>, CoreExecutionError> {
    let mut coverages = Vec::new();
    coverages.try_reserve_exact(identities.len()).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_coverage_allocation_failed",
        }
    })?;
    for identity in identities.iter().copied() {
        let key = NormalizedTilingSolutionKey::from_standard_board64_identity(identity);
        if let Some(coverage) = coverage_table_get(accepted, key.as_str()) {
            coverages.push(SolutionCoverage::new(identity, coverage.clone()));
        }
    }
    Ok(coverages)
}

fn retain_accepted_scoring_batches(
    batches: Vec<ExactScoringExecutionBatch>,
    accepted: &CoverageTable,
) -> Result<Vec<ExactScoringExecutionBatch>, CoreExecutionError> {
    let mut retained = Vec::new();
    retained.try_reserve_exact(batches.len()).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_batch_allocation_failed",
        }
    })?;
    for batch in batches {
        let mut graphs = Vec::new();
        graphs
            .try_reserve_exact(batch.graphs().len())
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_batch_allocation_failed",
            })?;
        for graph in batch.graphs() {
            let key = NormalizedTilingSolutionKey::from_standard_board64_identity(graph.identity());
            if coverage_table_get(accepted, key.as_str()).is_some() {
                graphs.push(try_clone_scoring_graph(graph)?);
            }
        }
        retained.push(ExactScoringExecutionBatch::new(
            batch.layout(),
            batch.initial_occupied(),
            try_clone_patterns(batch.patterns())?,
            batch.initial_cursor(),
            batch.initial_hold(),
            batch.hold_enabled(),
            batch.projects_unplaced_lookahead(),
            batch.projects_standard_bag_lookahead(),
            batch.kick_table_id(),
            batch.rule_profile_id(),
            graphs,
            batch.complete(),
        ));
    }
    Ok(retained)
}

fn retain_accepted_spin_batches(
    batches: Vec<SpinCoverageExecutionBatch>,
    accepted: &CoverageTable,
) -> Result<Vec<SpinCoverageExecutionBatch>, CoreExecutionError> {
    let mut retained = Vec::new();
    retained.try_reserve_exact(batches.len()).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_batch_allocation_failed",
        }
    })?;
    for batch in batches {
        let mut graphs = Vec::new();
        graphs
            .try_reserve_exact(batch.graphs().len())
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_batch_allocation_failed",
            })?;
        for graph in batch.graphs() {
            if coverage_table_get(accepted, graph.candidate_key()).is_some() {
                graphs.push(try_clone_spin_graph(graph)?);
            }
        }
        retained.push(SpinCoverageExecutionBatch::new(
            try_clone_patterns(batch.patterns())?,
            batch.initial_cursor(),
            batch.initial_hold(),
            batch.hold_enabled(),
            batch.projects_unplaced_lookahead(),
            batch.projects_standard_bag_lookahead(),
            batch.kick_table_id(),
            batch.rule_profile_id(),
            graphs,
            batch.complete(),
        ));
    }
    Ok(retained)
}

fn try_clone_scoring_graph(
    graph: &ExactScoringExecutionGraph,
) -> Result<ExactScoringExecutionGraph, CoreExecutionError> {
    let (nodes, edges) = try_clone_graph_parts(
        graph.node_count(),
        |index| graph.node(index),
        |node| graph.edges(node),
    )?;
    Ok(ExactScoringExecutionGraph::new(
        graph.candidate_id(),
        graph.identity(),
        graph.root(),
        nodes,
        edges,
    ))
}

fn try_clone_spin_graph(
    graph: &SpinCoverageExecutionGraph,
) -> Result<SpinCoverageExecutionGraph, CoreExecutionError> {
    let (nodes, edges) = try_clone_graph_parts(
        graph.node_count(),
        |index| graph.node(index),
        |node| graph.edges(node),
    )?;
    Ok(SpinCoverageExecutionGraph::new(
        graph.candidate_id(),
        try_owned_string(graph.candidate_key())?,
        graph.root(),
        nodes,
        edges,
    ))
}

fn try_clone_graph_parts<'a>(
    node_count: usize,
    mut node_at: impl FnMut(u32) -> Option<ScoringExecutionNode>,
    mut edges_for: impl FnMut(ScoringExecutionNode) -> &'a [ScoringExecutionEdge],
) -> Result<(Vec<ScoringExecutionNode>, Vec<ScoringExecutionEdge>), CoreExecutionError> {
    let mut edge_count = 0_usize;
    for index in 0..node_count {
        let node =
            node_at(
                u32::try_from(index).map_err(|_| CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?,
            )
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_execution_graph_invalid",
            })?;
        edge_count = edge_count.checked_add(edges_for(node).len()).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            },
        )?;
    }
    let mut nodes = Vec::new();
    nodes
        .try_reserve_exact(node_count)
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_batch_allocation_failed",
        })?;
    let mut edges = Vec::new();
    edges
        .try_reserve_exact(edge_count)
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_batch_allocation_failed",
        })?;
    for index in 0..node_count {
        let node =
            node_at(
                u32::try_from(index).map_err(|_| CoreExecutionError::RuntimeUnavailable {
                    component: "b2b_preservation_memory_projection_overflow",
                })?,
            )
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_execution_graph_invalid",
            })?;
        let edge_start =
            u32::try_from(edges.len()).map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        edges.extend_from_slice(edges_for(node));
        let cloned_edge_count = edges
            .len()
            .checked_sub(edge_start as usize)
            .and_then(|count| u32::try_from(count).ok())
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            })?;
        nodes.push(ScoringExecutionNode::new(
            edge_start,
            cloned_edge_count,
            node.accepting(),
        ));
    }
    Ok((nodes, edges))
}

fn coverage_table_get<'a>(table: &'a CoverageTable, key: &str) -> Option<&'a PatternBitSet> {
    table
        .binary_search_by(|(candidate, _)| candidate.as_str().cmp(key))
        .ok()
        .map(|index| &table[index].1)
}

fn try_owned_string(value: &str) -> Result<String, CoreExecutionError> {
    let mut owned = String::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_string_allocation_failed",
        })?;
    owned.push_str(value);
    Ok(owned)
}

fn try_owned_string_with_memory_guard(
    result: &CoreExecutionResult,
    already_retained_future: u128,
    value: &str,
    memory_guard: &mut impl FnMut(&CoreExecutionResult, u128) -> Result<(), CoreExecutionError>,
) -> Result<String, CoreExecutionError> {
    let requested_future = already_retained_future
        .checked_add(value.len() as u128)
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    memory_guard(result, requested_future)?;
    let owned = try_owned_string(value)?;
    let actual_future = already_retained_future
        .checked_add(owned.capacity() as u128)
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_memory_projection_overflow",
        })?;
    memory_guard(result, actual_future)?;
    Ok(owned)
}

fn try_clone_patterns(
    patterns: &[Vec<clearra_core_domain::piece::piece_kind::PieceKind>],
) -> Result<Vec<Vec<clearra_core_domain::piece::piece_kind::PieceKind>>, CoreExecutionError> {
    let mut cloned = Vec::new();
    cloned.try_reserve_exact(patterns.len()).map_err(|_| {
        CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_batch_allocation_failed",
        }
    })?;
    for pattern in patterns {
        let mut cloned_pattern = Vec::new();
        cloned_pattern
            .try_reserve_exact(pattern.len())
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_batch_allocation_failed",
            })?;
        cloned_pattern.extend_from_slice(pattern);
        cloned.push(cloned_pattern);
    }
    Ok(cloned)
}

struct PassConstraintResult {
    coverage: PatternBitSet,
    solution_count: usize,
    witnessed_pattern_count: u128,
}

fn append_build_symmetry_fields(
    replacements: &mut Vec<(String, String)>,
    passes: &[PassConstraintResult],
    weights: &WeightedPatternSet,
) {
    let Some(original) = passes.first() else {
        return;
    };
    replacements.push(field(
        "original_covered_pattern_count",
        original.coverage.count_ones(),
    ));
    replacements.push(field(
        "original_coverage_probability",
        coverage_probability(weights, &original.coverage),
    ));
    replacements.push(field(
        "original_unique_solution_count",
        original.solution_count,
    ));
    if let Some(mirror) = passes.get(1) {
        replacements.push(field(
            "mirror_covered_pattern_count",
            mirror.coverage.count_ones(),
        ));
        replacements.push(field(
            "mirror_coverage_probability",
            coverage_probability(weights, &mirror.coverage),
        ));
        replacements.push(field("mirror_unique_solution_count", mirror.solution_count));
    }
}

fn coverage_probability(weights: &WeightedPatternSet, coverage: &PatternBitSet) -> String {
    canonical_probability(
        weights
            .covered_weight(coverage)
            .expect("pass coverage uses the materialized pattern universe")
            .get(),
    )
}

fn spin_profile_id(selection: SpinProfileSelection) -> SpinProfileId {
    match selection {
        SpinProfileSelection::TSpins => SpinProfileId::TSpins,
        SpinProfileSelection::TSpinsPlus => SpinProfileId::TSpinsPlus,
        SpinProfileSelection::AllSpin => SpinProfileId::AllSpin,
        SpinProfileSelection::AllSpinPlus => SpinProfileId::AllSpinPlus,
        SpinProfileSelection::AllMini => SpinProfileId::AllMini,
        SpinProfileSelection::AllMiniPlus => SpinProfileId::AllMiniPlus,
    }
}

fn canonical_probability(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else if value == 1.0 {
        "1".to_owned()
    } else {
        value.to_string()
    }
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        piece::{piece_kind::PieceKind, rotation::RotationState},
        solution::normalized_tiling_solution::{
            normalized_tiling_solution_key_set_hash_from_sorted_strings,
            StandardBoard64TilingIdentity,
        },
    };
    use clearra_core_executor::{
        normalized_solution_probability_reports, solution_probability::probability_reports,
        CoreExecutionError, CoreExecutionResult, CorePathStep, FinesseReport,
        NormalizedSolutionCoverage, SolutionAverageScoreReport, SolutionCoverage,
    };
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
    };
    use clearra_postprocess::TSpinCoverageOnlyMaterializer;
    use clearra_replay::{
        ScoringExecutionEdge, ScoringExecutionNode, ScoringLockEvidence,
        SpinCoverageExecutionBatch, SpinCoverageExecutionGraph,
    };

    use clearra_problem::BuildSolutionProbabilityPolicy;

    use super::{
        apply_build_execution_constraints, apply_build_worker_execution_constraints,
        apply_execution_constraints, apply_execution_constraints_with_memory_guard,
        authoritative_solution_coverages, checked_merge_candidate_coverages_projection,
        merge_candidate_coverages, try_owned_string_with_memory_guard,
    };

    #[test]
    fn disabled_constraint_is_a_zero_future_noop() {
        let input = CoreExecutionResult::new(
            vec![(
                "execution_constraint_preserve_b2b".to_owned(),
                "false".to_owned(),
            )],
            Vec::new(),
        );
        let expected = input.clone();
        let mut observed = Vec::new();
        let actual = apply_execution_constraints_with_memory_guard(
            input,
            &ExecutionControl::default(),
            &mut |_, future| {
                observed.push(future);
                Ok(())
            },
        )
        .expect("disabled constraint");
        assert_eq!(actual, expected);
        assert_eq!(observed, vec![0]);
    }

    #[test]
    fn vacuous_constraint_accepts_exact_observed_peak_and_rejects_peak_minus_one() {
        let mut peak = 0_u128;
        apply_execution_constraints_with_memory_guard(
            empty_partition_result(1, 0, Vec::new()),
            &ExecutionControl::default(),
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                peak = peak.max(required);
                Ok(())
            },
        )
        .expect("dry vacuous constraint");
        assert!(peak > 0);

        apply_execution_constraints_with_memory_guard(
            empty_partition_result(1, 0, Vec::new()),
            &ExecutionControl::default(),
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                (required <= peak)
                    .then_some(())
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    })
            },
        )
        .expect("exact observed peak");

        let error = apply_execution_constraints_with_memory_guard(
            empty_partition_result(1, 0, Vec::new()),
            &ExecutionControl::default(),
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                (required < peak)
                    .then_some(())
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    })
            },
        )
        .expect_err("peak minus one");
        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "test_memory_cap"
            }
        );
    }

    #[test]
    fn coverage_merge_accepts_exact_actual_peak_rejects_peak_minus_one_and_keeps_sparse_inputs_lazy(
    ) {
        let pattern_count = 1;
        let allowed = PatternBitSet::from_words(pattern_count, vec![1])
            .expect("one sparse authoritative pattern");
        let authoritative = vec![("keep".to_owned(), allowed)];
        let batch = SpinCoverageExecutionBatch::new(
            vec![vec![PieceKind::I]],
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            vec![b2b_graph("keep", 0)],
            true,
        );
        let materialized = TSpinCoverageOnlyMaterializer::materialize_all_spin_paths(
            &batch,
            0..pattern_count,
            &ExecutionControl::default(),
        )
        .expect("one candidate-pattern coverage");
        assert_eq!(materialized.candidate_coverages().len(), 1);
        let authoritative_storage_before = authoritative[0]
            .1
            .checked_storage_retained_bytes()
            .expect("authoritative storage");
        let source_storage_before = materialized.candidate_coverages()[0]
            .covered_patterns()
            .checked_storage_retained_bytes()
            .expect("materialized storage");

        let mut accepted = Vec::new();
        accepted
            .try_reserve_exact(authoritative.len())
            .expect("accepted table");
        let projection = checked_merge_candidate_coverages_projection(
            &accepted,
            materialized.candidate_coverages(),
            pattern_count,
        )
        .expect("checked merge projection");
        let mut observed = Vec::new();
        let pass = merge_candidate_coverages(
            &mut accepted,
            materialized.candidate_coverages(),
            &authoritative,
            pattern_count,
            |actual| {
                observed.push(actual);
                Ok(())
            },
        )
        .expect("unbounded coverage merge");
        assert_eq!(pass.solution_count, 1);
        assert_eq!(pass.coverage.count_ones(), 1);
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].1.count_ones(), 1);
        let peak = observed.into_iter().max().expect("actual merge guard");
        assert!(peak > 0);
        assert!(projection >= peak);
        assert_eq!(
            authoritative[0]
                .1
                .checked_storage_retained_bytes()
                .expect("authoritative storage after merge"),
            authoritative_storage_before
        );
        assert_eq!(
            materialized.candidate_coverages()[0]
                .covered_patterns()
                .checked_storage_retained_bytes()
                .expect("materialized storage after merge"),
            source_storage_before
        );

        let mut exact_accepted = Vec::new();
        exact_accepted
            .try_reserve_exact(authoritative.len())
            .expect("exact accepted table");
        merge_candidate_coverages(
            &mut exact_accepted,
            materialized.candidate_coverages(),
            &authoritative,
            pattern_count,
            |actual| {
                (actual <= peak)
                    .then_some(())
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    })
            },
        )
        .expect("exact actual merge peak");

        let mut rejected_accepted = Vec::new();
        rejected_accepted
            .try_reserve_exact(authoritative.len())
            .expect("rejected accepted table");
        let error = match merge_candidate_coverages(
            &mut rejected_accepted,
            materialized.candidate_coverages(),
            &authoritative,
            pattern_count,
            |actual| {
                (actual < peak)
                    .then_some(())
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    })
            },
        ) {
            Ok(_) => panic!("actual merge peak minus one"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "test_memory_cap",
            }
        );
    }

    #[test]
    fn duplicate_authoritative_union_accepts_exact_peak_and_rejects_peak_minus_one() {
        let pattern_count = 128;
        let result = CoreExecutionResult::default().with_normalized_solution_coverages(vec![
            NormalizedSolutionCoverage::new(
                "duplicate",
                PatternBitSet::from_words(pattern_count, vec![1, 0]).expect("first coverage"),
            ),
            NormalizedSolutionCoverage::new(
                "duplicate",
                PatternBitSet::from_words(pattern_count, vec![0, 2]).expect("second coverage"),
            ),
        ]);
        let mut observed = Vec::new();
        let merged =
            authoritative_solution_coverages(&result, pattern_count, &mut |live, future| {
                observed.push(
                    live.checked_resource_retained_bytes()
                        .and_then(|bytes| bytes.checked_add(future))
                        .expect("checked authority guard input"),
                );
                Ok(())
            })
            .expect("duplicate authority union");
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].1.count_ones(), 2);
        let peak = observed.into_iter().max().expect("authority guard peak");
        drop(merged);

        authoritative_solution_coverages(&result, pattern_count, &mut |live, future| {
            let required = live
                .checked_resource_retained_bytes()
                .and_then(|bytes| bytes.checked_add(future))
                .expect("checked exact authority guard input");
            (required <= peak)
                .then_some(())
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "test_memory_cap",
                })
        })
        .expect("exact duplicate authority peak");

        let error =
            authoritative_solution_coverages(&result, pattern_count, &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked rejected authority guard input");
                (required < peak)
                    .then_some(())
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    })
            })
            .expect_err("duplicate authority peak minus one");
        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "test_memory_cap",
            }
        );
    }

    #[test]
    fn arbitrary_minimum_cover_reason_guards_requested_and_actual_capacity() {
        let mut reason = String::with_capacity(16_511);
        reason.push_str(&"minimum-cover-blocked:".repeat(257));
        let result = CoreExecutionResult::new(
            vec![("minimum_cover_incomplete_reason".to_owned(), reason)],
            Vec::new(),
        );
        let source = result
            .field("minimum_cover_incomplete_reason")
            .expect("arbitrary blocking reason");
        let already_retained_future = 97_u128;
        let mut observed = Vec::new();
        let owned = try_owned_string_with_memory_guard(
            &result,
            already_retained_future,
            source,
            &mut |live, future| {
                observed.push(
                    live.checked_resource_retained_bytes()
                        .and_then(|bytes| bytes.checked_add(future))
                        .expect("checked reason guard input"),
                );
                Ok(())
            },
        )
        .expect("guarded arbitrary reason");
        assert_eq!(owned, source);
        assert_eq!(observed.len(), 2);
        let live = result
            .checked_resource_retained_bytes()
            .expect("reason result live bytes");
        assert_eq!(
            observed[0],
            live + already_retained_future + source.len() as u128
        );
        assert_eq!(
            observed[1],
            live + already_retained_future + owned.capacity() as u128
        );
        let peak = *observed.iter().max().expect("reason guard peak");
        drop(owned);

        try_owned_string_with_memory_guard(
            &result,
            already_retained_future,
            source,
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked exact reason guard input");
                (required <= peak)
                    .then_some(())
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    })
            },
        )
        .expect("exact reason peak");

        let error = try_owned_string_with_memory_guard(
            &result,
            already_retained_future,
            source,
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked rejected reason guard input");
                (required < peak)
                    .then_some(())
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    })
            },
        )
        .expect_err("reason peak minus one");
        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "test_memory_cap",
            }
        );

        let mut guard_called = false;
        let error = try_owned_string_with_memory_guard(&result, u128::MAX, source, &mut |_, _| {
            guard_called = true;
            Ok(())
        })
        .expect_err("reason projection overflow");
        assert!(!guard_called);
        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_memory_projection_overflow",
            }
        );
    }

    #[test]
    fn proven_empty_worker_partition_materializes_as_an_empty_b2b_result() {
        let result = apply_execution_constraints(
            empty_partition_result(1, 0, Vec::new()),
            &ExecutionControl::default(),
        )
        .expect("empty worker partition");

        assert_eq!(
            result.field("execution_constraint_materialized"),
            Some("true")
        );
        assert_eq!(result.usize_field("b2b_preserving_solution_count"), Some(0));
        assert_eq!(result.usize_field("covered_pattern_count"), Some(0));
        assert!(result.normalized_solution_coverages().is_empty());
        assert_eq!(result.spin_coverage_execution_batches().len(), 1);
        assert!(result.spin_coverage_execution_batches()[0]
            .graphs()
            .is_empty());
    }

    #[test]
    fn coverage_summary_empty_worker_partition_materializes_as_an_empty_b2b_result() {
        let result = apply_execution_constraints(
            coverage_summary_empty_partition_result(),
            &ExecutionControl::default(),
        )
        .expect("empty coverage-summary worker partition");

        assert_eq!(
            result.field("execution_constraint_materialized"),
            Some("true")
        );
        assert_eq!(result.usize_field("b2b_preserving_solution_count"), Some(0));
        assert_eq!(result.usize_field("covered_pattern_count"), Some(0));
        assert!(result.normalized_solution_coverages().is_empty());
    }

    #[test]
    fn materialized_coverage_summary_is_not_filtered_again_by_an_empty_regenerated_batch() {
        let identity = StandardBoard64TilingIdentity::from_placements(0, std::iter::empty())
            .expect("empty identity");
        let patterns = PatternBitSet::from_words(1, vec![1]).expect("coverage bitset");
        let coverage = NormalizedSolutionCoverage::new("preserved-candidate", patterns.clone());
        let board64_coverage = SolutionCoverage::new(identity, patterns.clone());
        let probabilities = probability_reports(
            &[identity],
            std::slice::from_ref(&board64_coverage),
            &WeightedPatternSet::uniform(1).expect("uniform weights"),
            true,
        );
        let input = coverage_summary_empty_partition_result()
            .with_replaced_fields(vec![
                (
                    "execution_constraint_materialized".to_owned(),
                    "true".to_owned(),
                ),
                ("solution_found".to_owned(), "true".to_owned()),
                ("covered_pattern_count".to_owned(), "1".to_owned()),
            ])
            .with_packing_candidate_keys(vec!["packing-candidate".to_owned()])
            .with_path_steps(vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)])
            .with_representative_solution_identity(Some(identity))
            .with_normalized_solution_keys(vec!["preserved-candidate".to_owned()])
            .with_normalized_solution_identities(vec![identity])
            .with_coverage_pattern_words(vec![1])
            .with_solution_coverages(vec![board64_coverage])
            .with_normalized_solution_coverages(vec![coverage])
            .with_solution_probabilities(probabilities)
            .with_solution_average_scores(vec![SolutionAverageScoreReport::new(
                "preserved-candidate",
                "100",
                1,
                1,
                true,
            )]);
        let expected = input.clone();

        let result = apply_execution_constraints(input, &ExecutionControl::default())
            .expect("materialized coverage-summary result");

        assert_eq!(result, expected);
        assert!(!result.normalized_solution_coverages().is_empty());
        assert!(!result.solution_probabilities().is_empty());
        assert!(!result.solution_average_scores().is_empty());
    }

    #[test]
    fn b2b_filter_rewrites_the_rich_solution_contract_as_one_atomic_tuple() {
        let covered = PatternBitSet::from_words(1, vec![1]).expect("coverage bitset");
        let input = CoreExecutionResult::new(
            vec![
                ("search_output_policy".to_owned(), "summary".to_owned()),
                (
                    "execution_constraint_preserve_b2b".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "execution_constraint_spin_profile".to_owned(),
                    "t-spins".to_owned(),
                ),
                (
                    "execution_constraint_materialized".to_owned(),
                    "false".to_owned(),
                ),
                ("target_piece_count".to_owned(), "1".to_owned()),
                ("coverage_pattern_count".to_owned(), "1".to_owned()),
                ("covered_pattern_count".to_owned(), "1".to_owned()),
                ("solution_found".to_owned(), "true".to_owned()),
                ("unique_solution_count".to_owned(), "2".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "2".to_owned(),
                ),
                (
                    "actual_normalized_unique_solution_count".to_owned(),
                    "2".to_owned(),
                ),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "2".to_owned(),
                ),
                ("solution_keys_complete".to_owned(), "true".to_owned()),
                ("solution_page_available".to_owned(), "true".to_owned()),
                (
                    "normalized_solution_set_hash".to_owned(),
                    "stale-before-b2b".to_owned(),
                ),
                (
                    "actual_normalized_solution_set_hash".to_owned(),
                    "stale-before-b2b".to_owned(),
                ),
                ("objective".to_owned(), "unique".to_owned()),
                ("objective_search_complete".to_owned(), "true".to_owned()),
                ("count_complete".to_owned(), "true".to_owned()),
                ("probability_complete".to_owned(), "true".to_owned()),
                (
                    "postprocess_scoring_requested".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "solution_probabilities_requested".to_owned(),
                    "false".to_owned(),
                ),
            ],
            Vec::new(),
        )
        .with_packing_candidate_keys(vec!["keep".to_owned(), "reject".to_owned()])
        .with_normalized_solution_keys(vec!["keep".to_owned(), "reject".to_owned()])
        .with_normalized_solution_coverages(vec![
            NormalizedSolutionCoverage::new("keep", covered.clone()),
            NormalizedSolutionCoverage::new("reject", covered),
        ])
        .with_coverage_pattern_words(vec![1])
        .with_solution_average_scores(vec![SolutionAverageScoreReport::new(
            "reject", "100", 1, 1, true,
        )])
        .with_finesse_report(FinesseReport::new(
            "search",
            "oracle",
            true,
            None,
            Vec::new(),
        ))
        .with_spin_coverage_execution_batch(Some(SpinCoverageExecutionBatch::new(
            vec![vec![PieceKind::I]],
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            vec![b2b_graph("keep", 0), b2b_graph("reject", 1)],
            true,
        )))
        .with_postprocess_execution_batch(Vec::new(), true, vec!["1".to_owned()]);

        let result = apply_execution_constraints(input, &ExecutionControl::default())
            .expect("B2B constraint");
        let expected_hash =
            normalized_tiling_solution_key_set_hash_from_sorted_strings(&["keep".to_owned()]);

        assert_eq!(result.normalized_solution_keys(), ["keep"]);
        assert_eq!(
            result.field("b2b_preservation_selection"),
            Some("existential")
        );
        assert_eq!(
            result.field("b2b_preservation_denominator_semantics"),
            Some("original-materialized-queue")
        );
        assert_eq!(
            result.usize_field("b2b_preservation_pattern_universe_count"),
            Some(1)
        );
        assert_eq!(result.usize_field("b2b_preserving_pattern_count"), Some(1));
        assert_eq!(result.field("b2b_preservation_probability"), Some("1"));
        assert_eq!(
            result.bool_field("b2b_preservation_count_complete"),
            Some(true)
        );
        assert_eq!(
            result.bool_field("b2b_preservation_probability_complete"),
            Some(true)
        );
        assert_eq!(
            result.bool_field("b2b_preservation_path_multiplicity_counted"),
            Some(false)
        );
        assert_eq!(
            result.bool_field("b2b_preservation_witness_available"),
            Some(true)
        );
        assert_eq!(
            result.field("b2b_preservation_witness_kind"),
            Some("candidate-pattern")
        );
        assert_eq!(
            result.field("b2b_preservation_witness_candidate_key"),
            Some("keep")
        );
        assert_eq!(
            result.usize_field("b2b_preservation_witness_pattern_index"),
            Some(0)
        );
        assert_eq!(
            result.field("b2b_preservation_witness_pattern_semantics"),
            Some("original-queue-index")
        );
        for key in [
            "unique_solution_count",
            "normalized_unique_solution_count",
            "actual_normalized_unique_solution_count",
            "solution_keys_materialized_count",
        ] {
            assert_eq!(result.usize_field(key), Some(1), "{key}");
        }
        assert_eq!(
            result.field("normalized_solution_set_hash"),
            Some(expected_hash.as_str())
        );
        assert_eq!(
            result.field("actual_normalized_solution_set_hash"),
            Some(expected_hash.as_str())
        );
        assert_eq!(result.bool_field("solution_count_calculated"), Some(true));
        assert_eq!(result.bool_field("solution_set_materialized"), Some(true));
        assert_eq!(result.bool_field("solution_keys_complete"), Some(true));
        assert_eq!(result.bool_field("solution_page_available"), Some(false));
        assert!(result.packing_candidate_keys().is_empty());
        assert!(result.solution_average_scores().is_empty());
        assert!(result.finesse_report().is_none());
        assert!(result.tiling_solution_page_store().is_none());
        let availability = result.execution_report().solution_set_availability();
        assert!(availability.contract_valid());
        assert!(
            availability.materialized_key_count_matches(result.normalized_solution_keys().len())
        );
    }

    #[test]
    fn b2b_minimum_cover_keeps_original_candidate_lex_first_identity() {
        let coverage_a = PatternBitSet::from_words(4, vec![0b1110]).expect("candidate a coverage");
        let coverage_b = PatternBitSet::from_words(4, vec![0b0001])
            .expect("properly dominated candidate b coverage");
        let coverage_c = PatternBitSet::from_words(4, vec![0b0011]).expect("candidate c coverage");
        let input = CoreExecutionResult::new(
            vec![
                ("search_output_policy".to_owned(), "summary".to_owned()),
                (
                    "execution_constraint_preserve_b2b".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "execution_constraint_spin_profile".to_owned(),
                    "t-spins".to_owned(),
                ),
                (
                    "execution_constraint_materialized".to_owned(),
                    "false".to_owned(),
                ),
                ("target_piece_count".to_owned(), "1".to_owned()),
                ("coverage_pattern_count".to_owned(), "4".to_owned()),
                ("covered_pattern_count".to_owned(), "4".to_owned()),
                ("solution_found".to_owned(), "true".to_owned()),
                ("unique_solution_count".to_owned(), "3".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "3".to_owned(),
                ),
                (
                    "actual_normalized_unique_solution_count".to_owned(),
                    "3".to_owned(),
                ),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "3".to_owned(),
                ),
                ("solution_keys_complete".to_owned(), "true".to_owned()),
                ("solution_page_available".to_owned(), "true".to_owned()),
                ("objective".to_owned(), "minimum-cover".to_owned()),
                ("objective_search_complete".to_owned(), "true".to_owned()),
                ("count_complete".to_owned(), "true".to_owned()),
                ("probability_complete".to_owned(), "true".to_owned()),
                ("minimum_cover_complete".to_owned(), "true".to_owned()),
                ("minimum_cover_proven_minimum".to_owned(), "true".to_owned()),
                (
                    "minimum_cover_incomplete_reason".to_owned(),
                    "none".to_owned(),
                ),
                (
                    "postprocess_scoring_requested".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "solution_probabilities_requested".to_owned(),
                    "false".to_owned(),
                ),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(vec![
            "solution-a".to_owned(),
            "solution-b".to_owned(),
            "solution-c".to_owned(),
        ])
        .with_normalized_solution_coverages(vec![
            NormalizedSolutionCoverage::new("solution-a", coverage_a),
            NormalizedSolutionCoverage::new("solution-b", coverage_b),
            NormalizedSolutionCoverage::new("solution-c", coverage_c),
        ])
        .with_coverage_pattern_words(vec![0b1111])
        .with_spin_coverage_execution_batch(Some(SpinCoverageExecutionBatch::new(
            vec![
                vec![PieceKind::I],
                vec![PieceKind::O],
                vec![PieceKind::T],
                vec![PieceKind::S],
            ],
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            vec![
                b2b_piece_subset_graph(
                    1,
                    "solution-a",
                    &[PieceKind::O, PieceKind::T, PieceKind::S],
                ),
                b2b_piece_subset_graph(2, "solution-b", &[PieceKind::I]),
                b2b_piece_subset_graph(3, "solution-c", &[PieceKind::I, PieceKind::O]),
            ],
            true,
        )))
        .with_postprocess_execution_batch(Vec::new(), true, vec!["0.25".to_owned(); 4]);

        let result = apply_execution_constraints(input.clone(), &ExecutionControl::default())
            .expect("B2B minimum cover with a properly dominated original candidate");

        // Both [a,b] and [a,c] are minimum covers. The proof may discard b as
        // dominated by c, but the public result must retain the original-row
        // lex-first candidate identity [a,b].
        assert_eq!(
            result.normalized_solution_keys(),
            ["solution-a", "solution-b"]
        );
        assert_eq!(
            result.usize_field("minimum_cover_selected_solution_count"),
            Some(2)
        );
        assert_eq!(result.bool_field("minimum_cover_complete"), Some(true));
        assert_eq!(
            result.bool_field("minimum_cover_proven_minimum"),
            Some(true)
        );

        let mut peak = 0_u128;
        let guarded = apply_execution_constraints_with_memory_guard(
            input.clone(),
            &ExecutionControl::default(),
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked canonical minimum-cover guard input");
                peak = peak.max(required);
                Ok(())
            },
        )
        .expect("guarded B2B canonical minimum cover");
        assert_eq!(guarded, result);
        assert!(peak > 0);

        apply_execution_constraints_with_memory_guard(
            input.clone(),
            &ExecutionControl::default(),
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked exact canonical minimum-cover guard input");
                (required <= peak)
                    .then_some(())
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    })
            },
        )
        .expect("exact observed canonical minimum-cover peak");

        let error = apply_execution_constraints_with_memory_guard(
            input,
            &ExecutionControl::default(),
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked rejected canonical minimum-cover guard input");
                (required < peak)
                    .then_some(())
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    })
            },
        )
        .expect_err("canonical minimum-cover peak minus one");
        assert_eq!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "test_memory_cap"
            }
        );
    }

    #[test]
    fn extended_b2b_filter_rebuilds_requested_probabilities_without_board64_identities() {
        let key = "ctk2|height=7|extended-candidate";
        let covered = PatternBitSet::from_words(1, vec![1]).expect("coverage bitset");
        let weights = WeightedPatternSet::uniform(1).expect("uniform weights");
        let normalized_coverage = vec![NormalizedSolutionCoverage::new(key, covered.clone())];
        let initial_reports = normalized_solution_probability_reports(
            &[key.to_owned()],
            &normalized_coverage,
            &weights,
            true,
        )
        .expect("initial probability report");
        let input = CoreExecutionResult::new(
            vec![
                ("search_kind".to_owned(), "build-probability".to_owned()),
                ("search_output_policy".to_owned(), "summary".to_owned()),
                (
                    "execution_constraint_preserve_b2b".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "execution_constraint_spin_profile".to_owned(),
                    "t-spins".to_owned(),
                ),
                (
                    "execution_constraint_materialized".to_owned(),
                    "false".to_owned(),
                ),
                ("target_piece_count".to_owned(), "1".to_owned()),
                ("coverage_pattern_count".to_owned(), "1".to_owned()),
                ("covered_pattern_count".to_owned(), "1".to_owned()),
                ("solution_found".to_owned(), "true".to_owned()),
                ("unique_solution_count".to_owned(), "1".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                (
                    "actual_normalized_unique_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "1".to_owned(),
                ),
                ("solution_keys_complete".to_owned(), "true".to_owned()),
                ("solution_page_available".to_owned(), "false".to_owned()),
                ("objective".to_owned(), "unique".to_owned()),
                ("objective_search_complete".to_owned(), "true".to_owned()),
                ("count_complete".to_owned(), "true".to_owned()),
                ("probability_complete".to_owned(), "true".to_owned()),
                ("resource_truncated".to_owned(), "false".to_owned()),
                (
                    "postprocess_scoring_requested".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "solution_probabilities_requested".to_owned(),
                    "true".to_owned(),
                ),
                ("solution_probability_count".to_owned(), "1".to_owned()),
                (
                    "solution_probability_complete".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "solution_probability_basis".to_owned(),
                    "normalized-solution-pattern-bitset-or-union".to_owned(),
                ),
                (
                    "solution_probability_incomplete_reason".to_owned(),
                    "none".to_owned(),
                ),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(vec![key.to_owned()])
        .with_normalized_solution_coverages(normalized_coverage)
        .with_solution_probabilities(initial_reports)
        .with_coverage_pattern_words(vec![1])
        .with_spin_coverage_execution_batch(Some(SpinCoverageExecutionBatch::new(
            vec![vec![PieceKind::I]],
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            vec![b2b_graph(key, 0)],
            true,
        )))
        .with_postprocess_execution_batch(Vec::new(), true, vec!["1".to_owned()]);

        let worker_fields = input
            .summary_fields()
            .into_iter()
            .filter(|(field, _)| {
                !matches!(
                    field.as_str(),
                    "solution_probability_count"
                        | "solution_probability_complete"
                        | "solution_probability_basis"
                        | "solution_probability_incomplete_reason"
                        | "solution_keys_materialized_count"
                        | "solution_keys_complete"
                )
            })
            .collect();
        let worker_partial = rebuild_extended_probability_input(&input, worker_fields)
            .with_solution_probabilities(Vec::new());
        let materialized_worker = apply_build_worker_execution_constraints(
            worker_partial.clone(),
            BuildSolutionProbabilityPolicy::Include,
            &ExecutionControl::default(),
        )
        .expect("validated worker authority may materialize its first final tuple");
        assert!(materialized_worker.solution_probabilities().is_empty());
        for final_field in [
            "solution_probability_count",
            "solution_probability_complete",
            "solution_probability_basis",
            "solution_probability_incomplete_reason",
        ] {
            assert_eq!(materialized_worker.field_occurrence_count(final_field), 0);
        }
        let worker_policy_mismatch = apply_build_worker_execution_constraints(
            worker_partial.clone(),
            BuildSolutionProbabilityPolicy::Omit,
            &ExecutionControl::default(),
        )
        .expect_err("worker request policy mismatch must fail closed");
        assert!(matches!(
            worker_policy_mismatch,
            CoreExecutionError::RuntimeUnavailable {
                component: "build_solution_probability_input_policy_mismatch"
            }
        ));
        let missing_worker_resource_fields = worker_partial
            .summary_fields()
            .into_iter()
            .filter(|(field, _)| field != "resource_truncated")
            .collect();
        let missing_worker_resource =
            rebuild_extended_probability_input(&worker_partial, missing_worker_resource_fields)
                .with_solution_probabilities(Vec::new());
        let missing_worker_resource = apply_build_worker_execution_constraints(
            missing_worker_resource,
            BuildSolutionProbabilityPolicy::Include,
            &ExecutionControl::default(),
        )
        .expect_err("missing worker resource authority must not be defaulted");
        assert!(matches!(
            missing_worker_resource,
            CoreExecutionError::RuntimeUnavailable {
                component:
                    "build_solution_probability_input_resource_truncated_missing_or_duplicate"
            }
        ));
        let already_materialized_worker = worker_partial.clone().with_replaced_fields(vec![(
            "execution_constraint_materialized".to_owned(),
            "true".to_owned(),
        )]);
        let already_materialized_worker = apply_build_worker_execution_constraints(
            already_materialized_worker,
            BuildSolutionProbabilityPolicy::Include,
            &ExecutionControl::default(),
        )
        .expect_err("worker partial cannot bypass first tuple materialization");
        assert!(matches!(
            already_materialized_worker,
            CoreExecutionError::RuntimeUnavailable {
                component: "build_solution_probability_worker_partial_already_materialized"
            }
        ));
        let worker_with_final_key_authority =
            worker_partial.clone().with_additional_fields(vec![(
                "solution_keys_complete".to_owned(),
                "false".to_owned(),
            )]);
        let worker_with_final_key_authority = apply_build_worker_execution_constraints(
            worker_with_final_key_authority,
            BuildSolutionProbabilityPolicy::Include,
            &ExecutionControl::default(),
        )
        .expect_err("raw worker must not carry final-only key completeness authority");
        assert!(matches!(
            worker_with_final_key_authority,
            CoreExecutionError::RuntimeUnavailable {
                component: "build_solution_probability_worker_partial_final_field_present"
            }
        ));
        for malformed_worker in [
            worker_partial
                .clone()
                .with_additional_fields(vec![("unique_solution_count".to_owned(), "1".to_owned())]),
            worker_partial
                .clone()
                .with_replaced_fields(vec![("coverage_pattern_count".to_owned(), "01".to_owned())]),
        ] {
            let error = apply_build_worker_execution_constraints(
                malformed_worker,
                BuildSolutionProbabilityPolicy::Include,
                &ExecutionControl::default(),
            )
            .expect_err("worker count authority must be unique and canonically typed");
            assert!(matches!(
                error,
                CoreExecutionError::RuntimeUnavailable {
                    component
                } if component.starts_with("build_solution_probability_")
            ));
        }

        let missing_basis_fields = input
            .summary_fields()
            .into_iter()
            .filter(|(field, _)| field != "solution_probability_basis")
            .collect();
        let missing_basis = rebuild_extended_probability_input(&input, missing_basis_fields);
        let duplicate_metadata = input.clone().with_additional_fields(vec![(
            "solution_probability_count".to_owned(),
            "1".to_owned(),
        )]);
        let wrong_typed_input = input
            .clone()
            .with_replaced_fields(vec![("count_complete".to_owned(), "yes".to_owned())]);
        let wrong_report_binding = input.clone().with_normalized_solution_coverages(vec![
            NormalizedSolutionCoverage::new(
                key,
                PatternBitSet::from_words(1, vec![0]).expect("mutated coverage bitset"),
            ),
        ]);
        for malformed in [
            missing_basis,
            duplicate_metadata,
            wrong_typed_input,
            wrong_report_binding,
        ] {
            let error = apply_build_execution_constraints(
                malformed,
                BuildSolutionProbabilityPolicy::Include,
                &ExecutionControl::default(),
            )
            .expect_err("malformed Build probability input must not be laundered by B2B");
            assert!(matches!(
                error,
                CoreExecutionError::RuntimeUnavailable {
                    component
                } if component.starts_with("build_solution_probability_input_")
            ));
        }
        let policy_mismatch = apply_build_execution_constraints(
            input.clone(),
            BuildSolutionProbabilityPolicy::Omit,
            &ExecutionControl::default(),
        )
        .expect_err("B2B must preserve the expected Build request policy");
        assert!(matches!(
            policy_mismatch,
            CoreExecutionError::RuntimeUnavailable {
                component: "build_solution_probability_input_policy_mismatch"
            }
        ));

        let partial_reports = normalized_solution_probability_reports(
            input.normalized_solution_keys(),
            input.normalized_solution_coverages(),
            &weights,
            false,
        )
        .expect("partial input reports");
        let partial = input
            .clone()
            .with_solution_probabilities(partial_reports)
            .with_replaced_fields(vec![
                ("count_complete".to_owned(), "false".to_owned()),
                (
                    "solution_probability_complete".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "solution_probability_incomplete_reason".to_owned(),
                    "solution-count-incomplete".to_owned(),
                ),
            ]);
        let partial = apply_build_execution_constraints(
            partial,
            BuildSolutionProbabilityPolicy::Include,
            &ExecutionControl::default(),
        )
        .expect("legitimate partial Build result remains public and incomplete");
        assert_eq!(
            partial.field("solution_probability_incomplete_reason"),
            Some("solution-count-incomplete")
        );
        assert_eq!(
            partial.bool_field("solution_probability_complete"),
            Some(false)
        );
        assert!(partial
            .solution_probabilities()
            .iter()
            .all(|report| !report.probability_complete()));

        let result = apply_build_execution_constraints(
            input,
            BuildSolutionProbabilityPolicy::Include,
            &ExecutionControl::default(),
        )
        .expect("extended B2B solution probability projection");

        assert!(result.normalized_solution_identities().is_empty());
        assert_eq!(result.normalized_solution_keys(), [key]);
        assert_eq!(result.solution_probabilities().len(), 1);
        let report = &result.solution_probabilities()[0];
        assert_eq!(report.solution_key(), key);
        assert_eq!(report.probability(), "1");
        assert_eq!(report.covered_pattern_count(), 1);
        assert_eq!(report.pattern_count(), 1);
        assert!(report.probability_complete());
        for (field, expected) in [
            ("solution_probabilities_requested", "true"),
            ("solution_probability_count", "1"),
            ("solution_probability_complete", "true"),
            (
                "solution_probability_basis",
                "normalized-solution-pattern-bitset-or-union",
            ),
            ("solution_probability_incomplete_reason", "none"),
        ] {
            assert_eq!(result.field_occurrence_count(field), 1, "{field}");
            assert_eq!(result.unique_field(field), Some(expected), "{field}");
        }
    }

    fn rebuild_extended_probability_input(
        source: &CoreExecutionResult,
        fields: Vec<(String, String)>,
    ) -> CoreExecutionResult {
        CoreExecutionResult::new(fields, Vec::new())
            .with_normalized_solution_keys(source.normalized_solution_keys().to_vec())
            .with_normalized_solution_coverages(source.normalized_solution_coverages().to_vec())
            .with_solution_probabilities(source.solution_probabilities().to_vec())
            .with_coverage_pattern_words(source.coverage_pattern_words().to_vec())
            .with_spin_coverage_execution_batches(source.spin_coverage_execution_batches().to_vec())
            .with_postprocess_execution_batch(
                source.postprocess_executions().to_vec(),
                source.postprocess_execution_complete(),
                source.postprocess_pattern_weights().to_vec(),
            )
    }

    #[test]
    fn empty_authority_remains_fail_closed_when_a_graph_claims_a_candidate() {
        let graph =
            SpinCoverageExecutionGraph::new(1, "uncovered-candidate", 0, Vec::new(), Vec::new());
        let error = apply_execution_constraints(
            empty_partition_result(1, 0, vec![graph]),
            &ExecutionControl::default(),
        )
        .expect_err("a graph without authoritative coverage must fail closed");

        assert!(matches!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_authoritative_coverage_missing"
            }
        ));
    }

    #[test]
    fn visible_seven_minimum_cover_incompleteness_is_never_promoted_by_b2b_filtering() {
        let reason = "visible-seven-policy-minimum-cover-not-materialized";
        let input = empty_partition_result(1, 0, Vec::new()).with_replaced_fields(vec![
            ("objective".to_owned(), "minimum-cover".to_owned()),
            ("minimum_cover_complete".to_owned(), "false".to_owned()),
            (
                "minimum_cover_proven_minimum".to_owned(),
                "false".to_owned(),
            ),
            (
                "minimum_cover_incomplete_reason".to_owned(),
                reason.to_owned(),
            ),
        ]);

        let result = apply_execution_constraints(input, &ExecutionControl::default())
            .expect("unsupported visible-seven objective must remain a typed incomplete result");

        assert_eq!(result.bool_field("minimum_cover_complete"), Some(false));
        assert_eq!(
            result.bool_field("minimum_cover_proven_minimum"),
            Some(false)
        );
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some(reason)
        );
        assert_eq!(result.bool_field("objective_complete"), Some(false));
        assert_eq!(result.field("objective_incomplete_reason"), Some(reason));
    }

    #[test]
    fn named_product_deferred_minimum_cover_is_not_reproved_by_b2b_filtering() {
        let reason = "deferred-to-coordinator";
        let input = empty_partition_result(1, 0, Vec::new()).with_replaced_fields(vec![
            ("objective".to_owned(), "minimum-cover".to_owned()),
            ("minimum_cover_complete".to_owned(), "false".to_owned()),
            (
                "minimum_cover_proven_minimum".to_owned(),
                "false".to_owned(),
            ),
            (
                "minimum_cover_incomplete_reason".to_owned(),
                reason.to_owned(),
            ),
        ]);

        let result = apply_execution_constraints(input, &ExecutionControl::default())
            .expect("named-product minimum cover remains deferred after B2B validation");

        assert_eq!(result.bool_field("minimum_cover_complete"), Some(false));
        assert_eq!(
            result.bool_field("minimum_cover_proven_minimum"),
            Some(false)
        );
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some(reason)
        );
        assert_eq!(result.bool_field("objective_complete"), Some(false));
        assert_eq!(result.field("objective_incomplete_reason"), Some(reason));
    }

    #[test]
    fn missing_minimum_cover_status_remains_fail_closed_after_b2b_filtering() {
        let input = empty_partition_result(1, 0, Vec::new())
            .with_replaced_fields(vec![("objective".to_owned(), "minimum-cover".to_owned())]);

        let result = apply_execution_constraints(input, &ExecutionControl::default())
            .expect("missing minimum-cover status must remain incomplete");

        assert_eq!(result.bool_field("minimum_cover_complete"), Some(false));
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some("minimum-cover-status-missing")
        );
        assert_eq!(result.bool_field("objective_complete"), Some(false));
    }

    fn b2b_graph(candidate_key: &str, cleared_lines: u8) -> SpinCoverageExecutionGraph {
        SpinCoverageExecutionGraph::new(
            1,
            candidate_key,
            0,
            vec![
                ScoringExecutionNode::new(0, 1, false),
                ScoringExecutionNode::new(1, 0, true),
            ],
            vec![ScoringExecutionEdge::new(
                1,
                0,
                PieceKind::I,
                RotationState::Zero,
                0,
                0,
                cleared_lines,
                0,
                0,
                ScoringLockEvidence::no_rotation(RotationState::Zero),
            )],
        )
    }

    fn b2b_piece_subset_graph(
        candidate_id: u64,
        candidate_key: &str,
        pieces: &[PieceKind],
    ) -> SpinCoverageExecutionGraph {
        SpinCoverageExecutionGraph::new(
            candidate_id,
            candidate_key,
            0,
            vec![
                ScoringExecutionNode::new(0, pieces.len() as u32, false),
                ScoringExecutionNode::new(pieces.len() as u32, 0, true),
            ],
            pieces
                .iter()
                .copied()
                .enumerate()
                .map(|(operation_index, piece)| {
                    ScoringExecutionEdge::new(
                        1,
                        operation_index as u8,
                        piece,
                        RotationState::Zero,
                        0,
                        0,
                        0,
                        0,
                        0,
                        ScoringLockEvidence::no_rotation(RotationState::Zero),
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn vacuous_target_accepts_an_empty_execution_batch_wrapper() {
        let result = apply_execution_constraints(
            empty_partition_result(0, 1, Vec::new()),
            &ExecutionControl::default(),
        )
        .expect("zero-piece B2B constraint");

        assert_eq!(
            result.field("execution_constraint_materialized"),
            Some("true")
        );
        assert_eq!(result.usize_field("b2b_preserving_solution_count"), Some(1));
        assert_eq!(result.field("objective_complete"), Some("true"));
    }

    #[test]
    fn vacuous_target_rejects_an_incomplete_empty_execution_batch_wrapper() {
        let error = apply_execution_constraints(
            empty_partition_result_with_batch_complete(0, 1, Vec::new(), false),
            &ExecutionControl::default(),
        )
        .expect_err("an incomplete zero-piece wrapper must fail closed");

        assert!(matches!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_vacuous_evidence_incomplete"
            }
        ));
    }

    #[test]
    fn vacuous_target_rejects_missing_execution_evidence() {
        let error = apply_execution_constraints(
            empty_partition_result(0, 1, Vec::new())
                .with_spin_coverage_execution_batches(Vec::new()),
            &ExecutionControl::default(),
        )
        .expect_err("a zero-piece result without execution evidence must fail closed");

        assert!(matches!(
            error,
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_vacuous_evidence_incomplete"
            }
        ));
    }

    #[test]
    fn vacuous_target_preserves_minimum_cover_blocking_reason() {
        let reason = "visible-seven-policy-minimum-cover-not-materialized";
        let input = empty_partition_result(0, 1, Vec::new()).with_replaced_fields(vec![
            ("objective".to_owned(), "minimum-cover".to_owned()),
            ("minimum_cover_complete".to_owned(), "false".to_owned()),
            (
                "minimum_cover_proven_minimum".to_owned(),
                "false".to_owned(),
            ),
            (
                "minimum_cover_incomplete_reason".to_owned(),
                reason.to_owned(),
            ),
        ]);

        let result = apply_execution_constraints(input, &ExecutionControl::default())
            .expect("vacuous B2B filtering must preserve the blocking status");

        assert_eq!(result.bool_field("minimum_cover_complete"), Some(false));
        assert_eq!(
            result.bool_field("minimum_cover_proven_minimum"),
            Some(false)
        );
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some(reason)
        );
        assert_eq!(result.bool_field("objective_complete"), Some(false));
        assert_eq!(result.field("objective_incomplete_reason"), Some(reason));
    }

    #[test]
    fn vacuous_target_fails_closed_when_minimum_cover_status_is_missing() {
        let input = empty_partition_result(0, 1, Vec::new())
            .with_replaced_fields(vec![("objective".to_owned(), "minimum-cover".to_owned())]);

        let result = apply_execution_constraints(input, &ExecutionControl::default())
            .expect("missing minimum-cover status remains a typed incomplete result");

        assert_eq!(result.bool_field("minimum_cover_complete"), Some(false));
        assert_eq!(
            result.bool_field("minimum_cover_proven_minimum"),
            Some(false)
        );
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some("minimum-cover-status-missing")
        );
        assert_eq!(result.bool_field("objective_complete"), Some(false));
        assert_eq!(
            result.field("objective_incomplete_reason"),
            Some("minimum-cover-status-missing")
        );
    }

    #[test]
    fn vacuous_target_accepts_a_complete_proven_minimum_cover_status() {
        let input = empty_partition_result(0, 1, Vec::new()).with_replaced_fields(vec![
            ("objective".to_owned(), "minimum-cover".to_owned()),
            ("minimum_cover_complete".to_owned(), "true".to_owned()),
            ("minimum_cover_proven_minimum".to_owned(), "true".to_owned()),
            (
                "minimum_cover_incomplete_reason".to_owned(),
                "none".to_owned(),
            ),
        ]);

        let result = apply_execution_constraints(input, &ExecutionControl::default())
            .expect("complete proven trivial minimum cover");

        assert_eq!(result.bool_field("minimum_cover_complete"), Some(true));
        assert_eq!(
            result.bool_field("minimum_cover_proven_minimum"),
            Some(true)
        );
        assert_eq!(
            result.field("minimum_cover_incomplete_reason"),
            Some("none")
        );
        assert_eq!(result.bool_field("objective_complete"), Some(true));
        assert_eq!(result.field("objective_incomplete_reason"), Some("none"));
    }

    fn empty_partition_result(
        target_piece_count: usize,
        solution_count: usize,
        graphs: Vec<SpinCoverageExecutionGraph>,
    ) -> CoreExecutionResult {
        empty_partition_result_with_batch_complete(target_piece_count, solution_count, graphs, true)
    }

    fn empty_partition_result_with_batch_complete(
        target_piece_count: usize,
        solution_count: usize,
        graphs: Vec<SpinCoverageExecutionGraph>,
        batch_complete: bool,
    ) -> CoreExecutionResult {
        let solution_found = solution_count != 0;
        CoreExecutionResult::new(
            vec![
                (
                    "execution_constraint_preserve_b2b".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "execution_constraint_spin_profile".to_owned(),
                    "t-spins".to_owned(),
                ),
                (
                    "execution_constraint_materialized".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "target_piece_count".to_owned(),
                    target_piece_count.to_string(),
                ),
                ("coverage_pattern_count".to_owned(), "1".to_owned()),
                ("covered_pattern_count".to_owned(), "0".to_owned()),
                (
                    "unique_solution_count".to_owned(),
                    solution_count.to_string(),
                ),
                ("solution_found".to_owned(), solution_found.to_string()),
                ("objective".to_owned(), "unique".to_owned()),
                ("objective_search_complete".to_owned(), "true".to_owned()),
                ("count_complete".to_owned(), "true".to_owned()),
                ("probability_complete".to_owned(), "true".to_owned()),
                (
                    "postprocess_scoring_requested".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "solution_probabilities_requested".to_owned(),
                    "false".to_owned(),
                ),
            ],
            Vec::new(),
        )
        .with_coverage_pattern_words(vec![0])
        .with_spin_coverage_execution_batch(Some(SpinCoverageExecutionBatch::new(
            vec![vec![PieceKind::I]],
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            graphs,
            batch_complete,
        )))
        .with_postprocess_execution_batch(Vec::new(), true, vec!["1".to_owned()])
    }

    fn coverage_summary_empty_partition_result() -> CoreExecutionResult {
        empty_partition_result(1, 0, Vec::new()).with_replaced_fields(vec![
            (
                "search_output_policy".to_owned(),
                "coverage-summary".to_owned(),
            ),
            (
                "unique_solution_count".to_owned(),
                "not-calculated".to_owned(),
            ),
            (
                "normalized_unique_solution_count".to_owned(),
                "not-calculated".to_owned(),
            ),
            ("solution_count_calculated".to_owned(), "false".to_owned()),
            ("solution_set_materialized".to_owned(), "false".to_owned()),
            (
                "solution_keys_materialized_count".to_owned(),
                "0".to_owned(),
            ),
            (
                "normalized_solution_set_hash".to_owned(),
                "not-calculated".to_owned(),
            ),
            (
                "actual_normalized_solution_set_hash".to_owned(),
                "not-calculated".to_owned(),
            ),
        ])
    }
}
