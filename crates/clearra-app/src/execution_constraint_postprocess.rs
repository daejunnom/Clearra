//! SRP rationale: this module has one behavior-level change reason: validating and materializing requested execution constraints, including fail-closed evidence proofs.

use std::collections::{BTreeMap, BTreeSet};

use clearra_core_domain::{
    probability::probability_value::ProbabilityValue,
    solution::normalized_tiling_solution::{
        normalized_tiling_solution_key_set_hash_from_sorted_strings,
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
    },
};
use clearra_core_executor::{
    solution_probability::probability_reports, CoreExecutionError, CoreExecutionResult,
    NormalizedSolutionCoverage, SolutionCoverage,
};
use clearra_coverage::cover::exact_minimum_cover::exact_minimum_cover;
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
};
use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;
use clearra_postprocess::{
    BackToBackExecutionFilter, CandidatePatternCoverage, TSpinCoverageOnlyMaterializer,
};
use clearra_replay::{ExactScoringExecutionBatch, SpinCoverageExecutionBatch};
use clearra_scoring::profile::SpinProfileId;

use clearra_core_domain::execution_cancellation::ExecutionControl;

pub(crate) fn apply_execution_constraints(
    result: CoreExecutionResult,
    control: &ExecutionControl,
) -> Result<CoreExecutionResult, CoreExecutionError> {
    if result.field("execution_constraint_preserve_b2b") != Some("true") {
        return Ok(result);
    }
    if control.is_cancelled() {
        return Err(CoreExecutionError::Cancelled);
    }
    let (minimum_cover_requested, minimum_cover_blocking_reason) =
        minimum_cover_input_status(&result);
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
            minimum_cover_blocking_reason
                .as_deref()
                .unwrap_or("search_incomplete")
        } else {
            "search_incomplete"
        };
        let mut replacements = vec![
            field("execution_constraint_materialized", true),
            field("b2b_preserving_solution_count", solution_count),
            field("b2b_preserving_candidate_pattern_count", 0),
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
                    minimum_cover_blocking_reason.as_deref().unwrap_or("none"),
                ),
            ]);
        }
        return Ok(result.with_replaced_fields(replacements));
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
    let authoritative_coverage = authoritative_solution_coverages(&result, pattern_count)?;
    validate_execution_graph_authority(&result, &authoritative_coverage)?;

    let mut accepted = BTreeMap::<String, PatternBitSet>::new();
    let mut pass_results = Vec::<PassConstraintResult>::new();
    let mut witnessed_pattern_count = 0_u128;
    let mut complete = true;
    let mut filtered_scoring_batches = Vec::new();
    let mut filtered_spin_batches = Vec::new();

    for batch in result.exact_scoring_execution_batches() {
        let filtered = BackToBackExecutionFilter::scoring_batch(batch, profile_id);
        let materialized = TSpinCoverageOnlyMaterializer::materialize_all_paths(
            &filtered,
            0..filtered.patterns().len(),
            control,
        )
        .map_err(|_| CoreExecutionError::Cancelled)?;
        complete &= materialized.complete();
        let pass = merge_candidate_coverages(
            &mut accepted,
            materialized.candidate_coverages(),
            &authoritative_coverage,
            pattern_count,
        )?;
        witnessed_pattern_count =
            witnessed_pattern_count.saturating_add(pass.witnessed_pattern_count);
        pass_results.push(pass);
        filtered_scoring_batches.push(filtered);
    }
    for batch in result.spin_coverage_execution_batches() {
        let filtered = BackToBackExecutionFilter::spin_batch(batch, profile_id);
        let materialized = TSpinCoverageOnlyMaterializer::materialize_all_spin_paths(
            &filtered,
            0..filtered.patterns().len(),
            control,
        )
        .map_err(|_| CoreExecutionError::Cancelled)?;
        complete &= materialized.complete();
        let pass = merge_candidate_coverages(
            &mut accepted,
            materialized.candidate_coverages(),
            &authoritative_coverage,
            pattern_count,
        )?;
        witnessed_pattern_count =
            witnessed_pattern_count.saturating_add(pass.witnessed_pattern_count);
        pass_results.push(pass);
        filtered_spin_batches.push(filtered);
    }
    if !complete {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_execution_evidence_incomplete",
        });
    }
    let weights = materialized_weights(&result, pattern_count)?;
    let probability_complete = result.bool_field("probability_complete").unwrap_or(false);
    let count_complete = result.bool_field("count_complete").unwrap_or(false);
    let minimum_cover_source_solution_count = accepted.len();
    let mut minimum_cover_complete = false;
    let mut minimum_cover_proven = false;
    let mut minimum_cover_reason = if let Some(reason) = &minimum_cover_blocking_reason {
        reason.clone()
    } else if minimum_cover_requested {
        "search_incomplete".to_owned()
    } else {
        "not_requested".to_owned()
    };
    if minimum_cover_requested
        && minimum_cover_blocking_reason.is_none()
        && count_complete
        && probability_complete
    {
        let required = coverage_union(accepted.values(), pattern_count)?;
        let candidate_keys = accepted.keys().cloned().collect::<Vec<_>>();
        let rows = candidate_keys
            .iter()
            .map(|key| accepted[key].clone())
            .collect::<Vec<_>>();
        let selection = exact_minimum_cover(&required, &rows).map_err(|_| {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_minimum_cover_universe_mismatch",
            }
        })?;
        if !selection.complete() {
            return Err(CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_minimum_cover_incomplete",
            });
        }
        let selected = selection
            .row_indices()
            .iter()
            .map(|index| candidate_keys[*index].clone())
            .collect::<BTreeSet<_>>();
        accepted.retain(|key, _| selected.contains(key));
        minimum_cover_complete = true;
        minimum_cover_proven = true;
        minimum_cover_reason = "none".to_owned();
    } else if minimum_cover_requested
        && minimum_cover_blocking_reason.is_none()
        && !probability_complete
    {
        minimum_cover_reason = "pattern_universe_incomplete".to_owned();
    }
    filtered_scoring_batches = retain_accepted_scoring_batches(filtered_scoring_batches, &accepted);
    filtered_spin_batches = retain_accepted_spin_batches(filtered_spin_batches, &accepted);
    let union = coverage_union(accepted.values(), pattern_count)?;

    let mut identity_by_key = BTreeMap::new();
    for batch in &filtered_scoring_batches {
        for graph in batch.graphs() {
            identity_by_key.insert(
                NormalizedTilingSolutionKey::from_standard_board64_identity(graph.identity())
                    .as_str()
                    .to_owned(),
                graph.identity(),
            );
        }
    }
    let had_board64_identities = !filtered_scoring_batches.is_empty();
    let normalized_keys = accepted.keys().cloned().collect::<Vec<_>>();
    let mut identities = normalized_keys
        .iter()
        .filter_map(|key| identity_by_key.get(key).copied())
        .collect::<Vec<_>>();
    identities.sort_unstable();
    identities.dedup();
    if had_board64_identities && identities.len() != normalized_keys.len() {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_candidate_identity_mismatch",
        });
    }

    let solution_coverages = board64_solution_coverages(&identities, &accepted);
    let solution_probability_complete = count_complete && probability_complete;
    let solution_probabilities = if result
        .bool_field("solution_probabilities_requested")
        .unwrap_or(false)
    {
        probability_reports(
            &identities,
            &solution_coverages,
            &weights,
            solution_probability_complete,
        )
    } else {
        Vec::new()
    };
    let probability = weights
        .covered_weight(&union)
        .expect("materialized weights and filtered coverage share one universe")
        .get();
    let solution_count = normalized_keys.len();
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
        field("build_variant_count", 0),
        field("build_variant_count_exact", false),
        field("pattern_verified_execution_count", witnessed_pattern_count),
        field("execution_constraint_materialized", true),
        field("b2b_preserving_solution_count", solution_count),
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
    ];
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
            field("solution_keys_complete", true),
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
    let normalized_solution_coverages = accepted
        .iter()
        .map(|(key, coverage)| NormalizedSolutionCoverage::new(key.clone(), coverage.clone()))
        .collect();

    Ok(result
        .with_replaced_fields(replacements)
        .with_packing_candidate_keys(Vec::new())
        .with_path_steps(Vec::new())
        .with_representative_solution_identity(None)
        .with_normalized_solution_keys(normalized_keys)
        .with_normalized_solution_identities(identities)
        .with_coverage_pattern_words(union.words().to_vec())
        .with_solution_coverages(solution_coverages)
        .with_normalized_solution_coverages(normalized_solution_coverages)
        .with_solution_probabilities(solution_probabilities)
        .with_solution_average_scores(Vec::new())
        .with_exact_scoring_execution_batches(filtered_scoring_batches)
        .with_spin_coverage_execution_batches(filtered_spin_batches)
        .without_finesse_search_report()
        .without_tiling_solution_page_store())
}

fn minimum_cover_input_status(result: &CoreExecutionResult) -> (bool, Option<String>) {
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
        Some("none") | None => Some("minimum-cover-status-missing".to_owned()),
        Some(reason) => Some(reason.to_owned()),
    };
    (true, blocking_reason)
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
) -> Result<BTreeMap<String, PatternBitSet>, CoreExecutionError> {
    let mut authoritative = BTreeMap::<String, PatternBitSet>::new();
    for coverage in result.normalized_solution_coverages() {
        merge_authoritative_coverage(
            &mut authoritative,
            coverage.solution_key(),
            coverage.covered_patterns(),
            pattern_count,
        )?;
    }
    for coverage in result.solution_coverages() {
        let key = NormalizedTilingSolutionKey::from_standard_board64_identity(coverage.identity());
        merge_authoritative_coverage(
            &mut authoritative,
            key.as_str(),
            coverage.covered_patterns(),
            pattern_count,
        )?;
    }
    if authoritative.is_empty() && !is_proven_empty_candidate_partition(result, pattern_count) {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_authoritative_coverage_missing",
        });
    }
    Ok(authoritative)
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
    authoritative: &mut BTreeMap<String, PatternBitSet>,
    candidate_key: &str,
    coverage: &PatternBitSet,
    pattern_count: usize,
) -> Result<(), CoreExecutionError> {
    if coverage.pattern_count() != pattern_count {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_authoritative_coverage_mismatch",
        });
    }
    authoritative
        .entry(candidate_key.to_owned())
        .or_insert_with(|| PatternBitSet::new(pattern_count))
        .union_with(coverage)
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_authoritative_coverage_mismatch",
        })
}

fn validate_execution_graph_authority(
    result: &CoreExecutionResult,
    authoritative: &BTreeMap<String, PatternBitSet>,
) -> Result<(), CoreExecutionError> {
    let scoring_complete = result
        .exact_scoring_execution_batches()
        .iter()
        .flat_map(|batch| batch.graphs())
        .all(|graph| {
            let key = NormalizedTilingSolutionKey::from_standard_board64_identity(graph.identity());
            authoritative.contains_key(key.as_str())
        });
    let spin_complete = result
        .spin_coverage_execution_batches()
        .iter()
        .flat_map(|batch| batch.graphs())
        .all(|graph| authoritative.contains_key(graph.candidate_key()));
    if scoring_complete && spin_complete {
        Ok(())
    } else {
        Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_candidate_coverage_missing",
        })
    }
}

fn merge_candidate_coverages(
    accepted: &mut BTreeMap<String, PatternBitSet>,
    coverages: &[CandidatePatternCoverage],
    authoritative: &BTreeMap<String, PatternBitSet>,
    pattern_count: usize,
) -> Result<PassConstraintResult, CoreExecutionError> {
    let mut pass_coverage = PatternBitSet::new(pattern_count);
    let mut pass_solutions = BTreeSet::new();
    let mut witnessed_pattern_count = 0_u128;
    for coverage in coverages {
        let allowed = authoritative.get(coverage.candidate_key()).ok_or(
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_candidate_coverage_missing",
            },
        )?;
        let constrained = coverage_intersection(coverage.covered_patterns(), allowed)?;
        if constrained.is_empty() {
            continue;
        }
        let entry = accepted
            .entry(coverage.candidate_key().to_owned())
            .or_insert_with(|| PatternBitSet::new(pattern_count));
        entry
            .union_with(&constrained)
            .map_err(|_| CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_candidate_coverage_mismatch",
            })?;
        pass_coverage.union_with(&constrained).map_err(|_| {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_candidate_coverage_mismatch",
            }
        })?;
        pass_solutions.insert(coverage.candidate_key());
        witnessed_pattern_count =
            witnessed_pattern_count.saturating_add(u128::from(constrained.count_ones()));
    }
    Ok(PassConstraintResult {
        coverage: pass_coverage,
        solution_count: pass_solutions.len(),
        witnessed_pattern_count,
    })
}

fn coverage_intersection(
    left: &PatternBitSet,
    right: &PatternBitSet,
) -> Result<PatternBitSet, CoreExecutionError> {
    if left.pattern_count() != right.pattern_count() {
        return Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_candidate_coverage_mismatch",
        });
    }
    PatternBitSet::from_words(
        left.pattern_count(),
        left.words()
            .iter()
            .zip(right.words())
            .map(|(left, right)| left & right)
            .collect(),
    )
    .map_err(|_| CoreExecutionError::RuntimeUnavailable {
        component: "b2b_preservation_candidate_coverage_mismatch",
    })
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
    let weights = result
        .postprocess_pattern_weights()
        .iter()
        .map(|value| value.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_pattern_weight_invalid",
        })?
        .into_iter()
        .map(ProbabilityValue::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_pattern_weight_invalid",
        })?;
    WeightedPatternSet::new(weights).map_err(|_| CoreExecutionError::RuntimeUnavailable {
        component: "b2b_preservation_pattern_weight_invalid",
    })
}

fn board64_solution_coverages(
    identities: &[StandardBoard64TilingIdentity],
    accepted: &BTreeMap<String, PatternBitSet>,
) -> Vec<SolutionCoverage> {
    identities
        .iter()
        .copied()
        .filter_map(|identity| {
            let key = NormalizedTilingSolutionKey::from_standard_board64_identity(identity);
            accepted
                .get(key.as_str())
                .cloned()
                .map(|coverage| SolutionCoverage::new(identity, coverage))
        })
        .collect()
}

fn retain_accepted_scoring_batches(
    batches: Vec<ExactScoringExecutionBatch>,
    accepted: &BTreeMap<String, PatternBitSet>,
) -> Vec<ExactScoringExecutionBatch> {
    batches
        .into_iter()
        .map(|batch| {
            let graphs = batch
                .graphs()
                .iter()
                .filter(|graph| {
                    let key = NormalizedTilingSolutionKey::from_standard_board64_identity(
                        graph.identity(),
                    );
                    accepted.contains_key(key.as_str())
                })
                .cloned()
                .collect();
            ExactScoringExecutionBatch::new(
                batch.layout(),
                batch.initial_occupied(),
                batch.patterns().to_vec(),
                batch.initial_cursor(),
                batch.initial_hold(),
                batch.hold_enabled(),
                batch.projects_unplaced_lookahead(),
                batch.projects_standard_bag_lookahead(),
                batch.kick_table_id(),
                batch.rule_profile_id(),
                graphs,
                batch.complete(),
            )
        })
        .collect()
}

fn retain_accepted_spin_batches(
    batches: Vec<SpinCoverageExecutionBatch>,
    accepted: &BTreeMap<String, PatternBitSet>,
) -> Vec<SpinCoverageExecutionBatch> {
    batches
        .into_iter()
        .map(|batch| {
            let graphs = batch
                .graphs()
                .iter()
                .filter(|graph| accepted.contains_key(graph.candidate_key()))
                .cloned()
                .collect();
            SpinCoverageExecutionBatch::new(
                batch.patterns().to_vec(),
                batch.initial_cursor(),
                batch.initial_hold(),
                batch.hold_enabled(),
                batch.projects_unplaced_lookahead(),
                batch.projects_standard_bag_lookahead(),
                batch.kick_table_id(),
                batch.rule_profile_id(),
                graphs,
                batch.complete(),
            )
        })
        .collect()
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
        solution_probability::probability_reports, CoreExecutionError, CoreExecutionResult,
        CorePathStep, FinesseReport, NormalizedSolutionCoverage, SolutionAverageScoreReport,
        SolutionCoverage,
    };
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
    };
    use clearra_replay::{
        ScoringExecutionEdge, ScoringExecutionNode, ScoringLockEvidence,
        SpinCoverageExecutionBatch, SpinCoverageExecutionGraph,
    };

    use super::apply_execution_constraints;

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
