use clearra_replay::{trace::SolutionTrace, ReplayTrace};

use crate::{
    event::score_event::ScoreEvent,
    profile::{B2BChainRule, SpinRuleId},
    state::combo_state::ComboState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionTraceEvents {
    events: Vec<ScoreEvent>,
}

impl SolutionTraceEvents {
    pub fn from_trace(trace: &SolutionTrace, spin_rule: SpinRuleId) -> Self {
        Self::from_trace_with_initial_b2b(trace, spin_rule, 0)
    }
}
impl SolutionTraceEvents {
    pub fn from_trace_with_initial_b2b(
        trace: &SolutionTrace,
        spin_rule: SpinRuleId,
        initial_b2b: u32,
    ) -> Self {
        let mut combo = ComboState::default();
        let mut b2b_chain = initial_b2b;
        let mut events = Vec::with_capacity(trace.steps().len());

        for step in trace.steps() {
            let event = ScoreEvent::from_step_with_b2b_chain(
                *step,
                spin_rule,
                combo,
                b2b_chain,
                B2BChainRule::UnderlyingDifficultClearOnly,
            );
            combo = event.combo_after();
            b2b_chain = event.b2b_after_chain();
            events.push(event);
        }

        Self { events }
    }
}
impl SolutionTraceEvents {
    pub fn from_replay_trace(trace: &ReplayTrace, spin_rule: SpinRuleId) -> Self {
        Self::from_replay_trace_with_initial_b2b(trace, spin_rule, 0)
    }
}
impl SolutionTraceEvents {
    pub fn from_replay_trace_with_initial_b2b(
        trace: &ReplayTrace,
        spin_rule: SpinRuleId,
        initial_b2b: u32,
    ) -> Self {
        Self::from_replay_trace_with_b2b_rule(
            trace,
            spin_rule,
            initial_b2b,
            B2BChainRule::UnderlyingDifficultClearOnly,
        )
    }

    pub(crate) fn from_replay_trace_with_b2b_rule(
        trace: &ReplayTrace,
        spin_rule: SpinRuleId,
        initial_b2b: u32,
        b2b_chain_rule: B2BChainRule,
    ) -> Self {
        let mut combo = ComboState::default();
        let mut b2b_chain = initial_b2b;
        let mut events = Vec::with_capacity(trace.solution_trace().steps().len());
        for step in trace.solution_trace().steps() {
            let event = ScoreEvent::from_replay_step_with_b2b_chain(
                trace,
                *step,
                spin_rule,
                combo,
                b2b_chain,
                b2b_chain_rule,
            );
            combo = event.combo_after();
            b2b_chain = event.b2b_after_chain();
            events.push(event);
        }
        Self { events }
    }
}
impl SolutionTraceEvents {
    pub fn events(&self) -> &[ScoreEvent] {
        &self.events
    }
}
impl SolutionTraceEvents {
    pub fn len(&self) -> usize {
        self.events.len()
    }
}
impl SolutionTraceEvents {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
#[path = "solution_trace_events_tests.rs"]
mod tests;
