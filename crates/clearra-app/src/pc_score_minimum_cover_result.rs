use std::{collections::BTreeMap, fmt::Write, sync::Arc};

use clearra_core_domain::solution::normalized_tiling_solution::{
    NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
};
use clearra_core_executor::CoreExecutionResult;
use clearra_coverage::{cover::exact_minimum_cover, pattern::pattern_bitset::PatternBitSet};
use sha2::{Digest, Sha256};

use crate::{
    pc_score_postprocess::PcScoreDerivation,
    pc_score_summary_result::ValidatedPcScoreExecutionEvidence, CoveragePortfolioAlternativeSet,
    PcScoreIngressOrigin, PcScorePatternWinnerV1, PcScoreProblemPreset, PcScoreQuerySnapshot,
    PcScoreSummaryV2Result, PortfolioAlternativeSetIdentity, PC_SCORE_CANONICAL_SELECTION,
    PC_SCORE_MAX_PATTERNS,
};

pub const PC_SCORE_PORTFOLIO_RESULT_CONTRACT: &str = "pc-score-portfolio.v2";

const SCORE_SUMMARY_CONTRACT: &str = "pc-score-summary.v2";
const ELIGIBLE_CANDIDATE_MAP_DIGEST_DOMAIN: &[u8] = b"clearra.pc-score-eligible-candidate-map.v2\0";
const SCORE_ELIGIBILITY_DIGEST_DOMAIN: &[u8] = b"clearra.pc-score-eligibility.v2\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcScoreMinimalsIngressOrigin {
    CanonicalPcScoreMinimals,
}

impl PcScoreMinimalsIngressOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalPcScoreMinimals => "canonical-pc-score-minimals",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcScoreEligiblePatternV2 {
    pattern_id: usize,
    best_score: u64,
}

impl PcScoreEligiblePatternV2 {
    pub const fn pattern_id(self) -> usize {
        self.pattern_id
    }

    pub const fn best_score(self) -> u64 {
        self.best_score
    }
}

/// One score candidate and the exact set of patterns for which it attains the
/// score-only global maximum.
///
/// The portfolio-local ID is the stable dense ID used by the shared page
/// store. `score_candidate_id` is the numeric identity from the complete score
/// replay. Neither this row nor either identity contains attack information.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcScoreEligibleCandidateV2 {
    portfolio_candidate_id: u64,
    score_candidate_id: u64,
    normalized_solution_key: String,
    eligible_patterns: Vec<PcScoreEligiblePatternV2>,
}

impl PcScoreEligibleCandidateV2 {
    pub const fn portfolio_candidate_id(&self) -> u64 {
        self.portfolio_candidate_id
    }

    pub const fn score_candidate_id(&self) -> u64 {
        self.score_candidate_id
    }

    pub fn score_candidate_id_decimal(&self) -> String {
        self.score_candidate_id.to_string()
    }

    pub fn normalized_solution_key(&self) -> &str {
        &self.normalized_solution_key
    }

    pub fn eligible_patterns(&self) -> &[PcScoreEligiblePatternV2] {
        &self.eligible_patterns
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcScorePortfolioCompletenessEvidence {
    source_universe_complete: bool,
    weight_model_complete: bool,
    score_execution_complete: bool,
    legal_replay_complete: bool,
    winner_family_complete: bool,
    coverage_rows_complete: bool,
    exact_minimum_proven: bool,
    identity_bound: bool,
}

impl PcScorePortfolioCompletenessEvidence {
    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub const fn weight_model_complete(self) -> bool {
        self.weight_model_complete
    }

    pub const fn score_execution_complete(self) -> bool {
        self.score_execution_complete
    }

    pub const fn legal_replay_complete(self) -> bool {
        self.legal_replay_complete
    }

    pub const fn winner_family_complete(self) -> bool {
        self.winner_family_complete
    }

    pub const fn coverage_rows_complete(self) -> bool {
        self.coverage_rows_complete
    }

    pub const fn exact_minimum_proven(self) -> bool {
        self.exact_minimum_proven
    }

    pub const fn identity_bound(self) -> bool {
        self.identity_bound
    }

    pub const fn complete(self) -> bool {
        self.source_universe_complete
            && self.weight_model_complete
            && self.score_execution_complete
            && self.legal_replay_complete
            && self.winner_family_complete
            && self.coverage_rows_complete
            && self.exact_minimum_proven
            && self.identity_bound
    }
}

/// Typed B-option result for `pc.score-minimals`.
///
/// `pattern_winners` retains the complete score/replay projection (including
/// informational attack), while `eligible_candidates` and the shared
/// portfolio owner retain only score-derived eligibility. This separation is
/// intentional: attack can be displayed as trace information but cannot enter
/// equality, eligibility, canonical order, or portfolio membership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcScorePortfolioV2Result {
    contract_id: &'static str,
    origin: PcScoreMinimalsIngressOrigin,
    query: PcScoreQuerySnapshot,
    problem_preset: PcScoreProblemPreset,
    problem_id: Arc<str>,
    score_profile_id: Arc<str>,
    materialized_pattern_count: usize,
    pattern_best_scores: Vec<u64>,
    pattern_winners: Arc<Vec<PcScorePatternWinnerV1>>,
    eligible_candidates: Arc<[PcScoreEligibleCandidateV2]>,
    eligible_candidate_map_sha256: String,
    score_eligibility_sha256: String,
    selected_score_candidate_ids: Vec<u64>,
    selected_solution_keys: Vec<String>,
    canonical_score_candidate_id: u64,
    canonical_solution_identity: StandardBoard64TilingIdentity,
    portfolio_alternatives: Arc<CoveragePortfolioAlternativeSet>,
    completeness: PcScorePortfolioCompletenessEvidence,
}

impl PcScorePortfolioV2Result {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn origin(&self) -> PcScoreMinimalsIngressOrigin {
        self.origin
    }

    pub const fn query(&self) -> &PcScoreQuerySnapshot {
        &self.query
    }

    pub const fn problem_preset(&self) -> PcScoreProblemPreset {
        self.problem_preset
    }

    pub fn problem_id(&self) -> &str {
        self.problem_id.as_ref()
    }

    pub fn score_profile_id(&self) -> &str {
        self.score_profile_id.as_ref()
    }

    pub const fn materialized_pattern_count(&self) -> usize {
        self.materialized_pattern_count
    }

    pub fn pattern_best_scores(&self) -> &[u64] {
        &self.pattern_best_scores
    }

    /// Complete legal replay winner projection. Attack remains informational
    /// and is never copied into the eligibility or portfolio rows.
    pub fn pattern_winners(&self) -> &[PcScorePatternWinnerV1] {
        self.pattern_winners.as_slice()
    }

    pub fn pattern_winner_owner(&self) -> &Arc<Vec<PcScorePatternWinnerV1>> {
        &self.pattern_winners
    }

    pub fn eligible_candidates(&self) -> &[PcScoreEligibleCandidateV2] {
        &self.eligible_candidates
    }

    pub fn eligible_candidate_map_sha256(&self) -> &str {
        &self.eligible_candidate_map_sha256
    }

    pub fn score_eligibility_sha256(&self) -> &str {
        &self.score_eligibility_sha256
    }

    pub fn selected_score_candidate_ids(&self) -> &[u64] {
        &self.selected_score_candidate_ids
    }

    pub fn selected_solution_keys(&self) -> &[String] {
        &self.selected_solution_keys
    }

    /// App-owned representative of the canonical minimum portfolio for
    /// constrained downstream consumers. Numeric candidate identity is the
    /// only selector; score ties never read informational attack.
    pub const fn canonical_score_candidate_id(&self) -> u64 {
        self.canonical_score_candidate_id
    }

    pub const fn canonical_selection(&self) -> &'static str {
        PC_SCORE_CANONICAL_SELECTION
    }

    pub fn canonical_solution_key(&self) -> NormalizedTilingSolutionKey {
        NormalizedTilingSolutionKey::from_standard_board64_identity(
            self.canonical_solution_identity,
        )
    }

    pub fn portfolio_alternatives(&self) -> &CoveragePortfolioAlternativeSet {
        self.portfolio_alternatives.as_ref()
    }

    pub const fn portfolio_alternative_owner(&self) -> &Arc<CoveragePortfolioAlternativeSet> {
        &self.portfolio_alternatives
    }

    pub const fn completeness(&self) -> PcScorePortfolioCompletenessEvidence {
        self.completeness
    }

    /// Exact heap payload reachable exclusively or jointly from this public
    /// report. Inline values and `Arc` control blocks are excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self.query.checked_pointee_retained_bytes()?;
        bytes = bytes.checked_add(self.problem_id.len() as u128)?;
        bytes = bytes.checked_add(self.score_profile_id.len() as u128)?;
        bytes = bytes.checked_add(
            (self.pattern_winners.capacity() as u128)
                .checked_mul(core::mem::size_of::<PcScorePatternWinnerV1>() as u128)?,
        )?;
        bytes = bytes.checked_add(self.checked_portfolio_specific_retained_capacity_bytes()?)?;
        Some(bytes)
    }

    /// Heap payload newly retained by the score-minimals projection after the
    /// validated score summary already owns the query and winner family.
    pub(crate) fn checked_incremental_retained_capacity_bytes(&self) -> Option<u128> {
        self.checked_portfolio_specific_retained_capacity_bytes()
    }

    fn checked_portfolio_specific_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.pattern_best_scores.capacity() as u128)
            .checked_mul(core::mem::size_of::<u64>() as u128)?;
        bytes = bytes.checked_add(
            (self.eligible_candidates.len() as u128)
                .checked_mul(core::mem::size_of::<PcScoreEligibleCandidateV2>() as u128)?,
        )?;
        for candidate in self.eligible_candidates.iter() {
            bytes = bytes.checked_add(candidate.normalized_solution_key.capacity() as u128)?;
            bytes = bytes.checked_add(
                (candidate.eligible_patterns.capacity() as u128)
                    .checked_mul(core::mem::size_of::<PcScoreEligiblePatternV2>() as u128)?,
            )?;
        }
        bytes = bytes.checked_add(self.eligible_candidate_map_sha256.capacity() as u128)?;
        bytes = bytes.checked_add(self.score_eligibility_sha256.capacity() as u128)?;
        bytes = bytes.checked_add(
            (self.selected_score_candidate_ids.capacity() as u128)
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
        bytes = bytes.checked_add(
            (self.selected_solution_keys.capacity() as u128)
                .checked_mul(core::mem::size_of::<String>() as u128)?,
        )?;
        for key in &self.selected_solution_keys {
            bytes = bytes.checked_add(key.capacity() as u128)?;
        }
        bytes.checked_add(
            self.portfolio_alternatives
                .checked_retained_capacity_bytes()?,
        )
    }
}

/// Exactly-once App evidence tying the ordinary complete score execution to
/// the separately validated score-minimals projection. The ordinary score
/// evidence remains private and is not exposed as a `pc.score` product.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedPcScorePortfolioExecutionEvidence {
    score_execution: ValidatedPcScoreExecutionEvidence,
    report: Arc<PcScorePortfolioV2Result>,
}

impl ValidatedPcScorePortfolioExecutionEvidence {
    pub(crate) fn validate(
        score_execution: ValidatedPcScoreExecutionEvidence,
        derivation: &PcScoreDerivation,
    ) -> Result<Self, PcScorePortfolioValidationError> {
        let report = Arc::new(validate_pc_score_portfolio_v2_result(
            score_execution.report(),
            derivation,
        )?);
        Ok(Self {
            score_execution,
            report,
        })
    }

    pub(crate) fn report(&self) -> &PcScorePortfolioV2Result {
        self.report.as_ref()
    }

    pub(crate) const fn report_owner(&self) -> &Arc<PcScorePortfolioV2Result> {
        &self.report
    }

    pub(crate) fn matches_core_result(&self, result: &CoreExecutionResult) -> bool {
        self.score_execution.matches_core_result(result)
    }

    pub(crate) fn checked_incremental_retained_capacity_bytes(&self) -> Option<u128> {
        self.report.checked_incremental_retained_capacity_bytes()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcScorePortfolioValidationError {
    SummaryContractMismatch,
    QueryIdentityInvalid,
    SourceUniverseIncomplete,
    WeightModelIncomplete,
    ScoreExecutionIncomplete,
    LegalReplayIncomplete,
    CoverageIncomplete,
    WinnerEvidenceMismatch,
    WinnerFamilyInvalid,
    CandidateIdentityMismatch,
    CandidateMapNotCanonical,
    PatternIndexOverflow,
    ExactMinimumCoverFailed,
    PortfolioIdentityInvalid,
    PortfolioAlternativeSetInvalid,
}

impl PcScorePortfolioValidationError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SummaryContractMismatch => "pc-score-portfolio-summary-contract-mismatch",
            Self::QueryIdentityInvalid => "pc-score-portfolio-query-identity-invalid",
            Self::SourceUniverseIncomplete => "pc-score-portfolio-source-universe-incomplete",
            Self::WeightModelIncomplete => "pc-score-portfolio-weight-model-incomplete",
            Self::ScoreExecutionIncomplete => "pc-score-portfolio-score-execution-incomplete",
            Self::LegalReplayIncomplete => "pc-score-portfolio-legal-replay-incomplete",
            Self::CoverageIncomplete => "pc-score-portfolio-coverage-incomplete",
            Self::WinnerEvidenceMismatch => "pc-score-portfolio-winner-evidence-mismatch",
            Self::WinnerFamilyInvalid => "pc-score-portfolio-winner-family-invalid",
            Self::CandidateIdentityMismatch => "pc-score-portfolio-candidate-identity-mismatch",
            Self::CandidateMapNotCanonical => "pc-score-portfolio-candidate-map-not-canonical",
            Self::PatternIndexOverflow => "pc-score-portfolio-pattern-index-overflow",
            Self::ExactMinimumCoverFailed => "pc-score-portfolio-exact-minimum-cover-failed",
            Self::PortfolioIdentityInvalid => "pc-score-portfolio-identity-invalid",
            Self::PortfolioAlternativeSetInvalid => "pc-score-portfolio-alternative-set-invalid",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WinnerProjection {
    solution_identity: StandardBoard64TilingIdentity,
    score: u64,
}

struct CandidateAccumulator {
    score_candidate_id: u64,
    solution_identity: StandardBoard64TilingIdentity,
    eligible_patterns: Vec<PcScoreEligiblePatternV2>,
}

/// Builds the `pc.score-minimals` B-option authority only from an already
/// validated score summary and the exact derivation that produced it.
///
/// The score summary is the query/profile/universe/weight authority; the
/// derivation is the complete legal replay authority. The two projections are
/// compared fieldwise without reading informational attack.
pub(crate) fn validate_pc_score_portfolio_v2_result(
    summary: &PcScoreSummaryV2Result,
    derivation: &PcScoreDerivation,
) -> Result<PcScorePortfolioV2Result, PcScorePortfolioValidationError> {
    validate_summary_completeness(summary, derivation)?;

    let derivation_winners = canonical_winner_projection(derivation.pattern_winners())?;
    let summary_winners = canonical_winner_projection(summary.pattern_winners())?;
    if derivation_winners != summary_winners {
        return Err(PcScorePortfolioValidationError::WinnerEvidenceMismatch);
    }

    let pattern_count = summary.materialized_pattern_count();
    let mut pattern_best_scores = vec![None; pattern_count];
    let mut candidate_identities = BTreeMap::<u64, StandardBoard64TilingIdentity>::new();
    let mut identity_owners = BTreeMap::<StandardBoard64TilingIdentity, u64>::new();
    let mut candidates = BTreeMap::<u64, CandidateAccumulator>::new();

    for ((pattern_id, candidate_id), winner) in derivation_winners {
        let Some(pattern_score) = pattern_best_scores.get_mut(pattern_id) else {
            return Err(PcScorePortfolioValidationError::WinnerFamilyInvalid);
        };
        match pattern_score {
            Some(score) if *score != winner.score => {
                return Err(PcScorePortfolioValidationError::WinnerFamilyInvalid);
            }
            None => *pattern_score = Some(winner.score),
            _ => {}
        }

        if candidate_identities
            .insert(candidate_id, winner.solution_identity)
            .is_some_and(|identity| identity != winner.solution_identity)
        {
            return Err(PcScorePortfolioValidationError::CandidateIdentityMismatch);
        }
        if identity_owners
            .insert(winner.solution_identity, candidate_id)
            .is_some_and(|owner| owner != candidate_id)
        {
            return Err(PcScorePortfolioValidationError::CandidateIdentityMismatch);
        }

        let candidate = candidates
            .entry(candidate_id)
            .or_insert_with(|| CandidateAccumulator {
                score_candidate_id: candidate_id,
                solution_identity: winner.solution_identity,
                eligible_patterns: Vec::new(),
            });
        if candidate.solution_identity != winner.solution_identity {
            return Err(PcScorePortfolioValidationError::CandidateIdentityMismatch);
        }
        candidate.eligible_patterns.push(PcScoreEligiblePatternV2 {
            pattern_id,
            best_score: winner.score,
        });
    }

    let pattern_best_scores = pattern_best_scores
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(PcScorePortfolioValidationError::CoverageIncomplete)?;
    if summary.best_score() != pattern_best_scores.iter().copied().max() {
        return Err(PcScorePortfolioValidationError::WinnerFamilyInvalid);
    }

    let mut candidates = candidates.into_values().collect::<Vec<_>>();
    candidates.sort_unstable_by(|left, right| {
        left.solution_identity
            .cmp(&right.solution_identity)
            .then(left.score_candidate_id.cmp(&right.score_candidate_id))
    });
    if candidates.is_empty()
        || candidates
            .windows(2)
            .any(|pair| pair[0].score_candidate_id >= pair[1].score_candidate_id)
    {
        return Err(PcScorePortfolioValidationError::CandidateMapNotCanonical);
    }

    let mut eligible_candidates = Vec::with_capacity(candidates.len());
    let mut candidate_keys = Vec::with_capacity(candidates.len());
    let mut coverage_rows = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.into_iter().enumerate() {
        if candidate
            .eligible_patterns
            .windows(2)
            .any(|pair| pair[0].pattern_id >= pair[1].pattern_id)
        {
            return Err(PcScorePortfolioValidationError::WinnerFamilyInvalid);
        }
        let pattern_ids = candidate
            .eligible_patterns
            .iter()
            .map(|pattern| {
                u32::try_from(pattern.pattern_id)
                    .map_err(|_| PcScorePortfolioValidationError::PatternIndexOverflow)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let coverage = PatternBitSet::from_pattern_indices(pattern_count, pattern_ids)
            .map_err(|_| PcScorePortfolioValidationError::CoverageIncomplete)?;
        let normalized_solution_key = NormalizedTilingSolutionKey::from_standard_board64_identity(
            candidate.solution_identity,
        )
        .to_string();
        let portfolio_candidate_id = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(PcScorePortfolioValidationError::CandidateMapNotCanonical)?;
        candidate_keys.push(normalized_solution_key.clone());
        coverage_rows.push(coverage);
        eligible_candidates.push(PcScoreEligibleCandidateV2 {
            portfolio_candidate_id,
            score_candidate_id: candidate.score_candidate_id,
            normalized_solution_key,
            eligible_patterns: candidate.eligible_patterns,
        });
    }

    if !candidate_keys.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(PcScorePortfolioValidationError::CandidateMapNotCanonical);
    }

    let required_patterns = PatternBitSet::all(pattern_count);
    let selection = exact_minimum_cover(&required_patterns, &coverage_rows)
        .map_err(|_| PcScorePortfolioValidationError::ExactMinimumCoverFailed)?;
    if !selection.complete()
        || selection.covered_patterns() != &required_patterns
        || selection.row_indices().is_empty()
    {
        return Err(PcScorePortfolioValidationError::CoverageIncomplete);
    }

    let selected_solution_keys = selection
        .row_indices()
        .iter()
        .map(|index| candidate_keys[*index].clone())
        .collect::<Vec<_>>();
    let selected_score_candidate_ids = selection
        .row_indices()
        .iter()
        .map(|index| eligible_candidates[*index].score_candidate_id)
        .collect::<Vec<_>>();
    let canonical_candidate = selection
        .row_indices()
        .iter()
        .copied()
        .min_by_key(|index| eligible_candidates[*index].score_candidate_id)
        .and_then(|index| eligible_candidates.get(index))
        .ok_or(PcScorePortfolioValidationError::CoverageIncomplete)?;
    let canonical_score_candidate_id = canonical_candidate.score_candidate_id;
    let canonical_solution_identity = candidate_identities
        .get(&canonical_score_candidate_id)
        .copied()
        .ok_or(PcScorePortfolioValidationError::CandidateIdentityMismatch)?;

    let eligible_candidate_map_sha256 = eligible_candidate_map_digest(&eligible_candidates);
    let score_eligibility_sha256 = score_eligibility_digest(
        &pattern_best_scores,
        &eligible_candidates,
        &eligible_candidate_map_sha256,
    );
    let portfolio_identity = build_portfolio_identity(
        summary,
        &eligible_candidate_map_sha256,
        &score_eligibility_sha256,
    )?;
    let public_candidate_ids = eligible_candidates
        .iter()
        .map(PcScoreEligibleCandidateV2::score_candidate_id)
        .collect::<Vec<_>>();
    let portfolio_alternatives = Arc::new(
        CoveragePortfolioAlternativeSet::new(
            portfolio_identity,
            candidate_keys,
            required_patterns,
            coverage_rows,
            &selected_solution_keys,
        )
        .and_then(|set| set.with_public_candidate_ids(public_candidate_ids))
        .map_err(|_| PcScorePortfolioValidationError::PortfolioAlternativeSetInvalid)?,
    );

    Ok(PcScorePortfolioV2Result {
        contract_id: PC_SCORE_PORTFOLIO_RESULT_CONTRACT,
        origin: PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals,
        query: summary.query().clone(),
        problem_preset: summary.problem_preset(),
        problem_id: Arc::from(summary.problem_id()),
        score_profile_id: Arc::from(summary.score_profile_id()),
        materialized_pattern_count: pattern_count,
        pattern_best_scores,
        pattern_winners: Arc::clone(derivation.pattern_winner_owner()),
        eligible_candidates: eligible_candidates.into(),
        eligible_candidate_map_sha256,
        score_eligibility_sha256,
        selected_score_candidate_ids,
        selected_solution_keys,
        canonical_score_candidate_id,
        canonical_solution_identity,
        portfolio_alternatives,
        completeness: PcScorePortfolioCompletenessEvidence {
            source_universe_complete: true,
            weight_model_complete: true,
            score_execution_complete: true,
            legal_replay_complete: true,
            winner_family_complete: true,
            coverage_rows_complete: true,
            exact_minimum_proven: true,
            identity_bound: true,
        },
    })
}

fn validate_summary_completeness(
    summary: &PcScoreSummaryV2Result,
    derivation: &PcScoreDerivation,
) -> Result<(), PcScorePortfolioValidationError> {
    if summary.contract_id() != SCORE_SUMMARY_CONTRACT
        || summary.origin() != PcScoreIngressOrigin::CanonicalPcScore
    {
        return Err(PcScorePortfolioValidationError::SummaryContractMismatch);
    }
    if summary.problem_id().is_empty()
        || summary.problem_id().chars().any(char::is_control)
        || summary.score_profile_id().is_empty()
        || summary.score_profile_id().chars().any(char::is_control)
        || summary.piece_source_id() == 0
        || summary.pattern_universe_id() == 0
    {
        return Err(PcScorePortfolioValidationError::QueryIdentityInvalid);
    }

    let completeness = summary.completeness();
    if !completeness.source_universe_complete() {
        return Err(PcScorePortfolioValidationError::SourceUniverseIncomplete);
    }
    if summary.pattern_weight_model_id() == 0
        || !completeness.probability_complete()
        || !completeness.resource_probability_complete()
    {
        return Err(PcScorePortfolioValidationError::WeightModelIncomplete);
    }
    if !completeness.objective_complete()
        || !completeness.matrix_complete()
        || !completeness.summary_complete()
    {
        return Err(PcScorePortfolioValidationError::ScoreExecutionIncomplete);
    }
    if !completeness.execution_source_complete() || !derivation.execution_source_complete() {
        return Err(PcScorePortfolioValidationError::LegalReplayIncomplete);
    }
    let pattern_count = summary.materialized_pattern_count();
    if pattern_count == 0
        || pattern_count > PC_SCORE_MAX_PATTERNS
        || summary.total_pattern_count() != pattern_count as u128
        || !completeness.count_complete()
    {
        return Err(PcScorePortfolioValidationError::SourceUniverseIncomplete);
    }
    if !summary.all_universe_patterns_covered()
        || summary.pattern_optimal_count() != pattern_count
        || summary.failed_pc_pattern_count() != 0
    {
        return Err(PcScorePortfolioValidationError::CoverageIncomplete);
    }
    if summary.pattern_winner_count() == 0
        || summary.pattern_winner_count() != derivation.pattern_winners().len()
        || summary.matrix_cell_count() < summary.pattern_winner_count()
        || summary.best_score().is_none()
    {
        return Err(PcScorePortfolioValidationError::WinnerFamilyInvalid);
    }
    Ok(())
}

fn canonical_winner_projection(
    winners: &[PcScorePatternWinnerV1],
) -> Result<BTreeMap<(usize, u64), WinnerProjection>, PcScorePortfolioValidationError> {
    let mut projected = BTreeMap::new();
    let mut candidate_identities = BTreeMap::new();
    let mut identity_owners = BTreeMap::new();
    for winner in winners {
        if winner.candidate_id() == 0 {
            return Err(PcScorePortfolioValidationError::CandidateIdentityMismatch);
        }
        match candidate_identities.insert(winner.candidate_id(), winner.solution_identity()) {
            Some(identity) if identity != winner.solution_identity() => {
                return Err(PcScorePortfolioValidationError::CandidateIdentityMismatch);
            }
            None => match identity_owners.insert(winner.solution_identity(), winner.candidate_id())
            {
                Some(owner) if owner != winner.candidate_id() => {
                    return Err(PcScorePortfolioValidationError::CandidateIdentityMismatch);
                }
                _ => {}
            },
            _ => {}
        }
        if projected
            .insert(
                (winner.pattern_id(), winner.candidate_id()),
                WinnerProjection {
                    solution_identity: winner.solution_identity(),
                    score: winner.score(),
                },
            )
            .is_some()
        {
            return Err(PcScorePortfolioValidationError::WinnerFamilyInvalid);
        }
    }
    Ok(projected)
}

fn build_portfolio_identity(
    summary: &PcScoreSummaryV2Result,
    candidate_map_sha256: &str,
    eligibility_sha256: &str,
) -> Result<PortfolioAlternativeSetIdentity, PcScorePortfolioValidationError> {
    let origin = summary.origin().as_str();
    let preset = summary.problem_preset().as_str();
    let piece_source_id = summary.piece_source_id().to_string();
    let score_profile_selection = summary.score_profile_selection().as_str();
    let spin_profile_selection = summary.spin_profile_selection().as_str();
    let initial_b2b = summary.initial_b2b().to_string();
    let pattern_universe_id = summary.pattern_universe_id().to_string();
    let pattern_weight_model_id = summary.pattern_weight_model_id().to_string();
    let materialized_pattern_count = summary.materialized_pattern_count().to_string();
    let total_pattern_count = summary.total_pattern_count().to_string();
    let product_build = clearra_host_contract::ProductBuildIdentity::current();

    PortfolioAlternativeSetIdentity::new(
        identity_component(
            "pc-score-minimals-query.v2",
            &[
                PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals.as_str(),
                origin,
                preset,
                summary.problem_id(),
            ],
        ),
        identity_component(
            "pc-score-source.v2",
            &[&piece_source_id, candidate_map_sha256],
        ),
        identity_component(
            "pc-score-profile.v2",
            &[
                summary.score_profile_id(),
                score_profile_selection,
                spin_profile_selection,
                &initial_b2b,
            ],
        ),
        identity_component(
            "pc-score-pattern-universe.v2",
            &[
                &pattern_universe_id,
                &pattern_weight_model_id,
                &materialized_pattern_count,
                &total_pattern_count,
                eligibility_sha256,
            ],
        ),
        identity_component(
            "product-build.v1",
            &[
                product_build.engine_build_id(),
                product_build.source_commit(),
                product_build.contract_schema_version(),
                product_build.supply_semantics_id(),
                product_build.artifact_schema_version(),
            ],
        ),
    )
    .map_err(|_| PcScorePortfolioValidationError::PortfolioIdentityInvalid)
}

fn identity_component(domain: &str, fields: &[&str]) -> String {
    let mut identity = String::from(domain);
    for field in fields {
        write!(&mut identity, "|{}:", field.len()).expect("writing to String cannot fail");
        identity.push_str(field);
    }
    identity
}

fn eligible_candidate_map_digest(candidates: &[PcScoreEligibleCandidateV2]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ELIGIBLE_CANDIDATE_MAP_DIGEST_DOMAIN);
    hasher.update((candidates.len() as u64).to_be_bytes());
    for candidate in candidates {
        hasher.update(candidate.portfolio_candidate_id.to_be_bytes());
        hasher.update(candidate.score_candidate_id.to_be_bytes());
        update_length_delimited(&mut hasher, candidate.normalized_solution_key.as_bytes());
    }
    hex_sha256(hasher.finalize())
}

fn score_eligibility_digest(
    pattern_best_scores: &[u64],
    candidates: &[PcScoreEligibleCandidateV2],
    candidate_map_sha256: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SCORE_ELIGIBILITY_DIGEST_DOMAIN);
    update_length_delimited(&mut hasher, candidate_map_sha256.as_bytes());
    hasher.update((pattern_best_scores.len() as u64).to_be_bytes());
    for (pattern_id, score) in pattern_best_scores.iter().copied().enumerate() {
        hasher.update((pattern_id as u64).to_be_bytes());
        hasher.update(score.to_be_bytes());
    }
    for candidate in candidates {
        hasher.update(candidate.portfolio_candidate_id.to_be_bytes());
        hasher.update((candidate.eligible_patterns.len() as u64).to_be_bytes());
        for pattern in &candidate.eligible_patterns {
            hasher.update((pattern.pattern_id as u64).to_be_bytes());
            hasher.update(pattern.best_score.to_be_bytes());
        }
    }
    hex_sha256(hasher.finalize())
}

fn update_length_delimited(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
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
