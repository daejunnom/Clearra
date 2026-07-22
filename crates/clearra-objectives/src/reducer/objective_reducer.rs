use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_coverage::{
    cover::cover_selection::CoverSelection,
    matrix::coverage_matrix::TypedCoverageMatrix,
    pattern::{pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet},
    reducer::coverage_probability_reducer::{
        CoverageProbabilityReducer, CoverageProbabilityReducerError, CoverageProbabilitySummary,
    },
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        coverage_universe_guard::CoverageUniverseGuard, pattern_universe_id::PatternUniverseId,
        pattern_weight_model_id::PatternWeightModelId,
    },
};

use crate::{
    collect::{all_collector::AllCollector, unique_collector::UniqueCollector},
    cover::minimum_cover_objective::MinimumCoverObjective,
    max_score::{
        max_score_cover::MaxScoreCover,
        max_score_selection::{MaxScoreCoverError, MaxScoreCoverPolicy, MaxScoreCoverResult},
        scored_coverage_candidate::ScoredCoverageCandidate,
    },
    reducer::dominance_reducer::{DominanceCandidate, DominanceReducer},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectiveCandidate {
    candidate_id: usize,
    stable_canonical_key: String,
    patterns: PatternBitSet,
    score: Option<u64>,
    attack: Option<u32>,
}

impl ObjectiveCandidate {
    pub fn new(
        candidate_id: usize,
        stable_canonical_key: impl Into<String>,
        patterns: PatternBitSet,
        score: u64,
        attack: u32,
    ) -> Self {
        Self {
            candidate_id,
            stable_canonical_key: stable_canonical_key.into(),
            patterns,
            score: Some(score),
            attack: Some(attack),
        }
    }
}
impl ObjectiveCandidate {
    pub fn unscored(
        candidate_id: usize,
        stable_canonical_key: impl Into<String>,
        patterns: PatternBitSet,
    ) -> Self {
        Self {
            candidate_id,
            stable_canonical_key: stable_canonical_key.into(),
            patterns,
            score: None,
            attack: None,
        }
    }
}
impl ObjectiveCandidate {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl ObjectiveCandidate {
    pub fn stable_canonical_key(&self) -> &str {
        &self.stable_canonical_key
    }
}
impl ObjectiveCandidate {
    pub fn patterns(&self) -> &PatternBitSet {
        &self.patterns
    }
}
impl ObjectiveCandidate {
    pub fn score(&self) -> Option<u64> {
        self.score
    }

    pub fn attack(&self) -> Option<u32> {
        self.attack
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectiveCountInput {
    total_solution_count: usize,
    retained_trace_count: usize,
    count_complete: bool,
    trace_retention_truncated: bool,
}

impl ObjectiveCountInput {
    pub fn new(
        total_solution_count: usize,
        retained_trace_count: usize,
        count_complete: bool,
        trace_retention_truncated: bool,
    ) -> Self {
        Self {
            total_solution_count,
            retained_trace_count,
            count_complete,
            trace_retention_truncated,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectiveCoverageIdentity {
    row_kind: CoverageRowKind,
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
}

impl ObjectiveCoverageIdentity {
    pub fn new(
        row_kind: CoverageRowKind,
        piece_source_id: u64,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
    ) -> Self {
        Self {
            row_kind,
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveReductionResult {
    all_candidate_ids: Vec<usize>,
    unique_candidate_ids: Vec<usize>,
    non_dominated_candidate_ids: Vec<usize>,
    minimum_cover: CoverSelection,
    max_score: Option<MaxScoreCoverResult>,
    coverage: CoverageProbabilitySummary,
    total_solution_count: usize,
    unique_result_count: usize,
    retained_trace_count: usize,
    count_complete: bool,
    trace_retention_truncated: bool,
}

impl ObjectiveReductionResult {
    pub fn all_candidate_ids(&self) -> &[usize] {
        &self.all_candidate_ids
    }
}
impl ObjectiveReductionResult {
    pub fn unique_candidate_ids(&self) -> &[usize] {
        &self.unique_candidate_ids
    }
}
impl ObjectiveReductionResult {
    pub fn non_dominated_candidate_ids(&self) -> &[usize] {
        &self.non_dominated_candidate_ids
    }
}
impl ObjectiveReductionResult {
    pub fn minimum_cover(&self) -> &CoverSelection {
        &self.minimum_cover
    }
}
impl ObjectiveReductionResult {
    pub fn max_score(&self) -> Option<&MaxScoreCoverResult> {
        self.max_score.as_ref()
    }
}
impl ObjectiveReductionResult {
    pub fn coverage(&self) -> &CoverageProbabilitySummary {
        &self.coverage
    }
}
impl ObjectiveReductionResult {
    pub fn total_solution_count(&self) -> usize {
        self.total_solution_count
    }
}
impl ObjectiveReductionResult {
    pub fn unique_result_count(&self) -> usize {
        self.unique_result_count
    }
}
impl ObjectiveReductionResult {
    pub fn retained_trace_count(&self) -> usize {
        self.retained_trace_count
    }
}
impl ObjectiveReductionResult {
    pub fn count_complete(&self) -> bool {
        self.count_complete
    }
}
impl ObjectiveReductionResult {
    pub fn trace_retention_truncated(&self) -> bool {
        self.trace_retention_truncated
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectiveReducerError {
    CoverageMatrix(clearra_coverage::matrix::coverage_matrix::CoverageMatrixError),
    CoverageProbability(CoverageProbabilityReducerError),
    MaxScore(MaxScoreCoverError),
    CandidateIdOverflow(u64),
    CandidateOrderViolation { previous: u64, current: u64 },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ObjectiveReducer;

impl ObjectiveReducer {
    pub fn reduce(
        candidates: &[ObjectiveCandidate],
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        counts: ObjectiveCountInput,
        coverage_identity: ObjectiveCoverageIdentity,
        max_score_policy: MaxScoreCoverPolicy,
    ) -> Result<ObjectiveReductionResult, ObjectiveReducerError> {
        Self::reduce_internal(
            candidates,
            required_patterns,
            weights,
            counts,
            coverage_identity,
            max_score_policy,
            None,
        )
    }

    pub fn reduce_requested(
        candidates: &[ObjectiveCandidate],
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        counts: ObjectiveCountInput,
        coverage_identity: ObjectiveCoverageIdentity,
        max_score_policy: MaxScoreCoverPolicy,
        objective_kind: ObjectiveKind,
    ) -> Result<ObjectiveReductionResult, ObjectiveReducerError> {
        Self::reduce_internal(
            candidates,
            required_patterns,
            weights,
            counts,
            coverage_identity,
            max_score_policy,
            Some(objective_kind),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn reduce_internal(
        candidates: &[ObjectiveCandidate],
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        counts: ObjectiveCountInput,
        coverage_identity: ObjectiveCoverageIdentity,
        max_score_policy: MaxScoreCoverPolicy,
        requested_kind: Option<ObjectiveKind>,
    ) -> Result<ObjectiveReductionResult, ObjectiveReducerError> {
        let run_all_collector = requested_kind.is_none_or(|kind| kind == ObjectiveKind::All);
        let run_unique_collector = requested_kind.is_none_or(|kind| kind == ObjectiveKind::Unique);
        let run_minimum_cover =
            requested_kind.is_none_or(|kind| kind == ObjectiveKind::MinimumCover);
        // Product requests always pass an explicit coverage objective. Generic internal
        // reductions may still compute score dominance for setup-domain consumers.
        let run_max_score = requested_kind.is_none();
        let row_kind = coverage_identity.row_kind.clone();
        let piece_source_id = coverage_identity.piece_source_id;
        let pattern_universe_id = coverage_identity.pattern_universe_id;
        let pattern_weight_model_id = coverage_identity.pattern_weight_model_id;
        let coverage = CoverageProbabilityReducer::family_probability_from_pattern_sets(
            required_patterns.pattern_count(),
            candidates.iter().map(ObjectiveCandidate::patterns),
            weights,
        )
        .map_err(ObjectiveReducerError::CoverageProbability)?;

        let all_candidate_ids = if run_all_collector {
            AllCollector::collect(
                &candidates
                    .iter()
                    .map(ObjectiveCandidate::candidate_id)
                    .collect::<Vec<_>>(),
            )
        } else {
            Vec::new()
        };
        let unique_candidates = if run_unique_collector {
            UniqueCollector::collect_by_key(candidates, |candidate| {
                candidate.stable_canonical_key.clone()
            })
        } else {
            Vec::new()
        };
        let unique_candidate_ids = unique_candidates
            .iter()
            .map(ObjectiveCandidate::candidate_id)
            .collect::<Vec<_>>();

        let minimum_cover = if run_minimum_cover {
            let matrix = TypedCoverageMatrix::from_rows(
                row_kind.clone(),
                pattern_universe_id,
                pattern_weight_model_id,
                required_patterns.pattern_count(),
                candidates
                    .iter()
                    .map(|candidate| {
                        CoverageRow::new_with_piece_source(
                            candidate.candidate_id as u64,
                            row_kind.clone(),
                            piece_source_id,
                            pattern_universe_id,
                            pattern_weight_model_id,
                            candidate.patterns.clone(),
                        )
                    })
                    .collect(),
            )
            .map_err(ObjectiveReducerError::CoverageMatrix)?;
            MinimumCoverObjective::select(&matrix, required_patterns)
        } else {
            CoverSelection::not_requested(required_patterns.pattern_count())
        };
        let scores_materialized = candidates
            .iter()
            .all(|candidate| candidate.score.is_some() && candidate.attack.is_some());
        let max_score = if run_max_score && scores_materialized {
            let scored_candidates = candidates
                .iter()
                .map(|candidate| {
                    ScoredCoverageCandidate::new(
                        candidate.candidate_id,
                        candidate.patterns.clone(),
                        candidate.score.expect("materialized score was checked"),
                        candidate.attack.expect("materialized attack was checked"),
                    )
                })
                .collect::<Vec<_>>();
            Some(
                MaxScoreCover::select(
                    &scored_candidates,
                    required_patterns,
                    weights,
                    max_score_policy,
                )
                .map_err(ObjectiveReducerError::MaxScore)?,
            )
        } else {
            None
        };
        let non_dominated_candidate_ids = if run_max_score && scores_materialized {
            DominanceReducer::reduce(
                &candidates
                    .iter()
                    .map(|candidate| {
                        DominanceCandidate::new(
                            candidate.candidate_id,
                            candidate.patterns.clone(),
                            candidate.score.expect("materialized score was checked"),
                            candidate.attack.expect("materialized attack was checked"),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .iter()
            .map(DominanceCandidate::candidate_id)
            .collect()
        } else if run_all_collector {
            candidates
                .iter()
                .map(ObjectiveCandidate::candidate_id)
                .collect()
        } else {
            Vec::new()
        };

        Ok(ObjectiveReductionResult {
            all_candidate_ids,
            unique_candidate_ids,
            non_dominated_candidate_ids,
            minimum_cover,
            max_score,
            coverage,
            total_solution_count: counts.total_solution_count,
            unique_result_count: if run_unique_collector {
                unique_candidates.len()
            } else {
                candidates.len()
            },
            retained_trace_count: counts.retained_trace_count,
            count_complete: counts.count_complete,
            trace_retention_truncated: counts.trace_retention_truncated,
        })
    }

    pub fn reduce_canonical_unscored_rows_requested(
        rows: &[CoverageRow],
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        counts: ObjectiveCountInput,
        coverage_identity: ObjectiveCoverageIdentity,
        objective_kind: ObjectiveKind,
    ) -> Result<ObjectiveReductionResult, ObjectiveReducerError> {
        validate_canonical_rows(rows, required_patterns.pattern_count(), &coverage_identity)?;
        let coverage = CoverageProbabilityReducer::family_probability_from_pattern_sets(
            required_patterns.pattern_count(),
            rows.iter().map(CoverageRow::coverage_bits),
            weights,
        )
        .map_err(ObjectiveReducerError::CoverageProbability)?;
        let candidate_ids = rows
            .iter()
            .map(|row| {
                usize::try_from(row.candidate_id())
                    .map_err(|_| ObjectiveReducerError::CandidateIdOverflow(row.candidate_id()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let all_candidate_ids = if objective_kind == ObjectiveKind::All {
            candidate_ids.clone()
        } else {
            Vec::new()
        };
        let unique_candidate_ids = if objective_kind == ObjectiveKind::Unique {
            candidate_ids.clone()
        } else {
            Vec::new()
        };
        let minimum_cover = if objective_kind == ObjectiveKind::MinimumCover {
            let matrix = TypedCoverageMatrix::from_rows(
                coverage_identity.row_kind.clone(),
                coverage_identity.pattern_universe_id,
                coverage_identity.pattern_weight_model_id,
                required_patterns.pattern_count(),
                rows.to_vec(),
            )
            .map_err(ObjectiveReducerError::CoverageMatrix)?;
            MinimumCoverObjective::select(&matrix, required_patterns)
        } else {
            CoverSelection::not_requested(required_patterns.pattern_count())
        };
        let non_dominated_candidate_ids = if objective_kind == ObjectiveKind::All {
            candidate_ids
        } else {
            Vec::new()
        };

        Ok(ObjectiveReductionResult {
            all_candidate_ids,
            unique_candidate_ids,
            non_dominated_candidate_ids,
            minimum_cover,
            max_score: None,
            coverage,
            total_solution_count: counts.total_solution_count,
            unique_result_count: rows.len(),
            retained_trace_count: counts.retained_trace_count,
            count_complete: counts.count_complete,
            trace_retention_truncated: counts.trace_retention_truncated,
        })
    }
}

fn validate_canonical_rows(
    rows: &[CoverageRow],
    pattern_count: usize,
    identity: &ObjectiveCoverageIdentity,
) -> Result<(), ObjectiveReducerError> {
    let guard = CoverageUniverseGuard::new(
        identity.pattern_universe_id,
        identity.pattern_weight_model_id,
        pattern_count,
    );
    let mut previous_candidate_id = None;
    for row in rows {
        if row.row_kind() != &identity.row_kind {
            return Err(ObjectiveReducerError::CoverageMatrix(
                clearra_coverage::matrix::coverage_matrix::CoverageMatrixError::RowKindMismatch {
                    expected: identity.row_kind.clone(),
                    actual: row.row_kind().clone(),
                },
            ));
        }
        if row.piece_source_id() != identity.piece_source_id {
            return Err(ObjectiveReducerError::CoverageMatrix(
                clearra_coverage::matrix::coverage_matrix::CoverageMatrixError::PieceSourceIdMismatch {
                    expected: identity.piece_source_id,
                    actual: row.piece_source_id(),
                },
            ));
        }
        guard
            .check_row(row)
            .map_err(ObjectiveReducerError::CoverageMatrix)?;
        if let Some(previous) =
            previous_candidate_id.filter(|previous| *previous >= row.candidate_id())
        {
            return Err(ObjectiveReducerError::CandidateOrderViolation {
                previous,
                current: row.candidate_id(),
            });
        }
        previous_candidate_id = Some(row.candidate_id());
    }
    Ok(())
}

#[cfg(test)]
#[path = "objective_reducer_tests.rs"]
mod tests;
