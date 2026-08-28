// SRP rationale: this module has one behavior-level change reason: replaying supplied Build solutions and reducing reachable identities into exact minimum-cover products.

//! Actual supplied-solution replay and minimum-cover reduction for
//! `build.evaluate.minimals`.
//!
//! Static colored fields are admitted only as an allow-list on the real Build
//! geometry/reachability producer. Returned normalized solutions are mapped
//! back to their supplied colored identity, so a malformed or unreachable
//! supplied field can never become a coverage row or portfolio winner merely
//! because it appeared in the input document.

use std::{collections::BTreeMap, sync::Arc};

use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::{NormalizedTilingSolutionKey, StandardBoard64ColoredTilingIdentity},
};
use clearra_core_executor::{solution_probability_pattern_weights, CoreExecutionResult};
use clearra_coverage::{
    cover::{exact_minimum_cover, ExactMinimumCoverError},
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
    probability::union_probability::union_probability,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
    ProblemCompiler,
};
use sha2::{Digest, Sha256};

use crate::pc_score_postprocess::PcScoreDerivation;
use crate::portfolio_alternative_store::{
    CoveragePortfolioAlternativeSet, PortfolioAlternativeError, PortfolioAlternativeSetIdentity,
};

use super::{
    build_v2_contract::{
        BuildSuppliedSolutionEvaluationContract,
        ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    },
    validate_build_probability_response, validate_build_solution_probability_reducer_input,
    BuildSolutionProbabilityResultError,
};

pub(crate) const BUILD_SUPPLIED_MINIMUM_COVER_RESULT_CONTRACT: &str =
    "build-supplied-minimum-cover.v1";
pub(crate) const BUILD_SUPPLIED_COVER_PERCENT_RESULT_CONTRACT: &str =
    "build-supplied-probability.v1";
pub(crate) const BUILD_SUPPLIED_COVERAGE_RESULT_CONTRACT: &str = "build-supplied-coverage.v1";
pub(crate) const BUILD_SUPPLIED_B2B_COVERAGE_RESULT_CONTRACT: &str =
    "build-supplied-b2b-coverage.v1";
pub(crate) const BUILD_SUPPLIED_SCORE_RESULT_CONTRACT: &str = "build-supplied-score.v1";
pub(crate) const BUILD_SUPPLIED_REPLAY_BASIS: &str =
    "supplied-colored-identity-filter-plus-buildability-replay";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildSuppliedSolutionSetV1 {
    initial_board_mask: u64,
    target_cells_mask: u64,
    visible_height: u8,
    page_count: usize,
    document_hash: String,
    input_identity_sha256: String,
    candidate_keys: Vec<String>,
    identities: Vec<StandardBoard64ColoredTilingIdentity>,
}

impl BuildSuppliedSolutionSetV1 {
    pub fn new(
        visible_height: u8,
        page_count: usize,
        document_hash: impl Into<String>,
        identities: impl IntoIterator<Item = StandardBoard64ColoredTilingIdentity>,
    ) -> Result<Self, BuildSuppliedSolutionSetError> {
        if visible_height == 0 || visible_height > 6 {
            return Err(BuildSuppliedSolutionSetError::VisibleHeightInvalid);
        }
        let document_hash = document_hash.into();
        if document_hash.is_empty()
            || document_hash.trim() != document_hash
            || document_hash.chars().any(char::is_control)
        {
            return Err(BuildSuppliedSolutionSetError::DocumentHashInvalid);
        }
        let mut identities = identities.into_iter().collect::<Vec<_>>();
        identities.sort_unstable();
        identities.dedup();
        if identities.is_empty() {
            return Err(BuildSuppliedSolutionSetError::EmptyCandidateSet);
        }
        if page_count == 0 || page_count < identities.len() {
            return Err(BuildSuppliedSolutionSetError::PageCountInvalid);
        }

        let initial_board_mask = identities[0].initial_board_mask();
        let target_cells_mask = target_cells(identities[0]);
        if target_cells_mask == 0 {
            return Err(BuildSuppliedSolutionSetError::EmptyTarget);
        }
        let board_limit = if visible_height == 6 {
            (1_u64 << 60) - 1
        } else {
            (1_u64 << (usize::from(visible_height) * 10)) - 1
        };
        if initial_board_mask & !board_limit != 0 || target_cells_mask & !board_limit != 0 {
            return Err(BuildSuppliedSolutionSetError::CellOutsideVisibleHeight);
        }
        for identity in &identities {
            if identity.initial_board_mask() != initial_board_mask {
                return Err(BuildSuppliedSolutionSetError::InitialBoardMismatch);
            }
            if target_cells(*identity) != target_cells_mask {
                return Err(BuildSuppliedSolutionSetError::TargetMismatch);
            }
        }

        let candidate_keys = identities
            .iter()
            .copied()
            .map(colored_candidate_key)
            .collect::<Vec<_>>();
        let input_identity_sha256 = supplied_input_identity_sha256(
            initial_board_mask,
            target_cells_mask,
            visible_height,
            page_count,
            &document_hash,
            &candidate_keys,
        );
        Ok(Self {
            initial_board_mask,
            target_cells_mask,
            visible_height,
            page_count,
            document_hash,
            input_identity_sha256,
            candidate_keys,
            identities,
        })
    }

    pub const fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask
    }

    pub const fn target_cells_mask(&self) -> u64 {
        self.target_cells_mask
    }

    pub const fn visible_height(&self) -> u8 {
        self.visible_height
    }

    pub const fn page_count(&self) -> usize {
        self.page_count
    }

    pub fn document_hash(&self) -> &str {
        &self.document_hash
    }

    pub fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }

    pub fn candidate_keys(&self) -> &[String] {
        &self.candidate_keys
    }

    pub fn identities(&self) -> &[StandardBoard64ColoredTilingIdentity] {
        &self.identities
    }

    pub fn matches_query(&self, query: &BuildProbabilityQuery) -> bool {
        query.initial_board_mask() == Some(self.initial_board_mask)
            && query.target_cells() == Some(self.target_cells_mask)
            && query.field().height() == self.visible_height
            && query
                .allowed_colored_solution_identities()
                .is_none_or(|identities| identities == self.identities)
    }

    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.document_hash.capacity() as u128)
            .checked_add(self.input_identity_sha256.capacity() as u128)?
            .checked_add(
                (self.candidate_keys.capacity() as u128)
                    .checked_mul(core::mem::size_of::<String>() as u128)?,
            )?
            .checked_add((self.identities.capacity() as u128).checked_mul(
                core::mem::size_of::<StandardBoard64ColoredTilingIdentity>() as u128,
            )?)?;
        for key in &self.candidate_keys {
            bytes = bytes.checked_add(key.capacity() as u128)?;
        }
        Some(bytes)
    }
}

/// Product-neutral view of one normalized colored Build source. Target-search
/// documents and supplied-solution documents remain distinct nominal owners,
/// but both must pass the same producer allow-list and replay gate before any
/// coverage, probability, score, or portfolio authority can be minted.
pub(crate) trait BuildColoredReplaySource {
    fn initial_board_mask(&self) -> u64;
    fn target_cells_mask(&self) -> u64;
    fn visible_height(&self) -> u8;
    fn page_count(&self) -> usize;
    fn input_identity_sha256(&self) -> &str;
    fn candidate_keys(&self) -> &[String];
    fn identities(&self) -> &[StandardBoard64ColoredTilingIdentity];
    fn matches_query(&self, query: &BuildProbabilityQuery) -> bool;
}

impl BuildColoredReplaySource for BuildSuppliedSolutionSetV1 {
    fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask()
    }

    fn target_cells_mask(&self) -> u64 {
        self.target_cells_mask()
    }

    fn visible_height(&self) -> u8 {
        self.visible_height()
    }

    fn page_count(&self) -> usize {
        self.page_count()
    }

    fn input_identity_sha256(&self) -> &str {
        self.input_identity_sha256()
    }

    fn candidate_keys(&self) -> &[String] {
        self.candidate_keys()
    }

    fn identities(&self) -> &[StandardBoard64ColoredTilingIdentity] {
        self.identities()
    }

    fn matches_query(&self, query: &BuildProbabilityQuery) -> bool {
        self.matches_query(query)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildSuppliedSolutionSetError {
    VisibleHeightInvalid,
    DocumentHashInvalid,
    EmptyCandidateSet,
    PageCountInvalid,
    EmptyTarget,
    CellOutsideVisibleHeight,
    InitialBoardMismatch,
    TargetMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuildSuppliedReplayCompletenessEvidence {
    input_identity_bound: bool,
    producer_filter_bound: bool,
    buildability_replay_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
    exact_minimum_proven: bool,
}

impl BuildSuppliedReplayCompletenessEvidence {
    pub(crate) const fn input_identity_bound(self) -> bool {
        self.input_identity_bound
    }

    pub(crate) const fn producer_filter_bound(self) -> bool {
        self.producer_filter_bound
    }

    pub(crate) const fn buildability_replay_complete(self) -> bool {
        self.buildability_replay_complete
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

    pub(crate) const fn probability_complete(self) -> bool {
        self.input_identity_bound
            && self.producer_filter_bound
            && self.buildability_replay_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
    }

    pub(crate) const fn complete(self) -> bool {
        self.probability_complete() && self.exact_minimum_proven
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildSuppliedCoverPercentV1Result {
    authority: ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    input_identity_sha256: String,
    evaluation_identity_sha256: String,
    source_candidate_count: usize,
    reachable_candidate_count: usize,
    pattern_count: usize,
    covered_pattern_count: usize,
    union_probability: String,
    completeness: BuildSuppliedReplayCompletenessEvidence,
}

impl BuildSuppliedCoverPercentV1Result {
    pub(crate) const fn contract_id(&self) -> &'static str {
        BUILD_SUPPLIED_COVER_PERCENT_RESULT_CONTRACT
    }

    pub(crate) const fn replay_basis(&self) -> &'static str {
        BUILD_SUPPLIED_REPLAY_BASIS
    }

    pub(crate) fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }

    pub(crate) fn evaluation_identity_sha256(&self) -> &str {
        &self.evaluation_identity_sha256
    }

    pub(crate) const fn source_candidate_count(&self) -> usize {
        self.source_candidate_count
    }

    pub(crate) const fn reachable_candidate_count(&self) -> usize {
        self.reachable_candidate_count
    }

    pub(crate) const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub(crate) const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }

    pub(crate) fn union_probability(&self) -> &str {
        &self.union_probability
    }

    pub(crate) const fn completeness(&self) -> BuildSuppliedReplayCompletenessEvidence {
        self.completeness
    }

    pub(crate) const fn authority(
        &self,
    ) -> &ValidatedBuildSuppliedSolutionEvaluationResultAuthority {
        &self.authority
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildSuppliedMinimumCoverV1Result {
    authority: ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    input_identity_sha256: String,
    source_candidate_count: usize,
    reachable_candidate_count: usize,
    selected_candidate_count: usize,
    pattern_count: usize,
    required_pattern_count: usize,
    union_probability: String,
    canonical_candidate_keys: Vec<String>,
    alternatives: Arc<CoveragePortfolioAlternativeSet>,
    completeness: BuildSuppliedReplayCompletenessEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildSuppliedCandidateCoverageV1 {
    candidate_key: String,
    covered_pattern_count: usize,
}

impl BuildSuppliedCandidateCoverageV1 {
    pub fn candidate_key(&self) -> &str {
        &self.candidate_key
    }

    pub const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildSuppliedCoverageV1Result {
    authority: ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    contract_id: &'static str,
    input_identity_sha256: String,
    evaluation_identity_sha256: String,
    source_candidate_count: usize,
    reachable_candidate_count: usize,
    pattern_count: usize,
    covered_pattern_count: usize,
    union_probability: String,
    b2b_preservation_required: bool,
    candidates: Vec<BuildSuppliedCandidateCoverageV1>,
    completeness: BuildSuppliedReplayCompletenessEvidence,
}

impl BuildSuppliedCoverageV1Result {
    pub(crate) const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub(crate) fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }

    pub(crate) fn evaluation_identity_sha256(&self) -> &str {
        &self.evaluation_identity_sha256
    }

    pub(crate) const fn source_candidate_count(&self) -> usize {
        self.source_candidate_count
    }

    pub(crate) const fn reachable_candidate_count(&self) -> usize {
        self.reachable_candidate_count
    }

    pub(crate) const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub(crate) const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }

    pub(crate) fn union_probability(&self) -> &str {
        &self.union_probability
    }

    pub(crate) const fn b2b_preservation_required(&self) -> bool {
        self.b2b_preservation_required
    }

    pub(crate) fn candidates(&self) -> &[BuildSuppliedCandidateCoverageV1] {
        &self.candidates
    }

    pub(crate) const fn completeness(&self) -> BuildSuppliedReplayCompletenessEvidence {
        self.completeness
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildSuppliedScoreWinnerV1 {
    pattern_id: usize,
    candidate_key: String,
    score: u64,
    informational_attack: u32,
}

impl BuildSuppliedScoreWinnerV1 {
    pub const fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    pub fn candidate_key(&self) -> &str {
        &self.candidate_key
    }

    pub const fn score(&self) -> u64 {
        self.score
    }

    pub const fn informational_attack(&self) -> u32 {
        self.informational_attack
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildSuppliedScoreV1Result {
    authority: ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    input_identity_sha256: String,
    score_profile: String,
    initial_b2b: u16,
    source_candidate_count: usize,
    reachable_candidate_count: usize,
    selected_candidate_count: usize,
    pattern_count: usize,
    required_pattern_count: usize,
    canonical_candidate_keys: Vec<String>,
    winners: Vec<BuildSuppliedScoreWinnerV1>,
    alternatives: Arc<CoveragePortfolioAlternativeSet>,
    completeness: BuildSuppliedReplayCompletenessEvidence,
}

impl BuildSuppliedScoreV1Result {
    pub(crate) const fn contract_id(&self) -> &'static str {
        BUILD_SUPPLIED_SCORE_RESULT_CONTRACT
    }

    pub(crate) fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }

    pub(crate) fn score_profile(&self) -> &str {
        &self.score_profile
    }

    pub(crate) const fn initial_b2b(&self) -> u16 {
        self.initial_b2b
    }

    pub(crate) const fn source_candidate_count(&self) -> usize {
        self.source_candidate_count
    }

    pub(crate) const fn reachable_candidate_count(&self) -> usize {
        self.reachable_candidate_count
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

    pub(crate) fn canonical_candidate_keys(&self) -> &[String] {
        &self.canonical_candidate_keys
    }

    pub(crate) fn winners(&self) -> &[BuildSuppliedScoreWinnerV1] {
        &self.winners
    }

    pub(crate) fn portfolio_alternative_owner(
        &self,
    ) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
        self.completeness.complete().then_some(&self.alternatives)
    }

    pub(crate) const fn completeness(&self) -> BuildSuppliedReplayCompletenessEvidence {
        self.completeness
    }
}

impl BuildSuppliedMinimumCoverV1Result {
    pub(crate) const fn contract_id(&self) -> &'static str {
        BUILD_SUPPLIED_MINIMUM_COVER_RESULT_CONTRACT
    }

    pub(crate) const fn replay_basis(&self) -> &'static str {
        BUILD_SUPPLIED_REPLAY_BASIS
    }

    pub(crate) fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }

    pub(crate) const fn source_candidate_count(&self) -> usize {
        self.source_candidate_count
    }

    pub(crate) const fn reachable_candidate_count(&self) -> usize {
        self.reachable_candidate_count
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

    pub(crate) fn canonical_candidate_keys(&self) -> &[String] {
        &self.canonical_candidate_keys
    }

    pub(crate) fn portfolio_alternative_owner(
        &self,
    ) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
        self.completeness.complete().then_some(&self.alternatives)
    }

    pub(crate) const fn completeness(&self) -> BuildSuppliedReplayCompletenessEvidence {
        self.completeness
    }

    pub(crate) const fn authority(
        &self,
    ) -> &ValidatedBuildSuppliedSolutionEvaluationResultAuthority {
        &self.authority
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BuildSuppliedEvaluationResultError {
    UnsupportedCapability,
    QueryNotCoverageCapable,
    InputIdentityMismatch,
    Producer(BuildSolutionProbabilityResultError),
    IncompleteEvidence,
    ReplayEvidenceInvalid,
    ScoreEvidenceInvalid(&'static str),
    PatternUniverseInvalid,
    EmptyReachableCoverage,
    QueryCompileFailed,
    ProbabilityUnionInvalid,
    MinimumCover(ExactMinimumCoverError),
    Portfolio(PortfolioAlternativeError),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidatedBuildColoredReplay {
    pub(crate) pattern_count: usize,
    pub(crate) required: PatternBitSet,
    pub(crate) rows: Vec<PatternBitSet>,
    pub(crate) weights: clearra_coverage::pattern::weighted_pattern_set::WeightedPatternSet,
    pub(crate) union_probability: String,
    pub(crate) producer_solution_hash: String,
    pub(crate) problem_id: String,
    pub(crate) rule_profile_id: String,
    pub(crate) kick_profile_id: String,
    pub(crate) pattern_universe_identity: String,
    pub(crate) product_build_identity: String,
}

pub(crate) fn validate_build_supplied_minimum_cover_v1_result(
    authority: ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
    result: &CoreExecutionResult,
) -> Result<BuildSuppliedMinimumCoverV1Result, BuildSuppliedEvaluationResultError> {
    let replay = validate_build_supplied_replay(
        &authority,
        BuildSuppliedSolutionEvaluationContract::Minimals,
        query,
        supplied,
        result,
    )?;
    let selection = exact_minimum_cover(&replay.required, &replay.rows)
        .map_err(BuildSuppliedEvaluationResultError::MinimumCover)?;
    if !selection.complete() || selection.covered_patterns() != &replay.required {
        return Err(BuildSuppliedEvaluationResultError::IncompleteEvidence);
    }
    let canonical_candidate_keys = selection
        .row_indices()
        .iter()
        .map(|index| supplied.candidate_keys()[*index].clone())
        .collect::<Vec<_>>();
    let identity = PortfolioAlternativeSetIdentity::new(
        format!(
            "build.evaluate.minimals:{}:{}:{}:{:?}:{:?}",
            replay.problem_id,
            supplied.input_identity_sha256(),
            queue_knowledge_id(query.queue_observation_policy()),
            query.field().base_words(),
            query.field().target_words(),
        ),
        format!(
            "build-supplied-source.v1:{}:{}",
            supplied.input_identity_sha256(),
            replay.producer_solution_hash,
        ),
        format!(
            "rule:{}:kick:{}:queue-knowledge:{}:replay:{}",
            replay.rule_profile_id,
            replay.kick_profile_id,
            queue_knowledge_id(query.queue_observation_policy()),
            BUILD_SUPPLIED_REPLAY_BASIS,
        ),
        replay.pattern_universe_identity,
        replay.product_build_identity,
    )
    .map_err(BuildSuppliedEvaluationResultError::Portfolio)?;
    let alternatives = Arc::new(
        CoveragePortfolioAlternativeSet::new(
            identity,
            supplied.candidate_keys().to_vec(),
            replay.required.clone(),
            replay.rows.clone(),
            &canonical_candidate_keys,
        )
        .map_err(BuildSuppliedEvaluationResultError::Portfolio)?,
    );

    Ok(BuildSuppliedMinimumCoverV1Result {
        authority,
        input_identity_sha256: supplied.input_identity_sha256().to_owned(),
        source_candidate_count: supplied.identities().len(),
        reachable_candidate_count: replay.rows.iter().filter(|row| !row.is_empty()).count(),
        selected_candidate_count: canonical_candidate_keys.len(),
        pattern_count: replay.pattern_count,
        required_pattern_count: replay.required.count_ones() as usize,
        union_probability: replay.union_probability,
        canonical_candidate_keys,
        alternatives,
        completeness: BuildSuppliedReplayCompletenessEvidence {
            input_identity_bound: true,
            producer_filter_bound: true,
            buildability_replay_complete: true,
            coverage_rows_complete: true,
            probability_weights_complete: true,
            exact_minimum_proven: true,
        },
    })
}

pub(crate) fn validate_build_supplied_cover_percent_v1_result(
    authority: ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
    result: &CoreExecutionResult,
) -> Result<BuildSuppliedCoverPercentV1Result, BuildSuppliedEvaluationResultError> {
    let replay = validate_build_supplied_replay(
        &authority,
        BuildSuppliedSolutionEvaluationContract::CoverPercent,
        query,
        supplied,
        result,
    )?;
    let evaluation_identity_sha256 = supplied_evaluation_identity_sha256(
        BuildSuppliedSolutionEvaluationContract::CoverPercent.capability_id(),
        query,
        supplied,
        &replay,
    );
    Ok(BuildSuppliedCoverPercentV1Result {
        authority,
        input_identity_sha256: supplied.input_identity_sha256().to_owned(),
        evaluation_identity_sha256,
        source_candidate_count: supplied.identities().len(),
        reachable_candidate_count: replay.rows.iter().filter(|row| !row.is_empty()).count(),
        pattern_count: replay.pattern_count,
        covered_pattern_count: replay.required.count_ones() as usize,
        union_probability: replay.union_probability,
        completeness: BuildSuppliedReplayCompletenessEvidence {
            input_identity_bound: true,
            producer_filter_bound: true,
            buildability_replay_complete: true,
            coverage_rows_complete: true,
            probability_weights_complete: true,
            exact_minimum_proven: false,
        },
    })
}

pub(crate) fn validate_build_supplied_coverage_v1_result(
    authority: ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
    result: &CoreExecutionResult,
) -> Result<BuildSuppliedCoverageV1Result, BuildSuppliedEvaluationResultError> {
    let contract = authority.contract();
    if !matches!(
        contract,
        BuildSuppliedSolutionEvaluationContract::Cover
            | BuildSuppliedSolutionEvaluationContract::B2bCover
    ) {
        return Err(BuildSuppliedEvaluationResultError::UnsupportedCapability);
    }
    let replay = validate_build_supplied_replay(&authority, contract, query, supplied, result)?;
    let b2b_preservation_required = contract == BuildSuppliedSolutionEvaluationContract::B2bCover;
    if b2b_preservation_required
        && (result.unique_field("execution_constraint_materialized") != Some("true")
            || result.unique_field("b2b_preservation_count_complete") != Some("true")
            || result.unique_field("b2b_preservation_probability_complete") != Some("true"))
    {
        return Err(BuildSuppliedEvaluationResultError::ReplayEvidenceInvalid);
    }
    let candidates = supplied
        .candidate_keys()
        .iter()
        .zip(&replay.rows)
        .map(|(candidate_key, row)| BuildSuppliedCandidateCoverageV1 {
            candidate_key: candidate_key.clone(),
            covered_pattern_count: row.count_ones() as usize,
        })
        .collect::<Vec<_>>();
    let evaluation_identity_sha256 =
        supplied_evaluation_identity_sha256(contract.capability_id(), query, supplied, &replay);
    Ok(BuildSuppliedCoverageV1Result {
        authority,
        contract_id: if b2b_preservation_required {
            BUILD_SUPPLIED_B2B_COVERAGE_RESULT_CONTRACT
        } else {
            BUILD_SUPPLIED_COVERAGE_RESULT_CONTRACT
        },
        input_identity_sha256: supplied.input_identity_sha256().to_owned(),
        evaluation_identity_sha256,
        source_candidate_count: supplied.identities().len(),
        reachable_candidate_count: replay.rows.iter().filter(|row| !row.is_empty()).count(),
        pattern_count: replay.pattern_count,
        covered_pattern_count: replay.required.count_ones() as usize,
        union_probability: replay.union_probability,
        b2b_preservation_required,
        candidates,
        completeness: BuildSuppliedReplayCompletenessEvidence {
            input_identity_bound: true,
            producer_filter_bound: true,
            buildability_replay_complete: true,
            coverage_rows_complete: true,
            probability_weights_complete: true,
            exact_minimum_proven: false,
        },
    })
}

pub(crate) fn validate_build_supplied_score_v1_result(
    authority: ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
    result: &CoreExecutionResult,
    derivation: &PcScoreDerivation,
) -> Result<BuildSuppliedScoreV1Result, BuildSuppliedEvaluationResultError> {
    let replay = validate_build_supplied_replay(
        &authority,
        BuildSuppliedSolutionEvaluationContract::Score,
        query,
        supplied,
        result,
    )?;
    let (score_profile_id, initial_b2b, score_accuracy, profile_specific_exact) = {
        let authority_query = authority.query();
        let score_query = authority_query.score_query().ok_or(
            BuildSuppliedEvaluationResultError::ScoreEvidenceInvalid("score_query_missing"),
        )?;
        (
            String::from(score_query.score_profile_id()),
            score_query.initial_b2b(),
            authority_query
                .options()
                .score_accuracy()
                .unwrap_or("unavailable")
                .to_owned(),
            authority_query
                .options()
                .profile_specific_exact()
                .unwrap_or(false),
        )
    };
    let initial_b2b_decimal = initial_b2b.to_string();
    let execution_profile_id = match score_profile_id.as_str() {
        "tetrio" => "tetrio-pc-t-spins",
        "guideline" => "guideline-pc-t-spins",
        "jstris-ultra" => "jstris-ultra-pc-t-spins",
        _ => {
            return Err(BuildSuppliedEvaluationResultError::ScoreEvidenceInvalid(
                "score_profile_unsupported",
            ))
        }
    };
    for (matches, reason) in [
        (
            derivation.execution_source_complete(),
            "score_execution_source_incomplete",
        ),
        (
            result.unique_field("score_summary_complete") == Some("true"),
            "score_summary_incomplete",
        ),
        (
            result.unique_field("score_equality_basis") == Some("score-only"),
            "score_equality_basis_mismatch",
        ),
        (
            result.unique_field("informational_attack_basis")
                == Some("canonical-equal-score-trace"),
            "informational_attack_basis_mismatch",
        ),
        (
            result.unique_field("score_profile_requested") == Some(score_profile_id.as_str()),
            "score_profile_request_mismatch",
        ),
        (
            result.unique_field("score_spin_profile_requested") == Some("t-spins"),
            "score_spin_profile_request_mismatch",
        ),
        (
            result.unique_field("score_profile") == Some(execution_profile_id),
            "score_execution_profile_mismatch",
        ),
        (
            result.unique_field("score_initial_b2b") == Some(initial_b2b_decimal.as_str()),
            "score_initial_b2b_mismatch",
        ),
    ] {
        if !matches {
            return Err(BuildSuppliedEvaluationResultError::ScoreEvidenceInvalid(
                reason,
            ));
        }
    }

    let candidate_index = supplied
        .identities()
        .iter()
        .copied()
        .enumerate()
        .map(|(index, identity)| (identity, index))
        .collect::<BTreeMap<_, _>>();
    let mut score_rows = (0..supplied.identities().len())
        .map(|_| PatternBitSet::new(replay.pattern_count))
        .collect::<Vec<_>>();
    let mut winners = Vec::with_capacity(derivation.pattern_winners().len());
    for winner in derivation.pattern_winners() {
        if winner.pattern_id() >= replay.pattern_count {
            return Err(BuildSuppliedEvaluationResultError::ScoreEvidenceInvalid(
                "winner_pattern_out_of_range",
            ));
        }
        let colored = StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(
            winner.solution_identity(),
        );
        let index = *candidate_index.get(&colored).ok_or(
            BuildSuppliedEvaluationResultError::ScoreEvidenceInvalid(
                "winner_candidate_not_supplied",
            ),
        )?;
        score_rows[index]
            .insert(PatternId::new(winner.pattern_id()))
            .map_err(|_| BuildSuppliedEvaluationResultError::PatternUniverseInvalid)?;
        winners.push(BuildSuppliedScoreWinnerV1 {
            pattern_id: winner.pattern_id(),
            candidate_key: supplied.candidate_keys()[index].clone(),
            score: winner.score(),
            informational_attack: winner.informational_attack(),
        });
    }
    let score_required =
        score_rows
            .iter()
            .try_fold(PatternBitSet::new(replay.pattern_count), |union, row| {
                union
                    .union(row)
                    .map_err(|_| BuildSuppliedEvaluationResultError::PatternUniverseInvalid)
            })?;
    if score_required != replay.required {
        return Err(BuildSuppliedEvaluationResultError::ScoreEvidenceInvalid(
            "winner_union_does_not_cover_replay",
        ));
    }
    let selection = exact_minimum_cover(&score_required, &score_rows)
        .map_err(BuildSuppliedEvaluationResultError::MinimumCover)?;
    if !selection.complete() || selection.covered_patterns() != &score_required {
        return Err(BuildSuppliedEvaluationResultError::IncompleteEvidence);
    }
    let canonical_candidate_keys = selection
        .row_indices()
        .iter()
        .map(|index| supplied.candidate_keys()[*index].clone())
        .collect::<Vec<_>>();
    let identity = PortfolioAlternativeSetIdentity::new(
        format!(
            "build.evaluate.score:{}:{}:{}:{}:{}",
            replay.problem_id,
            supplied.input_identity_sha256(),
            queue_knowledge_id(query.queue_observation_policy()),
            score_profile_id,
            initial_b2b,
        ),
        format!(
            "build-supplied-source.v1:{}:{}",
            supplied.input_identity_sha256(),
            replay.producer_solution_hash,
        ),
        format!(
            "score-profile:{}:accuracy:{}:profile-specific-exact:{}:equality:score-only:attack:informational",
            score_profile_id,
            score_accuracy,
            profile_specific_exact,
        ),
        replay.pattern_universe_identity.clone(),
        replay.product_build_identity.clone(),
    )
    .map_err(BuildSuppliedEvaluationResultError::Portfolio)?;
    let alternatives = Arc::new(
        CoveragePortfolioAlternativeSet::new(
            identity,
            supplied.candidate_keys().to_vec(),
            score_required,
            score_rows,
            &canonical_candidate_keys,
        )
        .map_err(BuildSuppliedEvaluationResultError::Portfolio)?,
    );

    Ok(BuildSuppliedScoreV1Result {
        authority,
        input_identity_sha256: supplied.input_identity_sha256().to_owned(),
        score_profile: score_profile_id,
        initial_b2b,
        source_candidate_count: supplied.identities().len(),
        reachable_candidate_count: replay.rows.iter().filter(|row| !row.is_empty()).count(),
        selected_candidate_count: canonical_candidate_keys.len(),
        pattern_count: replay.pattern_count,
        required_pattern_count: replay.required.count_ones() as usize,
        canonical_candidate_keys,
        winners,
        alternatives,
        completeness: BuildSuppliedReplayCompletenessEvidence {
            input_identity_bound: true,
            producer_filter_bound: true,
            buildability_replay_complete: true,
            coverage_rows_complete: true,
            probability_weights_complete: true,
            exact_minimum_proven: true,
        },
    })
}

fn validate_build_supplied_replay(
    authority: &ValidatedBuildSuppliedSolutionEvaluationResultAuthority,
    expected_contract: BuildSuppliedSolutionEvaluationContract,
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
    result: &CoreExecutionResult,
) -> Result<ValidatedBuildColoredReplay, BuildSuppliedEvaluationResultError> {
    if authority.contract() != expected_contract {
        return Err(BuildSuppliedEvaluationResultError::UnsupportedCapability);
    }
    let document = authority.query().document();
    if document.initial_board_mask() != supplied.initial_board_mask()
        || document.visible_height() != supplied.visible_height()
        || document.page_count() != supplied.page_count()
        || document.normalized_solution_count() != supplied.identities().len()
        || !document.operation_replay_available()
        || document.document_hash() != supplied.input_identity_sha256()
        || !supplied.matches_query(query)
        || query.allowed_colored_solution_identities() != Some(supplied.identities())
    {
        return Err(BuildSuppliedEvaluationResultError::InputIdentityMismatch);
    }

    validate_build_colored_replay(query, supplied, result)
}

pub(crate) fn validate_build_colored_replay<S>(
    query: &BuildProbabilityQuery,
    source: &S,
    result: &CoreExecutionResult,
) -> Result<ValidatedBuildColoredReplay, BuildSuppliedEvaluationResultError>
where
    S: BuildColoredReplaySource + ?Sized,
{
    validate_build_colored_replay_internal(query, source, result, false)
}

pub(crate) fn validate_build_colored_replay_allow_empty<S>(
    query: &BuildProbabilityQuery,
    source: &S,
    result: &CoreExecutionResult,
) -> Result<ValidatedBuildColoredReplay, BuildSuppliedEvaluationResultError>
where
    S: BuildColoredReplaySource + ?Sized,
{
    validate_build_colored_replay_internal(query, source, result, true)
}

fn validate_build_colored_replay_internal<S>(
    query: &BuildProbabilityQuery,
    source: &S,
    result: &CoreExecutionResult,
    allow_empty_required: bool,
) -> Result<ValidatedBuildColoredReplay, BuildSuppliedEvaluationResultError>
where
    S: BuildColoredReplaySource + ?Sized,
{
    if query.aggregation() != BuildProbabilityAggregation::Buildability
        || query.finesse_metric().requested()
        || query.solution_probability_policy() != BuildSolutionProbabilityPolicy::Include
        || !source.matches_query(query)
        || query.allowed_colored_solution_identities() != Some(source.identities())
    {
        return Err(BuildSuppliedEvaluationResultError::QueryNotCoverageCapable);
    }

    validate_build_probability_response(
        query.finesse_request(),
        query.field(),
        query.aggregation(),
        query.solution_probability_policy(),
        result,
    )
    .map_err(BuildSuppliedEvaluationResultError::Producer)?;
    let state = validate_build_solution_probability_reducer_input(
        Some(BuildSolutionProbabilityPolicy::Include),
        result,
    )
    .map_err(BuildSuppliedEvaluationResultError::Producer)?;
    if !state.requested
        || !state.complete
        || !state.count_complete
        || !state.probability_complete
        || !state.solution_keys_complete
        || state.resource_truncated
    {
        return Err(BuildSuppliedEvaluationResultError::IncompleteEvidence);
    }
    for (field, expected) in [
        ("buildability_verified", "true"),
        ("coverage_calculated", "true"),
        ("probability_calculated", "true"),
        ("objective_complete", "true"),
    ] {
        if result.unique_field(field) != Some(expected) {
            return Err(BuildSuppliedEvaluationResultError::ReplayEvidenceInvalid);
        }
    }

    let pattern_count = canonical_usize(result, "coverage_pattern_count")
        .ok_or(BuildSuppliedEvaluationResultError::PatternUniverseInvalid)?;
    let required =
        PatternBitSet::from_words(pattern_count, result.coverage_pattern_words().to_vec())
            .map_err(|_| BuildSuppliedEvaluationResultError::PatternUniverseInvalid)?;
    if required.is_empty() && !allow_empty_required {
        return Err(BuildSuppliedEvaluationResultError::EmptyReachableCoverage);
    }
    let producer_keys = result.normalized_solution_keys();
    let producer_identities = result.normalized_solution_identities();
    let producer_rows = result.normalized_solution_coverages();
    if producer_keys.len() != producer_identities.len()
        || producer_keys.len() != producer_rows.len()
    {
        return Err(BuildSuppliedEvaluationResultError::ReplayEvidenceInvalid);
    }

    let candidate_index = source
        .identities()
        .iter()
        .copied()
        .enumerate()
        .map(|(index, identity)| (identity, index))
        .collect::<BTreeMap<_, _>>();
    let mut rows = (0..source.identities().len())
        .map(|_| PatternBitSet::new(pattern_count))
        .collect::<Vec<_>>();
    for ((key, identity), coverage) in producer_keys
        .iter()
        .zip(producer_identities)
        .zip(producer_rows)
    {
        if NormalizedTilingSolutionKey::from_standard_board64_identity(*identity).as_str() != key
            || coverage.solution_key() != key
        {
            return Err(BuildSuppliedEvaluationResultError::ReplayEvidenceInvalid);
        }
        let colored =
            StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(*identity);
        let index = *candidate_index
            .get(&colored)
            .ok_or(BuildSuppliedEvaluationResultError::InputIdentityMismatch)?;
        rows[index] = rows[index]
            .union(coverage.covered_patterns())
            .map_err(|_| BuildSuppliedEvaluationResultError::PatternUniverseInvalid)?;
    }
    let rebuilt_union = rows
        .iter()
        .try_fold(PatternBitSet::new(pattern_count), |union, row| {
            union
                .union(row)
                .map_err(|_| BuildSuppliedEvaluationResultError::PatternUniverseInvalid)
        })?;
    if rebuilt_union != required {
        return Err(BuildSuppliedEvaluationResultError::ReplayEvidenceInvalid);
    }
    let weights = solution_probability_pattern_weights(result)
        .map_err(|_| BuildSuppliedEvaluationResultError::ProbabilityUnionInvalid)?;
    let union_probability = union_probability(&required, &weights)
        .map_err(|_| BuildSuppliedEvaluationResultError::ProbabilityUnionInvalid)?
        .get()
        .to_string();
    let problem = ProblemCompiler::compile_scenario_pc(query.core_query())
        .map_err(|_| BuildSuppliedEvaluationResultError::QueryCompileFailed)?;
    let producer_solution_hash = result
        .unique_field("normalized_solution_set_hash")
        .filter(|hash| !hash.is_empty() && *hash != "not-calculated")
        .ok_or(BuildSuppliedEvaluationResultError::ReplayEvidenceInvalid)?;
    if result.unique_field("actual_normalized_solution_set_hash") != Some(producer_solution_hash) {
        return Err(BuildSuppliedEvaluationResultError::ReplayEvidenceInvalid);
    }

    Ok(ValidatedBuildColoredReplay {
        pattern_count,
        required,
        rows,
        weights,
        union_probability,
        producer_solution_hash: producer_solution_hash.to_owned(),
        problem_id: problem.problem_id().as_str().to_owned(),
        rule_profile_id: problem.rule_profile_value().id().as_str().to_owned(),
        kick_profile_id: problem.kick_profile().profile_id().as_str().to_owned(),
        pattern_universe_identity: supplied_pattern_universe_identity(pattern_count, result),
        product_build_identity: product_build_identity_component(),
    })
}

pub(crate) fn target_cells(identity: StandardBoard64ColoredTilingIdentity) -> u64 {
    identity
        .piece_masks()
        .into_iter()
        .fold(0_u64, |union, mask| union | mask)
}

pub(crate) const fn queue_knowledge_id(
    policy: clearra_supply::QueueObservationPolicy,
) -> &'static str {
    match policy {
        clearra_supply::QueueObservationPolicy::FullQueueOracle => "oracle",
        clearra_supply::QueueObservationPolicy::VisibleSeven => "visible-7",
    }
}

pub(crate) fn colored_candidate_key(identity: StandardBoard64ColoredTilingIdentity) -> String {
    let colors = PieceKind::STANDARD_TETROMINOES
        .iter()
        .copied()
        .zip(identity.piece_masks())
        .filter(|(_, mask)| *mask != 0)
        .map(|(piece, mask)| format!("{}:{mask:016x}", piece.as_ascii()))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "cfk1|initial={:016x}|colors={colors}",
        identity.initial_board_mask()
    )
}

fn supplied_input_identity_sha256(
    initial_board_mask: u64,
    target_cells_mask: u64,
    visible_height: u8,
    page_count: usize,
    document_hash: &str,
    candidate_keys: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.build-supplied-solution-set.v1\0");
    hasher.update(initial_board_mask.to_be_bytes());
    hasher.update(target_cells_mask.to_be_bytes());
    hasher.update([visible_height]);
    hasher.update((page_count as u128).to_be_bytes());
    hash_text(&mut hasher, document_hash);
    hasher.update((candidate_keys.len() as u128).to_be_bytes());
    for key in candidate_keys {
        hash_text(&mut hasher, key);
    }
    hex_sha256(hasher.finalize())
}

fn supplied_pattern_universe_identity(
    pattern_count: usize,
    result: &CoreExecutionResult,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.build-supplied-pattern-universe.v1\0");
    hasher.update((pattern_count as u128).to_be_bytes());
    for word in result.coverage_pattern_words() {
        hasher.update(word.to_be_bytes());
    }
    for weight in result.postprocess_pattern_weights() {
        hash_text(&mut hasher, weight);
    }
    format!(
        "build-supplied-pattern-universe.v1:{}",
        hex_sha256(hasher.finalize())
    )
}

pub(crate) fn product_build_identity_component() -> String {
    let identity = clearra_host_contract::ProductBuildIdentity::current();
    format!(
        "product-build.v1:{}:{}:{}:{}:{}",
        identity.engine_build_id(),
        identity.source_commit(),
        identity.contract_schema_version(),
        identity.supply_semantics_id(),
        identity.artifact_schema_version(),
    )
}

fn supplied_evaluation_identity_sha256(
    capability_id: &str,
    query: &BuildProbabilityQuery,
    supplied: &BuildSuppliedSolutionSetV1,
    replay: &ValidatedBuildColoredReplay,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.build-supplied-evaluation.v1\0");
    for value in [
        capability_id,
        replay.problem_id.as_str(),
        supplied.input_identity_sha256(),
        replay.producer_solution_hash.as_str(),
        replay.rule_profile_id.as_str(),
        replay.kick_profile_id.as_str(),
        queue_knowledge_id(query.queue_observation_policy()),
        BUILD_SUPPLIED_REPLAY_BASIS,
        replay.pattern_universe_identity.as_str(),
        replay.product_build_identity.as_str(),
    ] {
        hash_text(&mut hasher, value);
    }
    for word in query.field().base_words() {
        hasher.update(word.to_be_bytes());
    }
    for word in query.field().target_words() {
        hasher.update(word.to_be_bytes());
    }
    hex_sha256(hasher.finalize())
}

fn canonical_usize(result: &CoreExecutionResult, key: &str) -> Option<usize> {
    let value = result.unique_field(key)?;
    let parsed = value.parse::<usize>().ok()?;
    (parsed.to_string() == value).then_some(parsed)
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u128).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn hex_sha256(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
