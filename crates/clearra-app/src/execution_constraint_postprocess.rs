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
    SolutionCoverage,
};
use clearra_coverage::cover::exact_minimum_cover::exact_minimum_cover;
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
};
use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;
use clearra_postprocess::{
    BackToBackExecutionFilter, CandidatePatternCoverage, TSpinCoverageOnlyMaterialization,
    TSpinCoverageOnlyMaterializer,
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
    if result.exact_scoring_execution_batches().is_empty()
        && result.spin_coverage_execution_batches().is_empty()
    {
        if result.field("execution_constraint_materialized") == Some("true") {
            return Ok(result);
        }
        if result.usize_field("target_piece_count") == Some(0) {
            let solution_count = result.usize_field("unique_solution_count").unwrap_or(0);
            let score_requested = result
                .bool_field("postprocess_scoring_requested")
                .unwrap_or(false);
            let objective_complete = result
                .bool_field("objective_search_complete")
                .unwrap_or(false)
                && result.bool_field("count_complete").unwrap_or(false)
                && result.bool_field("probability_complete").unwrap_or(false)
                && !score_requested;
            return Ok(result.with_replaced_fields(vec![
                field("execution_constraint_materialized", true),
                field("b2b_preserving_solution_count", solution_count),
                field("b2b_preserving_candidate_pattern_count", 0),
                field(
                    "b2b_preservation_evaluation_basis",
                    "candidate-pattern-existence",
                ),
                field("b2b_preservation_path_multiplicity_counted", false),
                field("objective_complete", objective_complete),
                field(
                    "objective_incomplete_reason",
                    if objective_complete {
                        "none"
                    } else if score_requested {
                        "score_matrix_not_materialized"
                    } else {
                        "search_incomplete"
                    },
                ),
            ]));
        }
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
        witnessed_pattern_count =
            witnessed_pattern_count.saturating_add(materialized.witnessed_pattern_count());
        merge_candidate_coverages(&mut accepted, materialized.candidate_coverages())?;
        pass_results.push(PassConstraintResult::from_materialization(&materialized));
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
        witnessed_pattern_count =
            witnessed_pattern_count.saturating_add(materialized.witnessed_pattern_count());
        merge_candidate_coverages(&mut accepted, materialized.candidate_coverages())?;
        pass_results.push(PassConstraintResult::from_materialization(&materialized));
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
    let minimum_cover_requested = result.field("objective") == Some("minimum-cover");
    let minimum_cover_deferred =
        result.field("minimum_cover_incomplete_reason") == Some("deferred-to-coordinator");
    let minimum_cover_source_solution_count = accepted.len();
    let mut minimum_cover_complete = false;
    let mut minimum_cover_proven = false;
    let mut minimum_cover_reason = if minimum_cover_deferred {
        "deferred-to-coordinator"
    } else if minimum_cover_requested {
        "search_incomplete"
    } else {
        "not_requested"
    };
    if minimum_cover_requested && !minimum_cover_deferred && count_complete && probability_complete
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
        minimum_cover_reason = "none";
    } else if minimum_cover_requested && !minimum_cover_deferred && !probability_complete {
        minimum_cover_reason = "pattern_universe_incomplete";
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
        "score_matrix_not_materialized"
    } else if !count_complete {
        result
            .field("resource_truncation_reason")
            .filter(|reason| *reason != "none")
            .unwrap_or("search_incomplete")
    } else if !probability_complete {
        "pattern_universe_incomplete"
    } else if minimum_cover_requested && !minimum_cover_complete {
        minimum_cover_reason
    } else {
        "none"
    };

    let mut replacements = vec![
        field("solution_found", solution_count != 0),
        field("unique_solution_count", solution_count),
        field("normalized_solution_set_hash", solution_hash),
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
            field("minimum_cover_incomplete_reason", minimum_cover_reason),
        ]);
    }
    append_build_symmetry_fields(&mut replacements, &pass_results, &weights);

    Ok(result
        .with_replaced_fields(replacements)
        .with_path_steps(Vec::new())
        .with_representative_solution_identity(None)
        .with_normalized_solution_keys(normalized_keys)
        .with_normalized_solution_identities(identities)
        .with_coverage_pattern_words(union.words().to_vec())
        .with_solution_coverages(solution_coverages)
        .with_solution_probabilities(solution_probabilities)
        .with_exact_scoring_execution_batches(filtered_scoring_batches)
        .with_spin_coverage_execution_batches(filtered_spin_batches))
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

fn merge_candidate_coverages(
    accepted: &mut BTreeMap<String, PatternBitSet>,
    coverages: &[CandidatePatternCoverage],
) -> Result<(), CoreExecutionError> {
    for coverage in coverages {
        let entry = accepted
            .entry(coverage.candidate_key().to_owned())
            .or_insert_with(|| PatternBitSet::new(coverage.covered_patterns().pattern_count()));
        entry.union_with(coverage.covered_patterns()).map_err(|_| {
            CoreExecutionError::RuntimeUnavailable {
                component: "b2b_preservation_candidate_coverage_mismatch",
            }
        })?;
    }
    Ok(())
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
}

impl PassConstraintResult {
    fn from_materialization(materialized: &TSpinCoverageOnlyMaterialization) -> Self {
        Self {
            coverage: materialized.covered_patterns().clone(),
            solution_count: materialized.candidate_keys().count(),
        }
    }
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
