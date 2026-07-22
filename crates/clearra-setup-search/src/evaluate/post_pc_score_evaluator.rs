use clearra_core_executor::CoreExecutionResult;
use clearra_scoring::{model::score_evaluation::ScoreEvaluationSummary, profile::ScoreProfile};

use super::post_pc_score_summary::{PostPcScoreSummary, ScoreEvaluationBasis};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PostPcScoreEvaluator;

impl PostPcScoreEvaluator {
    pub(crate) fn score_retained_traces(
        result: &CoreExecutionResult,
        profile: &ScoreProfile,
    ) -> PostPcScoreSummary {
        let cleared_lines = result.u8_field("cleared_lines").unwrap_or(0);
        let retained_trace_count = result.usize_field("retained_trace_count").unwrap_or(0);
        let placed_piece_count = result.usize_field("placed_piece_count").unwrap_or(0);
        let best_score = if retained_trace_count > 0 {
            u64::from(cleared_lines) * 1_000 + placed_piece_count as u64 * 10
        } else {
            0
        };
        let best_attack = if retained_trace_count > 0 {
            u32::from(cleared_lines).max(1)
        } else {
            0
        };
        let score_evaluation_trace_count = retained_trace_count;
        let trace_retention_truncated = result
            .bool_field("trace_retention_truncated")
            .unwrap_or(false);
        let count_complete = result.bool_field("count_complete").unwrap_or(false);
        let total_solution_count = result.usize_field("total_solution_count").unwrap_or(0);
        let score_evaluation_complete = count_complete
            && !trace_retention_truncated
            && score_evaluation_trace_count == total_solution_count;
        let score_evaluation_basis = if score_evaluation_complete {
            ScoreEvaluationBasis::AllTraces
        } else if trace_retention_truncated {
            ScoreEvaluationBasis::Sample
        } else {
            ScoreEvaluationBasis::RetainedTraces
        };

        PostPcScoreSummary::from_summary(ScoreEvaluationSummary::new(
            profile.id(),
            best_score,
            best_attack,
            score_evaluation_trace_count,
            score_evaluation_complete,
            score_evaluation_basis,
        ))
    }
}
