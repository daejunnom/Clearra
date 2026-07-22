use clearra_pc_graph::request::{PcCountPolicy, PcQueueInput};
use clearra_scoring::profile::ScoreProfile;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::{
    evaluate::{
        setup_raw_metrics::{Requires180Evidence, RuleProfileEvidence},
        PostPcEvaluation, PostPcEvaluator, PostPcScenarioInput,
    },
    query::SetupSearchQuery,
    variant::build_variant::BuildVariant,
};

use super::{setup_family_grouper::BuildKey, setup_shape_packer::visible_height_for_mask};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SetupPostPcEvaluation {
    evaluation: PostPcEvaluation,
    requires_180: Requires180Evidence,
    rule_profile_evidence: RuleProfileEvidence,
}

impl SetupPostPcEvaluation {
    pub(crate) fn unsupported(reason: &'static str) -> Self {
        Self {
            evaluation: PostPcEvaluation::Unsupported { reason },
            requires_180: Requires180Evidence::NotModeled,
            rule_profile_evidence: RuleProfileEvidence::NotModeled,
        }
    }
}
impl SetupPostPcEvaluation {
    pub(crate) fn evaluation(&self) -> &PostPcEvaluation {
        &self.evaluation
    }
}
impl SetupPostPcEvaluation {
    pub(crate) fn into_evaluation(self) -> PostPcEvaluation {
        self.evaluation
    }
}
impl SetupPostPcEvaluation {
    pub(crate) fn requires_180(&self) -> Requires180Evidence {
        self.requires_180
    }
}
impl SetupPostPcEvaluation {
    pub(crate) fn rule_profile_evidence(&self) -> RuleProfileEvidence {
        self.rule_profile_evidence
    }
}

pub(crate) fn evaluate_post_pc(
    query: &SetupSearchQuery,
    variant: &BuildVariant,
    build_key: &BuildKey,
    score_profile: Option<&ScoreProfile>,
) -> SetupPostPcEvaluation {
    let remaining_queue =
        PcQueueInput::fixed_sequence(FixedSequence::new(build_key.remaining_queue.clone()));
    let max_pieces = build_key
        .remaining_queue
        .len()
        .min(query.piece_budget().max_piece_count() as usize);
    let input = PostPcScenarioInput::from_build_variant(
        variant,
        visible_height_for_mask(variant.identity().occupied_shape()),
        remaining_queue,
        max_pieces,
    )
    .with_allow_hold(query.hold_policy().is_enabled())
    .with_count_policy(PcCountPolicy::CountAll)
    .with_min_remaining_queue(0)
    .with_retained_trace_limit(query.limits().post_pc_retained_trace_limit());

    let requires_180 = if input.requires_180_modeled() {
        Requires180Evidence::known(input.requires_180())
    } else {
        Requires180Evidence::NotModeled
    };
    let rule_profile_evidence = input
        .rule_profile_id()
        .map(RuleProfileEvidence::Explicit)
        .unwrap_or(RuleProfileEvidence::DefaultMvpRule {
            post_pc_rule: input.effective_rule_profile_id(),
        });

    let evaluation = match score_profile {
        Some(profile) => PostPcEvaluator::evaluate_input_with_score_profile(input, profile),
        None => PostPcEvaluator::evaluate_input(input),
    };

    SetupPostPcEvaluation {
        evaluation,
        requires_180,
        rule_profile_evidence,
    }
}
