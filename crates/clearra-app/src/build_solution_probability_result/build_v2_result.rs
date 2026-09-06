// SRP rationale: one behavior-level change reason is admitting and sealing the
// exact, query-bound Build coverage portfolio. Its guarded preparation, source
// validation and completion tests share that authority; solver scheduling and
// host response formatting remain in their own owners.

//! Exact Build coverage-portfolio projection.
//!
//! This module consumes only a query-bound, fully authorized Build result. It
//! derives the portfolio universe by OR-ing candidate `PatternBitSet` rows;
//! per-candidate probabilities are never summed. The immutable shared
//! portfolio set is retained so GUI/WASM/CLI can enumerate every exact tie
//! without rerunning search or deep-cloning the source coverage table.

use std::{
    fmt::{self, Write},
    sync::Arc,
};

use clearra_core_executor::{solution_probability_pattern_weights, CoreExecutionResult};
use clearra_coverage::{
    cover::ExactMinimumCoverError,
    pattern::{pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet},
    probability::union_probability::union_probability,
};
#[cfg(test)]
use clearra_problem::ProblemCompiler;
use clearra_problem::{BuildProbabilityAggregation, BuildSolutionProbabilityPolicy};
use sha2::{Digest, Sha256};

use crate::portfolio_alternative_store::{
    CoveragePortfolioAlternativeSet, CoveragePortfolioAlternativeSetPreparation,
    CoveragePortfolioAlternativeSetPreparationAdvance, PortfolioAlternativeError,
    PortfolioAlternativeSetIdentity,
};

use super::{
    build_v2_contract::{BuildTargetSearchContract, ValidatedBuildTargetSearchResultAuthority},
    build_v2_options::BuildObjective,
    validate_build_probability_response, validate_build_solution_probability_reducer_input,
    BuildSolutionProbabilityResultError,
};

pub(crate) const BUILD_COVERAGE_PORTFOLIO_RESULT_CONTRACT: &str = "build-coverage-portfolio.v2";
pub(crate) const BUILD_COVERAGE_PORTFOLIO_PROBABILITY_BASIS: &str =
    "normalized-solution-pattern-bitset-or-union";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuildCoveragePortfolioCompletenessEvidence {
    source_universe_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
    exact_minimum_proven: bool,
    query_bound: bool,
}

impl BuildCoveragePortfolioCompletenessEvidence {
    pub(crate) const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub(crate) const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub(crate) const fn probability_weights_complete(self) -> bool {
        self.probability_weights_complete
    }

    pub(crate) const fn exact_minimum_proven(self) -> bool {
        self.exact_minimum_proven
    }

    pub(crate) const fn query_bound(self) -> bool {
        self.query_bound
    }

    // Retained as the single aggregate completeness predicate for product adapters.
    #[allow(dead_code)]
    pub(crate) const fn complete(self) -> bool {
        self.source_universe_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
            && self.exact_minimum_proven
            && self.query_bound
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildCoveragePortfolioV2Result {
    contract_id: &'static str,
    probability_basis: &'static str,
    authority: ValidatedBuildTargetSearchResultAuthority,
    objective: BuildObjective,
    source_candidate_count: usize,
    selected_candidate_count: usize,
    pattern_count: usize,
    required_pattern_count: usize,
    union_probability: String,
    normalized_solution_set_hash: String,
    canonical_candidate_keys: Vec<String>,
    alternatives: Arc<CoveragePortfolioAlternativeSet>,
    completeness: BuildCoveragePortfolioCompletenessEvidence,
}

impl BuildCoveragePortfolioV2Result {
    pub(crate) const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub(crate) const fn probability_basis(&self) -> &'static str {
        self.probability_basis
    }

    // Retained for product adapters that audit the validator-minted authority.
    #[allow(dead_code)]
    pub(crate) const fn authority(&self) -> &ValidatedBuildTargetSearchResultAuthority {
        &self.authority
    }

    pub(crate) const fn objective(&self) -> BuildObjective {
        self.objective
    }

    pub(crate) const fn source_candidate_count(&self) -> usize {
        self.source_candidate_count
    }

    pub(crate) const fn selected_candidate_count(&self) -> usize {
        self.selected_candidate_count
    }

    pub(crate) const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub(crate) const fn required_pattern_count(&self) -> usize {
        self.required_pattern_count
    }

    pub(crate) fn union_probability(&self) -> &str {
        &self.union_probability
    }

    pub(crate) fn normalized_solution_set_hash(&self) -> &str {
        &self.normalized_solution_set_hash
    }

    pub(crate) fn canonical_candidate_keys(&self) -> &[String] {
        &self.canonical_candidate_keys
    }

    // Retained as the borrowed counterpart to the shared-owner paging seam.
    #[allow(dead_code)]
    pub(crate) fn alternatives(&self) -> &CoveragePortfolioAlternativeSet {
        self.alternatives.as_ref()
    }

    // Retained for callers that need the exact shared alternative-store owner.
    #[allow(dead_code)]
    pub(crate) fn alternative_owner(&self) -> &Arc<CoveragePortfolioAlternativeSet> {
        &self.alternatives
    }

    /// Borrowed product-neutral page source for portfolio objectives. The
    /// validated result can only exist for these two objectives, but the
    /// `Option` keeps the seam compatible with finite Build result families
    /// that do not allocate a live alternative store.
    pub(crate) fn portfolio_alternative_owner(
        &self,
    ) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
        matches!(
            self.objective,
            BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum
        )
        .then_some(&self.alternatives)
    }

    pub(crate) const fn completeness(&self) -> BuildCoveragePortfolioCompletenessEvidence {
        self.completeness
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BuildCoveragePortfolioResultError {
    UnsupportedCapability,
    UnsupportedObjective,
    MissingBaseTargetQuery,
    QueryNotPortfolioCapable,
    QueryProblemMismatch,
    #[cfg(test)]
    QueryCompileFailed,
    Producer(BuildSolutionProbabilityResultError),
    IncompleteEvidence,
    PatternUniverseInvalid,
    NormalizedSolutionSetHashInvalid,
    ProbabilityUnionInvalid,
    // Preserves the exact-cover failure category at this product boundary.
    #[allow(dead_code)]
    MinimumCover(ExactMinimumCoverError),
    Portfolio(PortfolioAlternativeError),
}

/// Projects a complete `build.cover` result into the shared exact portfolio
/// store. `min-cover` and `max-probability-minimum` share this reducer: the
/// maximum attainable probability is the OR-union of all reachable rows, and
/// the second stage minimizes member count while retaining every tie.
#[cfg(test)]
pub(crate) fn validate_build_coverage_portfolio_v2_result(
    authority: ValidatedBuildTargetSearchResultAuthority,
    result: &CoreExecutionResult,
) -> Result<BuildCoveragePortfolioV2Result, BuildCoveragePortfolioResultError> {
    prepare_build_coverage_portfolio_v2_result(authority, result)?.complete(&mut || false)
}

/// Validates producer evidence once, then retains only the exact-cover input
/// and product projection while the host drives bounded work.
#[cfg(test)]
pub(crate) fn prepare_build_coverage_portfolio_v2_result(
    authority: ValidatedBuildTargetSearchResultAuthority,
    result: &CoreExecutionResult,
) -> Result<BuildCoveragePortfolioV2Preparation, BuildCoveragePortfolioResultError> {
    let expected_problem = ProblemCompiler::compile_scenario_pc(
        authority
            .query()
            .base_target_query()
            .ok_or(BuildCoveragePortfolioResultError::MissingBaseTargetQuery)?
            .core_query(),
    )
    .map_err(|_| BuildCoveragePortfolioResultError::QueryCompileFailed)?;
    prepare_build_coverage_portfolio_v2_result_with_memory_guard(
        authority,
        result,
        &expected_problem,
        &mut |_| Ok(()),
    )
}

pub(crate) fn prepare_build_coverage_portfolio_v2_result_with_memory_guard(
    authority: ValidatedBuildTargetSearchResultAuthority,
    result: &CoreExecutionResult,
    expected_problem: &clearra_problem::SearchProblem,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<BuildCoveragePortfolioV2Preparation, BuildCoveragePortfolioResultError> {
    if authority.contract() != BuildTargetSearchContract::Cover {
        return Err(BuildCoveragePortfolioResultError::UnsupportedCapability);
    }
    let objective = authority.query().options().objective();
    if !matches!(
        objective,
        BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum
    ) {
        return Err(BuildCoveragePortfolioResultError::UnsupportedObjective);
    }
    let query = authority
        .query()
        .base_target_query()
        .ok_or(BuildCoveragePortfolioResultError::MissingBaseTargetQuery)?;
    if expected_problem.problem_kind() != clearra_problem::SearchProblemKind::ScenarioPc
        || !expected_problem
            .core_query()
            .eq_after_initial_line_clear(query.core_query())
    {
        return Err(BuildCoveragePortfolioResultError::QueryProblemMismatch);
    }
    if query.aggregation() != BuildProbabilityAggregation::Buildability
        || query.finesse_metric().requested()
        || query.solution_probability_policy() != BuildSolutionProbabilityPolicy::Include
    {
        return Err(BuildCoveragePortfolioResultError::QueryNotPortfolioCapable);
    }

    validate_build_probability_response(
        query.finesse_request(),
        query.field(),
        query.aggregation(),
        query.solution_probability_policy(),
        result,
    )
    .map_err(BuildCoveragePortfolioResultError::Producer)?;
    let state = validate_build_solution_probability_reducer_input(
        Some(BuildSolutionProbabilityPolicy::Include),
        result,
    )
    .map_err(BuildCoveragePortfolioResultError::Producer)?;
    if !state.requested
        || !state.complete
        || !state.count_complete
        || !state.probability_complete
        || !state.solution_keys_complete
        || state.resource_truncated
    {
        return Err(BuildCoveragePortfolioResultError::IncompleteEvidence);
    }

    let pattern_count = unique_usize(result, "coverage_pattern_count")
        .ok_or(BuildCoveragePortfolioResultError::PatternUniverseInvalid)?;
    // The producer stays borrowed until preparation returns. The caller owns
    // that live source; this guard adds the reducer's new input/projection
    // owners before allocating their buffers. Shared row storage is charged
    // conservatively because a later Core projection may populate its cache.
    let overflow = || {
        BuildCoveragePortfolioResultError::MinimumCover(ExactMinimumCoverError::ProjectionOverflow)
    };
    let mut input_peak = (core::mem::size_of::<BuildCoveragePortfolioV2Preparation>() as u128)
        .checked_add(
            authority
                .query()
                .checked_retained_capacity_bytes()
                .ok_or_else(overflow)?,
        )
        .and_then(|bytes| {
            // Includes the owned word Vec and either dense or sparse compact
            // conversion's overlapping buffers and sparse owner metadata.
            bytes.checked_add(
                PatternBitSet::checked_external_words_materialize_union_future_bytes(
                    pattern_count,
                )?,
            )
        })
        .and_then(|bytes| {
            bytes.checked_add(
                (result.normalized_solution_keys().len() as u128)
                    .checked_mul(core::mem::size_of::<String>() as u128)?,
            )
        })
        .and_then(|bytes| {
            bytes.checked_add(
                (result.normalized_solution_coverages().len() as u128)
                    .checked_mul(core::mem::size_of::<PatternBitSet>() as u128)?,
            )
        })
        .ok_or_else(overflow)?;
    for key in result.normalized_solution_keys() {
        input_peak = input_peak
            .checked_add(key.len() as u128)
            .ok_or_else(overflow)?;
    }
    for coverage in result.normalized_solution_coverages() {
        input_peak = input_peak
            .checked_add(
                coverage
                    .covered_patterns()
                    .checked_storage_retained_bytes()
                    .ok_or_else(overflow)?,
            )
            .ok_or_else(overflow)?;
    }
    guard(input_peak).map_err(BuildCoveragePortfolioResultError::MinimumCover)?;
    let required =
        PatternBitSet::from_words(pattern_count, result.coverage_pattern_words().to_vec())
            .map_err(|_| BuildCoveragePortfolioResultError::PatternUniverseInvalid)?;
    // A fully validated zero-success source has one exact empty portfolio,
    // not an execution failure. Completeness is checked before this point.

    let candidate_keys = result.normalized_solution_keys().to_vec();
    let rows = result
        .normalized_solution_coverages()
        .iter()
        .map(|coverage| coverage.covered_patterns().clone())
        .collect::<Vec<_>>();
    if candidate_keys.len() != rows.len()
        || rows.iter().any(|row| row.pattern_count() != pattern_count)
    {
        return Err(BuildCoveragePortfolioResultError::PatternUniverseInvalid);
    }

    let normalized_solution_set_hash = result
        .unique_field("normalized_solution_set_hash")
        .filter(|value| !value.is_empty() && *value != "not-calculated")
        .ok_or(BuildCoveragePortfolioResultError::NormalizedSolutionSetHashInvalid)?;
    if result.unique_field("actual_normalized_solution_set_hash")
        != Some(normalized_solution_set_hash)
    {
        return Err(BuildCoveragePortfolioResultError::NormalizedSolutionSetHashInvalid);
    }

    let mut reducer_live = (core::mem::size_of::<BuildCoveragePortfolioV2Preparation>() as u128)
        .checked_add(
            authority
                .query()
                .checked_retained_capacity_bytes()
                .ok_or_else(overflow)?,
        )
        .and_then(|bytes| bytes.checked_add(required.checked_storage_retained_bytes()?))
        .and_then(|bytes| {
            bytes.checked_add(
                (candidate_keys.capacity() as u128)
                    .checked_mul(core::mem::size_of::<String>() as u128)?,
            )
        })
        .and_then(|bytes| {
            bytes.checked_add(
                (rows.capacity() as u128)
                    .checked_mul(core::mem::size_of::<PatternBitSet>() as u128)?,
            )
        })
        .ok_or_else(overflow)?;
    for key in &candidate_keys {
        reducer_live = reducer_live
            .checked_add(key.capacity() as u128)
            .ok_or_else(overflow)?;
    }
    for row in &rows {
        reducer_live = reducer_live
            .checked_add(row.checked_storage_retained_bytes().ok_or_else(overflow)?)
            .ok_or_else(overflow)?;
    }
    let mut weight_format_bytes = 0_u128;
    for value in result.postprocess_pattern_weights() {
        if let Ok(number) = value.parse::<f64>() {
            weight_format_bytes =
                weight_format_bytes.max(checked_format_length(format_args!("{number}"))? as u128);
        }
    }
    // The parsed Vec and immutable Arc slice overlap during conversion. The
    // canonical-format check also owns one bounded temporary decimal string.
    let weights_peak = (result.postprocess_pattern_weights().len() as u128)
        .checked_mul(2 * core::mem::size_of::<f64>() as u128)
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<WeightedPatternSet>() as u128))
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Vec<f64>>() as u128))
        .and_then(|bytes| bytes.checked_add(weight_format_bytes))
        .ok_or_else(overflow)?;
    guard(
        reducer_live
            .checked_add(weights_peak)
            .ok_or_else(overflow)?,
    )
    .map_err(BuildCoveragePortfolioResultError::MinimumCover)?;
    let weights = solution_probability_pattern_weights(result)
        .map_err(|_| BuildCoveragePortfolioResultError::ProbabilityUnionInvalid)?;
    reducer_live = reducer_live
        .checked_add(
            weights
                .checked_storage_retained_bytes()
                .ok_or_else(overflow)?,
        )
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<WeightedPatternSet>() as u128))
        .ok_or_else(overflow)?;
    let union_value = union_probability(&required, &weights)
        .map_err(|_| BuildCoveragePortfolioResultError::ProbabilityUnionInvalid)?
        .get();
    let union_probability =
        guarded_format(format_args!("{union_value}"), &mut reducer_live, guard)?;
    let identity = PortfolioAlternativeSetIdentity::new(
        build_query_identity(
            &authority,
            query,
            expected_problem.problem_id().as_str(),
            &mut reducer_live,
            guard,
        )?,
        guarded_format(
            format_args!("build-normalized-solution-source.v1:{normalized_solution_set_hash}"),
            &mut reducer_live,
            guard,
        )?,
        guarded_format(
            format_args!(
                "rule:{}:kick:{}:queue-knowledge:{}",
                expected_problem.rule_profile_value().id().as_str(),
                expected_problem.kick_profile().profile_id().as_str(),
                authority.query().options().queue_knowledge().as_str(),
            ),
            &mut reducer_live,
            guard,
        )?,
        pattern_universe_identity(pattern_count, result, &mut reducer_live, guard)?,
        product_build_identity_component(&mut reducer_live, guard)?,
    )
    .map_err(BuildCoveragePortfolioResultError::Portfolio)?;
    let projection = BuildCoveragePortfolioProjection {
        authority,
        objective,
        pattern_count,
        required_pattern_count: required.count_ones() as usize,
        union_probability,
        normalized_solution_set_hash: guarded_format(
            format_args!("{normalized_solution_set_hash}"),
            &mut reducer_live,
            guard,
        )?,
    };
    drop(weights);
    let outer = (core::mem::size_of::<BuildCoveragePortfolioV2Preparation>() as u128)
        .checked_sub(core::mem::size_of::<CoveragePortfolioAlternativeSetPreparation>() as u128)
        .and_then(|bytes| bytes.checked_add(projection.checked_retained_capacity_bytes()?))
        .ok_or_else(overflow)?;
    let portfolio = CoveragePortfolioAlternativeSetPreparation::new_with_memory_guard(
        identity,
        candidate_keys,
        required,
        rows,
        &mut |peak| {
            guard(
                outer
                    .checked_add(peak)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )
        },
    )
    .map_err(BuildCoveragePortfolioResultError::Portfolio)?;
    Ok(BuildCoveragePortfolioV2Preparation {
        projection: Some(projection),
        portfolio,
    })
}

struct BuildCoveragePortfolioProjection {
    authority: ValidatedBuildTargetSearchResultAuthority,
    objective: BuildObjective,
    pattern_count: usize,
    required_pattern_count: usize,
    union_probability: String,
    normalized_solution_set_hash: String,
}

impl BuildCoveragePortfolioProjection {
    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.authority
            .query()
            .checked_retained_capacity_bytes()?
            .checked_add(self.union_probability.capacity() as u128)?
            .checked_add(self.normalized_solution_set_hash.capacity() as u128)
    }
}

pub(crate) enum BuildCoveragePortfolioV2PreparationAdvance {
    Pending { work_steps: u64 },
    Completed(BuildCoveragePortfolioV2Result),
    Cancelled { work_steps: u64 },
}

pub(crate) struct BuildCoveragePortfolioV2Preparation {
    projection: Option<BuildCoveragePortfolioProjection>,
    portfolio: CoveragePortfolioAlternativeSetPreparation,
}

impl BuildCoveragePortfolioV2Preparation {
    pub(crate) fn parallel_work(&self) -> &CoveragePortfolioAlternativeSetPreparation {
        &self.portfolio
    }

    pub(crate) fn parallel_work_mut(&mut self) -> &mut CoveragePortfolioAlternativeSetPreparation {
        &mut self.portfolio
    }

    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.portfolio
            .checked_retained_capacity_bytes()?
            .checked_add(self.projection.as_ref().map_or(
                Some(0),
                BuildCoveragePortfolioProjection::checked_retained_capacity_bytes,
            )?)
    }

    pub(crate) fn advance_with_memory_guard(
        &mut self,
        work: u64,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<BuildCoveragePortfolioV2PreparationAdvance, BuildCoveragePortfolioResultError> {
        let overflow = || {
            BuildCoveragePortfolioResultError::MinimumCover(
                ExactMinimumCoverError::ProjectionOverflow,
            )
        };
        let projection_heap = self
            .projection
            .as_ref()
            .map_or(
                Some(0),
                BuildCoveragePortfolioProjection::checked_retained_capacity_bytes,
            )
            .ok_or_else(overflow)?;
        let outer = (core::mem::size_of::<Self>() as u128)
            .checked_sub(core::mem::size_of::<CoveragePortfolioAlternativeSetPreparation>() as u128)
            .and_then(|bytes| bytes.checked_add(projection_heap))
            .ok_or_else(overflow)?;
        let advanced = self
            .portfolio
            .advance_with_memory_guard(
                work,
                &mut |peak| {
                    guard(
                        outer
                            .checked_add(peak)
                            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                    )
                },
                cancelled,
            )
            .map_err(BuildCoveragePortfolioResultError::Portfolio)?;
        match advanced {
            CoveragePortfolioAlternativeSetPreparationAdvance::Pending { work_steps } => {
                Ok(BuildCoveragePortfolioV2PreparationAdvance::Pending { work_steps })
            }
            CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled { work_steps } => {
                self.projection = None;
                Ok(BuildCoveragePortfolioV2PreparationAdvance::Cancelled { work_steps })
            }
            CoveragePortfolioAlternativeSetPreparationAdvance::Completed(portfolio) => {
                let projection = self
                    .projection
                    .take()
                    .ok_or(BuildCoveragePortfolioResultError::IncompleteEvidence)?;
                let selected = portfolio.canonical_page().portfolio().candidate_ids();
                let mut key_bytes = (selected.len() as u128)
                    .checked_mul(core::mem::size_of::<String>() as u128)
                    .ok_or_else(overflow)?;
                for id in selected {
                    let index: usize = id
                        .checked_sub(1)
                        .and_then(|value| value.try_into().ok())
                        .ok_or(BuildCoveragePortfolioResultError::IncompleteEvidence)?;
                    let candidate = portfolio
                        .candidates()
                        .get(index)
                        .filter(|candidate| candidate.candidate_id() == *id)
                        .ok_or(BuildCoveragePortfolioResultError::IncompleteEvidence)?;
                    key_bytes = key_bytes
                        .checked_add(candidate.normalized_key().len() as u128)
                        .ok_or_else(overflow)?;
                }
                // Account for the drained preparation, typed return carrier,
                // Arc allocation/temporary value, and the requested key copies.
                let peak = (core::mem::size_of::<Self>() as u128)
                    .checked_add(
                        self.portfolio
                            .checked_retained_capacity_bytes()
                            .ok_or_else(overflow)?,
                    )
                    .and_then(|bytes| bytes.checked_add(projection_heap))
                    .and_then(|bytes| {
                        bytes.checked_add(portfolio.checked_retained_capacity_bytes()?)
                    })
                    .and_then(|bytes| {
                        bytes.checked_add(
                            2 * core::mem::size_of::<CoveragePortfolioAlternativeSet>() as u128,
                        )
                    })
                    .and_then(|bytes| bytes.checked_add(2 * core::mem::size_of::<usize>() as u128))
                    .and_then(|bytes| {
                        bytes.checked_add(
                            core::mem::size_of::<BuildCoveragePortfolioV2Result>() as u128
                        )
                    })
                    .and_then(|bytes| bytes.checked_add(key_bytes))
                    .ok_or_else(overflow)?;
                guard(peak).map_err(BuildCoveragePortfolioResultError::MinimumCover)?;
                let alternatives = Arc::new(portfolio);
                // Copy directly from dense IDs. The convenience projection
                // builds an intermediate Vec<&str>, which is unnecessary here.
                let selected = alternatives.canonical_page().portfolio().candidate_ids();
                let mut canonical_candidate_keys = Vec::<String>::new();
                canonical_candidate_keys
                    .try_reserve_exact(selected.len())
                    .map_err(|_| overflow())?;
                let mut copy_live = peak
                    .checked_sub(key_bytes)
                    .and_then(|bytes| bytes.checked_add(core::mem::size_of::<String>() as u128))
                    .and_then(|bytes| {
                        bytes.checked_add(
                            (canonical_candidate_keys.capacity() as u128)
                                .checked_mul(core::mem::size_of::<String>() as u128)?,
                        )
                    })
                    .ok_or_else(overflow)?;
                guard(copy_live).map_err(BuildCoveragePortfolioResultError::MinimumCover)?;
                for id in selected {
                    let index: usize = id
                        .checked_sub(1)
                        .and_then(|value| value.try_into().ok())
                        .ok_or(BuildCoveragePortfolioResultError::IncompleteEvidence)?;
                    let key = alternatives
                        .candidates()
                        .get(index)
                        .filter(|candidate| candidate.candidate_id() == *id)
                        .ok_or(BuildCoveragePortfolioResultError::IncompleteEvidence)?
                        .normalized_key();
                    let value = guarded_format(format_args!("{key}"), &mut copy_live, guard)?;
                    canonical_candidate_keys.push(value);
                }
                let actual_key_bytes = canonical_candidate_keys.iter().try_fold(
                    (canonical_candidate_keys.capacity() as u128)
                        .checked_mul(core::mem::size_of::<String>() as u128)
                        .ok_or_else(overflow)?,
                    |bytes, key| {
                        bytes
                            .checked_add(key.capacity() as u128)
                            .ok_or_else(overflow)
                    },
                )?;
                guard(
                    peak.checked_sub(key_bytes)
                        .and_then(|bytes| bytes.checked_add(actual_key_bytes))
                        .ok_or_else(overflow)?,
                )
                .map_err(BuildCoveragePortfolioResultError::MinimumCover)?;
                Ok(BuildCoveragePortfolioV2PreparationAdvance::Completed(
                    BuildCoveragePortfolioV2Result {
                        contract_id: BUILD_COVERAGE_PORTFOLIO_RESULT_CONTRACT,
                        probability_basis: BUILD_COVERAGE_PORTFOLIO_PROBABILITY_BASIS,
                        authority: projection.authority,
                        objective: projection.objective,
                        source_candidate_count: alternatives.candidates().len(),
                        selected_candidate_count: canonical_candidate_keys.len(),
                        pattern_count: projection.pattern_count,
                        required_pattern_count: projection.required_pattern_count,
                        union_probability: projection.union_probability,
                        normalized_solution_set_hash: projection.normalized_solution_set_hash,
                        canonical_candidate_keys,
                        alternatives,
                        completeness: BuildCoveragePortfolioCompletenessEvidence {
                            source_universe_complete: true,
                            coverage_rows_complete: true,
                            probability_weights_complete: true,
                            exact_minimum_proven: true,
                            query_bound: true,
                        },
                    },
                ))
            }
        }
    }

    pub(crate) fn complete(
        mut self,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<BuildCoveragePortfolioV2Result, BuildCoveragePortfolioResultError> {
        loop {
            match self.advance_with_memory_guard(u64::MAX, &mut |_| Ok(()), cancelled)? {
                BuildCoveragePortfolioV2PreparationAdvance::Pending { work_steps }
                    if work_steps > 0 => {}
                BuildCoveragePortfolioV2PreparationAdvance::Completed(result) => return Ok(result),
                _ => return Err(BuildCoveragePortfolioResultError::IncompleteEvidence),
            }
        }
    }
}

fn build_query_identity(
    authority: &ValidatedBuildTargetSearchResultAuthority,
    query: &clearra_problem::BuildProbabilityQuery,
    core_problem_id: &str,
    live: &mut u128,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<String, BuildCoveragePortfolioResultError> {
    let field = query.field();
    guarded_format(
        format_args!(
            "{}:{}:{}:{}:{}:{:?}:{:?}:{}:{}:{}",
            authority.contract().capability_id(),
            authority.contract().problem_contract_id(),
            core_problem_id,
            field.height(),
            field.includes_horizontal_mirror(),
            field.base_words(),
            field.target_words(),
            query.aggregation().as_str(),
            authority.query().options().queue_knowledge().as_str(),
            authority.query().options().objective().as_str(),
        ),
        live,
        guard,
    )
}

fn pattern_universe_identity(
    pattern_count: usize,
    result: &CoreExecutionResult,
    live: &mut u128,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<String, BuildCoveragePortfolioResultError> {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.build-pattern-universe.v1\0");
    hasher.update((pattern_count as u64).to_be_bytes());
    for word in result.coverage_pattern_words() {
        hasher.update(word.to_be_bytes());
    }
    for weight in result.postprocess_pattern_weights() {
        hasher.update((weight.len() as u64).to_be_bytes());
        hasher.update(weight.as_bytes());
    }
    guarded_format(
        format_args!(
            "build-pattern-universe.v1:{}",
            HexDigest(&hasher.finalize())
        ),
        live,
        guard,
    )
}

fn product_build_identity_component(
    live: &mut u128,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<String, BuildCoveragePortfolioResultError> {
    let identity = clearra_host_contract::ProductBuildIdentity::current();
    guarded_format(
        format_args!(
            "product-build.v1:{}:{}:{}:{}:{}",
            identity.engine_build_id(),
            identity.source_commit(),
            identity.contract_schema_version(),
            identity.supply_semantics_id(),
            identity.artifact_schema_version(),
        ),
        live,
        guard,
    )
}

#[cfg(test)]
fn union_probability_from_pattern_weights(
    union: &PatternBitSet,
    weights: &WeightedPatternSet,
) -> Result<String, BuildCoveragePortfolioResultError> {
    let probability = union_probability(union, weights)
        .map_err(|_| BuildCoveragePortfolioResultError::ProbabilityUnionInvalid)?;
    Ok(probability.get().to_string())
}

fn unique_usize(result: &CoreExecutionResult, key: &str) -> Option<usize> {
    result.unique_field(key)?.parse::<usize>().ok()
}

struct HexDigest<'a>(&'a [u8]);

impl fmt::Display for HexDigest<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(output, "{byte:02x}")?;
        }
        Ok(())
    }
}

struct FormatLength(usize);

impl fmt::Write for FormatLength {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0 = self.0.checked_add(text.len()).ok_or(fmt::Error)?;
        Ok(())
    }
}

fn checked_format_length(
    args: fmt::Arguments<'_>,
) -> Result<usize, BuildCoveragePortfolioResultError> {
    let mut length = FormatLength(0);
    length.write_fmt(args).map_err(|_| {
        BuildCoveragePortfolioResultError::MinimumCover(ExactMinimumCoverError::ProjectionOverflow)
    })?;
    Ok(length.0)
}

/// Primitive query/identity formatting is run first into an allocation-free
/// counter, then into one exactly reserved buffer under the same memory owner.
fn guarded_format(
    args: fmt::Arguments<'_>,
    live: &mut u128,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<String, BuildCoveragePortfolioResultError> {
    let overflow = || {
        BuildCoveragePortfolioResultError::MinimumCover(ExactMinimumCoverError::ProjectionOverflow)
    };
    let length = checked_format_length(args)?;
    guard(live.checked_add(length as u128).ok_or_else(overflow)?)
        .map_err(BuildCoveragePortfolioResultError::MinimumCover)?;
    let mut value = String::new();
    value.try_reserve_exact(length).map_err(|_| overflow())?;
    *live = live
        .checked_add(value.capacity() as u128)
        .ok_or_else(overflow)?;
    guard(*live).map_err(BuildCoveragePortfolioResultError::MinimumCover)?;
    value.write_fmt(args).map_err(|_| overflow())?;
    if value.len() != length {
        return Err(overflow());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
        probability::probability_value::ProbabilityValue,
    };
    use clearra_pc_graph::request::{
        PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    use clearra_problem::{
        BuildProbabilityField, BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
        ProblemCompiler,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{
        prepare_build_coverage_portfolio_v2_result_with_memory_guard,
        union_probability_from_pattern_weights, validate_build_coverage_portfolio_v2_result,
        BuildCoveragePortfolioResultError, BuildCoveragePortfolioV2PreparationAdvance,
        BUILD_COVERAGE_PORTFOLIO_PROBABILITY_BASIS, BUILD_COVERAGE_PORTFOLIO_RESULT_CONTRACT,
    };
    use crate::{
        build_solution_probability_result::{
            build_probability_resource_test_guard,
            build_v2_contract::{
                BuildTargetSearchQuerySnapshot, ReportedBuildTargetSearchResultIdentity,
                ValidatedBuildTargetSearchResultAuthority,
            },
            build_v2_options::{BuildObjective, BuildV2OptionRequest},
        },
        AppCoreExecutorService,
    };
    use clearra_coverage::pattern::{
        pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
    };

    fn build_cover_core_query(piece: PieceKind) -> PcScenarioQuery {
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![piece])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        // These fixtures exercise result authority and exact-cover reduction,
        // not process-owned native pool/provider registration. Keep the real
        // one-session Build producer independent of the CI host's CPU count.
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_workers(1)
                .with_worker_hardware_limit(1),
        )
    }

    fn build_cover_query(
        solution_probability_policy: BuildSolutionProbabilityPolicy,
    ) -> BuildProbabilityQuery {
        let core = build_cover_core_query(PieceKind::I);
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("canonical one-piece target");
        BuildProbabilityQuery::new(core, field)
            .with_solution_probability_policy(solution_probability_policy)
    }

    fn authority_for(query: &BuildProbabilityQuery) -> ValidatedBuildTargetSearchResultAuthority {
        let snapshot = BuildTargetSearchQuerySnapshot::cover(query.clone())
            .expect("valid Build coverage query")
            .with_options(BuildV2OptionRequest::default().with_objective(BuildObjective::MinCover))
            .expect("coverage objective is supported");
        let contract = snapshot.contract();
        ValidatedBuildTargetSearchResultAuthority::validate(
            snapshot,
            ReportedBuildTargetSearchResultIdentity::new(
                contract.capability_id(),
                contract.problem_contract_id(),
                contract.input_schema_id(),
                contract.result_contract_id(),
            ),
        )
        .expect("fieldwise identity matches the query-owned contract")
    }

    fn execute(query: &BuildProbabilityQuery) -> clearra_core_executor::CoreExecutionResult {
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
            .expect("one-piece Build problem compiles");
        assert_eq!(
            problem.backend_request().workers(),
            1,
            "result-contract fixtures must not require a process-wide native provider"
        );
        AppCoreExecutorService::wasm_cpu()
            .execute_build_probability_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                &ExecutionControl::default(),
            )
            .expect("one-piece Build producer completes")
    }

    #[test]
    fn complete_build_cover_result_mints_exact_shared_portfolio() {
        let _resource_guard = build_probability_resource_test_guard();
        let query = build_cover_query(BuildSolutionProbabilityPolicy::Include);
        let result = execute(&query);
        let portfolio = validate_build_coverage_portfolio_v2_result(authority_for(&query), &result)
            .expect("complete query-bound result is portfolio-authorized");

        assert_eq!(
            portfolio.contract_id(),
            BUILD_COVERAGE_PORTFOLIO_RESULT_CONTRACT
        );
        assert_eq!(
            portfolio.probability_basis(),
            BUILD_COVERAGE_PORTFOLIO_PROBABILITY_BASIS
        );
        assert_eq!(portfolio.objective(), BuildObjective::MinCover);
        assert_eq!(portfolio.source_candidate_count(), 1);
        assert_eq!(portfolio.selected_candidate_count(), 1);
        assert_eq!(portfolio.pattern_count(), 1);
        assert_eq!(portfolio.required_pattern_count(), 1);
        assert_eq!(portfolio.union_probability(), "1");
        assert!(portfolio.completeness().complete());
        assert_eq!(portfolio.alternatives().candidates().len(), 1);
        assert_eq!(
            portfolio.canonical_candidate_keys(),
            &[portfolio.alternatives().candidates()[0]
                .normalized_key()
                .to_owned()]
        );
        assert_eq!(
            portfolio
                .alternatives()
                .canonical_page()
                .portfolio()
                .candidate_ids(),
            &[1]
        );
        assert_eq!(
            portfolio.alternative_owner().identity(),
            portfolio.alternatives().identity()
        );
    }

    #[test]
    fn bounded_build_cover_preparation_matches_direct_complete_result() {
        let _resource_guard = build_probability_resource_test_guard();
        let query = build_cover_query(BuildSolutionProbabilityPolicy::Include);
        let result = execute(&query);
        let expected = validate_build_coverage_portfolio_v2_result(authority_for(&query), &result)
            .expect("direct exact portfolio");
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query()).unwrap();
        let mut peak = 0_u128;
        let mut guard = |bytes| {
            peak = peak.max(bytes);
            Ok(())
        };
        let mut preparation = prepare_build_coverage_portfolio_v2_result_with_memory_guard(
            authority_for(&query),
            &result,
            &problem,
            &mut guard,
        )
        .expect("admitted preparation");
        let mut completed = None;
        for _ in 0..10_000 {
            match preparation
                .advance_with_memory_guard(1, &mut guard, &mut || false)
                .expect("bounded exact step")
            {
                BuildCoveragePortfolioV2PreparationAdvance::Pending { work_steps } => {
                    assert!(work_steps <= 1)
                }
                BuildCoveragePortfolioV2PreparationAdvance::Completed(actual) => {
                    completed = Some(actual);
                    break;
                }
                BuildCoveragePortfolioV2PreparationAdvance::Cancelled { .. } => {
                    panic!("uncancelled source cannot cancel")
                }
            }
        }
        assert_eq!(
            completed.expect("bounded exact preparation completes"),
            expected
        );
        assert!(peak > 0);
    }

    #[test]
    fn build_cover_constructor_rejection_never_publishes_exact_evidence() {
        let _resource_guard = build_probability_resource_test_guard();
        let query = build_cover_query(BuildSolutionProbabilityPolicy::Include);
        let result = execute(&query);
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query()).unwrap();
        let rejected = prepare_build_coverage_portfolio_v2_result_with_memory_guard(
            authority_for(&query),
            &result,
            &problem,
            &mut |_| Err(clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow),
        );
        assert!(matches!(
            rejected,
            Err(BuildCoveragePortfolioResultError::MinimumCover(
                clearra_coverage::cover::ExactMinimumCoverError::ProjectionOverflow,
            ))
        ));
    }

    #[test]
    fn build_cover_cancelled_preparation_does_not_complete() {
        let _resource_guard = build_probability_resource_test_guard();
        let query = build_cover_query(BuildSolutionProbabilityPolicy::Include);
        let result = execute(&query);
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query()).unwrap();
        let mut preparation = prepare_build_coverage_portfolio_v2_result_with_memory_guard(
            authority_for(&query),
            &result,
            &problem,
            &mut |_| Ok(()),
        )
        .expect("admitted preparation");
        assert!(matches!(
            preparation
                .advance_with_memory_guard(1, &mut |_| Ok(()), &mut || true)
                .expect("cancellation is not proof failure"),
            BuildCoveragePortfolioV2PreparationAdvance::Cancelled { .. }
        ));
    }

    #[test]
    fn build_cover_rejects_a_different_source_problem_before_minting_query_bound_evidence() {
        let _resource_guard = build_probability_resource_test_guard();
        let query = build_cover_query(BuildSolutionProbabilityPolicy::Include);
        let result = execute(&query);
        let other_query = build_cover_core_query(PieceKind::O);
        let other_problem = ProblemCompiler::compile_scenario_pc(&other_query).unwrap();
        assert!(matches!(
            prepare_build_coverage_portfolio_v2_result_with_memory_guard(
                authority_for(&query),
                &result,
                &other_problem,
                &mut |_| Ok(()),
            ),
            Err(BuildCoveragePortfolioResultError::QueryProblemMismatch)
        ));
    }

    #[test]
    fn complete_unreachable_build_cover_has_one_exact_empty_portfolio() {
        let _resource_guard = build_probability_resource_test_guard();
        let core = build_cover_core_query(PieceKind::O);
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0]).unwrap();
        let query = BuildProbabilityQuery::new(core, field)
            .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
        let result = execute(&query);
        let portfolio = validate_build_coverage_portfolio_v2_result(authority_for(&query), &result)
            .expect("complete unreachable source is an exact empty result");
        assert_eq!(portfolio.source_candidate_count(), 0);
        assert_eq!(portfolio.selected_candidate_count(), 0);
        assert_eq!(portfolio.pattern_count(), 1);
        assert_eq!(portfolio.required_pattern_count(), 0);
        assert_eq!(portfolio.union_probability(), "0");
        assert!(portfolio.completeness().complete());
        assert!(portfolio.canonical_candidate_keys().is_empty());
        assert!(portfolio
            .alternatives()
            .canonical_page()
            .portfolio()
            .candidate_ids()
            .is_empty());
    }

    #[test]
    fn omitted_probability_evidence_never_becomes_a_portfolio_winner() {
        let _resource_guard = build_probability_resource_test_guard();
        let query = build_cover_query(BuildSolutionProbabilityPolicy::Omit);
        let result = execute(&query);
        assert_eq!(
            validate_build_coverage_portfolio_v2_result(authority_for(&query), &result),
            Err(BuildCoveragePortfolioResultError::QueryNotPortfolioCapable)
        );
    }

    #[test]
    fn union_probability_is_an_or_union_and_never_a_candidate_sum() {
        let both = PatternBitSet::from_words(2, vec![0b11]).expect("two-pattern union");
        let first = PatternBitSet::from_words(2, vec![0b01]).expect("first pattern");
        let weights = WeightedPatternSet::new(vec![
            ProbabilityValue::new(0.4).expect("first weight"),
            ProbabilityValue::new(0.6).expect("second weight"),
        ])
        .expect("complete weight set");

        assert_eq!(
            union_probability_from_pattern_weights(&both, &weights)
                .expect("complete union probability"),
            "1"
        );
        assert_eq!(
            union_probability_from_pattern_weights(&first, &weights)
                .expect("partial union probability"),
            "0.4"
        );
    }
}
