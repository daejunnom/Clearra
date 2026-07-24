use clearra_replay::{trace::SolutionTrace, ReplayEvent, ReplayTrace};
use clearra_spin::SpinInterpretationSet;

use crate::{
    event::{clear_event::ClearEvent, score_event::ScoreEvent},
    model::{
        attack_model_evaluator::AttackModelEvaluator, score_evaluation::ScoreEvaluation,
        score_evaluation_policy::ScoreEvaluationPolicy, score_table::ScoreModelTable,
        spin_interpretation_evaluator::SpinInterpretationEvaluator,
        spin_interpretation_score::SpinInterpretationScore,
    },
    profile::{AttackProfile, DropScorePolicy, ScoreModelId, ScoreProfile},
    state::score_state::ScoreState,
    trace::solution_trace_events::SolutionTraceEvents,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScoreModelEvaluator;

impl ScoreModelEvaluator {
    pub fn initial_state(policy: ScoreEvaluationPolicy) -> ScoreState {
        ScoreState::default().with_b2b_chain(policy.initial_b2b())
    }

    pub fn evaluate_classified_lock(
        profile: &ScoreProfile,
        state: ScoreState,
        step_index: usize,
        cleared_lines: u8,
        perfect_clear: bool,
        spin: Option<crate::event::spin_event::SpinEvent>,
    ) -> ScoreState {
        let event = ScoreEvent::from_classified_clear_with_b2b_chain(
            step_index,
            ClearEvent::new(cleared_lines, perfect_clear),
            spin,
            state.combo(),
            state.b2b_chain(),
            profile.b2b_policy().chain_rule(),
        );
        Self::evaluate_event(profile, state, event)
    }
}
impl ScoreModelEvaluator {
    pub fn evaluate(state: ScoreState, clear: ClearEvent) -> ScoreState {
        state.add_score(Self::clear_score(ScoreModelId::Guideline, clear))
    }
}
impl ScoreModelEvaluator {
    pub fn evaluate_event(
        profile: &ScoreProfile,
        state: ScoreState,
        event: ScoreEvent,
    ) -> ScoreState {
        let score = Self::score_for_event(profile, event);
        let attack = AttackModelEvaluator::evaluate_event(profile, event);

        state
            .add_score(score)
            .add_attack(attack)
            .with_combo(event.combo_after())
            .with_b2b_chain(event.b2b_after_chain())
    }
}
impl ScoreModelEvaluator {
    pub fn evaluate_trace(profile: &ScoreProfile, trace: &SolutionTrace) -> ScoreEvaluation {
        let events = SolutionTraceEvents::from_trace(trace, profile.spin_rule());
        Self::evaluate_events(profile, events, ScoreEvaluationPolicy::profile_defaults())
    }
}
impl ScoreModelEvaluator {
    pub fn evaluate_replay_trace(profile: &ScoreProfile, trace: &ReplayTrace) -> ScoreEvaluation {
        Self::evaluate_replay_trace_with_policy(
            profile,
            trace,
            ScoreEvaluationPolicy::profile_defaults(),
        )
    }
}
impl ScoreModelEvaluator {
    pub fn evaluate_replay_trace_with_policy(
        profile: &ScoreProfile,
        trace: &ReplayTrace,
        policy: ScoreEvaluationPolicy,
    ) -> ScoreEvaluation {
        let events = SolutionTraceEvents::from_replay_trace_with_b2b_rule(
            trace,
            profile.spin_rule(),
            policy.initial_b2b(),
            profile.b2b_policy().chain_rule(),
        );
        let mut evaluation = Self::evaluate_events(profile, events, policy);
        let drop_score = if policy.include_drop_score() {
            replay_drop_score(profile.drop_score_policy(), trace)
        } else {
            0
        };
        if drop_score > 0 {
            evaluation = ScoreEvaluation::new(
                evaluation.profile_id(),
                evaluation.final_state().add_score(drop_score),
                evaluation.events().to_vec(),
            );
        }
        evaluation
    }
}
impl ScoreModelEvaluator {
    pub fn evaluate_spin_interpretation_set(
        profile: &ScoreProfile,
        attack_profile: &AttackProfile,
        interpretations: &SpinInterpretationSet,
        cleared_lines: u8,
        perfect_clear: bool,
    ) -> SpinInterpretationScore {
        SpinInterpretationEvaluator::evaluate(
            profile,
            attack_profile,
            interpretations,
            cleared_lines,
            perfect_clear,
        )
    }
}
impl ScoreModelEvaluator {
    fn evaluate_events(
        profile: &ScoreProfile,
        events: SolutionTraceEvents,
        policy: ScoreEvaluationPolicy,
    ) -> ScoreEvaluation {
        let mut state = Self::initial_state(policy);
        for event in events.events().iter().copied() {
            state = Self::evaluate_event(profile, state, event);
        }

        ScoreEvaluation::new(profile.id(), state, events.events().to_vec())
    }
}
impl ScoreModelEvaluator {
    fn score_for_event(profile: &ScoreProfile, event: ScoreEvent) -> u64 {
        let event_score = ScoreModelTable::for_model(profile.score_model()).map_or(0, |table| {
            table.score_event_with_b2b(event, profile.b2b_policy())
        });
        let combo_bonus = combo_score_bonus(profile, event);
        event_score.saturating_add(combo_bonus)
    }
}
impl ScoreModelEvaluator {
    fn clear_score(model: ScoreModelId, clear: ClearEvent) -> u64 {
        ScoreModelTable::for_model(model).map_or(0, |table| table.score_clear(clear))
    }
}

fn combo_score_bonus(profile: &ScoreProfile, event: ScoreEvent) -> u64 {
    let policy = profile.combo_policy();
    if !policy.enabled() || event.clear().lines() == 0 {
        return 0;
    }
    u64::from(event.combo_after().combo().saturating_sub(1))
        .saturating_mul(policy.score_bonus_per_combo())
}

fn replay_drop_score(policy: DropScorePolicy, trace: &ReplayTrace) -> u64 {
    if !policy.requires_drop_events() {
        return 0;
    }

    trace
        .events()
        .iter()
        .filter_map(|event| match event {
            ReplayEvent::Drop(drop) => Some(policy.hard_drop_score(drop.distance())),
            _ => None,
        })
        .sum()
}

#[cfg(test)]
#[path = "score_model_evaluator_tests.rs"]
mod tests;
