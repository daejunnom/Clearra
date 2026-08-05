use std::collections::{BTreeMap, BTreeSet};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_objectives::max_score::{MaterializedScoreCell, MaterializedScoreMatrix};
use clearra_replay::ReplayEvent;
use clearra_scoring::{
    model::{ScoreEvaluationPolicy, ScoreModelEvaluator},
    profile::{ScoreProfile, ScoringAccuracyLevel, TraceRequirement},
};

use super::candidate_execution_aggregate::CandidateExecutionAggregate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreCell {
    candidate_id: u64,
    pattern_id: usize,
    trace_identity: String,
    score: u64,
    attack: u32,
    accuracy_level: String,
}

impl ScoreCell {
    pub fn new(
        candidate_id: u64,
        pattern_id: usize,
        trace_identity: impl Into<String>,
        score: u64,
        attack: u32,
        accuracy_level: impl Into<String>,
    ) -> Self {
        Self {
            candidate_id,
            pattern_id,
            trace_identity: trace_identity.into(),
            score,
            attack,
            accuracy_level: accuracy_level.into(),
        }
    }

    pub fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub fn score(&self) -> u64 {
        self.score
    }

    pub fn attack(&self) -> u32 {
        self.attack
    }

    pub fn accuracy_level(&self) -> &str {
        &self.accuracy_level
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreMatrix {
    cells: Vec<ScoreCell>,
    pattern_count: usize,
    profile_id: String,
    accuracy_level: String,
    materialized: bool,
    complete: bool,
    incomplete_reason: Option<&'static str>,
}

impl ScoreMatrix {
    pub fn from_materialized_cells(
        mut cells: Vec<ScoreCell>,
        profile: &ScoreProfile,
        pattern_count: usize,
        source_complete: bool,
    ) -> Self {
        let profile_supported = !matches!(
            profile.accuracy_level(),
            ScoringAccuracyLevel::Unsupported | ScoringAccuracyLevel::InsufficientTrace
        );
        let shape_valid = pattern_count > 0
            && cells.iter().all(|cell| {
                cell.candidate_id != 0
                    && cell.pattern_id < pattern_count
                    && cell.accuracy_level == profile.accuracy_level().as_str()
            });
        cells.sort_by(|left, right| {
            (
                left.pattern_id,
                left.candidate_id,
                left.trace_identity.as_str(),
            )
                .cmp(&(
                    right.pattern_id,
                    right.candidate_id,
                    right.trace_identity.as_str(),
                ))
        });
        let conflicting_cells = cells.windows(2).any(|pair| {
            pair[0].candidate_id == pair[1].candidate_id
                && pair[0].pattern_id == pair[1].pattern_id
                && pair[0].trace_identity == pair[1].trace_identity
                && (pair[0].score != pair[1].score || pair[0].attack != pair[1].attack)
        });
        cells.dedup_by(|left, right| {
            left.candidate_id == right.candidate_id
                && left.pattern_id == right.pattern_id
                && left.trace_identity == right.trace_identity
        });
        let complete = source_complete && profile_supported && shape_valid && !conflicting_cells;
        let incomplete_reason = if complete {
            None
        } else if !source_complete {
            Some("score_execution_source_incomplete")
        } else if pattern_count == 0 {
            Some("pattern_universe_not_materialized")
        } else if !profile_supported {
            Some("score_profile_not_supported")
        } else if conflicting_cells {
            Some("score_materialized_cell_conflict")
        } else {
            Some("score_materialized_cell_identity_invalid")
        };
        let materialized =
            score_matrix_materialized(pattern_count, source_complete, !cells.is_empty());
        Self {
            cells,
            pattern_count,
            profile_id: profile.id().to_owned(),
            accuracy_level: profile.accuracy_level().as_str().to_owned(),
            materialized,
            complete,
            incomplete_reason,
        }
    }

    pub fn materialize(
        aggregates: &[CandidateExecutionAggregate],
        profile: &ScoreProfile,
        pattern_count: usize,
        source_complete: bool,
    ) -> Self {
        Self::materialize_with_control(
            aggregates,
            profile,
            pattern_count,
            source_complete,
            &ExecutionControl::default(),
        )
        .expect("default execution control cannot be cancelled")
    }

    pub fn materialize_with_control(
        aggregates: &[CandidateExecutionAggregate],
        profile: &ScoreProfile,
        pattern_count: usize,
        source_complete: bool,
        control: &ExecutionControl,
    ) -> Result<Self, ScoreMatrixCancelled> {
        Self::materialize_with_policy_and_control(
            aggregates,
            profile,
            ScoreEvaluationPolicy::profile_defaults(),
            pattern_count,
            source_complete,
            control,
        )
    }

    pub fn materialize_with_policy_and_control(
        aggregates: &[CandidateExecutionAggregate],
        profile: &ScoreProfile,
        evaluation_policy: ScoreEvaluationPolicy,
        pattern_count: usize,
        source_complete: bool,
        control: &ExecutionControl,
    ) -> Result<Self, ScoreMatrixCancelled> {
        let mut cells = Vec::new();
        let mut identities = BTreeSet::new();
        let mut incomplete_reason = None;
        let mut complete = source_complete
            && pattern_count > 0
            && !matches!(
                profile.accuracy_level(),
                ScoringAccuracyLevel::Unsupported | ScoringAccuracyLevel::InsufficientTrace
            );
        if !source_complete {
            incomplete_reason = Some(if aggregates.is_empty() {
                "score_matrix_not_materialized"
            } else {
                "score_execution_source_incomplete"
            });
        } else if pattern_count == 0 {
            incomplete_reason = Some("pattern_universe_not_materialized");
        } else if !complete {
            incomplete_reason = Some("score_profile_not_supported");
        }

        for (aggregate_index, aggregate) in aggregates.iter().enumerate() {
            if control.is_cancelled() {
                return Err(ScoreMatrixCancelled);
            }
            control.report_progress(
                "score-matrix",
                aggregate_index as u64,
                Some(aggregates.len() as u64),
            );
            for execution in aggregate.executions() {
                if control.is_cancelled() {
                    return Err(ScoreMatrixCancelled);
                }
                if execution.pattern_id() >= pattern_count {
                    complete = false;
                    incomplete_reason.get_or_insert("score_pattern_identity_out_of_range");
                    continue;
                }
                if !trace_satisfies_requirement(
                    execution.replay_trace().events(),
                    profile.trace_requirement(),
                ) {
                    complete = false;
                    incomplete_reason.get_or_insert("score_trace_requirement_not_met");
                    continue;
                }
                if !trace_has_complete_spin_evidence(execution.replay_trace(), profile) {
                    complete = false;
                    incomplete_reason.get_or_insert("score_spin_evidence_incomplete");
                    continue;
                }
                let identity = (
                    aggregate.candidate_id(),
                    execution.pattern_id(),
                    execution.trace_identity().to_owned(),
                );
                if !identities.insert(identity) {
                    continue;
                }
                let evaluation = ScoreModelEvaluator::evaluate_replay_trace_with_policy(
                    profile,
                    execution.replay_trace(),
                    evaluation_policy,
                );
                cells.push(ScoreCell {
                    candidate_id: aggregate.candidate_id(),
                    pattern_id: execution.pattern_id(),
                    trace_identity: execution.trace_identity().to_owned(),
                    score: evaluation.final_state().score(),
                    attack: evaluation.final_state().attack(),
                    accuracy_level: profile.accuracy_level().as_str().to_owned(),
                });
            }
        }
        cells.sort_by(|left, right| {
            (
                left.pattern_id,
                left.candidate_id,
                left.trace_identity.as_str(),
            )
                .cmp(&(
                    right.pattern_id,
                    right.candidate_id,
                    right.trace_identity.as_str(),
                ))
        });

        control.report_progress(
            "score-matrix",
            aggregates.len() as u64,
            Some(aggregates.len() as u64),
        );
        let materialized =
            score_matrix_materialized(pattern_count, source_complete, !cells.is_empty());
        Ok(Self {
            cells,
            pattern_count,
            profile_id: profile.id().to_owned(),
            accuracy_level: profile.accuracy_level().as_str().to_owned(),
            materialized,
            complete,
            incomplete_reason: (!complete)
                .then_some(incomplete_reason.unwrap_or("score_matrix_incomplete")),
        })
    }

    pub fn unavailable(profile: &ScoreProfile, pattern_count: usize) -> Self {
        Self {
            cells: Vec::new(),
            pattern_count,
            profile_id: profile.id().to_owned(),
            accuracy_level: profile.accuracy_level().as_str().to_owned(),
            materialized: false,
            complete: false,
            incomplete_reason: Some("score_matrix_not_materialized"),
        }
    }

    pub fn cells(&self) -> &[ScoreCell] {
        &self.cells
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn accuracy_level(&self) -> &str {
        &self.accuracy_level
    }

    pub fn complete(&self) -> bool {
        self.complete
    }

    pub fn materialized(&self) -> bool {
        self.materialized
    }

    pub fn exact(&self) -> bool {
        self.complete && self.accuracy_level == "profile-specific-exact"
    }

    pub fn incomplete_reason(&self) -> Option<&'static str> {
        self.incomplete_reason
    }

    pub fn to_objective_matrix(&self) -> Option<MaterializedScoreMatrix> {
        let cells = self
            .cells
            .iter()
            .map(|cell| {
                Some(MaterializedScoreCell::new(
                    usize::try_from(cell.candidate_id).ok()?,
                    clearra_coverage::pattern::pattern_id::PatternId::new(cell.pattern_id),
                    cell.trace_identity.clone(),
                    cell.score,
                    cell.attack,
                    cell.accuracy_level.clone(),
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(MaterializedScoreMatrix::new(
            self.pattern_count,
            cells,
            self.profile_id.clone(),
            self.accuracy_level.clone(),
            self.complete,
        ))
    }

    /// Each geometry candidate may have multiple legal movement traces for the
    /// same supply pattern. Scoring assigns that shape its highest legal trace
    /// before any cross-candidate objective is evaluated.
    pub fn highest_legal_cells_by_candidate_pattern(&self) -> Vec<&ScoreCell> {
        let mut cells = BTreeMap::<(u64, usize), &ScoreCell>::new();
        for cell in &self.cells {
            cells
                .entry((cell.candidate_id, cell.pattern_id))
                .and_modify(|current| {
                    if score_cell_order(cell, current).is_gt() {
                        *current = cell;
                    }
                })
                .or_insert(cell);
        }
        cells.into_values().collect()
    }

    pub fn highest_legal_cells_by_pattern(&self) -> Vec<&ScoreCell> {
        extrema_by_pattern(self.highest_legal_cells_by_candidate_pattern(), true)
    }

    pub fn minimal_shape_cells_by_pattern(&self) -> Vec<&ScoreCell> {
        extrema_by_pattern(self.highest_legal_cells_by_candidate_pattern(), false)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreMatrixCancelled;

fn score_matrix_materialized(pattern_count: usize, source_complete: bool, has_cells: bool) -> bool {
    pattern_count > 0 && (source_complete || has_cells)
}

fn trace_satisfies_requirement(events: &[ReplayEvent], requirement: TraceRequirement) -> bool {
    match requirement {
        TraceRequirement::None => true,
        TraceRequirement::PlacementTrace => events
            .iter()
            .any(|event| matches!(event, ReplayEvent::Placement(_))),
        TraceRequirement::FullDropTrace => events
            .iter()
            .any(|event| matches!(event, ReplayEvent::Drop(_))),
        TraceRequirement::KickEvidenceTrace => events
            .iter()
            .any(|event| matches!(event, ReplayEvent::KickEvidence(_))),
    }
}

fn trace_has_complete_spin_evidence(
    trace: &clearra_replay::ReplayTrace,
    profile: &ScoreProfile,
) -> bool {
    if trace.events().iter().any(|event| {
        matches!(
            event,
            ReplayEvent::TraceCompleteness(completeness)
                if completeness.completeness() != clearra_replay::TraceCompleteness::Complete
        )
    }) {
        return false;
    }
    trace.solution_trace().steps().iter().all(|step| {
        let requires_evidence = profile
            .spin_profile()
            .requires_complete_movement_evidence(step.piece_decision().active_piece().as_ascii());
        if !requires_evidence {
            return true;
        }
        let movement = trace.events().iter().find_map(|event| match event {
            ReplayEvent::MovementEvidence(evidence)
                if evidence.step_index() == step.step_index() =>
            {
                Some(*evidence)
            }
            _ => None,
        });
        let Some(movement) = movement else {
            return false;
        };
        if !movement.path_complete() || !movement.rotation_evidence_complete() {
            return false;
        }
        if !movement.last_action_was_rotation() {
            return !movement.used_kick();
        }
        if !movement.used_kick() {
            return true;
        }
        let Ok(result_x) = i16::try_from(step.placement().x()) else {
            return false;
        };
        let Ok(result_y) = i16::try_from(step.placement().y()) else {
            return false;
        };
        trace.events().iter().any(|event| {
            matches!(
                event,
                ReplayEvent::KickEvidence(evidence)
                    if evidence.step_index() == step.step_index()
                        && evidence.first_success_confirmed()
                        && evidence.to_rotation()
                            == step.placement().rotation().quarter_turns() as u8
                        && evidence.result() == (result_x, result_y)
            )
        })
    })
}

fn extrema_by_pattern(mut cells: Vec<&ScoreCell>, maximum: bool) -> Vec<&ScoreCell> {
    cells.sort_by_key(|cell| (cell.pattern_id, cell.candidate_id));
    let mut selected = BTreeMap::<usize, &ScoreCell>::new();
    for cell in cells {
        selected
            .entry(cell.pattern_id)
            .and_modify(|current| {
                if score_cell_is_preferred(cell, current, maximum) {
                    *current = cell;
                }
            })
            .or_insert(cell);
    }
    selected.into_values().collect()
}

fn score_cell_is_preferred(candidate: &ScoreCell, current: &ScoreCell, maximum: bool) -> bool {
    let score_order = (candidate.score, candidate.attack).cmp(&(current.score, current.attack));
    if !score_order.is_eq() {
        return if maximum {
            score_order.is_gt()
        } else {
            score_order.is_lt()
        };
    }
    (candidate.candidate_id, candidate.trace_identity.as_str())
        < (current.candidate_id, current.trace_identity.as_str())
}

fn score_cell_order(left: &ScoreCell, right: &ScoreCell) -> std::cmp::Ordering {
    (left.score, left.attack)
        .cmp(&(right.score, right.attack))
        .then_with(|| right.candidate_id.cmp(&left.candidate_id))
        .then_with(|| right.trace_identity.cmp(&left.trace_identity))
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_replay::{BuildVariantOperation, BuildVariantReplayInput, ReplayEngine};

    use super::*;
    use crate::{CandidateExecution, CandidateExecutionAggregate};

    #[test]
    fn partial_replay_matrix_is_materialized_but_incomplete() {
        let profile = ScoreProfile::new("partial", "Partial");
        let input = BuildVariantReplayInput::new(
            "partial-trace",
            Board64Layout::standard_10_by_lines(2).expect("layout"),
            0,
            vec![BuildVariantOperation::new(
                PieceKind::O,
                RotationState::Zero,
                0,
                0,
            )],
        );
        let trace = ReplayEngine::build_variant_to_trace(&input).expect("replay");
        let aggregates = [CandidateExecutionAggregate::new(
            1,
            vec![CandidateExecution::new(0, "partial-trace", trace)],
        )];

        let matrix = ScoreMatrix::materialize(&aggregates, &profile, 1, false);

        assert!(matrix.materialized());
        assert!(!matrix.complete());
        assert_eq!(matrix.cell_count(), 1);
        assert_eq!(
            matrix.incomplete_reason(),
            Some("score_execution_source_incomplete")
        );
    }

    #[test]
    fn partial_precomputed_matrix_is_materialized_but_incomplete() {
        let profile = ScoreProfile::new("partial", "Partial");
        let matrix = ScoreMatrix::from_materialized_cells(
            vec![ScoreCell::new(
                1,
                0,
                "partial-cell",
                100,
                0,
                profile.accuracy_level().as_str(),
            )],
            &profile,
            1,
            false,
        );

        assert!(matrix.materialized());
        assert!(!matrix.complete());
        assert_eq!(matrix.cell_count(), 1);
        assert_eq!(
            matrix.incomplete_reason(),
            Some("score_execution_source_incomplete")
        );
    }

    #[test]
    fn incomplete_empty_matrix_is_not_materialized() {
        let profile = ScoreProfile::new("empty", "Empty");
        let matrix = ScoreMatrix::from_materialized_cells(Vec::new(), &profile, 1, false);

        assert!(!matrix.materialized());
        assert!(!matrix.complete());
        assert_eq!(matrix.cell_count(), 0);
    }

    #[test]
    fn complete_empty_matrix_remains_materialized() {
        let profile = ScoreProfile::new("empty", "Empty");
        let matrix = ScoreMatrix::from_materialized_cells(Vec::new(), &profile, 1, true);

        assert!(matrix.materialized());
        assert!(matrix.complete());
        assert_eq!(matrix.cell_count(), 0);
    }
}
