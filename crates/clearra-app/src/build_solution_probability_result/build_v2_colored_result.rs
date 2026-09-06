// SRP rationale: this module has one behavior-level change reason: replaying colored-target Build queries and reducing verified executions into typed results.

//! Actual colored-target Build replay and reduction.
//!
//! A colored target is a nominal input authority distinct from a supplied
//! solution document. Both authorities share the same exact producer
//! allow-list and BuildOrder/HoldAutomaton replay gate, but this module alone
//! decides which target-search result contract may be minted from that
//! evidence.

use std::{collections::BTreeMap, sync::Arc};

use clearra_core_domain::solution::StandardBoard64ColoredTilingIdentity;
use clearra_core_executor::CoreExecutionResult;
use clearra_coverage::{
    cover::ExactMinimumCoverError,
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
};
use clearra_problem::BuildProbabilityQuery;
use sha2::{Digest, Sha256};

use crate::{
    pc_score_postprocess::PcScoreDerivation,
    portfolio_alternative_store::{
        CoveragePortfolioAlternativeSet, PortfolioAlternativeError, PortfolioAlternativeSetIdentity,
    },
};

use super::{
    build_v2_contract::{
        BuildColoredTargetDocumentSnapshot, BuildTargetSearchContract,
        ValidatedBuildTargetSearchResultAuthority,
    },
    build_v2_options::BuildObjective,
    build_v2_supplied_result::{
        queue_knowledge_id, target_cells, validate_build_colored_replay, BuildColoredReplaySource,
        BuildSuppliedEvaluationResultError, BuildSuppliedSolutionSetError,
        BuildSuppliedSolutionSetV1, ValidatedBuildColoredReplay,
    },
};

pub(crate) const BUILD_COLORED_REPLAY_BASIS: &str =
    "colored-target-identity-filter-plus-buildability-replay";

/// Nominal target-search owner. The private supplied source is only the shared
/// normalization implementation; no conversion to a supplied-solution query
/// or result authority is exposed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildColoredTargetSetV1 {
    source: BuildSuppliedSolutionSetV1,
    input_identity_sha256: String,
}

impl BuildColoredTargetSetV1 {
    pub fn new(
        visible_height: u8,
        page_count: usize,
        document_hash: impl Into<String>,
        identities: impl IntoIterator<Item = StandardBoard64ColoredTilingIdentity>,
    ) -> Result<Self, BuildColoredTargetSetError> {
        let source =
            BuildSuppliedSolutionSetV1::new(visible_height, page_count, document_hash, identities)
                .map_err(BuildColoredTargetSetError::Source)?;
        let input_identity_sha256 = colored_target_input_identity_sha256(&source);
        Ok(Self {
            source,
            input_identity_sha256,
        })
    }

    pub const fn initial_board_mask(&self) -> u64 {
        self.source.initial_board_mask()
    }

    pub const fn target_cells_mask(&self) -> u64 {
        self.source.target_cells_mask()
    }

    pub const fn visible_height(&self) -> u8 {
        self.source.visible_height()
    }

    pub const fn page_count(&self) -> usize {
        self.source.page_count()
    }

    pub fn document_hash(&self) -> &str {
        self.source.document_hash()
    }

    pub fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }

    pub fn candidate_keys(&self) -> &[String] {
        self.source.candidate_keys()
    }

    pub fn identities(&self) -> &[StandardBoard64ColoredTilingIdentity] {
        self.source.identities()
    }

    pub fn matches_query(&self, query: &BuildProbabilityQuery) -> bool {
        self.source.matches_query(query)
    }

    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.source
            .checked_retained_capacity_bytes()?
            .checked_add(self.input_identity_sha256.capacity() as u128)
    }
}

impl BuildColoredReplaySource for BuildColoredTargetSetV1 {
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
pub enum BuildColoredTargetSetError {
    Source(BuildSuppliedSolutionSetError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuildColoredTargetCompletenessEvidence {
    input_identity_bound: bool,
    producer_filter_bound: bool,
    buildability_replay_complete: bool,
    coverage_rows_complete: bool,
    probability_weights_complete: bool,
    score_evidence_complete: bool,
    exact_minimum_proven: bool,
}

impl BuildColoredTargetCompletenessEvidence {
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

    pub(crate) const fn score_evidence_complete(self) -> bool {
        self.score_evidence_complete
    }

    pub(crate) const fn exact_minimum_proven(self) -> bool {
        self.exact_minimum_proven
    }

    pub(crate) const fn replay_complete(self) -> bool {
        self.input_identity_bound
            && self.producer_filter_bound
            && self.buildability_replay_complete
            && self.coverage_rows_complete
            && self.probability_weights_complete
    }

    pub(crate) const fn complete(self) -> bool {
        self.replay_complete() && self.score_evidence_complete && self.exact_minimum_proven
    }

    pub(crate) const fn portfolio_complete(self) -> bool {
        self.replay_complete() && self.exact_minimum_proven
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildColoredTargetCandidateCoverageV1 {
    candidate_key: String,
    covered_pattern_count: usize,
}

impl BuildColoredTargetCandidateCoverageV1 {
    pub fn candidate_key(&self) -> &str {
        &self.candidate_key
    }

    pub const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildColoredTargetScoreWinnerV1 {
    pattern_id: usize,
    candidate_key: String,
    score: u64,
    informational_attack: u32,
}

impl BuildColoredTargetScoreWinnerV1 {
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
pub(crate) struct BuildColoredTargetFamilyV1Result {
    authority: ValidatedBuildTargetSearchResultAuthority,
    contract_id: &'static str,
    input_identity_sha256: String,
    evaluation_identity_sha256: String,
    objective: BuildObjective,
    source_candidate_count: usize,
    reachable_candidate_count: usize,
    pattern_count: usize,
    covered_pattern_count: usize,
    union_probability: String,
    candidates: Vec<BuildColoredTargetCandidateCoverageV1>,
    completeness: BuildColoredTargetCompletenessEvidence,
}

impl BuildColoredTargetFamilyV1Result {
    // Retained for product adapters that audit the validator-minted authority.
    #[allow(dead_code)]
    pub(crate) const fn authority(&self) -> &ValidatedBuildTargetSearchResultAuthority {
        &self.authority
    }

    pub(crate) const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub(crate) fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }

    pub(crate) fn evaluation_identity_sha256(&self) -> &str {
        &self.evaluation_identity_sha256
    }

    pub(crate) const fn objective(&self) -> BuildObjective {
        self.objective
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

    pub(crate) fn candidates(&self) -> &[BuildColoredTargetCandidateCoverageV1] {
        &self.candidates
    }

    pub(crate) const fn completeness(&self) -> BuildColoredTargetCompletenessEvidence {
        self.completeness
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildColoredTargetProbabilityV1Result {
    authority: ValidatedBuildTargetSearchResultAuthority,
    input_identity_sha256: String,
    evaluation_identity_sha256: String,
    objective: BuildObjective,
    source_candidate_count: usize,
    reachable_candidate_count: usize,
    pattern_count: usize,
    covered_pattern_count: usize,
    union_probability: String,
    completeness: BuildColoredTargetCompletenessEvidence,
}

impl BuildColoredTargetProbabilityV1Result {
    pub(crate) const fn contract_id(&self) -> &'static str {
        "build-setup-cover-probability.v1"
    }

    // Retained for product adapters that audit the validator-minted authority.
    #[allow(dead_code)]
    pub(crate) const fn authority(&self) -> &ValidatedBuildTargetSearchResultAuthority {
        &self.authority
    }

    pub(crate) fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }

    pub(crate) fn evaluation_identity_sha256(&self) -> &str {
        &self.evaluation_identity_sha256
    }

    pub(crate) const fn objective(&self) -> BuildObjective {
        self.objective
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

    pub(crate) const fn completeness(&self) -> BuildColoredTargetCompletenessEvidence {
        self.completeness
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildColoredTargetPortfolioV1Result {
    authority: ValidatedBuildTargetSearchResultAuthority,
    contract_id: &'static str,
    input_identity_sha256: String,
    objective: BuildObjective,
    source_candidate_count: usize,
    reachable_candidate_count: usize,
    selected_candidate_count: usize,
    pattern_count: usize,
    required_pattern_count: usize,
    union_probability: String,
    canonical_candidate_keys: Vec<String>,
    alternatives: Arc<CoveragePortfolioAlternativeSet>,
    completeness: BuildColoredTargetCompletenessEvidence,
}

impl BuildColoredTargetPortfolioV1Result {
    // Retained for product adapters that audit the validator-minted authority.
    #[allow(dead_code)]
    pub(crate) const fn authority(&self) -> &ValidatedBuildTargetSearchResultAuthority {
        &self.authority
    }

    pub(crate) const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub(crate) fn input_identity_sha256(&self) -> &str {
        &self.input_identity_sha256
    }

    pub(crate) const fn objective(&self) -> BuildObjective {
        self.objective
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
        self.completeness
            .portfolio_complete()
            .then_some(&self.alternatives)
    }

    pub(crate) const fn completeness(&self) -> BuildColoredTargetCompletenessEvidence {
        self.completeness
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuildColoredTargetScoreV1Result {
    authority: ValidatedBuildTargetSearchResultAuthority,
    input_identity_sha256: String,
    score_profile: String,
    initial_b2b: u16,
    source_candidate_count: usize,
    reachable_candidate_count: usize,
    selected_candidate_count: usize,
    pattern_count: usize,
    required_pattern_count: usize,
    canonical_candidate_keys: Vec<String>,
    winners: Vec<BuildColoredTargetScoreWinnerV1>,
    alternatives: Arc<CoveragePortfolioAlternativeSet>,
    completeness: BuildColoredTargetCompletenessEvidence,
}

impl BuildColoredTargetScoreV1Result {
    pub(crate) const fn contract_id(&self) -> &'static str {
        "build-setup-cover-score.v1"
    }

    // Retained for product adapters that audit the validator-minted authority.
    #[allow(dead_code)]
    pub(crate) const fn authority(&self) -> &ValidatedBuildTargetSearchResultAuthority {
        &self.authority
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

    pub(crate) fn winners(&self) -> &[BuildColoredTargetScoreWinnerV1] {
        &self.winners
    }

    pub(crate) fn portfolio_alternative_owner(
        &self,
    ) -> Option<&Arc<CoveragePortfolioAlternativeSet>> {
        self.completeness.complete().then_some(&self.alternatives)
    }

    pub(crate) const fn completeness(&self) -> BuildColoredTargetCompletenessEvidence {
        self.completeness
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BuildColoredTargetResultError {
    UnsupportedCapability,
    UnsupportedObjective,
    MissingTargetDocument,
    InputIdentityMismatch,
    Replay(BuildSuppliedEvaluationResultError),
    ScoreEvidenceInvalid(&'static str),
    PatternUniverseInvalid,
    // Reserved to distinguish incomplete producer evidence from malformed evidence.
    #[allow(dead_code)]
    IncompleteEvidence,
    // Preserves the exact-cover failure category at this product boundary.
    #[allow(dead_code)]
    MinimumCover(ExactMinimumCoverError),
    Portfolio(PortfolioAlternativeError),
}

pub(crate) fn validate_build_colored_family_v1_result(
    authority: ValidatedBuildTargetSearchResultAuthority,
    query: &BuildProbabilityQuery,
    target: &BuildColoredTargetSetV1,
    result: &CoreExecutionResult,
) -> Result<BuildColoredTargetFamilyV1Result, BuildColoredTargetResultError> {
    let contract = authority.contract();
    let contract_id = match contract {
        BuildTargetSearchContract::Setup => "build-target-family.v2",
        BuildTargetSearchContract::Congruent => "build-congruence-family.v1",
        _ => return Err(BuildColoredTargetResultError::UnsupportedCapability),
    };
    let objective = authority.query().options().objective();
    if !matches!(objective, BuildObjective::All | BuildObjective::Unique) {
        return Err(BuildColoredTargetResultError::UnsupportedObjective);
    }
    validate_target_binding(&authority, query, target)?;
    let replay = validate_build_colored_replay(query, target, result)
        .map_err(BuildColoredTargetResultError::Replay)?;
    let candidates = target
        .candidate_keys()
        .iter()
        .cloned()
        .zip(&replay.rows)
        .map(
            |(candidate_key, row)| BuildColoredTargetCandidateCoverageV1 {
                candidate_key,
                covered_pattern_count: row.count_ones() as usize,
            },
        )
        .collect::<Vec<_>>();
    Ok(BuildColoredTargetFamilyV1Result {
        evaluation_identity_sha256: colored_evaluation_identity_sha256(
            contract, objective, query, target, &replay,
        ),
        authority,
        contract_id,
        input_identity_sha256: target.input_identity_sha256().to_owned(),
        objective,
        source_candidate_count: target.identities().len(),
        reachable_candidate_count: replay.rows.iter().filter(|row| !row.is_empty()).count(),
        pattern_count: replay.pattern_count,
        covered_pattern_count: replay.required.count_ones() as usize,
        union_probability: replay.union_probability,
        candidates,
        completeness: replay_completeness(false, false),
    })
}

pub(crate) fn validate_build_colored_probability_v1_result(
    authority: ValidatedBuildTargetSearchResultAuthority,
    query: &BuildProbabilityQuery,
    target: &BuildColoredTargetSetV1,
    result: &CoreExecutionResult,
) -> Result<BuildColoredTargetProbabilityV1Result, BuildColoredTargetResultError> {
    if authority.contract() != BuildTargetSearchContract::SetupCoverPercent {
        return Err(BuildColoredTargetResultError::UnsupportedCapability);
    }
    let objective = authority.query().options().objective();
    if !matches!(objective, BuildObjective::All | BuildObjective::Unique) {
        return Err(BuildColoredTargetResultError::UnsupportedObjective);
    }
    validate_target_binding(&authority, query, target)?;
    let replay = validate_build_colored_replay(query, target, result)
        .map_err(BuildColoredTargetResultError::Replay)?;
    Ok(BuildColoredTargetProbabilityV1Result {
        evaluation_identity_sha256: colored_evaluation_identity_sha256(
            authority.contract(),
            objective,
            query,
            target,
            &replay,
        ),
        authority,
        input_identity_sha256: target.input_identity_sha256().to_owned(),
        objective,
        source_candidate_count: target.identities().len(),
        reachable_candidate_count: replay.rows.iter().filter(|row| !row.is_empty()).count(),
        pattern_count: replay.pattern_count,
        covered_pattern_count: replay.required.count_ones() as usize,
        union_probability: replay.union_probability,
        completeness: replay_completeness(false, false),
    })
}

pub(crate) fn validate_build_colored_portfolio_v1_result(
    authority: ValidatedBuildTargetSearchResultAuthority,
    query: &BuildProbabilityQuery,
    target: &BuildColoredTargetSetV1,
    result: &CoreExecutionResult,
) -> Result<BuildColoredTargetPortfolioV1Result, BuildColoredTargetResultError> {
    let contract_id = match authority.contract() {
        BuildTargetSearchContract::CongruentCover => "build-congruence-coverage.v1",
        BuildTargetSearchContract::SetupCover => "build-setup-cover.v1",
        _ => return Err(BuildColoredTargetResultError::UnsupportedCapability),
    };
    let objective = authority.query().options().objective();
    if !matches!(
        objective,
        BuildObjective::MinCover | BuildObjective::MaxProbabilityMinimum
    ) {
        return Err(BuildColoredTargetResultError::UnsupportedObjective);
    }
    validate_target_binding(&authority, query, target)?;
    let replay = validate_build_colored_replay(query, target, result)
        .map_err(BuildColoredTargetResultError::Replay)?;
    let identity = portfolio_identity(
        authority.contract(),
        objective,
        query,
        target,
        &replay,
        "coverage-or-union",
    )?;
    let alternatives = Arc::new(
        CoveragePortfolioAlternativeSet::new_canonical(
            identity,
            target.candidate_keys().to_vec(),
            replay.required.clone(),
            replay.rows.clone(),
        )
        .map_err(BuildColoredTargetResultError::Portfolio)?,
    );
    let canonical_candidate_keys = alternatives
        .canonical_candidate_keys_owned()
        .map_err(BuildColoredTargetResultError::Portfolio)?;
    Ok(BuildColoredTargetPortfolioV1Result {
        authority,
        contract_id,
        input_identity_sha256: target.input_identity_sha256().to_owned(),
        objective,
        source_candidate_count: target.identities().len(),
        reachable_candidate_count: replay.rows.iter().filter(|row| !row.is_empty()).count(),
        selected_candidate_count: canonical_candidate_keys.len(),
        pattern_count: replay.pattern_count,
        required_pattern_count: replay.required.count_ones() as usize,
        union_probability: replay.union_probability,
        canonical_candidate_keys,
        alternatives,
        completeness: replay_completeness(true, false),
    })
}

pub(crate) fn validate_build_colored_score_v1_result(
    authority: ValidatedBuildTargetSearchResultAuthority,
    query: &BuildProbabilityQuery,
    target: &BuildColoredTargetSetV1,
    result: &CoreExecutionResult,
    derivation: &PcScoreDerivation,
) -> Result<BuildColoredTargetScoreV1Result, BuildColoredTargetResultError> {
    if authority.contract() != BuildTargetSearchContract::SetupCoverScore
        || authority.query().options().objective() != BuildObjective::MaxScoreCover
    {
        return Err(BuildColoredTargetResultError::UnsupportedCapability);
    }
    validate_target_binding(&authority, query, target)?;
    let replay = validate_build_colored_replay(query, target, result)
        .map_err(BuildColoredTargetResultError::Replay)?;
    let score_query = authority.query().colored_target_score_query().ok_or(
        BuildColoredTargetResultError::ScoreEvidenceInvalid("score_query_missing"),
    )?;
    let score_profile = score_query.score_profile_id().to_owned();
    let initial_b2b = score_query.initial_b2b();
    validate_score_metadata(result, derivation, &score_profile, initial_b2b)?;

    let candidate_index = target
        .identities()
        .iter()
        .copied()
        .enumerate()
        .map(|(index, identity)| (identity, index))
        .collect::<BTreeMap<_, _>>();
    let mut score_rows = (0..target.identities().len())
        .map(|_| PatternBitSet::new(replay.pattern_count))
        .collect::<Vec<_>>();
    let mut winners = Vec::with_capacity(derivation.pattern_winners().len());
    for winner in derivation.pattern_winners() {
        if winner.pattern_id() >= replay.pattern_count {
            return Err(BuildColoredTargetResultError::ScoreEvidenceInvalid(
                "winner_pattern_out_of_range",
            ));
        }
        let colored = StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(
            winner.solution_identity(),
        );
        let index = *candidate_index.get(&colored).ok_or(
            BuildColoredTargetResultError::ScoreEvidenceInvalid("winner_candidate_not_target"),
        )?;
        score_rows[index]
            .insert(PatternId::new(winner.pattern_id()))
            .map_err(|_| BuildColoredTargetResultError::PatternUniverseInvalid)?;
        winners.push(BuildColoredTargetScoreWinnerV1 {
            pattern_id: winner.pattern_id(),
            candidate_key: target.candidate_keys()[index].clone(),
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
                    .map_err(|_| BuildColoredTargetResultError::PatternUniverseInvalid)
            })?;
    if score_required != replay.required {
        return Err(BuildColoredTargetResultError::ScoreEvidenceInvalid(
            "winner_union_does_not_cover_replay",
        ));
    }
    let identity = portfolio_identity(
        authority.contract(),
        BuildObjective::MaxScoreCover,
        query,
        target,
        &replay,
        &format!(
            "score-profile:{score_profile}:initial-b2b:{initial_b2b}:accuracy:basic-approximation:profile-specific-exact:false:equality:score-only:attack:informational"
        ),
    )?;
    let alternatives = Arc::new(
        CoveragePortfolioAlternativeSet::new_canonical(
            identity,
            target.candidate_keys().to_vec(),
            score_required,
            score_rows,
        )
        .map_err(BuildColoredTargetResultError::Portfolio)?,
    );
    let canonical_candidate_keys = alternatives
        .canonical_candidate_keys_owned()
        .map_err(BuildColoredTargetResultError::Portfolio)?;
    Ok(BuildColoredTargetScoreV1Result {
        authority,
        input_identity_sha256: target.input_identity_sha256().to_owned(),
        score_profile,
        initial_b2b,
        source_candidate_count: target.identities().len(),
        reachable_candidate_count: replay.rows.iter().filter(|row| !row.is_empty()).count(),
        selected_candidate_count: canonical_candidate_keys.len(),
        pattern_count: replay.pattern_count,
        required_pattern_count: replay.required.count_ones() as usize,
        canonical_candidate_keys,
        winners,
        alternatives,
        completeness: replay_completeness(true, true),
    })
}

fn validate_target_binding(
    authority: &ValidatedBuildTargetSearchResultAuthority,
    query: &BuildProbabilityQuery,
    target: &BuildColoredTargetSetV1,
) -> Result<(), BuildColoredTargetResultError> {
    let document =
        target_document(authority).ok_or(BuildColoredTargetResultError::MissingTargetDocument)?;
    if document.initial_board_mask() != target.initial_board_mask()
        || document.visible_height() != target.visible_height()
        || document.page_count() != target.page_count()
        || document.normalized_target_count() != target.identities().len()
        || !document.operation_replay_available()
        || document.document_hash() != target.input_identity_sha256()
        || authority.query().target_query() != query
        || !target.matches_query(query)
        || query.allowed_colored_solution_identities() != Some(target.identities())
    {
        return Err(BuildColoredTargetResultError::InputIdentityMismatch);
    }
    Ok(())
}

pub(crate) fn validate_target_binding_for_setup_score(
    authority: &ValidatedBuildTargetSearchResultAuthority,
    query: &BuildProbabilityQuery,
    target: &BuildColoredTargetSetV1,
) -> Result<(), BuildColoredTargetResultError> {
    validate_target_binding(authority, query, target)
}

fn target_document(
    authority: &ValidatedBuildTargetSearchResultAuthority,
) -> Option<&BuildColoredTargetDocumentSnapshot> {
    authority.query().colored_target_document().or_else(|| {
        authority
            .query()
            .colored_target_score_query()
            .map(|query| query.document())
    })
}

fn validate_score_metadata(
    result: &CoreExecutionResult,
    derivation: &PcScoreDerivation,
    score_profile: &str,
    initial_b2b: u16,
) -> Result<(), BuildColoredTargetResultError> {
    let execution_profile = match score_profile {
        "tetrio" => "tetrio-pc-t-spins",
        "guideline" => "guideline-pc-t-spins",
        "jstris-ultra" => "jstris-ultra-pc-t-spins",
        _ => {
            return Err(BuildColoredTargetResultError::ScoreEvidenceInvalid(
                "score_profile_unsupported",
            ))
        }
    };
    let initial_b2b = initial_b2b.to_string();
    for (valid, reason) in [
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
            result.unique_field("score_profile_requested") == Some(score_profile),
            "score_profile_request_mismatch",
        ),
        (
            result.unique_field("score_spin_profile_requested") == Some("t-spins"),
            "score_spin_profile_request_mismatch",
        ),
        (
            result.unique_field("score_profile") == Some(execution_profile),
            "score_execution_profile_mismatch",
        ),
        (
            result.unique_field("score_initial_b2b") == Some(initial_b2b.as_str()),
            "score_initial_b2b_mismatch",
        ),
    ] {
        if !valid {
            return Err(BuildColoredTargetResultError::ScoreEvidenceInvalid(reason));
        }
    }
    Ok(())
}

fn replay_completeness(
    exact_minimum_proven: bool,
    score_evidence_complete: bool,
) -> BuildColoredTargetCompletenessEvidence {
    BuildColoredTargetCompletenessEvidence {
        input_identity_bound: true,
        producer_filter_bound: true,
        buildability_replay_complete: true,
        coverage_rows_complete: true,
        probability_weights_complete: true,
        score_evidence_complete,
        exact_minimum_proven,
    }
}

fn portfolio_identity(
    contract: BuildTargetSearchContract,
    objective: BuildObjective,
    query: &BuildProbabilityQuery,
    target: &BuildColoredTargetSetV1,
    replay: &ValidatedBuildColoredReplay,
    reducer: &str,
) -> Result<PortfolioAlternativeSetIdentity, BuildColoredTargetResultError> {
    PortfolioAlternativeSetIdentity::new(
        format!(
            "{}:{}:{}:{}:{}",
            contract.capability_id(),
            replay.problem_id,
            target.input_identity_sha256(),
            queue_knowledge_id(query.queue_observation_policy()),
            objective.as_str(),
        ),
        format!(
            "build-colored-target-source.v1:{}:{}",
            target.input_identity_sha256(),
            replay.producer_solution_hash,
        ),
        format!(
            "rule:{}:kick:{}:queue-knowledge:{}:replay:{}:reducer:{}",
            replay.rule_profile_id,
            replay.kick_profile_id,
            queue_knowledge_id(query.queue_observation_policy()),
            BUILD_COLORED_REPLAY_BASIS,
            reducer,
        ),
        replay.pattern_universe_identity.clone(),
        replay.product_build_identity.clone(),
    )
    .map_err(BuildColoredTargetResultError::Portfolio)
}

fn colored_target_input_identity_sha256(source: &BuildSuppliedSolutionSetV1) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.build-colored-target-set.v1\0");
    hasher.update(source.initial_board_mask().to_be_bytes());
    hasher.update(source.target_cells_mask().to_be_bytes());
    hasher.update([source.visible_height()]);
    hasher.update((source.page_count() as u128).to_be_bytes());
    hash_text(&mut hasher, source.document_hash());
    hasher.update((source.candidate_keys().len() as u128).to_be_bytes());
    for (key, identity) in source.candidate_keys().iter().zip(source.identities()) {
        hash_text(&mut hasher, key);
        hasher.update(identity.initial_board_mask().to_be_bytes());
        hasher.update(target_cells(*identity).to_be_bytes());
        for mask in identity.piece_masks() {
            hasher.update(mask.to_be_bytes());
        }
    }
    hex_sha256(hasher.finalize())
}

fn colored_evaluation_identity_sha256(
    contract: BuildTargetSearchContract,
    objective: BuildObjective,
    query: &BuildProbabilityQuery,
    target: &BuildColoredTargetSetV1,
    replay: &ValidatedBuildColoredReplay,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.build-colored-target-evaluation.v1\0");
    for value in [
        contract.capability_id(),
        contract.problem_contract_id(),
        contract.input_schema_id(),
        contract.result_contract_id(),
        objective.as_str(),
        target.input_identity_sha256(),
        replay.problem_id.as_str(),
        replay.producer_solution_hash.as_str(),
        replay.rule_profile_id.as_str(),
        replay.kick_profile_id.as_str(),
        queue_knowledge_id(query.queue_observation_policy()),
        BUILD_COLORED_REPLAY_BASIS,
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

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
        solution::StandardBoard64ColoredTilingIdentity,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityField, BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
        ProblemCompiler,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::*;
    use crate::build_solution_probability_result::build_v2_supplied_result::colored_candidate_key;
    use crate::{
        build_solution_probability_result::{
            build_probability_resource_test_guard,
            build_v2_contract::{
                BuildTargetSearchQuerySnapshot, ReportedBuildTargetSearchResultIdentity,
            },
        },
        AppCoreExecutorService,
    };

    fn query() -> BuildProbabilityQuery {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("target");
        BuildProbabilityQuery::new(core, field)
            .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include)
    }

    fn target() -> BuildColoredTargetSetV1 {
        let mut masks = [0_u64; 7];
        masks[0] = 0xf;
        BuildColoredTargetSetV1::new(
            4,
            1,
            "colored-target:test",
            [
                StandardBoard64ColoredTilingIdentity::from_piece_masks(0, masks)
                    .expect("colored target"),
            ],
        )
        .expect("target set")
    }

    fn authority(
        snapshot: BuildTargetSearchQuerySnapshot,
    ) -> ValidatedBuildTargetSearchResultAuthority {
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
        .expect("matching authority")
    }

    #[test]
    fn target_identity_is_nominal_and_canonical() {
        let target = target();
        assert_eq!(target.input_identity_sha256().len(), 64);
        assert_ne!(
            target.input_identity_sha256(),
            target.source.input_identity_sha256()
        );
        assert_eq!(
            target.candidate_keys()[0],
            colored_candidate_key(target.identities()[0])
        );
    }

    #[test]
    fn partial_producer_never_mints_any_colored_target_result_family() {
        let target = target();
        let query =
            query().with_allowed_colored_solution_identities(target.identities().iter().copied());
        let document = BuildColoredTargetDocumentSnapshot::new(
            target.initial_board_mask(),
            target.visible_height(),
            target.page_count(),
            target.identities().len(),
            true,
            target.input_identity_sha256(),
        )
        .expect("document");
        let partial = CoreExecutionResult::default();
        for snapshot in [
            BuildTargetSearchQuerySnapshot::setup(query.clone(), document.clone()),
            BuildTargetSearchQuerySnapshot::congruent(query.clone(), document.clone()),
        ] {
            assert!(matches!(
                validate_build_colored_family_v1_result(
                    authority(snapshot),
                    &query,
                    &target,
                    &partial
                ),
                Err(BuildColoredTargetResultError::Replay(_))
            ));
        }
        assert!(matches!(
            validate_build_colored_probability_v1_result(
                authority(BuildTargetSearchQuerySnapshot::setup_cover_percent(
                    query.clone(),
                    document,
                )),
                &query,
                &target,
                &partial,
            ),
            Err(BuildColoredTargetResultError::Replay(_))
        ));
    }

    #[test]
    fn product_neutral_replay_gate_accepts_only_actual_complete_producer_output() {
        let _guard = build_probability_resource_test_guard();
        let target = target();
        let query =
            query().with_allowed_colored_solution_identities(target.identities().iter().copied());
        let problem = ProblemCompiler::compile_scenario_pc(query.core_query()).expect("compile");
        let result = AppCoreExecutorService::wasm_cpu()
            .execute_build_probability_with_control(
                &problem,
                query.field(),
                query.aggregation(),
                query.finesse_request().clone(),
                query.solution_probability_policy(),
                &ExecutionControl::default(),
            )
            .expect("actual replay");
        let replay = validate_build_colored_replay(&query, &target, &result).expect("complete");
        assert_eq!(replay.pattern_count, 1);
        assert_eq!(replay.required.count_ones(), 1);
        assert_eq!(replay.union_probability, "1");
    }
}
