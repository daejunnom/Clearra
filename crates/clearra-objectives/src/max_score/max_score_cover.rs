use std::collections::{BTreeMap, BTreeSet};

use clearra_coverage::{
    pattern::{
        pattern_bitset::{PatternBitSet, PatternBitSetError},
        weighted_pattern_set::WeightedPatternSet,
    },
    probability::union_probability::union_probability,
};

use super::{
    materialized_score_matrix::{MaterializedScoreCell, MaterializedScoreMatrix},
    max_score_portfolio_enumerator::{
        MaxScoreCoverPortfolioEnumerator, MaxScoreCoverPortfolioRestart,
    },
    max_score_selection::{
        MaxScoreCoverError, MaxScoreCoverPolicy, MaxScoreCoverResult, PatternScoreContribution,
    },
    optimal_pattern_minimum_cover::{exact_optimal_pattern_cover, OptimalCoverageRow},
    scored_coverage_candidate::ScoredCoverageCandidate,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MaxScoreCover;

impl MaxScoreCover {
    /// Prepares lazy enumeration of every minimum-cardinality portfolio over
    /// the per-pattern score-optimal candidate rows. Attack is retained only
    /// as informational evidence and never influences row eligibility.
    pub fn portfolio_enumerator(
        candidates: &[ScoredCoverageCandidate],
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        _policy: MaxScoreCoverPolicy,
    ) -> Result<MaxScoreCoverPortfolioEnumerator, MaxScoreCoverError> {
        Self::validate_universes(candidates, required_patterns, weights)?;
        let (candidate_ids, coverage) =
            score_only_candidate_coverage_rows(candidates, required_patterns)?;
        MaxScoreCoverPortfolioEnumerator::new(candidate_ids, &coverage, required_patterns)
            .map_err(MaxScoreCoverError::Portfolio)
    }

    pub fn resume_portfolio_enumerator(
        candidates: &[ScoredCoverageCandidate],
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        _policy: MaxScoreCoverPolicy,
        restart: MaxScoreCoverPortfolioRestart,
    ) -> Result<MaxScoreCoverPortfolioEnumerator, MaxScoreCoverError> {
        Self::validate_universes(candidates, required_patterns, weights)?;
        let (candidate_ids, coverage) =
            score_only_candidate_coverage_rows(candidates, required_patterns)?;
        MaxScoreCoverPortfolioEnumerator::resume(
            candidate_ids,
            &coverage,
            required_patterns,
            restart,
        )
        .map_err(MaxScoreCoverError::Portfolio)
    }

    pub fn select(
        candidates: &[ScoredCoverageCandidate],
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        _policy: MaxScoreCoverPolicy,
    ) -> Result<MaxScoreCoverResult, MaxScoreCoverError> {
        Self::validate_universes(candidates, required_patterns, weights)?;
        let pattern_count = required_patterns.pattern_count();
        let mut optimal_rows = BTreeMap::<usize, PatternBitSet>::new();
        for pattern in required_patterns.covered_patterns() {
            let best_value = candidates
                .iter()
                .filter(|candidate| candidate.patterns().contains(pattern))
                .map(ScoredCoverageCandidate::score)
                .max();
            let Some(best_value) = best_value else {
                continue;
            };
            for candidate in candidates.iter().filter(|candidate| {
                candidate.patterns().contains(pattern) && candidate.score() == best_value
            }) {
                optimal_rows
                    .entry(candidate.candidate_id())
                    .or_insert_with(|| PatternBitSet::new(pattern_count))
                    .insert(pattern)?;
            }
        }
        let cover = exact_optimal_pattern_cover(
            pattern_count,
            required_patterns,
            optimal_rows
                .into_iter()
                .map(|(candidate_id, coverage)| OptimalCoverageRow::new(candidate_id, coverage))
                .collect(),
        );
        let selected = cover
            .selected_candidate_ids()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut pattern_contributions = Vec::new();
        for pattern in cover.covered_patterns().covered_patterns() {
            let probability = pattern_weight(weights, pattern)?;
            let Some(best) = candidates
                .iter()
                .filter(|candidate| {
                    selected.contains(&candidate.candidate_id())
                        && candidate.patterns().contains(pattern)
                })
                .map(|candidate| CandidateContribution {
                    candidate_id: candidate.candidate_id(),
                    score: candidate.score(),
                    attack: candidate.attack(),
                })
                .max_by(best_contribution_order)
            else {
                continue;
            };
            pattern_contributions.push(PatternScoreContribution::new(
                pattern,
                best.candidate_id,
                probability,
                best.score,
                best.attack,
            ));
        }
        let covered_probability = union_probability(cover.covered_patterns(), weights)
            .map_err(MaxScoreCoverError::Probability)?;
        Ok(MaxScoreCoverResult::new(
            cover.selected_candidate_ids().to_vec(),
            cover.covered_patterns().clone(),
            covered_probability,
            cover.complete(),
            pattern_contributions,
        ))
    }
}

impl MaxScoreCover {
    pub fn portfolio_enumerator_matrix(
        matrix: &MaterializedScoreMatrix,
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        _policy: MaxScoreCoverPolicy,
    ) -> Result<MaxScoreCoverPortfolioEnumerator, MaxScoreCoverError> {
        validate_score_matrix(matrix, required_patterns, weights)?;
        let (candidate_ids, coverage) = score_only_matrix_coverage_rows(matrix, required_patterns)?;
        MaxScoreCoverPortfolioEnumerator::new(candidate_ids, &coverage, required_patterns)
            .map_err(MaxScoreCoverError::Portfolio)
    }

    pub fn resume_portfolio_enumerator_matrix(
        matrix: &MaterializedScoreMatrix,
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        _policy: MaxScoreCoverPolicy,
        restart: MaxScoreCoverPortfolioRestart,
    ) -> Result<MaxScoreCoverPortfolioEnumerator, MaxScoreCoverError> {
        validate_score_matrix(matrix, required_patterns, weights)?;
        let (candidate_ids, coverage) = score_only_matrix_coverage_rows(matrix, required_patterns)?;
        MaxScoreCoverPortfolioEnumerator::resume(
            candidate_ids,
            &coverage,
            required_patterns,
            restart,
        )
        .map_err(MaxScoreCoverError::Portfolio)
    }

    pub fn select_matrix(
        matrix: &MaterializedScoreMatrix,
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
        _policy: MaxScoreCoverPolicy,
    ) -> Result<MaxScoreCoverResult, MaxScoreCoverError> {
        validate_score_matrix(matrix, required_patterns, weights)?;

        let mut best_candidate_pattern = BTreeMap::<(usize, usize), &MaterializedScoreCell>::new();
        for cell in matrix
            .cells()
            .iter()
            .filter(|cell| required_patterns.contains(cell.pattern_id()))
        {
            best_candidate_pattern
                .entry((cell.candidate_id(), cell.pattern_id().index()))
                .and_modify(|current| {
                    if materialized_cell_order(cell, current).is_gt() {
                        *current = cell;
                    }
                })
                .or_insert(cell);
        }
        let mut best_value_by_pattern = BTreeMap::<usize, u64>::new();
        for cell in best_candidate_pattern.values() {
            let value = cell.score();
            best_value_by_pattern
                .entry(cell.pattern_id().index())
                .and_modify(|current| {
                    if value > *current {
                        *current = value;
                    }
                })
                .or_insert(value);
        }
        let mut optimal_rows = BTreeMap::<usize, PatternBitSet>::new();
        for cell in best_candidate_pattern.values() {
            let best_value = best_value_by_pattern[&cell.pattern_id().index()];
            if cell.score() == best_value {
                optimal_rows
                    .entry(cell.candidate_id())
                    .or_insert_with(|| PatternBitSet::new(matrix.pattern_count()))
                    .insert(cell.pattern_id())?;
            }
        }
        let cover = exact_optimal_pattern_cover(
            matrix.pattern_count(),
            required_patterns,
            optimal_rows
                .into_iter()
                .map(|(candidate_id, coverage)| OptimalCoverageRow::new(candidate_id, coverage))
                .collect(),
        );
        let selected = cover
            .selected_candidate_ids()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut pattern_contributions = Vec::new();
        for pattern in cover.covered_patterns().covered_patterns() {
            let probability = pattern_weight(weights, pattern)?;
            let Some(best) = best_candidate_pattern
                .values()
                .copied()
                .filter(|cell| {
                    cell.pattern_id() == pattern
                        && selected.contains(&cell.candidate_id())
                        && cell.score() == best_value_by_pattern[&pattern.index()]
                })
                .max_by(|left, right| materialized_cell_order(left, right))
            else {
                continue;
            };
            pattern_contributions.push(PatternScoreContribution::from_materialized_cell(
                pattern,
                best.candidate_id(),
                probability,
                best.score(),
                best.attack(),
                best.trace_identity(),
                best.accuracy_level(),
            ));
        }
        let covered_probability = union_probability(cover.covered_patterns(), weights)
            .map_err(MaxScoreCoverError::Probability)?;
        Ok(MaxScoreCoverResult::new(
            cover.selected_candidate_ids().to_vec(),
            cover.covered_patterns().clone(),
            covered_probability,
            cover.complete(),
            pattern_contributions,
        ))
    }
}

fn score_only_candidate_coverage_rows(
    candidates: &[ScoredCoverageCandidate],
    required_patterns: &PatternBitSet,
) -> Result<(Vec<usize>, Vec<PatternBitSet>), MaxScoreCoverError> {
    let pattern_count = required_patterns.pattern_count();
    let mut optimal_rows = BTreeMap::<usize, PatternBitSet>::new();
    for pattern in required_patterns.covered_patterns() {
        let Some(best_score) = candidates
            .iter()
            .filter(|candidate| candidate.patterns().contains(pattern))
            .map(ScoredCoverageCandidate::score)
            .max()
        else {
            continue;
        };
        for candidate in candidates.iter().filter(|candidate| {
            candidate.patterns().contains(pattern) && candidate.score() == best_score
        }) {
            optimal_rows
                .entry(candidate.candidate_id())
                .or_insert_with(|| PatternBitSet::new(pattern_count))
                .insert(pattern)?;
        }
    }
    Ok(optimal_rows.into_iter().unzip())
}

fn score_only_matrix_coverage_rows(
    matrix: &MaterializedScoreMatrix,
    required_patterns: &PatternBitSet,
) -> Result<(Vec<usize>, Vec<PatternBitSet>), MaxScoreCoverError> {
    let mut best_candidate_pattern = BTreeMap::<(usize, usize), &MaterializedScoreCell>::new();
    for cell in matrix
        .cells()
        .iter()
        .filter(|cell| required_patterns.contains(cell.pattern_id()))
    {
        best_candidate_pattern
            .entry((cell.candidate_id(), cell.pattern_id().index()))
            .and_modify(|current| {
                if materialized_cell_order(cell, current).is_gt() {
                    *current = cell;
                }
            })
            .or_insert(cell);
    }
    let mut best_score_by_pattern = BTreeMap::<usize, u64>::new();
    for cell in best_candidate_pattern.values() {
        best_score_by_pattern
            .entry(cell.pattern_id().index())
            .and_modify(|current| *current = (*current).max(cell.score()))
            .or_insert(cell.score());
    }
    let mut optimal_rows = BTreeMap::<usize, PatternBitSet>::new();
    for cell in best_candidate_pattern.values() {
        if cell.score() == best_score_by_pattern[&cell.pattern_id().index()] {
            optimal_rows
                .entry(cell.candidate_id())
                .or_insert_with(|| PatternBitSet::new(matrix.pattern_count()))
                .insert(cell.pattern_id())?;
        }
    }
    Ok(optimal_rows.into_iter().unzip())
}

fn validate_score_matrix(
    matrix: &MaterializedScoreMatrix,
    required_patterns: &PatternBitSet,
    weights: &WeightedPatternSet,
) -> Result<(), MaxScoreCoverError> {
    if !matrix.complete() {
        return Err(MaxScoreCoverError::ScoreMatrixIncomplete);
    }
    if matrix.pattern_count() != required_patterns.pattern_count() {
        return Err(MaxScoreCoverError::ScoreMatrixPatternUniverseMismatch {
            expected: required_patterns.pattern_count(),
            actual: matrix.pattern_count(),
        });
    }
    if weights.len() != required_patterns.pattern_count() {
        return Err(MaxScoreCoverError::RequiredPatternUniverseMismatch {
            expected: required_patterns.pattern_count(),
            actual: weights.len(),
        });
    }
    if let Some(cell) = matrix
        .cells()
        .iter()
        .find(|cell| cell.pattern_id().index() >= matrix.pattern_count())
    {
        return Err(MaxScoreCoverError::ScoreCellPatternOutOfRange {
            pattern_index: cell.pattern_id().index(),
            pattern_count: matrix.pattern_count(),
        });
    }
    Ok(())
}

fn pattern_weight(
    weights: &WeightedPatternSet,
    pattern: clearra_coverage::pattern::pattern_id::PatternId,
) -> Result<clearra_core_domain::probability::probability_value::ProbabilityValue, MaxScoreCoverError>
{
    weights
        .weight(pattern)
        .ok_or(MaxScoreCoverError::Probability(
        clearra_coverage::probability::union_probability::UnionProbabilityError::MissingWeight {
            pattern_index: pattern.index(),
        },
    ))
}

fn materialized_cell_order(
    left: &MaterializedScoreCell,
    right: &MaterializedScoreCell,
) -> std::cmp::Ordering {
    left.score()
        .cmp(&right.score())
        .then_with(|| right.candidate_id().cmp(&left.candidate_id()))
        .then_with(|| right.trace_identity().cmp(left.trace_identity()))
}
impl MaxScoreCover {
    fn validate_universes(
        candidates: &[ScoredCoverageCandidate],
        required_patterns: &PatternBitSet,
        weights: &WeightedPatternSet,
    ) -> Result<(), MaxScoreCoverError> {
        if required_patterns.pattern_count() != weights.len() {
            return Err(MaxScoreCoverError::RequiredPatternUniverseMismatch {
                expected: weights.len(),
                actual: required_patterns.pattern_count(),
            });
        }

        for candidate in candidates {
            if candidate.patterns().pattern_count() != required_patterns.pattern_count() {
                return Err(MaxScoreCoverError::CandidatePatternUniverseMismatch {
                    expected: required_patterns.pattern_count(),
                    actual: candidate.patterns().pattern_count(),
                });
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CandidateContribution {
    candidate_id: usize,
    score: u64,
    attack: u32,
}

fn best_contribution_order(
    left: &CandidateContribution,
    right: &CandidateContribution,
) -> std::cmp::Ordering {
    left.score
        .cmp(&right.score)
        .then_with(|| right.candidate_id.cmp(&left.candidate_id))
}

impl From<PatternBitSetError> for MaxScoreCoverError {
    fn from(error: PatternBitSetError) -> Self {
        match error {
            PatternBitSetError::PatternOutOfRange {
                index,
                pattern_count,
            } => Self::RequiredPatternUniverseMismatch {
                expected: pattern_count,
                actual: index + 1,
            },
            PatternBitSetError::PatternUniverseMismatch { left, right } => {
                Self::RequiredPatternUniverseMismatch {
                    expected: left,
                    actual: right,
                }
            }
            PatternBitSetError::WordCapacityExceeded {
                word_count,
                word_limit,
            } => Self::PatternBitSetWordCapacityExceeded {
                word_count,
                word_limit,
            },
            PatternBitSetError::WordCountMismatch { expected, actual } => {
                Self::PatternBitSetWordCountMismatch { expected, actual }
            }
        }
    }
}

#[cfg(test)]
#[path = "max_score_cover_tests.rs"]
mod tests;
