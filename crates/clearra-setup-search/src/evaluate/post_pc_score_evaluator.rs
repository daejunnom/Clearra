use clearra_core_executor::CoreExecutionResult;
use clearra_scoring::{model::score_evaluation::ScoreEvaluationSummary, profile::ScoreProfile};

use super::{
    post_pc_evaluation_summary::PostPcWitness,
    post_pc_score_summary::{PostPcScoreSummary, ScoreEvaluationBasis},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PostPcScoreEvaluator;

impl PostPcScoreEvaluator {
    pub(crate) fn score_retained_traces(
        result: &CoreExecutionResult,
        witness: Option<PostPcWitness>,
        profile: &ScoreProfile,
    ) -> PostPcScoreSummary {
        let retained = usize::from(witness.is_some());
        let best_score = witness.map_or(0, |witness| {
            u64::from(witness.cleared_lines) * 1_000 + witness.placed_pieces as u64 * 10
        });
        let best_attack = witness.map_or(0, |witness| u32::from(witness.cleared_lines).max(1));
        let variants = result.usize_field("build_variant_count");
        let score_evaluation_complete = result.bool_field("build_variant_count_exact")
            == Some(true)
            && variants == Some(retained);
        let trace_retention_truncated = variants.is_some_and(|count| count > retained)
            || result.bool_field("trace_retention_truncated") == Some(true);
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
            retained,
            score_evaluation_complete,
            score_evaluation_basis,
        ))
    }
}
