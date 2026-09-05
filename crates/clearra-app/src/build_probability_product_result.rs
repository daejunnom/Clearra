//! Query-bound products for Build probability result aggregations.
//!
//! This module is the sole adapter from executed Build evidence to the public
//! field-average and complete-replay families.  Presenters receive finite DTOs
//! and never reconstruct score or path semantics from display fields.

use std::{collections::BTreeMap, sync::Arc};

use clearra_core_domain::solution::NormalizedTilingSolutionKey;
use clearra_core_executor::{CoreExecutionResult, CorePostProcessExecution};
use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};
use clearra_host_contract::{
    BuildPathFamilyPayload, BuildV2CompletenessPayload, BuildV2ProductPayload,
    BuildV2ScoreWinnerPayload, PcPathStepPayload, PcPathWitnessPayload, PcScoreFieldPayload,
    PcScoreFieldSummaryPayload, ProductBuildIdentity, ProductResultPayload,
    ProductResultPayloadContent, ScorePatternWinnerFamilyPayload, ScorePatternWinnerPayload,
};
use clearra_problem::{BuildProbabilityQuery, ProblemCompiler};
use sha2::{Digest, Sha256};

use crate::pc_score_field_result::{
    PC_SCORE_OVERALL_SCORE_BASIS, PC_SCORE_SOLUTION_FIELD_AVERAGE_BASIS,
};
use crate::pc_score_postprocess::PcScoreDerivation;
use crate::portfolio_alternative_store::{
    CoveragePortfolioAlternativeSet, PortfolioAlternativeSetIdentity, ProductPageSourceOwner,
};

pub const BUILD_FIELD_AVERAGE_CAPABILITY: &str = "build.field-average-score";
pub const BUILD_FIELD_AVERAGE_RESULT_CONTRACT: &str = "build-field-average-score.v1";
pub const BUILD_PATH_CAPABILITY: &str = "build.complete-replay-paths";
pub const BUILD_PATH_RESULT_CONTRACT: &str = "build-path-family.v1";
pub const BUILD_PATH_WITNESS_CONTRACT: &str = "build-path-witness.v1";
pub const BUILD_PATH_ORDERING: &str =
    "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending";
pub const BUILD_PATH_CANONICAL_SELECTION: &str = "smallest-canonical-candidate-id";
pub const BUILD_FIXED_SCORE_CAPABILITY: &str = "build.fixed-queue-maximum-score";
pub const BUILD_FIXED_SCORE_RESULT_CONTRACT: &str = "build-fixed-score-witness.v1";
pub const BUILD_FIXED_SCORE_WINNER_CONTRACT: &str = "build-score-pattern-winner.v1";
pub const BUILD_SCORE_MINIMUM_CAPABILITY: &str = "build.highest-score-minimum-set";
pub const BUILD_SCORE_MINIMUM_RESULT_CONTRACT: &str = "build-probability-score-minimum.v1";

const SCORE_WINNER_ORDERING: &str = "pattern-id-ascending-then-candidate-id-ascending";
const SCORE_WINNER_EQUALITY: &str = "score-only-attack-informational";
const SCORE_ATTACK_BASIS: &str = "canonical-equal-score-trace";
const PRODUCT_MEMBER_PAGE_SIZE: &str = "100";
const BUILD_PRODUCT_PORTABLE_WHOLE_LIVE_BYTES: u128 = 16_u128 * 1024_u128 * 1024_u128;
const BUILD_PRODUCT_WORKSPACE_FIXED_BYTES: u128 = 64_u128 * 1024_u128;
const BUILD_PRODUCT_WORKSPACE_BYTES_PER_ITEM: u128 = 1024;
const BUILD_PRODUCT_WORKSPACE_VARIABLE_EXPANSION: u128 = 8;

/// One fail-closed whole-live admission shared by every Build-probability
/// result adapter. Core evidence is still live while a typed payload (and, for
/// portfolios, its restartable page owner) is created, so neither a result
/// count nor an allocator's willingness to grow is release authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuildProductMemoryBudget {
    maximum_bytes: u128,
    source_bytes: u128,
}

impl BuildProductMemoryBudget {
    fn from_query_and_result(
        query: &BuildProbabilityQuery,
        result: &CoreExecutionResult,
    ) -> Result<Self, &'static str> {
        Self::from_query_result_and_external(query, result, 0)
    }

    fn from_query_result_and_external(
        query: &BuildProbabilityQuery,
        result: &CoreExecutionResult,
        external_retained_bytes: u128,
    ) -> Result<Self, &'static str> {
        let occurrences = result.field_occurrence_count("execution_max_memory_mib");
        if occurrences > 1 {
            return Err("Build product memory authority is missing or duplicated");
        }
        let runtime_limit = match (occurrences, result.unique_field("execution_max_memory_mib")) {
            (0, None) | (1, Some("none")) => BUILD_PRODUCT_PORTABLE_WHOLE_LIVE_BYTES,
            (1, Some(value)) => value
                .parse::<u128>()
                .ok()
                .and_then(|mib| mib.checked_mul(1024_u128 * 1024_u128))
                .ok_or("Build product memory authority is invalid")?,
            _ => return Err("Build product memory authority is missing or duplicated"),
        };
        let maximum_bytes = runtime_limit.min(BUILD_PRODUCT_PORTABLE_WHOLE_LIVE_BYTES);
        let source_bytes = result
            .checked_resource_retained_bytes()
            .and_then(|bytes| bytes.checked_add(query.checked_retained_capacity_bytes()?))
            .and_then(|bytes| bytes.checked_add(external_retained_bytes))
            .ok_or("Build product memory projection overflow")?;
        if source_bytes > maximum_bytes {
            return Err("Build product whole-live memory limit exceeded");
        }
        Ok(Self {
            maximum_bytes,
            source_bytes,
        })
    }

    fn reserve_linear_workspace(
        self,
        item_count: usize,
        variable_bytes: u128,
    ) -> Result<Self, &'static str> {
        let projected = (item_count as u128)
            .checked_mul(BUILD_PRODUCT_WORKSPACE_BYTES_PER_ITEM)
            .and_then(|bytes| {
                variable_bytes
                    .checked_mul(BUILD_PRODUCT_WORKSPACE_VARIABLE_EXPANSION)
                    .and_then(|variable| bytes.checked_add(variable))
            })
            .and_then(|bytes| bytes.checked_add(BUILD_PRODUCT_WORKSPACE_FIXED_BYTES))
            .ok_or("Build product memory projection overflow")?;
        self.admit_future(projected)?;
        Ok(Self {
            maximum_bytes: self.maximum_bytes,
            source_bytes: self
                .source_bytes
                .checked_add(projected)
                .ok_or("Build product memory projection overflow")?,
        })
    }

    fn admit_future(self, future_bytes: u128) -> Result<(), &'static str> {
        if self
            .source_bytes
            .checked_add(future_bytes)
            .is_none_or(|whole_live| whole_live > self.maximum_bytes)
        {
            return Err("Build product whole-live memory limit exceeded");
        }
        Ok(())
    }

    fn authorize_payload(
        self,
        payload: &ProductResultPayload,
        owner: Option<&ProductPageSourceOwner>,
    ) -> Result<(), &'static str> {
        let payload_bytes = payload
            .checked_retained_capacity_bytes()
            .ok_or("Build product memory projection overflow")?;
        let owner_bytes = match owner {
            Some(owner) => owner
                .checked_retained_capacity_bytes()
                .ok_or("Build product memory projection overflow")?,
            None => 0,
        };
        let product_bytes = payload_bytes
            .checked_add(owner_bytes)
            .ok_or("Build product memory projection overflow")?;
        self.admit_future(product_bytes)
    }
}

fn checked_total_string_lengths<'a>(values: impl IntoIterator<Item = &'a String>) -> Option<u128> {
    values.into_iter().try_fold(0_u128, |bytes, value| {
        bytes.checked_add(value.len() as u128)
    })
}

fn try_clone_string(value: &str) -> Result<String, &'static str> {
    let mut owned = String::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| "Build product allocation failed")?;
    owned.push_str(value);
    Ok(owned)
}

pub(crate) fn build_field_average_payload(
    query: &BuildProbabilityQuery,
    result: &CoreExecutionResult,
    derivation: &PcScoreDerivation,
) -> Result<ProductResultPayload, &'static str> {
    let derivation_bytes = derivation
        .checked_retained_capacity_bytes()
        .ok_or("Build product memory projection overflow")?;
    let budget =
        BuildProductMemoryBudget::from_query_result_and_external(query, result, derivation_bytes)?;
    if !query.core_query().objective().score().requested()
        || !derivation.execution_source_complete()
        || result.bool_field("score_summary_complete") != Some(true)
        || result.bool_field("score_matrix_complete") != Some(true)
        || result.bool_field("score_evaluation_complete") != Some(true)
    {
        return Err("Build field-average score evidence is incomplete");
    }
    let fields = derivation.solution_field_average_owner();
    let identities = result.normalized_solution_identities();
    if fields.len() != identities.len() {
        return Err("Build field-average row count does not match the solution identity family");
    }
    if fields.iter().zip(identities).any(|(field, identity)| {
        field.field_identity() != *identity
            || field.normalized_field_key()
                != NormalizedTilingSolutionKey::from_standard_board64_identity(*identity)
    }) {
        return Err("Build field-average row identity does not match the solution identity family");
    }
    if fields.iter().any(|field| {
        !field.score_complete()
            || field.pattern_count()
                != result
                    .usize_field("materialized_pattern_count")
                    .unwrap_or(0)
    }) {
        return Err("Build field-average row evidence is incomplete");
    }
    let required = |key| {
        result
            .unique_field(key)
            .filter(|value| !value.is_empty())
            .ok_or("Build field-average score metadata is missing")
    };
    let materialized_pattern_count = required("materialized_pattern_count")?;
    let scored_pattern_count = required("score_pattern_optimal_count")?;
    let failed_pattern_count = required("score_failed_pc_pattern_count")?;
    let overall_score_basis = required("score_field_average_basis")?;
    if overall_score_basis != PC_SCORE_OVERALL_SCORE_BASIS {
        return Err("Build field-average score basis does not match the complete universe");
    }
    let materialized_count = materialized_pattern_count
        .parse::<usize>()
        .map_err(|_| "Build field-average pattern count is invalid")?;
    let scored_count = scored_pattern_count
        .parse::<usize>()
        .map_err(|_| "Build field-average scored pattern count is invalid")?;
    let failed_count = failed_pattern_count
        .parse::<usize>()
        .map_err(|_| "Build field-average failed pattern count is invalid")?;
    if scored_count.checked_add(failed_count) != Some(materialized_count) {
        return Err("Build field-average scored and failed rows do not cover the universe");
    }
    let budget = budget.reserve_linear_workspace(
        fields.len(),
        checked_total_string_lengths(result.normalized_solution_keys())
            .ok_or("Build product memory projection overflow")?,
    )?;
    let mut payload_fields = Vec::new();
    payload_fields
        .try_reserve_exact(fields.len())
        .map_err(|_| "Build product allocation failed")?;
    for field in fields.iter() {
        let average_score = field.average_score();
        payload_fields.push(PcScoreFieldPayload::new(
            field.normalized_field_key().to_string(),
            if average_score == 0.0 {
                "0".to_owned()
            } else {
                average_score.to_string()
            },
            field.covered_pattern_count().to_string(),
            field.pattern_count().to_string(),
            field.score_complete(),
        ));
    }
    let payload = ProductResultPayload::new(
        BUILD_FIELD_AVERAGE_CAPABILITY,
        BUILD_FIELD_AVERAGE_RESULT_CONTRACT,
        ProductResultPayloadContent::PcScoreFieldSummary(PcScoreFieldSummaryPayload::new(
            "build-solution-field-average.v1",
            "normalized-solution-field-order",
            PC_SCORE_SOLUTION_FIELD_AVERAGE_BASIS,
            required("score_evaluation_basis")?,
            required("score_evaluation_scope")?,
            overall_score_basis,
            required("piece_source_id")?,
            required("pattern_universe_id")?,
            required("pattern_weight_model_id")?,
            materialized_pattern_count,
            fields.len().to_string(),
            scored_pattern_count,
            failed_pattern_count,
            required("score_covered_probability")?,
            required("score_field_average_score")?,
            result
                .unique_field("score_covered_pattern_conditional_average_score")
                .map(ToOwned::to_owned),
            true,
            payload_fields,
        )),
    );
    budget.authorize_payload(&payload, None)?;
    Ok(payload)
}

pub(crate) fn build_complete_replay_payload(
    query: &BuildProbabilityQuery,
    result: &CoreExecutionResult,
) -> Result<ProductResultPayload, &'static str> {
    let budget = BuildProductMemoryBudget::from_query_and_result(query, result)?;
    if !query.field().is_compact() {
        return Err("Build complete replay paths currently require a compact six-row field");
    }
    if result.bool_field("resource_truncated") == Some(true)
        || result.bool_field("count_complete") != Some(true)
        || result.bool_field("objective_complete") != Some(true)
        || result.bool_field("objective_search_complete") != Some(true)
        || !result.postprocess_execution_complete()
    {
        return Err("Build replay evidence is incomplete");
    }
    let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
        .map_err(|_| "Build replay query did not compile")?;
    let expected_terminal = query
        .field()
        .compact_final_board_mask()
        .ok_or("Build replay terminal board is unavailable")?;
    let pattern_count = result
        .usize_field("materialized_pattern_count")
        .filter(|count| *count > 0)
        .ok_or("Build replay pattern count is unavailable")?;
    let execution_count = result.postprocess_executions().len();
    let step_count = result
        .postprocess_executions()
        .iter()
        .try_fold(0_usize, |count, execution| {
            count.checked_add(execution.replay_trace().solution_trace().steps().len())
        })
        .ok_or("Build replay step count overflow")?;
    let variable_bytes = result
        .postprocess_executions()
        .iter()
        .try_fold(0_u128, |bytes, execution| {
            bytes
                .checked_add(execution.trace_identity().len() as u128)?
                .checked_add(execution.replay_trace().canonical_key().len() as u128)
        })
        .ok_or("Build product memory projection overflow")?;
    let budget = budget.reserve_linear_workspace(
        execution_count
            .checked_add(step_count)
            .ok_or("Build replay step count overflow")?,
        variable_bytes,
    )?;
    let mut producer_ids = Vec::new();
    producer_ids
        .try_reserve_exact(execution_count)
        .map_err(|_| "Build product allocation failed")?;
    producer_ids.extend(
        result
            .postprocess_executions()
            .iter()
            .map(CorePostProcessExecution::candidate_id),
    );
    producer_ids.sort_unstable();
    producer_ids.dedup();
    let mut witnesses = Vec::new();
    witnesses
        .try_reserve_exact(execution_count)
        .map_err(|_| "Build product allocation failed")?;
    for execution in result.postprocess_executions() {
        let producer_index = producer_ids
            .binary_search(&execution.candidate_id())
            .map_err(|_| "Build replay canonical candidate id is missing")?;
        let candidate_id = u64::try_from(producer_index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or("Build replay canonical candidate id overflow")?;
        witnesses.push(project_execution(
            &problem,
            execution,
            pattern_count,
            expected_terminal,
            candidate_id,
        )?);
    }
    witnesses.sort_by(|left, right| {
        (
            left.candidate_id(),
            left.pattern_id(),
            left.normalized_trace_key(),
            left.trace_identity(),
        )
            .cmp(&(
                right.candidate_id(),
                right.pattern_id(),
                right.normalized_trace_key(),
                right.trace_identity(),
            ))
    });
    if witnesses.windows(2).any(|pair| {
        pair[0].candidate_id() == pair[1].candidate_id()
            && pair[0].pattern_id() == pair[1].pattern_id()
            && pair[0].normalized_trace_key() == pair[1].normalized_trace_key()
            && pair[0].trace_identity() == pair[1].trace_identity()
    }) {
        return Err("Build replay identity is duplicated");
    }
    let canonical_witness = witnesses.first().cloned();
    let payload = ProductResultPayload::new(
        BUILD_PATH_CAPABILITY,
        BUILD_PATH_RESULT_CONTRACT,
        ProductResultPayloadContent::BuildPathFamily(BuildPathFamilyPayload::new(
            BUILD_PATH_WITNESS_CONTRACT,
            BUILD_PATH_ORDERING,
            problem.problem_id().as_str(),
            format!("0x{expected_terminal:016x}"),
            pattern_count.to_string(),
            witnesses.len().to_string(),
            true,
            BUILD_PATH_CANONICAL_SELECTION,
            canonical_witness,
            witnesses,
        )),
    );
    budget.authorize_payload(&payload, None)?;
    Ok(payload)
}

/// Projects the fixed-queue Build score product from the complete Core score
/// cell family. Score is the sole equality coordinate; attack is copied only
/// from the lexicographically canonical equal-score trace.
pub(crate) fn build_fixed_queue_max_score_payload(
    query: &BuildProbabilityQuery,
    result: &CoreExecutionResult,
    derivation: &PcScoreDerivation,
) -> Result<ProductResultPayload, &'static str> {
    let derivation_bytes = derivation
        .checked_retained_capacity_bytes()
        .ok_or("Build product memory projection overflow")?;
    let budget =
        BuildProductMemoryBudget::from_query_result_and_external(query, result, derivation_bytes)?;
    if query
        .core_query()
        .remaining_queue()
        .as_fixed_sequence()
        .is_none()
    {
        return Err("Build fixed-queue maximum score requires one exact fixed queue");
    }
    let pattern_count = validated_score_pattern_count(result, derivation)?;
    if pattern_count != 1 {
        return Err("Build fixed-queue maximum score requires one materialized source pattern");
    }
    let candidate_map = validated_score_candidate_map(result)?;
    let winners_by_identity = validated_score_winners(derivation, &candidate_map, pattern_count)?;
    let budget = budget.reserve_linear_workspace(
        candidate_map
            .len()
            .checked_add(winners_by_identity.len())
            .ok_or("Build score product item count overflow")?,
        candidate_map
            .values()
            .try_fold(0_u128, |bytes, (_, key)| {
                bytes.checked_add(key.len() as u128)
            })
            .ok_or("Build product memory projection overflow")?,
    )?;
    let best_score = winners_by_identity
        .values()
        .map(|winner| winner.score())
        .max()
        .ok_or("Build fixed-queue score has no reachable candidate")?;
    if winners_by_identity
        .values()
        .any(|winner| winner.pattern_id() != 0 || winner.score() != best_score)
    {
        return Err("Build fixed-queue score winners do not share one exact maximum");
    }
    let winner_count = winners_by_identity
        .iter()
        .filter(|((pattern_id, _), winner)| *pattern_id == 0 && winner.score() == best_score)
        .count();
    let mut winners = Vec::new();
    winners
        .try_reserve_exact(winner_count)
        .map_err(|_| "Build product allocation failed")?;
    for ((pattern_id, candidate_id), winner) in winners_by_identity {
        if pattern_id == 0 && winner.score() == best_score {
            let (_, key) = candidate_map
                .get(&winner.solution_identity())
                .ok_or("Build fixed-queue winner candidate is missing")?;
            winners.push(ScorePatternWinnerPayload::new(
                "0",
                candidate_id.to_string(),
                try_clone_string(key)?,
                winner.score().to_string(),
                winner.informational_attack().to_string(),
            ));
        }
    }
    winners.sort_by_key(|winner| winner.candidate_id().parse::<u64>().unwrap_or(u64::MAX));
    let canonical = winners
        .first()
        .cloned()
        .ok_or("Build fixed-queue score has no canonical winner")?;
    let payload = ProductResultPayload::new(
        BUILD_FIXED_SCORE_CAPABILITY,
        BUILD_FIXED_SCORE_RESULT_CONTRACT,
        ProductResultPayloadContent::ScorePatternWinnerFamily(
            ScorePatternWinnerFamilyPayload::new(
                BUILD_FIXED_SCORE_WINNER_CONTRACT,
                SCORE_WINNER_ORDERING,
                SCORE_WINNER_EQUALITY,
                SCORE_ATTACK_BASIS,
                PRODUCT_MEMBER_PAGE_SIZE,
                winners.len().to_string(),
                BUILD_PATH_CANONICAL_SELECTION,
                canonical,
                winners,
            ),
        ),
    );
    budget.authorize_payload(&payload, None)?;
    Ok(payload)
}

/// Builds a score-only exact minimum portfolio over the Build candidates that
/// attain each successful source pattern's maximum score. This is a nominal
/// Build-probability adapter: it retains the exact ctk1 candidate identities
/// instead of weakening them to colored-field equivalence.
pub(crate) fn build_highest_score_minimum_payload(
    query: &BuildProbabilityQuery,
    result: &CoreExecutionResult,
    derivation: &PcScoreDerivation,
) -> Result<(ProductResultPayload, ProductPageSourceOwner), &'static str> {
    let derivation_bytes = derivation
        .checked_retained_capacity_bytes()
        .ok_or("Build product memory projection overflow")?;
    let budget =
        BuildProductMemoryBudget::from_query_result_and_external(query, result, derivation_bytes)?;
    let pattern_count = validated_score_pattern_count(result, derivation)?;
    let budget = budget.reserve_linear_workspace(
        result
            .normalized_solution_keys()
            .len()
            .checked_add(derivation.pattern_winners().len())
            .and_then(|count| count.checked_add(pattern_count))
            .ok_or("Build score-minimum item count overflow")?,
        checked_total_string_lengths(result.normalized_solution_keys())
            .ok_or("Build product memory projection overflow")?,
    )?;
    let candidate_map = validated_score_candidate_map(result)?;
    let maxima = validated_score_winners(derivation, &candidate_map, pattern_count)?;
    let mut required_words = Vec::new();
    required_words
        .try_reserve_exact(result.coverage_pattern_words().len())
        .map_err(|_| "Build product allocation failed")?;
    required_words.extend_from_slice(result.coverage_pattern_words());
    let required = PatternBitSet::from_words(pattern_count, required_words)
        .map_err(|_| "Build score-minimum coverage union is invalid")?;
    if required.is_empty() {
        return Err("Build score-minimum has no successful source pattern");
    }

    let mut best_by_pattern = BTreeMap::<usize, u64>::new();
    for ((pattern_id, _), winner) in &maxima {
        best_by_pattern
            .entry(*pattern_id)
            .and_modify(|score| *score = (*score).max(winner.score()))
            .or_insert(winner.score());
    }
    for pattern_id in 0..pattern_count {
        if required.contains(PatternId::new(pattern_id))
            != best_by_pattern.contains_key(&pattern_id)
        {
            return Err("Build score-minimum winners do not match successful source coverage");
        }
    }

    let mut eligible_by_key = BTreeMap::<String, (u64, PatternBitSet)>::new();
    for ((pattern_id, candidate_id), winner) in &maxima {
        if best_by_pattern.get(pattern_id) != Some(&winner.score()) {
            continue;
        }
        let (_, key) = candidate_map
            .get(&winner.solution_identity())
            .ok_or("Build score-minimum candidate is missing")?;
        let row = eligible_by_key
            .entry(key.clone())
            .or_insert_with(|| (*candidate_id, PatternBitSet::new(pattern_count)));
        if row.0 != *candidate_id {
            return Err("Build score-minimum candidate identity is ambiguous");
        }
        row.1
            .insert(PatternId::new(*pattern_id))
            .map_err(|_| "Build score-minimum pattern index is invalid")?;
    }
    let mut candidate_keys = Vec::new();
    let mut public_candidate_ids = Vec::new();
    let mut rows = Vec::new();
    for values in [
        candidate_keys.try_reserve_exact(eligible_by_key.len()),
        rows.try_reserve_exact(eligible_by_key.len()),
    ] {
        values.map_err(|_| "Build product allocation failed")?;
    }
    public_candidate_ids
        .try_reserve_exact(eligible_by_key.len())
        .map_err(|_| "Build product allocation failed")?;
    for (key, (candidate_id, row)) in &eligible_by_key {
        candidate_keys.push(try_clone_string(key)?);
        public_candidate_ids.push(*candidate_id);
        rows.push(row.clone());
    }
    let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
        .map_err(|_| "Build score-minimum query did not compile")?;
    let score = query.core_query().objective().score();
    let product_build = ProductBuildIdentity::current();
    let identity = PortfolioAlternativeSetIdentity::new(
        format!(
            "build-probability-score-minimum.v1:{}",
            problem.problem_id().as_str()
        ),
        format!(
            "piece-source:{}:solutions:{}",
            required_field(result, "piece_source_id")?,
            required_field(result, "actual_normalized_solution_set_hash")?
        ),
        format!(
            "score:{}:initial-b2b:{}:equality:score-only",
            score.profile().as_str(),
            score.initial_b2b()
        ),
        format!(
            "pattern-universe:{}:weights:{}:count:{}",
            required_field(result, "pattern_universe_id")?,
            required_field(result, "pattern_weight_model_id")?,
            pattern_count
        ),
        format!(
            "{}:{}:{}:{}:{}",
            product_build.engine_build_id(),
            product_build.source_commit(),
            product_build.contract_schema_version(),
            product_build.supply_semantics_id(),
            product_build.artifact_schema_version()
        ),
    )
    .map_err(|_| "Build score-minimum product identity is invalid")?;
    let owner = CoveragePortfolioAlternativeSet::new_canonical(
        identity,
        candidate_keys,
        required.clone(),
        rows,
    )
    .and_then(|owner| owner.with_public_candidate_ids(public_candidate_ids))
    .map(Arc::new)
    .map_err(|_| "Build score-minimum exact portfolio failed")?;
    let canonical_keys = owner
        .canonical_candidate_keys_owned()
        .map_err(|_| "Build score-minimum canonical portfolio is unavailable")?;
    let mut selected = Vec::new();
    selected
        .try_reserve_exact(canonical_keys.len())
        .map_err(|_| "Build product allocation failed")?;
    for key in &canonical_keys {
        selected.push(try_clone_string(key)?);
    }
    selected.sort_unstable();
    let winner_count = usize::try_from(required.count_ones())
        .map_err(|_| "Build score-minimum winner count overflow")?;
    let mut public_winners = Vec::new();
    public_winners
        .try_reserve_exact(winner_count)
        .map_err(|_| "Build product allocation failed")?;
    for pattern_id in 0..pattern_count {
        if !required.contains(PatternId::new(pattern_id)) {
            continue;
        }
        let (candidate_id, key, winner) = maxima
            .iter()
            .filter(|((candidate_pattern, _), winner)| {
                *candidate_pattern == pattern_id
                    && best_by_pattern.get(&pattern_id) == Some(&winner.score())
            })
            .filter_map(|((_, candidate_id), winner)| {
                let (_, key) = candidate_map.get(&winner.solution_identity())?;
                selected.binary_search(key).is_ok().then_some((
                    *candidate_id,
                    key.as_str(),
                    *winner,
                ))
            })
            .min_by(|left, right| (left.1, left.0).cmp(&(right.1, right.0)))
            .ok_or("Build score-minimum canonical portfolio does not cover a winner pattern")?;
        public_winners.push(
            BuildV2ScoreWinnerPayload::try_new(
                pattern_id.to_string(),
                key,
                winner.score().to_string(),
                winner.informational_attack().to_string(),
            )
            .map_err(|_| "Build score-minimum winner payload is invalid")?,
        );
        let _ = candidate_id;
    }
    let input_identity = sha256_hex(&[
        problem.problem_id().as_str(),
        required_field(result, "actual_normalized_solution_set_hash")?,
        score.profile().as_str(),
        &score.initial_b2b().to_string(),
    ]);
    let payload = BuildV2ProductPayload::try_score_portfolio(
        BUILD_SCORE_MINIMUM_CAPABILITY,
        BUILD_SCORE_MINIMUM_RESULT_CONTRACT,
        input_identity,
        score.profile().as_str(),
        score.initial_b2b().to_string(),
        "basic-approximation",
        false,
        "score-only",
        SCORE_ATTACK_BASIS,
        candidate_map.len().to_string(),
        eligible_by_key.len().to_string(),
        canonical_keys.len().to_string(),
        pattern_count.to_string(),
        required.count_ones().to_string(),
        canonical_keys,
        public_winners,
        BuildV2CompletenessPayload::new(true, true, true, true, true, true, true),
        owner.set_identity_sha256(),
    )
    .map_err(|_| "Build score-minimum Host payload is invalid")?;
    let payload = ProductResultPayload::new(
        BUILD_SCORE_MINIMUM_CAPABILITY,
        BUILD_SCORE_MINIMUM_RESULT_CONTRACT,
        ProductResultPayloadContent::BuildV2(payload),
    );
    let owner = ProductPageSourceOwner::CoveragePortfolio(owner);
    budget.authorize_payload(&payload, Some(&owner))?;
    Ok((payload, owner))
}

pub(crate) fn decorate_build_failed_queues(
    query: &BuildProbabilityQuery,
    result: CoreExecutionResult,
    failed_pattern_limit: usize,
) -> Result<CoreExecutionResult, &'static str> {
    let budget = BuildProductMemoryBudget::from_query_and_result(query, &result)?;
    if result.bool_field("probability_complete") != Some(true)
        || result.bool_field("count_complete") != Some(true)
        || result.bool_field("resource_truncated") == Some(true)
    {
        return Err("Build failed-queue coverage evidence is incomplete");
    }
    let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
        .map_err(|_| "Build failed-queue query did not compile")?;
    let universe = problem
        .piece_source()
        .materialized_universe()
        .filter(|universe| {
            problem.piece_source().complete()
                && universe.complete()
                && universe.total_possible_pattern_count() == universe.pattern_count() as u128
        })
        .ok_or("Build failed-queue materialized universe is incomplete")?;
    let mut coverage_words = Vec::new();
    coverage_words
        .try_reserve_exact(result.coverage_pattern_words().len())
        .map_err(|_| "Build product allocation failed")?;
    coverage_words.extend_from_slice(result.coverage_pattern_words());
    let coverage = PatternBitSet::from_words(universe.pattern_count(), coverage_words)
        .map_err(|_| "Build failed-queue coverage words are invalid")?;
    let failed_count = universe
        .pattern_count()
        .saturating_sub(coverage.count_ones() as usize);
    let example_limit = failed_pattern_limit.min(failed_count);
    let sequence_bytes = (0..universe.pattern_count())
        .filter(|pattern_index| !coverage.contains(PatternId::new(*pattern_index)))
        .take(example_limit)
        .try_fold(0_u128, |bytes, pattern_index| {
            bytes.checked_add(universe.sequence_at(pattern_index).len() as u128)
        })
        .ok_or("Build failed-queue memory projection overflow")?;
    budget.reserve_linear_workspace(example_limit, sequence_bytes)?;
    let failed_probability = exact_build_failed_probability(&result)?;
    if result.field_occurrence_count("failed_pattern_count") != 1
        || result.usize_field("failed_pattern_count") != Some(failed_count)
        || result.field_occurrence_count("covered_pattern_count") != 1
        || result.usize_field("covered_pattern_count")
            != Some(universe.pattern_count().saturating_sub(failed_count))
    {
        return Err("Build failed-queue coverage counts do not match the exact complement");
    }
    const RESERVED_FAILED_QUEUE_FIELDS: &[&str] = &[
        "result_mode",
        "build_failed_queue_contract",
        "failed_queue_probability",
        "total_pattern_count",
        "failed_pattern_scope",
        "failed_pattern_count_complete",
        "failed_pattern_limit",
        "failed_pattern_examples_materialized",
        "failed_pattern_examples_truncated",
    ];
    if RESERVED_FAILED_QUEUE_FIELDS
        .iter()
        .any(|key| result.field_occurrence_count(key) != 0)
        || result.summary_field_entries().any(|(key, _)| {
            key.strip_prefix("failed_pattern_")
                .is_some_and(|suffix| suffix.bytes().all(|byte| byte.is_ascii_digit()))
        })
    {
        return Err("Build failed-queue product fields already exist");
    }
    let mut fields = Vec::new();
    fields
        .try_reserve_exact(
            RESERVED_FAILED_QUEUE_FIELDS
                .len()
                .checked_add(example_limit)
                .ok_or("Build failed-queue field count overflow")?,
        )
        .map_err(|_| "Build product allocation failed")?;
    fields.extend([
        ("result_mode".to_owned(), "build-failed-queues".to_owned()),
        (
            "build_failed_queue_contract".to_owned(),
            "exact-build-coverage-complement.v1".to_owned(),
        ),
        (
            "failed_queue_probability".to_owned(),
            failed_probability.to_owned(),
        ),
        (
            "total_pattern_count".to_owned(),
            universe.pattern_count().to_string(),
        ),
        (
            "failed_pattern_scope".to_owned(),
            "materialized-build-universe".to_owned(),
        ),
        (
            "failed_pattern_count_complete".to_owned(),
            "true".to_owned(),
        ),
        ("failed_pattern_limit".to_owned(), example_limit.to_string()),
    ]);
    let mut materialized = 0_usize;
    for pattern_index in 0..universe.pattern_count() {
        if materialized == example_limit {
            break;
        }
        if coverage.contains(PatternId::new(pattern_index)) {
            continue;
        }
        let source_sequence = universe.sequence_at(pattern_index);
        let mut sequence = String::new();
        sequence
            .try_reserve_exact(source_sequence.len())
            .map_err(|_| "Build product allocation failed")?;
        sequence.extend(source_sequence.iter().map(|piece| piece.as_ascii()));
        fields.push((format!("failed_pattern_{materialized}"), sequence));
        materialized += 1;
    }
    fields.extend([
        (
            "failed_pattern_examples_materialized".to_owned(),
            materialized.to_string(),
        ),
        (
            "failed_pattern_examples_truncated".to_owned(),
            (materialized < failed_count).to_string(),
        ),
    ]);
    let decorated = result.with_additional_fields(fields);
    let decorated_bytes = decorated
        .checked_resource_retained_bytes()
        .and_then(|bytes| bytes.checked_add(query.checked_retained_capacity_bytes()?))
        .ok_or("Build product memory projection overflow")?;
    if decorated_bytes > budget.maximum_bytes {
        return Err("Build product whole-live memory limit exceeded");
    }
    Ok(decorated)
}

fn validated_score_pattern_count(
    result: &CoreExecutionResult,
    derivation: &PcScoreDerivation,
) -> Result<usize, &'static str> {
    if !derivation.execution_source_complete()
        || result.bool_field("score_summary_complete") != Some(true)
        || result.bool_field("score_matrix_complete") != Some(true)
        || result.bool_field("score_evaluation_complete") != Some(true)
    {
        return Err("Build score evidence is incomplete");
    }
    result
        .usize_field("materialized_pattern_count")
        .filter(|count| *count > 0)
        .ok_or("Build score pattern universe is unavailable")
}

fn validated_score_candidate_map(
    result: &CoreExecutionResult,
) -> Result<
    BTreeMap<clearra_core_domain::solution::StandardBoard64TilingIdentity, (u64, String)>,
    &'static str,
> {
    let identities = result.normalized_solution_identities();
    let keys = result.normalized_solution_keys();
    if identities.is_empty() || identities.len() != keys.len() {
        return Err("Build score candidate family is incomplete");
    }
    if identities.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("Build score candidate identities are not in canonical candidate-id order");
    }
    let mut map = BTreeMap::new();
    for (index, (identity, key)) in identities.iter().copied().zip(keys).enumerate() {
        if NormalizedTilingSolutionKey::from_standard_board64_identity(identity).as_str() != key {
            return Err("Build score candidate key does not match its identity");
        }
        let candidate_id = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or("Build score candidate id overflow")?;
        if map.insert(identity, (candidate_id, key.clone())).is_some() {
            return Err("Build score candidate identity is duplicated");
        }
    }
    Ok(map)
}

fn validated_score_winners<'a>(
    derivation: &'a PcScoreDerivation,
    candidate_map: &BTreeMap<
        clearra_core_domain::solution::StandardBoard64TilingIdentity,
        (u64, String),
    >,
    pattern_count: usize,
) -> Result<
    BTreeMap<(usize, u64), &'a crate::pc_score_winner_result::PcScorePatternWinnerV1>,
    &'static str,
> {
    let mut winners = BTreeMap::new();
    for winner in derivation.pattern_winners() {
        if winner.pattern_id() >= pattern_count {
            return Err("Build score winner pattern identity is invalid");
        }
        let (candidate_id, _) = candidate_map
            .get(&winner.solution_identity())
            .ok_or("Build score winner references an unknown candidate")?;
        if *candidate_id != winner.candidate_id() {
            return Err("Build score winner candidate identity is inconsistent");
        }
        if winners
            .insert((winner.pattern_id(), *candidate_id), winner)
            .is_some()
        {
            return Err("Build score winner identity is duplicated");
        }
    }
    Ok(winners)
}

fn required_field<'a>(result: &'a CoreExecutionResult, key: &str) -> Result<&'a str, &'static str> {
    result
        .unique_field(key)
        .filter(|value| !value.is_empty())
        .ok_or("Build score identity metadata is missing")
}

fn sha256_hex(fields: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn exact_build_failed_probability(result: &CoreExecutionResult) -> Result<&str, &'static str> {
    for field in [
        "coverage_probability",
        "failed_coverage_probability",
        "materialized_probability_mass",
        "coverage_probability_denominator",
    ] {
        if result.field_occurrence_count(field) != 1 {
            return Err("Build failed-queue probability authority is missing or duplicated");
        }
    }
    if result.field("coverage_probability_denominator")
        != Some("full-materialized-pattern-universe")
        || result.field("materialized_probability_mass") != Some("1")
    {
        return Err("Build failed-queue probability denominator is not the complete universe");
    }
    let success = result
        .field("coverage_probability")
        .and_then(canonical_probability_decimal)
        .ok_or("Build success probability is not canonical")?;
    let failed_text = result
        .field("failed_coverage_probability")
        .ok_or("Build failed probability is unavailable")?;
    let failed = canonical_probability_decimal(failed_text)
        .ok_or("Build failed probability is not canonical")?;
    if !decimal_probabilities_sum_to_one(success, failed) {
        return Err("Build success and failed probabilities do not sum to one");
    }
    Ok(failed_text)
}

/// Parses Rust's canonical finite probability spelling into an exact base-10
/// integer/scale pair. The product adapter only verifies the engine-owned
/// complement; it never derives or rounds a replacement probability.
fn canonical_probability_decimal(value: &str) -> Option<(u128, u32)> {
    let parsed = value.parse::<f64>().ok()?;
    if !parsed.is_finite() || !(0.0..=1.0).contains(&parsed) {
        return None;
    }
    let canonical = if parsed == 0.0 {
        "0".to_owned()
    } else if parsed == 1.0 {
        "1".to_owned()
    } else {
        parsed.to_string()
    };
    if canonical != value {
        return None;
    }
    let (mantissa, exponent) = match value.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
        None => (value, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{whole}{fraction}").parse::<u128>().ok()?;
    let scale = i32::try_from(fraction.len()).ok()?.checked_sub(exponent)?;
    if scale >= 0 {
        Some((digits, u32::try_from(scale).ok()?))
    } else {
        let multiplier = 10_u128.checked_pow(scale.unsigned_abs())?;
        Some((digits.checked_mul(multiplier)?, 0))
    }
}

fn decimal_probabilities_sum_to_one(left: (u128, u32), right: (u128, u32)) -> bool {
    let scale = left.1.max(right.1);
    let Some(left_factor) = 10_u128.checked_pow(scale.saturating_sub(left.1)) else {
        return false;
    };
    let Some(right_factor) = 10_u128.checked_pow(scale.saturating_sub(right.1)) else {
        return false;
    };
    let Some(unit) = 10_u128.checked_pow(scale) else {
        return false;
    };
    left.0.checked_mul(left_factor).and_then(|left| {
        right
            .0
            .checked_mul(right_factor)
            .and_then(|right| left.checked_add(right))
    }) == Some(unit)
}

fn project_execution(
    problem: &clearra_problem::SearchProblem,
    execution: &CorePostProcessExecution,
    pattern_count: usize,
    expected_terminal: u64,
    candidate_id: u64,
) -> Result<PcPathWitnessPayload, &'static str> {
    if execution.pattern_id() >= pattern_count || execution.trace_identity().is_empty() {
        return Err("Build replay execution identity is invalid");
    }
    let trace = execution.replay_trace();
    let source_steps = trace.solution_trace().steps();
    if source_steps.is_empty() {
        return Err("Build replay trace is empty");
    }
    let mut expected_board = problem.initial_board().occupied_mask();
    let mut expected_cursor = usize::from(problem.initial_hold().cursor());
    let mut expected_hold = problem.initial_hold().hold_piece();
    let mut steps = Vec::new();
    steps
        .try_reserve_exact(source_steps.len())
        .map_err(|_| "Build product allocation failed")?;
    for (index, step) in source_steps.iter().enumerate() {
        let decision = step.piece_decision();
        let placement = step.placement();
        let before = step.board_before();
        let after = step.board_after();
        if step.step_index() != index
            || decision.input_cursor() != expected_cursor
            || decision.input_hold_piece() != expected_hold
            || placement.piece_kind() != decision.active_piece()
            || before.occupied() != expected_board
            || before.layout() != after.after_placement().layout()
            || before.layout() != after.after_line_clear().layout()
        {
            return Err("Build replay chain is invalid");
        }
        let layout = before.layout();
        let width = u32::from(layout.width());
        if width == 0 || width > 64 {
            return Err("Build replay layout width is invalid");
        }
        let row_bits = if width == 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        let mut cleared_row_mask = 0_u64;
        for row in 0..u32::from(layout.height()) {
            let shift = row
                .checked_mul(width)
                .ok_or("Build replay row shift overflow")?;
            if shift >= 64 {
                return Err("Build replay row shift is out of range");
            }
            let mask = row_bits
                .checked_shl(shift)
                .ok_or("Build replay row mask overflow")?;
            if after.after_placement().occupied() & mask == mask {
                cleared_row_mask |= 1_u64 << row;
            }
        }
        let cleared_lines = step.line_clear().cleared_lines();
        if cleared_row_mask.count_ones() != u32::from(cleared_lines) {
            return Err("Build replay line-clear identity is invalid");
        }
        steps.push(PcPathStepPayload::new(
            index.to_string(),
            step.operation_id().0.to_string(),
            decision.active_piece().as_ascii().to_string(),
            decision.input_cursor().to_string(),
            decision.output_cursor().to_string(),
            decision
                .input_hold_piece()
                .map(|piece| piece.as_ascii().to_string()),
            decision
                .output_hold_piece()
                .map(|piece| piece.as_ascii().to_string()),
            decision.hold_decision().as_str(),
            placement.rotation().quarter_turns().to_string(),
            placement.x().to_string(),
            placement.y().to_string(),
            format!("0x{:016x}", placement.mask()),
            format!("0x{:016x}", before.occupied()),
            format!("0x{:016x}", after.after_placement().occupied()),
            format!("0x{:016x}", after.after_line_clear().occupied()),
            format!("0x{cleared_row_mask:016x}"),
            cleared_lines.to_string(),
            format!("rows:{cleared_row_mask:016x}:count:{cleared_lines}"),
        ));
        expected_board = after.after_line_clear().occupied();
        expected_cursor = decision.output_cursor();
        expected_hold = decision.output_hold_piece();
    }
    if expected_board != expected_terminal {
        return Err("Build replay does not terminate at the requested cleared field");
    }
    Ok(PcPathWitnessPayload::new(
        candidate_id.to_string(),
        execution.candidate_id().to_string(),
        execution.pattern_id().to_string(),
        try_clone_string(execution.trace_identity())?,
        try_clone_string(&trace.canonical_key())?,
        expected_cursor.to_string(),
        expected_hold.map(|piece| piece.as_ascii().to_string()),
        steps,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probability_result(success: &str, failed: &str) -> CoreExecutionResult {
        CoreExecutionResult::new(
            vec![
                ("coverage_probability".to_owned(), success.to_owned()),
                ("failed_coverage_probability".to_owned(), failed.to_owned()),
                ("materialized_probability_mass".to_owned(), "1".to_owned()),
                (
                    "coverage_probability_denominator".to_owned(),
                    "full-materialized-pattern-universe".to_owned(),
                ),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn failed_queue_probability_reuses_the_exact_engine_complement() {
        let result = probability_result("0.25", "0.75");

        assert_eq!(exact_build_failed_probability(&result), Ok("0.75"));
    }

    #[test]
    fn failed_queue_probability_rejects_noncanonical_or_mismatched_authority() {
        assert!(exact_build_failed_probability(&probability_result("0.25", "0.5")).is_err());
        assert!(exact_build_failed_probability(&probability_result("0.250", "0.75")).is_err());

        let duplicated = probability_result("0.25", "0.75").with_additional_fields(vec![(
            "failed_coverage_probability".to_owned(),
            "0.75".to_owned(),
        )]);
        assert!(exact_build_failed_probability(&duplicated).is_err());
    }
}
