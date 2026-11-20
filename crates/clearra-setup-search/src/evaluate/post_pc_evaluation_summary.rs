use clearra_core_executor::{CoreExecutionResult, CorePathStep};
use clearra_pc_graph::request::{PcCompletionGoal, PcScenarioQuery};

use super::{
    post_pc_continuation_status::PostPcContinuationStatus,
    post_pc_score_summary::{PostPcScoreSummary, ScoreEvaluationBasis},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostPcEvaluationSummary {
    solution_found: bool,
    completion_goal: PcCompletionGoal,
    cleared_lines: u8,
    total_solution_count: usize,
    unique_solution_count: usize,
    min_queue_consumed: usize,
    max_queue_consumed: usize,
    sample_queue_consumed: usize,
    placed_piece_count: usize,
    best_remaining_queue_len: usize,
    retained_trace_count: usize,
    count_complete: bool,
    sample_trace_available: bool,
    continuation_available: bool,
    continuation_available_complete: bool,
    searched_nodes: u64,
    budget_exceeded: bool,
    score: PostPcScoreSummary,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PostPcWitness {
    pub(crate) placed_pieces: usize,
    pub(crate) consumed_queue: usize,
    pub(crate) cleared_lines: u8,
}

impl PostPcWitness {
    pub(crate) fn from_retained_path(path: &[CorePathStep], queue_len: usize) -> Option<Self> {
        if path.is_empty() {
            return None;
        }
        let mut witness = Self {
            placed_pieces: path.len(),
            ..Self::default()
        };
        for (index, step) in path.iter().enumerate() {
            let consumes = match step.hold() {
                "use-current" | "swap-held" => 1,
                "store-current-use-next" => 2,
                "release-held-at-terminal"
                    if index + 1 == path.len() && witness.consumed_queue == queue_len =>
                {
                    0
                }
                _ => return None,
            };
            witness.consumed_queue = witness.consumed_queue.checked_add(consumes)?;
            witness.cleared_lines = witness.cleared_lines.checked_add(step.cleared_lines())?;
        }
        (witness.consumed_queue <= queue_len).then_some(witness)
    }

    pub(crate) fn from_result(
        query: &PcScenarioQuery,
        result: &CoreExecutionResult,
    ) -> Option<Self> {
        if query.retained_trace_limit() == 0 || !result.solution_found() {
            return None;
        }
        Self::from_retained_path(result.path_steps(), query.remaining_queue().len())
    }
}

impl PostPcEvaluationSummary {
    // Constructor fields mirror the persisted post-PC summary contract explicitly.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        solution_found: bool,
        completion_goal: PcCompletionGoal,
        cleared_lines: u8,
        total_solution_count: usize,
        unique_solution_count: usize,
        retained_trace_count: usize,
        count_complete: bool,
        continuation_available: bool,
        score: PostPcScoreSummary,
    ) -> Self {
        Self {
            solution_found,
            completion_goal,
            cleared_lines,
            total_solution_count,
            unique_solution_count,
            min_queue_consumed: 0,
            max_queue_consumed: 0,
            sample_queue_consumed: 0,
            placed_piece_count: 0,
            best_remaining_queue_len: 0,
            retained_trace_count,
            count_complete,
            sample_trace_available: retained_trace_count > 0,
            continuation_available,
            continuation_available_complete: count_complete || continuation_available,
            searched_nodes: 0,
            budget_exceeded: false,
            score,
        }
    }
}
impl PostPcEvaluationSummary {
    pub(crate) fn from_query_result(
        query: &PcScenarioQuery,
        result: &CoreExecutionResult,
        witness: Option<PostPcWitness>,
        score: PostPcScoreSummary,
    ) -> Self {
        let remaining_queue_len =
            witness.map(|witness| query.remaining_queue().len() - witness.consumed_queue);
        let continuation =
            PostPcContinuationStatus::from_query_result(query, result, remaining_queue_len);
        let exact_family_count = result
            .usize_field("normalized_unique_solution_count")
            .unwrap_or(0);
        let consumed = witness.map_or(0, |witness| witness.consumed_queue);
        Self {
            solution_found: result.solution_found(),
            completion_goal: query.completion_goal(),
            cleared_lines: witness.map_or(0, |witness| witness.cleared_lines),
            // Rust counts canonical search-family members here. BuildUp path
            // multiplicity is separate and is not a second solution count.
            total_solution_count: exact_family_count,
            unique_solution_count: result.usize_field("unique_solution_count").unwrap_or(0),
            // This interface reports queue lengths for its retained witness
            // (historically the first accepted candidate). It does not claim
            // that one representative established global extrema.
            min_queue_consumed: consumed,
            max_queue_consumed: consumed,
            sample_queue_consumed: consumed,
            placed_piece_count: witness.map_or(0, |witness| witness.placed_pieces),
            best_remaining_queue_len: remaining_queue_len.unwrap_or(0),
            retained_trace_count: usize::from(witness.is_some()),
            count_complete: result.bool_field("count_complete").unwrap_or(false),
            sample_trace_available: witness.is_some(),
            continuation_available: continuation.available(),
            continuation_available_complete: continuation.complete(),
            searched_nodes: result.u64_field("searched_nodes").unwrap_or(0),
            budget_exceeded: result.bool_field("resource_truncated").unwrap_or(false),
            score,
        }
    }
}
impl PostPcEvaluationSummary {
    pub fn solution_found(&self) -> bool {
        self.solution_found
    }
}
impl PostPcEvaluationSummary {
    pub fn completion_goal(&self) -> PcCompletionGoal {
        self.completion_goal
    }
}
impl PostPcEvaluationSummary {
    pub fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }
}
impl PostPcEvaluationSummary {
    pub fn total_solution_count(&self) -> usize {
        self.total_solution_count
    }
}
impl PostPcEvaluationSummary {
    pub fn unique_solution_count(&self) -> usize {
        self.unique_solution_count
    }
}
impl PostPcEvaluationSummary {
    pub fn min_queue_consumed(&self) -> usize {
        self.min_queue_consumed
    }
}
impl PostPcEvaluationSummary {
    pub fn max_queue_consumed(&self) -> usize {
        self.max_queue_consumed
    }
}
impl PostPcEvaluationSummary {
    pub fn sample_queue_consumed(&self) -> usize {
        self.sample_queue_consumed
    }
}
impl PostPcEvaluationSummary {
    pub fn placed_piece_count(&self) -> usize {
        self.placed_piece_count
    }
}
impl PostPcEvaluationSummary {
    pub fn best_remaining_queue_len(&self) -> usize {
        self.best_remaining_queue_len
    }
}
impl PostPcEvaluationSummary {
    pub fn retained_trace_count(&self) -> usize {
        self.retained_trace_count
    }
}
impl PostPcEvaluationSummary {
    pub fn count_complete(&self) -> bool {
        self.count_complete
    }
}
impl PostPcEvaluationSummary {
    pub fn sample_trace_available(&self) -> bool {
        self.sample_trace_available
    }
}
impl PostPcEvaluationSummary {
    pub fn continuation_available(&self) -> bool {
        self.continuation_available
    }
}
impl PostPcEvaluationSummary {
    pub fn continuation_available_complete(&self) -> bool {
        self.continuation_available_complete
    }
}
impl PostPcEvaluationSummary {
    pub fn searched_nodes(&self) -> u64 {
        self.searched_nodes
    }
}
impl PostPcEvaluationSummary {
    pub fn budget_exceeded(&self) -> bool {
        self.budget_exceeded
    }
}
impl PostPcEvaluationSummary {
    pub fn score(&self) -> &PostPcScoreSummary {
        &self.score
    }
}
impl PostPcEvaluationSummary {
    pub fn score_evaluation_trace_count(&self) -> usize {
        self.score.score_evaluation_trace_count()
    }
}
impl PostPcEvaluationSummary {
    pub fn score_evaluation_complete(&self) -> bool {
        self.score.score_evaluation_complete()
    }
}
impl PostPcEvaluationSummary {
    pub fn score_evaluation_basis(&self) -> ScoreEvaluationBasis {
        self.score.score_evaluation_basis()
    }
}
