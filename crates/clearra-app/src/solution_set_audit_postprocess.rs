// SRP rationale: this module has one behavior-level change reason: attaching deterministic solution-set audit evidence to materialized PC and Build results.

//! Attaches the private typed solution-set audit at the common PC/build application boundary.
//!
//! This adapter only consumes already-materialized result authority. It does not evaluate
//! probability, replay execution, spin, or B2B preservation again.

use clearra_core_domain::solution::normalized_tiling_solution::{
    normalized_tiling_solution_key_set_hash_from_sorted_strings, NormalizedTilingSolutionKey,
    NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
};
use clearra_core_executor::{
    CoreExecutionResult, SolutionAuditCandidate, SolutionAuditCheckpoint,
    SolutionPortfolioSelectionPolicy, SolutionProductFamily, SolutionSemanticDimensions,
    SolutionSetAuditFieldBuildError, SolutionSetAuditGuardedError, SolutionSetAuditInput,
    SolutionSetAuditReport,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

pub(crate) fn attach_solution_set_audit_with_memory_guard(
    result: CoreExecutionResult,
    memory_guard: &mut impl FnMut(
        &CoreExecutionResult,
        u128,
    ) -> Result<(), clearra_core_executor::CoreExecutionError>,
) -> Result<CoreExecutionResult, clearra_core_executor::CoreExecutionError> {
    memory_guard(&result, 0)?;
    let Some(product_family) = classify_product_family(&result) else {
        return Ok(result);
    };
    let availability = result.execution_report().solution_set_availability();
    let policy_occurrences = result.field_occurrence_count("search_output_policy");
    let unique_policy = result.unique_field("search_output_policy");
    let fail_closed_public_surface = unique_policy == Some("coverage-summary")
        || policy_occurrences > 1
        || ((policy_occurrences != 0 || availability.uses_explicit_contract())
            && (!availability.contract_valid()
                || !availability
                    .materialized_key_count_matches(result.normalized_solution_keys().len())));
    if fail_closed_public_surface {
        return attach_redacted_solution_set_audit_fields(result, product_family, memory_guard);
    }
    let input_projection = checked_audit_input_construction_projection(&result).ok_or(
        clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
            component: "solution_set_audit_memory_projection_overflow",
        },
    )?;
    memory_guard(&result, input_projection)?;
    let input = audit_input(&result, product_family).map_err(|component| {
        clearra_core_executor::CoreExecutionError::RuntimeUnavailable { component }
    })?;
    let input_bytes = input.checked_nested_retained_bytes().ok_or(
        clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
            component: "solution_set_audit_memory_projection_overflow",
        },
    )?;
    let analysis_projection = SolutionSetAuditReport::checked_analysis_memory_projection(&input)
        .ok_or(
            clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                component: "solution_set_audit_memory_projection_overflow",
            },
        )?;
    let initial_analysis_future = input_bytes
        .checked_add(analysis_projection.required_peak_bytes)
        .ok_or(
            clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                component: "solution_set_audit_memory_projection_overflow",
            },
        )?;
    memory_guard(&result, initial_analysis_future)?;
    let (report, _) =
        SolutionSetAuditReport::analyze_with_memory_guard(input, &mut |analysis_owned_bytes| {
            let future_bytes = input_bytes.checked_add(analysis_owned_bytes).ok_or(
                clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                    component: "solution_set_audit_memory_projection_overflow",
                },
            )?;
            memory_guard(&result, future_bytes)
        })
        .map_err(map_audit_guarded_error)?;
    let report_bytes = report.checked_nested_retained_bytes().ok_or(
        clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
            component: "solution_set_audit_memory_projection_overflow",
        },
    )?;
    let field_projection = report.checked_summary_field_projection().ok_or(
        clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
            component: "solution_set_audit_memory_projection_overflow",
        },
    )?;
    memory_guard(
        &result,
        report_bytes
            .checked_add(field_projection.required_bytes)
            .ok_or(
                clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                    component: "solution_set_audit_memory_projection_overflow",
                },
            )?,
    )?;
    let (fields, actual_field_bytes) = report
        .try_summary_fields()
        .map_err(map_audit_field_build_error)?;
    memory_guard(
        &result,
        report_bytes.checked_add(actual_field_bytes).ok_or(
            clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                component: "solution_set_audit_memory_projection_overflow",
            },
        )?,
    )?;
    let result = try_replace_audit_fields(result, fields, report_bytes, memory_guard)?;
    Ok(result.with_solution_set_audit_report(report))
}

fn attach_redacted_solution_set_audit_fields(
    result: CoreExecutionResult,
    product_family: SolutionProductFamily,
    memory_guard: &mut impl FnMut(
        &CoreExecutionResult,
        u128,
    ) -> Result<(), clearra_core_executor::CoreExecutionError>,
) -> Result<CoreExecutionResult, clearra_core_executor::CoreExecutionError> {
    let selection_policy = selection_policy(&result);
    let required_pattern_count = result
        .usize_field("coverage_pattern_count")
        .and_then(|pattern_count| redacted_required_pattern_count(&result, pattern_count))
        .unwrap_or(0);
    let projection = SolutionSetAuditReport::checked_redacted_summary_field_projection(
        product_family,
        selection_policy,
        required_pattern_count,
    )
    .ok_or(
        clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
            component: "solution_set_audit_memory_projection_overflow",
        },
    )?;
    memory_guard(&result, projection.required_bytes)?;
    let (fields, actual_field_bytes) = SolutionSetAuditReport::try_redacted_summary_fields(
        product_family,
        selection_policy,
        required_pattern_count,
    )
    .map_err(map_audit_field_build_error)?;
    memory_guard(&result, actual_field_bytes)?;
    try_replace_audit_fields(
        result.without_solution_set_audit_report(),
        fields,
        0,
        memory_guard,
    )
}

fn selection_policy(result: &CoreExecutionResult) -> SolutionPortfolioSelectionPolicy {
    if result.bool_field("minimum_cover_requested") == Some(true)
        || result.field("objective") == Some("minimum-cover")
    {
        SolutionPortfolioSelectionPolicy::ExactMinimumCover
    } else {
        SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives
    }
}

fn redacted_required_pattern_count(
    result: &CoreExecutionResult,
    pattern_count: usize,
) -> Option<usize> {
    let words = result.coverage_pattern_words();
    let word_count = pattern_count.div_ceil(u64::BITS as usize);
    if !words.is_empty() {
        if words.len() != word_count || has_bits_outside_pattern_universe(words, pattern_count) {
            return None;
        }
        return words.iter().try_fold(0_usize, |count, word| {
            count.checked_add(word.count_ones() as usize)
        });
    }
    let mut count = 0_usize;
    for word_index in 0..word_count {
        let mut union = 0_u64;
        for coverage in result.normalized_solution_coverages() {
            if coverage.covered_patterns().pattern_count() != pattern_count {
                return None;
            }
            union |= coverage.covered_patterns().word_at(word_index);
        }
        for coverage in result.solution_coverages() {
            if coverage.covered_patterns().pattern_count() != pattern_count {
                return None;
            }
            union |= coverage.covered_patterns().word_at(word_index);
        }
        count = count.checked_add(union.count_ones() as usize)?;
    }
    Some(count)
}

fn has_bits_outside_pattern_universe(words: &[u64], pattern_count: usize) -> bool {
    let remainder = pattern_count % u64::BITS as usize;
    remainder != 0
        && words.last().is_some_and(|word| {
            let valid_mask = (1_u64 << remainder) - 1;
            word & !valid_mask != 0
        })
}

fn try_replace_audit_fields(
    result: CoreExecutionResult,
    fields: Vec<(String, String)>,
    additional_live_bytes: u128,
    memory_guard: &mut impl FnMut(
        &CoreExecutionResult,
        u128,
    ) -> Result<(), clearra_core_executor::CoreExecutionError>,
) -> Result<CoreExecutionResult, clearra_core_executor::CoreExecutionError> {
    result
        .try_with_replaced_fields_with_memory_guard(fields, |live, future| {
            let future = future.checked_add(additional_live_bytes).ok_or(
                clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                    component: "solution_set_audit_memory_projection_overflow",
                },
            )?;
            memory_guard(live, future)
        })
        .map_err(|error| match error {
            clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::ProjectionOverflow => {
                clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                    component: "solution_set_audit_memory_projection_overflow",
                }
            }
            clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::AllocationFailed { .. } => {
                clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                    component: "solution_set_audit_field_allocation_failed",
                }
            }
            clearra_core_executor::core_execution_result::CoreResultFieldReplacementError::MemoryGuard(error) => error,
        })
}

fn map_audit_field_build_error(
    error: SolutionSetAuditFieldBuildError,
) -> clearra_core_executor::CoreExecutionError {
    match error {
        SolutionSetAuditFieldBuildError::ProjectionOverflow => {
            clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                component: "solution_set_audit_memory_projection_overflow",
            }
        }
        SolutionSetAuditFieldBuildError::AllocationFailed { .. } => {
            clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                component: "solution_set_audit_field_allocation_failed",
            }
        }
    }
}

fn map_audit_guarded_error(
    error: SolutionSetAuditGuardedError<clearra_core_executor::CoreExecutionError>,
) -> clearra_core_executor::CoreExecutionError {
    match error {
        SolutionSetAuditGuardedError::ProjectionOverflow => {
            clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                component: "solution_set_audit_memory_projection_overflow",
            }
        }
        SolutionSetAuditGuardedError::MemoryGuard(error) => error,
        SolutionSetAuditGuardedError::Audit(error) => {
            clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                component: error.code(),
            }
        }
    }
}

fn checked_audit_input_construction_projection(result: &CoreExecutionResult) -> Option<u128> {
    let candidate_count = result
        .normalized_solution_coverages()
        .len()
        .checked_add(result.solution_coverages().len())?;
    let mut candidate_key_bytes = result
        .normalized_solution_coverages()
        .iter()
        .try_fold(0_u128, |bytes, coverage| {
            bytes.checked_add(coverage.solution_key().len() as u128)
        })?;
    for coverage in result.solution_coverages() {
        let capacity =
            42_usize.checked_add(coverage.identity().placement_count().checked_mul(20)?)?;
        candidate_key_bytes = candidate_key_bytes.checked_add(capacity as u128)?;
    }
    let objective = result.field("objective").unwrap_or("unknown");
    let score_profile = result
        .field("score_profile_requested")
        .or_else(|| result.field("finesse_metric_requested"))
        .or_else(|| result.field("build_probability_aggregation"))
        .unwrap_or("none");
    let preserve_b2b = result.bool_field("execution_constraint_preserve_b2b") == Some(true);
    let spin_profile = if preserve_b2b {
        result
            .field("execution_constraint_spin_profile")
            .unwrap_or("unknown")
    } else {
        result
            .field("spin_profile_requested")
            .or_else(|| result.field("spin_coverage_target"))
            .unwrap_or("none")
    };
    let dimension_payload = objective
        .len()
        .max("unknown".len())
        .checked_add(score_profile.len().max("unknown".len()))?
        .checked_add(spin_profile.len().max("unknown".len()))?
        .checked_add("existential-preserve".len())? as u128;
    let candidate_slots = (candidate_count as u128).checked_mul(
        (core::mem::size_of::<(String, PatternBitSet)>()
            + core::mem::size_of::<SolutionAuditCandidate>()) as u128,
    )?;
    let candidate_dimensions =
        dimension_payload.checked_mul((candidate_count as u128).checked_add(1)?)?;
    let (normalized_key_count, normalized_payload) =
        if selection_policy(result) == SolutionPortfolioSelectionPolicy::ExactMinimumCover {
            (
                result.normalized_solution_coverages().len(),
                result
                    .normalized_solution_coverages()
                    .iter()
                    .try_fold(0_u128, |bytes, coverage| {
                        bytes.checked_add(coverage.solution_key().len() as u128)
                    })?,
            )
        } else {
            (
                result.normalized_solution_keys().len(),
                result
                    .normalized_solution_keys()
                    .iter()
                    .try_fold(0_u128, |bytes, key| bytes.checked_add(key.len() as u128))?,
            )
        };
    let normalized_slots =
        (normalized_key_count as u128).checked_mul(core::mem::size_of::<String>() as u128)?;
    let pattern_count = result.usize_field("coverage_pattern_count").unwrap_or(0);
    let pattern_owners = (candidate_count as u128).checked_add(2)?;
    let pattern_bytes = PatternBitSet::checked_shared_construction_upper_bound(
        pattern_count,
        pattern_owners,
        pattern_owners.checked_mul(pattern_count as u128)?,
    )?;
    let checkpoint_bytes = result
        .pre_b2b_produced_solution_audit_checkpoint()
        .map_or(Some(128), SolutionAuditCheckpoint::checked_clone_peak_bytes)?
        .checked_add(
            result
                .pre_b2b_solution_audit_checkpoint()
                .map_or(Some(128), SolutionAuditCheckpoint::checked_clone_peak_bytes)?,
        )?;
    // All conditional reasons have static payloads. This exact list upper
    // bounds the vectors built by `audit_input` even when every condition is
    // simultaneously incomplete.
    const REASONS: &[&str] = &[
        "source-solution-count-incomplete",
        "solution-availability-explicit-contract-missing",
        "solution-availability-contract-invalid",
        "solution-set-not-materialized",
        "solution-key-materialization-incomplete",
        "solution-key-count-mismatch",
        "minimum-cover-source-solution-count-mismatch",
        "b2b-filter-not-materialized",
        "b2b-filter-count-incomplete",
        "build-spin-evidence-incomplete",
    ];
    let reason_bytes = (REASONS.len() as u128)
        .checked_mul(core::mem::size_of::<String>() as u128)?
        .checked_add(REASONS.iter().try_fold(0_u128, |bytes, reason| {
            bytes.checked_add(reason.len() as u128)
        })?)?;
    candidate_slots
        .checked_add(candidate_key_bytes)?
        .checked_add(candidate_dimensions)?
        .checked_add(normalized_slots)?
        .checked_add(normalized_payload)?
        .checked_add(pattern_bytes)?
        .checked_add(checkpoint_bytes)?
        .checked_add(reason_bytes)?
        .checked_add(2 * "ctks1:0000000000000000".len() as u128)
}

fn classify_product_family(result: &CoreExecutionResult) -> Option<SolutionProductFamily> {
    match result.field("search_kind") {
        Some("build-probability") => Some(SolutionProductFamily::BuildProbability),
        Some(_) => None,
        None if result.field("normalized_solution_key_algorithm")
            == Some(NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM)
            && result.field("objective").is_some()
            && result.usize_field("coverage_pattern_count").is_some() =>
        {
            Some(SolutionProductFamily::Pc)
        }
        None => None,
    }
}

fn audit_input(
    result: &CoreExecutionResult,
    product_family: SolutionProductFamily,
) -> Result<SolutionSetAuditInput, &'static str> {
    let pattern_count = result
        .usize_field("coverage_pattern_count")
        .ok_or("solution-set-audit-pattern-universe-missing")?;
    let dimensions = semantic_dimensions(result, product_family);
    let selection_policy = selection_policy(result);
    let candidates = audit_candidates(result, pattern_count, &dimensions)?;
    let required_patterns = required_patterns(result, pattern_count, &candidates)?;
    // Exact minimum-cover results expose only the canonical selected cover in
    // `normalized_solution_keys`, while retaining the complete pass-1 source
    // matrix in `normalized_solution_coverages`. The generic audit owns the
    // pass-2 selection, so its normalized universe must be reconstructed from
    // those source rows rather than from the already-selected public keys.
    let mut normalized_keys =
        if selection_policy == SolutionPortfolioSelectionPolicy::ExactMinimumCover {
            try_clone_coverage_keys(result.normalized_solution_coverages())?
        } else {
            try_clone_strings(result.normalized_solution_keys())?
        };
    normalized_keys.sort_unstable();
    let normalized_identity_hash = normalized_key_hash(&normalized_keys);
    let count_complete = result.bool_field("count_complete").unwrap_or(false);
    let availability = result.execution_report().solution_set_availability();
    let availability_complete = availability.uses_explicit_contract()
        && availability.contract_valid()
        && availability.solution_set_materialized()
        && availability.solution_keys_complete()
        && availability.materialized_key_count_matches(result.normalized_solution_keys().len());
    let minimum_cover_source_complete = selection_policy
        != SolutionPortfolioSelectionPolicy::ExactMinimumCover
        || result.usize_field("minimum_cover_source_solution_count") == Some(normalized_keys.len());

    let mut normalized_reasons = Vec::new();
    if !count_complete {
        normalized_reasons.push("source-solution-count-incomplete".to_owned());
    }
    if !availability.uses_explicit_contract() {
        normalized_reasons.push("solution-availability-explicit-contract-missing".to_owned());
    } else if !availability.contract_valid() {
        normalized_reasons.push("solution-availability-contract-invalid".to_owned());
    } else {
        if !availability.solution_set_materialized() {
            normalized_reasons.push("solution-set-not-materialized".to_owned());
        }
        if !availability.solution_keys_complete() {
            normalized_reasons.push("solution-key-materialization-incomplete".to_owned());
        }
        if !availability.materialized_key_count_matches(result.normalized_solution_keys().len()) {
            normalized_reasons.push("solution-key-count-mismatch".to_owned());
        }
    }
    if !minimum_cover_source_complete {
        normalized_reasons.push("minimum-cover-source-solution-count-mismatch".to_owned());
    }

    let preserve_b2b = result.bool_field("execution_constraint_preserve_b2b") == Some(true);
    let build_spin_requested = product_family == SolutionProductFamily::BuildProbability
        && result.bool_field("postprocess_build_spin_requested") == Some(true);
    let materialized_solution_checkpoint = SolutionAuditCheckpoint::new(
        Some(normalized_keys.len()),
        count_complete && availability_complete && minimum_cover_source_complete,
        Some(normalized_identity_hash.clone()),
        normalized_reasons.clone(),
    );
    let (produced, execution_validated) = if preserve_b2b {
        let produced = result
            .pre_b2b_produced_solution_audit_checkpoint()
            .cloned()
            .unwrap_or_else(|| {
                SolutionAuditCheckpoint::unknown("pre-b2b-produced-solution-checkpoint-missing")
            });
        let execution_validated = result
            .pre_b2b_solution_audit_checkpoint()
            .cloned()
            .unwrap_or_else(|| {
                SolutionAuditCheckpoint::unknown("pre-b2b-solution-checkpoint-missing")
            });
        (produced, execution_validated)
    } else {
        (
            materialized_solution_checkpoint.clone(),
            materialized_solution_checkpoint,
        )
    };

    let mut filter_reasons = normalized_reasons.clone();
    let mut filter_complete =
        count_complete && availability_complete && minimum_cover_source_complete;
    if preserve_b2b {
        if result.bool_field("execution_constraint_materialized") != Some(true) {
            filter_complete = false;
            filter_reasons.push("b2b-filter-not-materialized".to_owned());
        }
        if result.bool_field("b2b_preservation_count_complete") != Some(true) {
            filter_complete = false;
            filter_reasons.push("b2b-filter-count-incomplete".to_owned());
        }
    }
    if build_spin_requested && result.bool_field("spin_search_probability_complete") != Some(true) {
        filter_complete = false;
        filter_reasons.push("build-spin-evidence-incomplete".to_owned());
    }
    let spin_b2b_filtered = SolutionAuditCheckpoint::new(
        Some(normalized_keys.len()),
        filter_complete,
        Some(normalized_identity_hash),
        filter_reasons.clone(),
    );

    Ok(
        SolutionSetAuditInput::new(product_family, required_patterns, selection_policy)
            .with_source_checkpoints(produced, execution_validated, spin_b2b_filtered)
            .with_normalized_keys(normalized_keys, filter_complete, filter_reasons)
            .with_candidates(candidates),
    )
}

fn semantic_dimensions(
    result: &CoreExecutionResult,
    product_family: SolutionProductFamily,
) -> SolutionSemanticDimensions {
    let objective = result.field("objective").unwrap_or("unknown");
    let score_profile = result
        .field("score_profile_requested")
        .or_else(|| result.field("finesse_metric_requested"))
        .or_else(|| result.field("build_probability_aggregation"))
        .unwrap_or("none");
    let preserve_b2b = result.bool_field("execution_constraint_preserve_b2b") == Some(true);
    let spin_profile = if preserve_b2b {
        result
            .field("execution_constraint_spin_profile")
            .unwrap_or("unknown")
    } else {
        result
            .field("spin_profile_requested")
            .or_else(|| result.field("spin_coverage_target"))
            .unwrap_or("none")
    };
    SolutionSemanticDimensions::new(
        product_family,
        objective,
        score_profile,
        spin_profile,
        if preserve_b2b {
            "existential-preserve"
        } else {
            "disabled"
        },
    )
}

fn audit_candidates(
    result: &CoreExecutionResult,
    pattern_count: usize,
    dimensions: &SolutionSemanticDimensions,
) -> Result<Vec<SolutionAuditCandidate>, &'static str> {
    let capacity = result
        .normalized_solution_coverages()
        .len()
        .checked_add(result.solution_coverages().len())
        .ok_or("solution-set-audit-memory-projection-overflow")?;
    let mut coverage_by_key = Vec::<(String, PatternBitSet)>::new();
    coverage_by_key
        .try_reserve_exact(capacity)
        .map_err(|_| "solution-set-audit-candidate-allocation-failed")?;
    for coverage in result.normalized_solution_coverages() {
        insert_coverage(
            &mut coverage_by_key,
            try_owned_audit_string(coverage.solution_key())?,
            coverage.covered_patterns().clone(),
            pattern_count,
        )?;
    }
    for coverage in result.solution_coverages() {
        let key = NormalizedTilingSolutionKey::from_standard_board64_identity(coverage.identity());
        insert_coverage(
            &mut coverage_by_key,
            try_owned_audit_string(key.as_str())?,
            coverage.covered_patterns().clone(),
            pattern_count,
        )?;
    }
    let mut candidates = Vec::new();
    candidates
        .try_reserve_exact(coverage_by_key.len())
        .map_err(|_| "solution-set-audit-candidate-allocation-failed")?;
    for (key, coverage) in coverage_by_key {
        candidates.push(
            SolutionAuditCandidate::new(key, coverage, dimensions.clone())
                .map_err(|error| error.code())?,
        );
    }
    Ok(candidates)
}

fn insert_coverage(
    coverage_by_key: &mut Vec<(String, PatternBitSet)>,
    key: String,
    coverage: PatternBitSet,
    pattern_count: usize,
) -> Result<(), &'static str> {
    if coverage.pattern_count() != pattern_count {
        return Err("solution-set-audit-candidate-pattern-universe-mismatch");
    }
    match coverage_by_key.binary_search_by(|(candidate, _)| candidate.cmp(&key)) {
        Ok(index) if coverage_by_key[index].1 == coverage => Ok(()),
        Ok(_) => Err("solution-set-audit-conflicting-candidate-coverage"),
        Err(index) => {
            coverage_by_key.insert(index, (key, coverage));
            Ok(())
        }
    }
}

fn required_patterns(
    result: &CoreExecutionResult,
    pattern_count: usize,
    candidates: &[SolutionAuditCandidate],
) -> Result<PatternBitSet, &'static str> {
    let mut candidate_union = PatternBitSet::new(pattern_count);
    for candidate in candidates {
        candidate_union
            .union_with(candidate.coverage())
            .map_err(|_| "solution-set-audit-candidate-union-mismatch")?;
    }
    let words = result.coverage_pattern_words();
    if words.is_empty() && pattern_count != 0 {
        return Ok(candidate_union);
    }
    let mut declared_words = Vec::new();
    declared_words
        .try_reserve_exact(words.len())
        .map_err(|_| "solution-set-audit-coverage-allocation-failed")?;
    declared_words.extend_from_slice(words);
    let declared = PatternBitSet::from_words(pattern_count, declared_words)
        .map_err(|_| "solution-set-audit-result-coverage-words-invalid")?;
    if !candidates.is_empty() && declared != candidate_union {
        return Err("solution-set-audit-result-coverage-union-mismatch");
    }
    Ok(declared)
}

fn normalized_key_hash(keys: &[String]) -> String {
    debug_assert!(keys.windows(2).all(|pair| pair[0] <= pair[1]));
    normalized_tiling_solution_key_set_hash_from_sorted_strings(keys)
}

fn try_clone_strings(values: &[String]) -> Result<Vec<String>, &'static str> {
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(values.len())
        .map_err(|_| "solution-set-audit-string-allocation-failed")?;
    for value in values {
        let mut owned = String::new();
        owned
            .try_reserve_exact(value.len())
            .map_err(|_| "solution-set-audit-string-allocation-failed")?;
        owned.push_str(value);
        cloned.push(owned);
    }
    Ok(cloned)
}

fn try_clone_coverage_keys(
    values: &[clearra_core_executor::NormalizedSolutionCoverage],
) -> Result<Vec<String>, &'static str> {
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(values.len())
        .map_err(|_| "solution-set-audit-string-allocation-failed")?;
    for value in values {
        cloned.push(try_owned_audit_string(value.solution_key())?);
    }
    Ok(cloned)
}

fn try_owned_audit_string(value: &str) -> Result<String, &'static str> {
    let mut owned = String::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| "solution-set-audit-string-allocation-failed")?;
    owned.push_str(value);
    Ok(owned)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        piece::{piece_kind::PieceKind, rotation::RotationState},
        solution::normalized_tiling_solution::{
            normalized_tiling_solution_key_set_hash_from_sorted_strings,
            NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
        },
    };
    use clearra_core_executor::{
        CoreExecutionResult, NormalizedSolutionCoverage, ScoringExecutionEdge,
        ScoringExecutionNode, ScoringLockEvidence, SolutionProductFamily,
        SolutionSetAuditStageKind, SpinCoverageExecutionBatch, SpinCoverageExecutionGraph,
    };
    use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

    use crate::app_services::AppCoreExecutorService;

    use super::attach_solution_set_audit_with_memory_guard;

    fn attach_solution_set_audit(result: CoreExecutionResult) -> CoreExecutionResult {
        attach_solution_set_audit_with_memory_guard(result, &mut |_, _| Ok(()))
            .expect("solution-set audit")
    }

    fn bits(pattern_count: usize, patterns: &[u32]) -> PatternBitSet {
        PatternBitSet::from_pattern_indices(pattern_count, patterns.to_vec())
            .expect("test pattern bitset")
    }

    fn materialized_result(product_family: SolutionProductFamily) -> CoreExecutionResult {
        let objective = match product_family {
            SolutionProductFamily::Pc => "coverage",
            SolutionProductFamily::BuildProbability => "build-probability",
        };
        let mut fields = vec![
            ("search_output_policy".to_owned(), "summary".to_owned()),
            ("objective".to_owned(), objective.to_owned()),
            ("coverage_pattern_count".to_owned(), "2".to_owned()),
            ("count_complete".to_owned(), "true".to_owned()),
            ("packing_candidate_count".to_owned(), "1".to_owned()),
            ("packing_count_complete".to_owned(), "true".to_owned()),
            (
                "packing_candidate_set_digest".to_owned(),
                "synthetic-packing-digest".to_owned(),
            ),
            (
                "packing_candidate_set_digest_calculated".to_owned(),
                "true".to_owned(),
            ),
            ("unique_solution_count".to_owned(), "2".to_owned()),
            (
                "normalized_unique_solution_count".to_owned(),
                "2".to_owned(),
            ),
            ("solution_count_calculated".to_owned(), "true".to_owned()),
            ("solution_set_materialized".to_owned(), "true".to_owned()),
            (
                "solution_keys_materialized_count".to_owned(),
                "2".to_owned(),
            ),
            ("solution_keys_complete".to_owned(), "true".to_owned()),
            ("solution_page_available".to_owned(), "false".to_owned()),
            (
                "normalized_solution_set_hash".to_owned(),
                "synthetic-set-hash".to_owned(),
            ),
            (
                "actual_normalized_solution_set_hash".to_owned(),
                "synthetic-set-hash".to_owned(),
            ),
            (
                "execution_constraint_preserve_b2b".to_owned(),
                "false".to_owned(),
            ),
        ];
        match product_family {
            SolutionProductFamily::Pc => fields.push((
                "normalized_solution_key_algorithm".to_owned(),
                NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM.to_owned(),
            )),
            SolutionProductFamily::BuildProbability => {
                fields.push(("search_kind".to_owned(), "build-probability".to_owned()));
            }
        }
        CoreExecutionResult::new(fields, Vec::new())
            .with_normalized_solution_keys(vec!["solution-a".to_owned(), "solution-b".to_owned()])
            .with_normalized_solution_coverages(vec![
                NormalizedSolutionCoverage::new("solution-a", bits(2, &[0])),
                NormalizedSolutionCoverage::new("solution-b", bits(2, &[1])),
            ])
            .with_coverage_pattern_words(vec![0b11])
    }

    #[test]
    fn attaches_only_to_positive_pc_and_build_probability_families() {
        for product_family in [
            SolutionProductFamily::Pc,
            SolutionProductFamily::BuildProbability,
        ] {
            let result = attach_solution_set_audit(materialized_result(product_family));
            let report = result.solution_set_audit_report().expect("typed audit");
            assert_eq!(report.product_family(), product_family);
            assert!(report.complete());
            assert_eq!(result.usize_field("packing_candidate_count"), Some(1));
            assert_eq!(
                report
                    .stage(SolutionSetAuditStageKind::Produced)
                    .output_count(),
                Some(2)
            );
            assert_eq!(
                report
                    .stage(SolutionSetAuditStageKind::ExecutionValidated)
                    .output_count(),
                Some(2)
            );
            assert_eq!(
                result.field("solution_set_audit_private_authority"),
                Some("attached")
            );
        }

        let unrelated = materialized_result(SolutionProductFamily::Pc)
            .with_replaced_fields(vec![("search_kind".to_owned(), "finesse-score".to_owned())]);
        let expected = unrelated.clone();
        assert_eq!(attach_solution_set_audit(unrelated), expected);

        let setup = materialized_result(SolutionProductFamily::Pc).with_replaced_fields(vec![(
            "normalized_solution_key_algorithm".to_owned(),
            "clearra-setup-candidate-key-v2-exact-partial-state".to_owned(),
        )]);
        let expected = setup.clone();
        assert_eq!(attach_solution_set_audit(setup), expected);
    }

    #[test]
    fn minimum_cover_audits_complete_source_rows_before_canonical_selection() {
        let input = materialized_result(SolutionProductFamily::Pc)
            .with_replaced_fields(vec![
                ("objective".to_owned(), "minimum-cover".to_owned()),
                ("minimum_cover_requested".to_owned(), "true".to_owned()),
                ("minimum_cover_complete".to_owned(), "true".to_owned()),
                ("minimum_cover_proven_minimum".to_owned(), "true".to_owned()),
                (
                    "minimum_cover_source_solution_count".to_owned(),
                    "2".to_owned(),
                ),
                (
                    "minimum_cover_selected_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                ("unique_solution_count".to_owned(), "1".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "1".to_owned(),
                ),
            ])
            .with_normalized_solution_keys(vec!["solution-a".to_owned()])
            .with_normalized_solution_coverages(vec![
                NormalizedSolutionCoverage::new("solution-a", bits(2, &[0, 1])),
                NormalizedSolutionCoverage::new("solution-b", bits(2, &[0])),
            ]);

        let result = attach_solution_set_audit(input);
        let report = result.solution_set_audit_report().expect("typed audit");

        assert!(report.complete());
        assert_eq!(
            result.normalized_solution_keys(),
            &["solution-a".to_owned()]
        );
        assert_eq!(
            report
                .stage(SolutionSetAuditStageKind::Normalized)
                .output_count(),
            Some(2)
        );
        assert_eq!(
            report
                .stage(SolutionSetAuditStageKind::PortfolioSelected)
                .output_count(),
            Some(1)
        );
        assert_eq!(report.portfolio_families().len(), 1);
        assert_eq!(
            report.portfolio_families()[0].representative_keys(),
            &["solution-a".to_owned()]
        );
    }

    #[test]
    fn minimum_cover_source_solution_count_mismatch_fails_closed() {
        let input = materialized_result(SolutionProductFamily::Pc)
            .with_replaced_fields(vec![
                ("objective".to_owned(), "minimum-cover".to_owned()),
                ("minimum_cover_requested".to_owned(), "true".to_owned()),
                ("minimum_cover_complete".to_owned(), "true".to_owned()),
                ("minimum_cover_proven_minimum".to_owned(), "true".to_owned()),
                (
                    "minimum_cover_source_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                (
                    "minimum_cover_selected_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                ("unique_solution_count".to_owned(), "1".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "1".to_owned(),
                ),
            ])
            .with_normalized_solution_keys(vec!["solution-a".to_owned()])
            .with_normalized_solution_coverages(vec![
                NormalizedSolutionCoverage::new("solution-a", bits(2, &[0, 1])),
                NormalizedSolutionCoverage::new("solution-b", bits(2, &[0])),
            ]);

        let result = attach_solution_set_audit(input);
        let report = result.solution_set_audit_report().expect("typed audit");

        assert!(!report.complete());
        let normalized = report.stage(SolutionSetAuditStageKind::Normalized);
        assert!(!normalized.complete());
        assert!(normalized
            .rejection_reasons()
            .iter()
            .any(|reason| reason == "minimum-cover-source-solution-count-mismatch"));
    }

    #[test]
    fn coverage_summary_redacts_snapshot_and_removes_typed_authority() {
        let result = materialized_result(SolutionProductFamily::Pc).with_replaced_fields(vec![
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
            ("solution_keys_complete".to_owned(), "false".to_owned()),
            (
                "normalized_solution_set_hash".to_owned(),
                "not-calculated".to_owned(),
            ),
            (
                "actual_normalized_solution_set_hash".to_owned(),
                "not-calculated".to_owned(),
            ),
        ]);

        let public = AppCoreExecutorService::default()
            .postprocess_search_result(result, &ExecutionControl::default())
            .expect("public postprocess");

        assert!(public.solution_set_audit_report().is_none());
        assert!(public.normalized_solution_keys().is_empty());
        assert_eq!(
            public.field("solution_portfolio_snapshot_id"),
            Some("not-materialized")
        );
        assert_eq!(
            public.field("solution_coverage_class_count"),
            Some("not-materialized")
        );
        assert_eq!(
            public.field("solution_set_audit_private_authority"),
            Some("not-materialized")
        );
    }

    #[test]
    fn redacted_audit_path_accepts_exact_observed_peak_and_never_builds_private_report() {
        fn coverage_summary_input() -> CoreExecutionResult {
            materialized_result(SolutionProductFamily::Pc).with_replaced_fields(vec![(
                "search_output_policy".to_owned(),
                "coverage-summary".to_owned(),
            )])
        }

        let mut peak = 0_u128;
        let dry = attach_solution_set_audit_with_memory_guard(
            coverage_summary_input(),
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                peak = peak.max(required);
                Ok(())
            },
        )
        .expect("dry redacted audit");
        assert!(dry.solution_set_audit_report().is_none());
        assert_eq!(
            dry.field("solution_set_audit_private_authority"),
            Some("not-materialized")
        );

        attach_solution_set_audit_with_memory_guard(
            coverage_summary_input(),
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                (required <= peak).then_some(()).ok_or(
                    clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    },
                )
            },
        )
        .expect("exact observed peak");

        let error = attach_solution_set_audit_with_memory_guard(
            coverage_summary_input(),
            &mut |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                (required < peak).then_some(()).ok_or(
                    clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                        component: "test_memory_cap",
                    },
                )
            },
        )
        .expect_err("peak minus one");
        assert_eq!(
            error,
            clearra_core_executor::CoreExecutionError::RuntimeUnavailable {
                component: "test_memory_cap"
            }
        );
    }

    #[test]
    fn ambiguous_output_policy_takes_redacted_path_without_private_audit() {
        let input = materialized_result(SolutionProductFamily::Pc).with_additional_fields(vec![(
            "search_output_policy".to_owned(),
            "coverage-summary".to_owned(),
        )]);
        assert_eq!(input.field_occurrence_count("search_output_policy"), 2);
        let result = attach_solution_set_audit(input);
        assert!(result.solution_set_audit_report().is_none());
        assert_eq!(
            result.field("solution_set_audit_private_authority"),
            Some("not-materialized")
        );
    }

    #[test]
    fn unrelated_audit_boundary_is_a_zero_future_noop() {
        let input = CoreExecutionResult::new(
            vec![("search_kind".to_owned(), "setup".to_owned())],
            Vec::new(),
        );
        let expected = input.clone();
        let mut observed = Vec::new();
        let actual = attach_solution_set_audit_with_memory_guard(input, &mut |_, future| {
            observed.push(future);
            Ok(())
        })
        .expect("unrelated audit");
        assert_eq!(actual, expected);
        assert_eq!(observed, vec![0]);
    }

    #[test]
    fn legacy_numeric_counts_without_explicit_materialized_keys_remain_incomplete() {
        let result = CoreExecutionResult::new(
            vec![
                ("search_output_policy".to_owned(), "summary".to_owned()),
                ("objective".to_owned(), "coverage".to_owned()),
                ("coverage_pattern_count".to_owned(), "1".to_owned()),
                ("count_complete".to_owned(), "true".to_owned()),
                ("packing_candidate_count".to_owned(), "1".to_owned()),
                ("packing_count_complete".to_owned(), "true".to_owned()),
                (
                    "packing_candidate_set_digest".to_owned(),
                    "legacy-packing-digest".to_owned(),
                ),
                ("unique_solution_count".to_owned(), "1".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                (
                    "normalized_solution_key_algorithm".to_owned(),
                    NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM.to_owned(),
                ),
            ],
            Vec::new(),
        );

        let result = attach_solution_set_audit(result);
        let report = result.solution_set_audit_report().expect("typed audit");
        assert!(!report.complete());
        let normalized = report.stage(SolutionSetAuditStageKind::Normalized);
        assert!(!normalized.complete());
        assert!(normalized
            .rejection_reasons()
            .iter()
            .any(|reason| reason == "solution-availability-explicit-contract-missing"));
    }

    #[test]
    fn b2b_audit_is_complete_and_retains_exact_rejection_and_edge_evidence() {
        let edge = ScoringExecutionEdge::new(
            1,
            0,
            PieceKind::T,
            RotationState::Zero,
            4,
            0,
            4,
            3,
            2,
            ScoringLockEvidence::no_rotation(RotationState::Zero),
        )
        .with_perfect_clear(true);
        let graph = SpinCoverageExecutionGraph::new(
            7,
            "solution-a",
            0,
            vec![
                ScoringExecutionNode::new(0, 1, false),
                ScoringExecutionNode::new(1, 0, true),
            ],
            vec![edge],
        );
        let batch = SpinCoverageExecutionBatch::new(
            vec![vec![PieceKind::T]],
            0,
            None,
            true,
            false,
            false,
            1,
            1,
            vec![graph],
            true,
        );
        let input = CoreExecutionResult::new(
            vec![
                ("search_output_policy".to_owned(), "summary".to_owned()),
                ("objective".to_owned(), "coverage".to_owned()),
                ("coverage_pattern_count".to_owned(), "1".to_owned()),
                ("covered_pattern_count".to_owned(), "1".to_owned()),
                ("count_complete".to_owned(), "true".to_owned()),
                ("probability_complete".to_owned(), "true".to_owned()),
                ("packing_candidate_count".to_owned(), "1".to_owned()),
                ("packing_count_complete".to_owned(), "true".to_owned()),
                (
                    "packing_candidate_set_digest".to_owned(),
                    "packing-digest".to_owned(),
                ),
                (
                    "packing_candidate_set_digest_calculated".to_owned(),
                    "true".to_owned(),
                ),
                ("unique_solution_count".to_owned(), "2".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "2".to_owned(),
                ),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "2".to_owned(),
                ),
                ("solution_keys_complete".to_owned(), "true".to_owned()),
                ("solution_page_available".to_owned(), "false".to_owned()),
                (
                    "normalized_solution_set_hash".to_owned(),
                    "solution-digest".to_owned(),
                ),
                (
                    "actual_normalized_solution_set_hash".to_owned(),
                    "solution-digest".to_owned(),
                ),
                (
                    "normalized_solution_key_algorithm".to_owned(),
                    NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM.to_owned(),
                ),
                (
                    "execution_constraint_preserve_b2b".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "execution_constraint_materialized".to_owned(),
                    "false".to_owned(),
                ),
                ("target_piece_count".to_owned(), "1".to_owned()),
                ("objective_search_complete".to_owned(), "true".to_owned()),
                (
                    "postprocess_scoring_requested".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "solution_probabilities_requested".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "execution_constraint_spin_profile".to_owned(),
                    "t-spins".to_owned(),
                ),
            ],
            Vec::new(),
        )
        .with_packing_candidate_keys(vec!["solution-a".to_owned(), "solution-b".to_owned()])
        .with_normalized_solution_keys(vec!["solution-a".to_owned(), "solution-b".to_owned()])
        .with_normalized_solution_coverages(vec![
            NormalizedSolutionCoverage::new("solution-a", bits(1, &[0])),
            NormalizedSolutionCoverage::new("solution-b", bits(1, &[0])),
        ])
        .with_coverage_pattern_words(vec![1])
        .with_spin_coverage_execution_batch(Some(batch.clone()))
        .with_postprocess_execution_batch(Vec::new(), true, vec!["1".to_owned()]);

        let result = AppCoreExecutorService::default()
            .postprocess_search_result(input, &ExecutionControl::default())
            .expect("private postprocess");

        let report = result.solution_set_audit_report().expect("typed audit");
        assert!(report.complete());
        let pre_b2b_hash = normalized_tiling_solution_key_set_hash_from_sorted_strings(&[
            "solution-a".to_owned(),
            "solution-b".to_owned(),
        ]);
        let post_b2b_hash =
            normalized_tiling_solution_key_set_hash_from_sorted_strings(&["solution-a".to_owned()]);
        let produced = report.stage(SolutionSetAuditStageKind::Produced);
        assert_eq!(produced.output_count(), Some(2));
        assert_eq!(produced.output_identity_hash(), Some(pre_b2b_hash.as_str()));
        let execution_validated = report.stage(SolutionSetAuditStageKind::ExecutionValidated);
        assert_eq!(execution_validated.input_count(), Some(2));
        assert_eq!(execution_validated.output_count(), Some(2));
        assert_eq!(
            execution_validated.output_identity_hash(),
            Some(pre_b2b_hash.as_str())
        );
        let spin_b2b_filtered = report.stage(SolutionSetAuditStageKind::SpinB2bFiltered);
        assert_eq!(spin_b2b_filtered.input_count(), Some(2));
        assert_eq!(spin_b2b_filtered.output_count(), Some(1));
        assert_eq!(spin_b2b_filtered.rejection_count(), Some(1));
        assert_eq!(
            spin_b2b_filtered.input_identity_hash(),
            Some(pre_b2b_hash.as_str())
        );
        assert_eq!(
            spin_b2b_filtered.output_identity_hash(),
            Some(post_b2b_hash.as_str())
        );
        assert_eq!(result.spin_coverage_execution_batches(), &[batch]);
        assert_eq!(
            result.spin_coverage_execution_batches()[0].graphs()[0]
                .edges(ScoringExecutionNode::new(0, 1, false)),
            &[edge]
        );
    }
}
