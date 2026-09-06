// SRP rationale: one change reason is the query-bound publication contract for
// Build-probability aggregations. Score fields, winner/portfolio families,
// replay witnesses and failed-queue decorations are projections of the same
// executed Build evidence, so their identity, completeness and retained-owner
// admission checks belong at this shared result boundary. Core execution and
// the portfolio engine own search/proof; CLI, GUI and Discord own presentation.
//! Query-bound products for Build probability result aggregations.
//!
//! This module is the sole adapter from executed Build evidence to the public
//! field-average and complete-replay families.  Presenters receive finite DTOs
//! and never reconstruct score or path semantics from display fields.

use std::{collections::BTreeMap, sync::Arc};

use clearra_core_domain::solution::NormalizedTilingSolutionKey;
use clearra_core_executor::{CoreExecutionResult, CorePostProcessExecution};
use clearra_coverage::{
    cover::ExactMinimumCoverError,
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
};
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
    CoveragePortfolioAlternativeSet, CoveragePortfolioAlternativeSetPreparation,
    CoveragePortfolioAlternativeSetPreparationAdvance, PortfolioAlternativeSetIdentity,
    ProductPageSourceOwner,
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
        host: Option<crate::ProductRetentionBudget>,
    ) -> Result<Self, &'static str> {
        Self::from_query_result_and_external(query, result, 0, host)
    }

    fn from_query_result_and_external(
        query: &BuildProbabilityQuery,
        result: &CoreExecutionResult,
        external_retained_bytes: u128,
        host: Option<crate::ProductRetentionBudget>,
    ) -> Result<Self, &'static str> {
        let maximum_bytes = crate::product_retention_budget::result_product_memory_limit(
            result,
            host,
            BUILD_PRODUCT_PORTABLE_WHOLE_LIVE_BYTES,
        )
        .map_err(|_| "Build product memory authority is invalid")?;
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
    host: Option<crate::ProductRetentionBudget>,
) -> Result<ProductResultPayload, &'static str> {
    let derivation_bytes = derivation
        .checked_retained_capacity_bytes()
        .ok_or("Build product memory projection overflow")?;
    let budget = BuildProductMemoryBudget::from_query_result_and_external(
        query,
        result,
        derivation_bytes,
        host,
    )?;
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
    host: Option<crate::ProductRetentionBudget>,
) -> Result<ProductResultPayload, &'static str> {
    let budget = BuildProductMemoryBudget::from_query_and_result(query, result, host)?;
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
    let mirrored_terminal = if query.field().includes_applicable_horizontal_mirror() {
        query
            .field()
            .mirrored_horizontally()
            .compact_final_board_mask()
            .filter(|mask| *mask != expected_terminal)
    } else {
        None
    };
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
            mirrored_terminal,
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
        ProductResultPayloadContent::BuildPathFamily(
            BuildPathFamilyPayload::new(
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
            )
            .with_mirrored_terminal_board_mask(
                mirrored_terminal.map(|mask| format!("0x{mask:016x}")),
            ),
        ),
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
    host: Option<crate::ProductRetentionBudget>,
) -> Result<ProductResultPayload, &'static str> {
    let derivation_bytes = derivation
        .checked_retained_capacity_bytes()
        .ok_or("Build product memory projection overflow")?;
    let budget = BuildProductMemoryBudget::from_query_result_and_external(
        query,
        result,
        derivation_bytes,
        host,
    )?;
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
    host: Option<crate::ProductRetentionBudget>,
) -> Result<(ProductResultPayload, ProductPageSourceOwner), &'static str> {
    prepare_build_highest_score_minimum_payload(query, result, derivation, host, &mut |_| Ok(()))?
        .complete(&mut || false)
}

/// Builds only the validated score-coverage source. Exact minimum proof and
/// canonical selection are advanced by the shared portfolio preparation;
/// neither native nor browser callers maintain an independent solver here.
/// The external guard receives this preparation's whole inline + heap peak.
/// Borrowed query, Core result and score derivation remain caller-owned.
pub(crate) fn prepare_build_highest_score_minimum_payload(
    query: &BuildProbabilityQuery,
    result: &CoreExecutionResult,
    derivation: &PcScoreDerivation,
    host: Option<crate::ProductRetentionBudget>,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<BuildScoreMinimumPreparation, &'static str> {
    let derivation_bytes = derivation
        .checked_retained_capacity_bytes()
        .ok_or("Build product memory projection overflow")?;
    let budget = BuildProductMemoryBudget::from_query_result_and_external(
        query,
        result,
        derivation_bytes,
        host,
    )?;
    let pattern_count = validated_score_pattern_count(result, derivation)?;
    let construction = budget.reserve_linear_workspace(
        result
            .normalized_solution_keys()
            .len()
            .checked_add(derivation.pattern_winners().len())
            .and_then(|count| count.checked_add(pattern_count))
            .ok_or("Build score-minimum item count overflow")?,
        checked_total_string_lengths(result.normalized_solution_keys())
            .ok_or("Build product memory projection overflow")?,
    )?;
    // Retain the existing bounded map/string workspace admission, and also
    // cover all dense eligible rows, their temporary storage replacements,
    // and the query clone used only while deriving the unchanged identity.
    // This is a conservative admission reserve, not a measured heap count.
    let word_bytes = (pattern_count.div_ceil(64) as u128)
        .checked_mul(core::mem::size_of::<u64>() as u128)
        .and_then(|bytes| bytes.checked_add(4 * core::mem::size_of::<usize>() as u128))
        .ok_or("Build score-minimum source projection overflow")?;
    let matrix_peak = (result.normalized_solution_keys().len() as u128)
        .checked_add(1)
        .and_then(|rows| rows.checked_mul(word_bytes))
        .and_then(|bytes| bytes.checked_mul(4))
        .ok_or("Build score-minimum source projection overflow")?;
    let metadata_bytes = [
        "piece_source_id",
        "actual_normalized_solution_set_hash",
        "pattern_universe_id",
        "pattern_weight_model_id",
    ]
    .into_iter()
    .try_fold(0_u128, |bytes, field| {
        bytes.checked_add(required_field(result, field).ok()?.len() as u128)
    })
    .ok_or("Build score identity metadata is missing")?;
    let source_peak = construction
        .source_bytes
        .checked_sub(budget.source_bytes)
        .and_then(|bytes| bytes.checked_add(matrix_peak))
        .and_then(|bytes| bytes.checked_add(metadata_bytes.checked_mul(4)?))
        .and_then(|bytes| {
            bytes.checked_add(query.checked_retained_capacity_bytes()?.checked_mul(4)?)
        })
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<BuildScoreMinimumPreparation>() as u128)
        })
        .ok_or("Build score-minimum source projection overflow")?;
    authorize_build_score_peak(budget, source_peak, guard)?;
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
    let input_identity = sha256_hex(&[
        problem.problem_id().as_str(),
        required_field(result, "actual_normalized_solution_set_hash")?,
        score.profile().as_str(),
        &score.initial_b2b().to_string(),
    ]);
    let mut winners = Vec::new();
    winners
        .try_reserve_exact(maxima.len())
        .map_err(|_| "Build product allocation failed")?;
    for ((pattern_id, candidate_id), winner) in &maxima {
        if best_by_pattern.get(pattern_id) != Some(&winner.score()) {
            continue;
        }
        let (_, key) = candidate_map
            .get(&winner.solution_identity())
            .ok_or("Build score-minimum candidate is missing")?;
        let candidate_index = candidate_keys
            .binary_search(key)
            .map_err(|_| "Build score-minimum candidate is missing")?;
        if public_candidate_ids.get(candidate_index) != Some(candidate_id) {
            return Err("Build score-minimum candidate identity is ambiguous");
        }
        winners.push(BuildScoreMinimumWinner {
            pattern_id: *pattern_id,
            candidate_index,
            score: winner.score(),
            informational_attack: winner.informational_attack(),
        });
    }
    // Dense row order is key order. This is exactly the old (key, public ID)
    // winner tie-break; neither attack nor derived display strings rank rows.
    winners.sort_unstable_by_key(|winner| (winner.pattern_id, winner.candidate_index));
    let projection = BuildScoreMinimumProjection {
        input_identity,
        score_profile: try_clone_string(score.profile().as_str())?,
        initial_b2b: score.initial_b2b().to_string(),
        source_candidate_count: candidate_map.len(),
        eligible_candidate_count: candidate_keys.len(),
        pattern_count,
        required_pattern_count: usize::try_from(required.count_ones())
            .map_err(|_| "Build score-minimum winner count overflow")?,
        public_candidate_ids,
        winners,
    };
    // These scratch maps and the compiled identity problem must not survive
    // across host turns or become replicated with every exact-cover shard.
    drop(problem);
    drop(eligible_by_key);
    drop(best_by_pattern);
    drop(maxima);
    drop(candidate_map);
    BuildScoreMinimumPreparation::from_source(
        budget,
        projection,
        identity,
        candidate_keys,
        required,
        rows,
        guard,
    )
}

#[derive(Clone, Copy)]
struct BuildScoreMinimumWinner {
    pattern_id: usize,
    candidate_index: usize,
    score: u64,
    informational_attack: u32,
}

struct BuildScoreMinimumProjection {
    input_identity: String,
    score_profile: String,
    initial_b2b: String,
    source_candidate_count: usize,
    eligible_candidate_count: usize,
    pattern_count: usize,
    required_pattern_count: usize,
    public_candidate_ids: Vec<u64>,
    winners: Vec<BuildScoreMinimumWinner>,
}

impl BuildScoreMinimumProjection {
    fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.input_identity.capacity() as u128)
            .checked_add(self.score_profile.capacity() as u128)?
            .checked_add(self.initial_b2b.capacity() as u128)?
            .checked_add(
                (self.public_candidate_ids.capacity() as u128)
                    .checked_mul(core::mem::size_of::<u64>() as u128)?,
            )?
            .checked_add(
                (self.winners.capacity() as u128)
                    .checked_mul(core::mem::size_of::<BuildScoreMinimumWinner>() as u128)?,
            )
    }
}

pub(crate) struct BuildScoreMinimumPreparation {
    budget: BuildProductMemoryBudget,
    projection: Option<BuildScoreMinimumProjection>,
    portfolio: CoveragePortfolioAlternativeSetPreparation,
}

pub(crate) enum BuildScoreMinimumPreparationAdvance {
    Pending { work_steps: u64 },
    Completed((ProductResultPayload, ProductPageSourceOwner)),
    Cancelled { work_steps: u64 },
}

impl BuildScoreMinimumPreparation {
    #[allow(clippy::too_many_arguments)]
    fn from_source(
        budget: BuildProductMemoryBudget,
        projection: BuildScoreMinimumProjection,
        identity: PortfolioAlternativeSetIdentity,
        candidate_keys: Vec<String>,
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<Self, &'static str> {
        let outer = (core::mem::size_of::<Self>() as u128)
            .checked_sub(core::mem::size_of::<CoveragePortfolioAlternativeSetPreparation>() as u128)
            .and_then(|bytes| bytes.checked_add(projection.checked_retained_capacity_bytes()?))
            .ok_or("Build score-minimum preparation projection overflow")?;
        let portfolio = CoveragePortfolioAlternativeSetPreparation::new_with_memory_guard(
            identity,
            candidate_keys,
            required,
            rows,
            &mut |peak| authorize_build_score_exact_peak(budget, outer, peak, guard),
        )
        .map_err(|_| "Build score-minimum exact portfolio preparation failed")?;
        let preparation = Self {
            budget,
            projection: Some(projection),
            portfolio,
        };
        authorize_build_score_peak(budget, preparation.whole_retained_bytes()?, guard)?;
        Ok(preparation)
    }

    pub(crate) fn parallel_work(&self) -> &CoveragePortfolioAlternativeSetPreparation {
        &self.portfolio
    }

    pub(crate) fn parallel_work_mut(&mut self) -> &mut CoveragePortfolioAlternativeSetPreparation {
        &mut self.portfolio
    }

    /// Heap-only, excluding `Self`. The producer/derivation are not cloned
    /// into this owner and must be accounted by their existing App owner.
    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.portfolio
            .checked_retained_capacity_bytes()?
            .checked_add(self.projection.as_ref().map_or(Some(0), |projection| {
                projection.checked_retained_capacity_bytes()
            })?)
    }

    fn whole_retained_bytes(&self) -> Result<u128, &'static str> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(
                self.checked_retained_capacity_bytes()
                    .ok_or("Build score-minimum preparation projection overflow")?,
            )
            .ok_or("Build score-minimum preparation projection overflow")
    }

    pub(crate) fn advance_with_memory_guard(
        &mut self,
        work: u64,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<BuildScoreMinimumPreparationAdvance, &'static str> {
        let projection = self
            .projection
            .as_ref()
            .ok_or("Build score-minimum preparation is already terminal")?;
        let outer = (core::mem::size_of::<Self>() as u128)
            .checked_sub(core::mem::size_of::<CoveragePortfolioAlternativeSetPreparation>() as u128)
            .and_then(|bytes| bytes.checked_add(projection.checked_retained_capacity_bytes()?))
            .ok_or("Build score-minimum preparation projection overflow")?;
        let budget = self.budget;
        let advanced = self
            .portfolio
            .advance_with_memory_guard(
                work,
                &mut |peak| authorize_build_score_exact_peak(budget, outer, peak, guard),
                cancelled,
            )
            .map_err(|_| "Build score-minimum exact portfolio failed")?;
        Ok(match advanced {
            CoveragePortfolioAlternativeSetPreparationAdvance::Pending { work_steps } => {
                BuildScoreMinimumPreparationAdvance::Pending { work_steps }
            }
            CoveragePortfolioAlternativeSetPreparationAdvance::Cancelled { work_steps } => {
                self.projection = None;
                BuildScoreMinimumPreparationAdvance::Cancelled { work_steps }
            }
            CoveragePortfolioAlternativeSetPreparationAdvance::Completed(portfolio) => {
                let retained = self.whole_retained_bytes()?;
                let projection = self
                    .projection
                    .take()
                    .ok_or("Build score-minimum preparation is already terminal")?;
                let mut cancelled_during_seal = false;
                let sealed = seal_build_score_minimum(
                    budget,
                    retained,
                    projection,
                    portfolio,
                    &mut |peak| {
                        if cancelled() {
                            cancelled_during_seal = true;
                            Err(ExactMinimumCoverError::MemoryGuardRejected)
                        } else {
                            guard(peak)
                        }
                    },
                );
                match sealed {
                    Ok(product) => BuildScoreMinimumPreparationAdvance::Completed(product),
                    Err(_) if cancelled_during_seal => {
                        BuildScoreMinimumPreparationAdvance::Cancelled { work_steps: 0 }
                    }
                    Err(reason) => return Err(reason),
                }
            }
        })
    }

    fn complete(
        mut self,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(ProductResultPayload, ProductPageSourceOwner), &'static str> {
        loop {
            match self.advance_with_memory_guard(u64::MAX, &mut |_| Ok(()), cancelled)? {
                BuildScoreMinimumPreparationAdvance::Pending { .. } => {}
                BuildScoreMinimumPreparationAdvance::Completed(product) => return Ok(product),
                BuildScoreMinimumPreparationAdvance::Cancelled { .. } => {
                    return Err("Build score-minimum exact portfolio cancelled");
                }
            }
        }
    }
}

fn authorize_build_score_exact_peak(
    budget: BuildProductMemoryBudget,
    outer: u128,
    peak: u128,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<(), ExactMinimumCoverError> {
    let peak = outer
        .checked_add(peak)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    let whole = budget
        .source_bytes
        .checked_add(peak)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    if whole > budget.maximum_bytes {
        return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
            required_memory_bytes: whole,
            max_memory_bytes: budget.maximum_bytes,
        });
    }
    guard(peak)
}

fn authorize_build_score_peak(
    budget: BuildProductMemoryBudget,
    peak: u128,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<(), &'static str> {
    authorize_build_score_exact_peak(budget, 0, peak, guard)
        .map_err(|_| "Build score-minimum whole-live memory limit exceeded")
}

fn seal_build_score_minimum(
    budget: BuildProductMemoryBudget,
    retained_preparation: u128,
    mut projection: BuildScoreMinimumProjection,
    portfolio: CoveragePortfolioAlternativeSet,
    guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
) -> Result<(ProductResultPayload, ProductPageSourceOwner), &'static str> {
    let overflow = "Build score-minimum completed product projection overflow";
    let owner_heap = portfolio
        .checked_retained_capacity_bytes()
        .ok_or(overflow)?;
    // `with_public_candidate_ids` simultaneously retains its input vector,
    // sorted duplicate-check clone and replacement Arc. The first vector is
    // already in retained_preparation; reserve both new payloads and controls.
    let replacement_peak = (projection.public_candidate_ids.len() as u128)
        .checked_mul(2 * core::mem::size_of::<u64>() as u128)
        .and_then(|bytes| bytes.checked_add(4 * core::mem::size_of::<usize>() as u128))
        .ok_or(overflow)?;
    let base = retained_preparation
        .checked_add(owner_heap)
        .and_then(|bytes| {
            bytes.checked_add(3 * core::mem::size_of::<CoveragePortfolioAlternativeSet>() as u128)
        })
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<ProductResultPayload>() as u128))
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<ProductPageSourceOwner>() as u128))
        .ok_or(overflow)?;
    authorize_build_score_peak(
        budget,
        base.checked_add(replacement_peak).ok_or(overflow)?,
        guard,
    )?;
    let public_ids = core::mem::take(&mut projection.public_candidate_ids);
    let portfolio = portfolio
        .with_public_candidate_ids(public_ids)
        .map_err(|_| "Build score-minimum public candidate map is invalid")?;
    let updated_owner_heap = portfolio
        .checked_retained_capacity_bytes()
        .ok_or(overflow)?;
    let base = base
        .checked_add(updated_owner_heap.checked_sub(owner_heap).ok_or(overflow)?)
        .and_then(|bytes| bytes.checked_add(2 * core::mem::size_of::<usize>() as u128))
        .ok_or(overflow)?;
    authorize_build_score_peak(budget, base, guard)?;
    let owner = Arc::new(portfolio);
    // The complete selected set is retained and copied, not just its first
    // 100 GUI members. Each canonical key is cloned only once for the DTO.
    let selected_ids = owner.canonical_page().portfolio().candidate_ids();
    let mut keys_requested = (selected_ids.len() as u128)
        .checked_mul(core::mem::size_of::<String>() as u128)
        .ok_or(overflow)?;
    for id in selected_ids {
        let index = usize::try_from(id.checked_sub(1).ok_or(overflow)?).map_err(|_| overflow)?;
        let key = owner
            .candidates()
            .get(index)
            .filter(|candidate| candidate.candidate_id() == *id)
            .ok_or("Build score-minimum canonical candidate is invalid")?
            .normalized_key();
        keys_requested = keys_requested
            .checked_add(key.len() as u128)
            .ok_or(overflow)?;
    }
    authorize_build_score_peak(
        budget,
        base.checked_add(keys_requested).ok_or(overflow)?,
        guard,
    )?;
    let mut canonical_keys = Vec::new();
    canonical_keys
        .try_reserve_exact(selected_ids.len())
        .map_err(|_| "Build product allocation failed")?;
    let mut keys_actual = (canonical_keys.capacity() as u128)
        .checked_mul(core::mem::size_of::<String>() as u128)
        .ok_or(overflow)?;
    authorize_build_score_peak(
        budget,
        base.checked_add(keys_actual).ok_or(overflow)?,
        guard,
    )?;
    for id in selected_ids {
        let index = usize::try_from(id - 1).map_err(|_| overflow)?;
        let key = owner.candidates()[index].normalized_key();
        authorize_build_score_peak(
            budget,
            base.checked_add(keys_actual)
                .and_then(|bytes| bytes.checked_add(key.len() as u128))
                .ok_or(overflow)?,
            guard,
        )?;
        let key = try_clone_string(key)?;
        keys_actual = keys_actual
            .checked_add(key.capacity() as u128)
            .ok_or(overflow)?;
        authorize_build_score_peak(
            budget,
            base.checked_add(keys_actual).ok_or(overflow)?,
            guard,
        )?;
        canonical_keys.push(key);
    }
    let mut public_winners = Vec::new();
    let requested = (projection.required_pattern_count as u128)
        .checked_mul(core::mem::size_of::<BuildV2ScoreWinnerPayload>() as u128)
        .ok_or(overflow)?;
    authorize_build_score_peak(
        budget,
        base.checked_add(keys_actual)
            .and_then(|bytes| bytes.checked_add(requested))
            .ok_or(overflow)?,
        guard,
    )?;
    public_winners
        .try_reserve_exact(projection.required_pattern_count)
        .map_err(|_| "Build product allocation failed")?;
    let mut winners_actual = (public_winners.capacity() as u128)
        .checked_mul(core::mem::size_of::<BuildV2ScoreWinnerPayload>() as u128)
        .ok_or(overflow)?;
    authorize_build_score_peak(
        budget,
        base.checked_add(keys_actual)
            .and_then(|bytes| bytes.checked_add(winners_actual))
            .ok_or(overflow)?,
        guard,
    )?;
    let mut previous_pattern = None;
    for winner in &projection.winners {
        if previous_pattern == Some(winner.pattern_id) {
            continue;
        }
        let dense_id = u64::try_from(winner.candidate_index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or(overflow)?;
        if selected_ids.binary_search(&dense_id).is_err() {
            continue;
        }
        let key = owner
            .candidates()
            .get(winner.candidate_index)
            .ok_or("Build score-minimum winner candidate is invalid")?
            .normalized_key();
        // Three decimal fields need at most 20 bytes each (usize <= u64),
        // plus one key. Their inline String/DTO carriers are included too.
        let new_winner_peak = (key.len() as u128)
            .checked_add(60)
            .and_then(|bytes| {
                bytes.checked_add(core::mem::size_of::<BuildV2ScoreWinnerPayload>() as u128)
            })
            .ok_or(overflow)?;
        authorize_build_score_peak(
            budget,
            base.checked_add(keys_actual)
                .and_then(|bytes| bytes.checked_add(winners_actual))
                .and_then(|bytes| bytes.checked_add(new_winner_peak))
                .ok_or(overflow)?,
            guard,
        )?;
        let value = BuildV2ScoreWinnerPayload::try_new(
            winner.pattern_id.to_string(),
            key,
            winner.score.to_string(),
            winner.informational_attack.to_string(),
        )
        .map_err(|_| "Build score-minimum winner payload is invalid")?;
        winners_actual = winners_actual
            .checked_add(value.checked_retained_capacity_bytes().ok_or(overflow)?)
            .ok_or(overflow)?;
        authorize_build_score_peak(
            budget,
            base.checked_add(keys_actual)
                .and_then(|bytes| bytes.checked_add(winners_actual))
                .ok_or(overflow)?,
            guard,
        )?;
        public_winners.push(value);
        previous_pattern = Some(winner.pattern_id);
    }
    if public_winners.len() != projection.required_pattern_count {
        return Err("Build score-minimum canonical portfolio does not cover a winner pattern");
    }
    // Only fixed contract names, a 64-byte digest, booleans and six decimal
    // counters remain to allocate. Variable key/winner payload is already
    // admitted. A conservative 4 KiB reserve includes all their inline parts.
    let payload_peak = base
        .checked_add(keys_actual)
        .and_then(|bytes| bytes.checked_add(winners_actual))
        .and_then(|bytes| bytes.checked_add(4096))
        .ok_or(overflow)?;
    authorize_build_score_peak(budget, payload_peak, guard)?;
    let payload = BuildV2ProductPayload::try_score_portfolio(
        BUILD_SCORE_MINIMUM_CAPABILITY,
        BUILD_SCORE_MINIMUM_RESULT_CONTRACT,
        projection.input_identity,
        projection.score_profile,
        projection.initial_b2b,
        "basic-approximation",
        false,
        "score-only",
        SCORE_ATTACK_BASIS,
        projection.source_candidate_count.to_string(),
        projection.eligible_candidate_count.to_string(),
        canonical_keys.len().to_string(),
        projection.pattern_count.to_string(),
        projection.required_pattern_count.to_string(),
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
    let source = ProductPageSourceOwner::CoveragePortfolio(owner);
    let actual_peak = retained_preparation
        .checked_add(core::mem::size_of::<ProductResultPayload>() as u128)
        .and_then(|bytes| bytes.checked_add(core::mem::size_of::<ProductPageSourceOwner>() as u128))
        .and_then(|bytes| bytes.checked_add(payload.checked_retained_capacity_bytes()?))
        .and_then(|bytes| bytes.checked_add(source.checked_retained_capacity_bytes()?))
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<CoveragePortfolioAlternativeSet>() as u128)
        })
        .and_then(|bytes| bytes.checked_add(2 * core::mem::size_of::<usize>() as u128))
        .ok_or(overflow)?;
    authorize_build_score_peak(budget, actual_peak, guard)?;
    budget.authorize_payload(&payload, Some(&source))?;
    Ok((payload, source))
}

pub(crate) fn decorate_build_failed_queues(
    query: &BuildProbabilityQuery,
    result: CoreExecutionResult,
    failed_pattern_limit: usize,
    host: Option<crate::ProductRetentionBudget>,
) -> Result<CoreExecutionResult, &'static str> {
    let budget = BuildProductMemoryBudget::from_query_and_result(query, &result, host)?;
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
    mirrored_terminal: Option<u64>,
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
    if expected_board != expected_terminal && Some(expected_board) != mirrored_terminal {
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

    fn score_minimum_source_fixture() -> (
        BuildProductMemoryBudget,
        BuildScoreMinimumProjection,
        PortfolioAlternativeSetIdentity,
        Vec<String>,
        PatternBitSet,
        Vec<PatternBitSet>,
    ) {
        let row = |patterns: &[usize]| {
            let mut row = PatternBitSet::new(2);
            for pattern in patterns {
                row.insert(PatternId::new(*pattern)).unwrap();
            }
            row
        };
        (
            BuildProductMemoryBudget {
                maximum_bytes: 64 * 1024 * 1024,
                source_bytes: 4096,
            },
            BuildScoreMinimumProjection {
                input_identity: "a".repeat(64),
                score_profile: "tetrio".to_owned(),
                initial_b2b: "0".to_owned(),
                source_candidate_count: 4,
                eligible_candidate_count: 4,
                pattern_count: 2,
                required_pattern_count: 2,
                // Public IDs deliberately differ from canonical key order.
                public_candidate_ids: vec![4, 2, 3, 1],
                winners: [(0, 0), (0, 1), (0, 3), (1, 1), (1, 2), (1, 3)]
                    .into_iter()
                    .map(|(pattern_id, candidate_index)| BuildScoreMinimumWinner {
                        pattern_id,
                        candidate_index,
                        score: 1200,
                        informational_attack: if candidate_index == 3 { 999 } else { 7 },
                    })
                    .collect(),
            },
            PortfolioAlternativeSetIdentity::new(
                "build-score-query",
                "build-score-source",
                "score-only-rules",
                "two-pattern-universe",
                "build-score-engine",
            )
            .unwrap(),
            ["candidate-a", "candidate-b", "candidate-c", "candidate-d"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            row(&[0, 1]),
            vec![row(&[0]), row(&[0, 1]), row(&[1]), row(&[0, 1])],
        )
    }

    fn score_minimum_preparation_fixture(
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<BuildScoreMinimumPreparation, &'static str> {
        let (budget, projection, identity, keys, required, rows) = score_minimum_source_fixture();
        BuildScoreMinimumPreparation::from_source(
            budget, projection, identity, keys, required, rows, guard,
        )
    }

    #[test]
    fn build_score_minimum_bounded_preparation_preserves_score_only_public_identity_and_payload() {
        // Build the previous synchronous projection explicitly as the parity
        // oracle, not by calling the new sync delegate under test.
        let (_, projection, identity, keys, required, rows) = score_minimum_source_fixture();
        let expected_owner =
            CoveragePortfolioAlternativeSet::new_canonical(identity, keys, required, rows)
                .unwrap()
                .with_public_candidate_ids(projection.public_candidate_ids)
                .unwrap();
        let expected_keys = expected_owner.canonical_candidate_keys_owned().unwrap();
        assert_eq!(expected_keys, ["candidate-b"]);
        let expected_payload = ProductResultPayload::new(
            BUILD_SCORE_MINIMUM_CAPABILITY,
            BUILD_SCORE_MINIMUM_RESULT_CONTRACT,
            ProductResultPayloadContent::BuildV2(
                BuildV2ProductPayload::try_score_portfolio(
                    BUILD_SCORE_MINIMUM_CAPABILITY,
                    BUILD_SCORE_MINIMUM_RESULT_CONTRACT,
                    projection.input_identity,
                    projection.score_profile,
                    projection.initial_b2b,
                    "basic-approximation",
                    false,
                    "score-only",
                    SCORE_ATTACK_BASIS,
                    "4",
                    "4",
                    "1",
                    "2",
                    "2",
                    expected_keys,
                    vec![
                        BuildV2ScoreWinnerPayload::try_new("0", "candidate-b", "1200", "7")
                            .unwrap(),
                        BuildV2ScoreWinnerPayload::try_new("1", "candidate-b", "1200", "7")
                            .unwrap(),
                    ],
                    BuildV2CompletenessPayload::new(true, true, true, true, true, true, true),
                    expected_owner.set_identity_sha256(),
                )
                .unwrap(),
            ),
        );
        let mut peak = 0;
        let mut guard = |bytes| {
            peak = peak.max(bytes);
            Ok(())
        };
        let mut preparation = score_minimum_preparation_fixture(&mut guard).unwrap();
        let mut completed = None;
        for _ in 0..10_000 {
            match preparation
                .advance_with_memory_guard(1, &mut guard, &mut || false)
                .unwrap()
            {
                BuildScoreMinimumPreparationAdvance::Pending { work_steps } => {
                    assert!(work_steps <= 1)
                }
                BuildScoreMinimumPreparationAdvance::Completed(product) => {
                    completed = Some(product);
                    break;
                }
                BuildScoreMinimumPreparationAdvance::Cancelled { .. } => panic!("not cancelled"),
            }
        }
        let (payload, owner) = completed.expect("bounded tiny source completes");
        assert_eq!(payload, expected_payload);
        let ProductPageSourceOwner::CoveragePortfolio(owner) = owner else {
            panic!("score minimum must retain its complete portfolio source");
        };
        assert_eq!(owner.as_ref(), &expected_owner);
        assert_eq!(owner.public_candidate_id(2), Some(2));
        assert!(peak > owner.checked_retained_capacity_bytes().unwrap());
    }

    #[test]
    fn build_score_minimum_cancelled_or_rejected_preparation_never_seals() {
        let mut calls = 0;
        let rejected = score_minimum_preparation_fixture(&mut |_| {
            calls += 1;
            Err(ExactMinimumCoverError::MemoryGuardRejected)
        });
        assert!(rejected.is_err());
        assert_eq!(calls, 1, "initial owner rejected before Core allocation");
        let mut preparation = score_minimum_preparation_fixture(&mut |_| Ok(())).unwrap();
        assert!(matches!(
            preparation
                .advance_with_memory_guard(1, &mut |_| Ok(()), &mut || true)
                .unwrap(),
            BuildScoreMinimumPreparationAdvance::Cancelled { .. },
        ));
        assert!(preparation
            .advance_with_memory_guard(1, &mut |_| Ok(()), &mut || false)
            .is_err());
    }

    #[test]
    fn build_score_minimum_seal_checks_each_allocation_boundary_and_actual_capacity() {
        let fixture = || {
            let (budget, projection, identity, keys, required, rows) =
                score_minimum_source_fixture();
            let portfolio =
                CoveragePortfolioAlternativeSet::new_canonical(identity, keys, required, rows)
                    .unwrap();
            let retained = core::mem::size_of::<BuildScoreMinimumPreparation>() as u128
                + projection.checked_retained_capacity_bytes().unwrap();
            (budget, retained, projection, portfolio)
        };
        let (budget, retained, projection, portfolio) = fixture();
        let mut checkpoints = Vec::new();
        let (payload, owner) =
            seal_build_score_minimum(budget, retained, projection, portfolio, &mut |bytes| {
                checkpoints.push(bytes);
                Ok(())
            })
            .unwrap();
        assert!(
            checkpoints.len() > 12,
            "every replacement, key, winner and final carrier is guarded"
        );
        assert!(
            checkpoints.last().copied().unwrap()
                >= retained
                    + payload.checked_retained_capacity_bytes().unwrap()
                    + owner.checked_retained_capacity_bytes().unwrap()
        );
        for reject_at in 0..checkpoints.len() {
            let (budget, retained, projection, portfolio) = fixture();
            let mut seen = 0;
            let result =
                seal_build_score_minimum(budget, retained, projection, portfolio, &mut |_| {
                    let current = seen;
                    seen += 1;
                    if current == reject_at {
                        Err(ExactMinimumCoverError::MemoryGuardRejected)
                    } else {
                        Ok(())
                    }
                });
            assert!(
                result.is_err(),
                "rejected checkpoint {reject_at} cannot publish exact evidence"
            );
            assert_eq!(
                seen,
                reject_at + 1,
                "no allocation continues past a rejected guard"
            );
        }
    }

    #[test]
    fn build_score_minimum_projection_counts_spare_capacity_and_never_copies_attack_into_rows() {
        let (_, mut projection, _, _, _, _) = score_minimum_source_fixture();
        let before = projection.checked_retained_capacity_bytes().unwrap();
        projection.public_candidate_ids.reserve_exact(128);
        projection.winners.reserve_exact(128);
        projection.input_identity.reserve_exact(1024);
        assert!(projection.checked_retained_capacity_bytes().unwrap() > before);
        // Only numeric pattern/row identities and score are selection inputs.
        // Attacks are retained for the selected canonical trace's DTO only.
        for winner in &mut projection.winners {
            winner.informational_attack = u32::MAX;
        }
        assert!(projection.winners.windows(2).all(|pair| (
            pair[0].pattern_id,
            pair[0].candidate_index
        ) < (
            pair[1].pattern_id,
            pair[1].candidate_index
        )));
        let mut called = false;
        assert!(authorize_build_score_exact_peak(
            BuildProductMemoryBudget {
                maximum_bytes: u128::MAX,
                source_bytes: 1
            },
            u128::MAX,
            1,
            &mut |_| {
                called = true;
                Ok(())
            },
        )
        .is_err());
        assert!(
            !called,
            "overflow fails before consulting the external admission owner"
        );
    }

    #[test]
    fn build_product_budget_honors_finite_execution_authority_above_portable_fallback() {
        for (value, expected_mib) in [("none", 16), ("1", 1), ("64", 64)] {
            let result = CoreExecutionResult::new(
                vec![("execution_max_memory_mib".to_owned(), value.to_owned())],
                Vec::new(),
            );
            assert_eq!(
                crate::product_retention_budget::result_product_memory_limit(
                    &result,
                    None,
                    BUILD_PRODUCT_PORTABLE_WHOLE_LIVE_BYTES,
                ),
                Ok(expected_mib * 1024_u128 * 1024_u128),
            );
        }
    }

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
