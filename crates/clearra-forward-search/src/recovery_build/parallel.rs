//! Bounded, demand-driven supply-pair tasks. No pair is skipped or sampled.
//! Task completion order never changes floating-point reduction or witnesses.
//! Native threads and browser verifiers execute exactly the same wire protocol.
use super::parallel_wire as wire;
use super::{
    population::Sum, RecoveryBuildError, RecoveryBuildExample, RecoveryBuildFixedQuery,
    RecoveryBuildPopulation, RecoveryBuildQuery, RecoveryBuildStatus,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_supply::{
    pattern_universe::{
        materialized_pattern_universe::MaterializedPatternUniverse,
        pattern_universe_materializer::PatternUniverseMaterializer,
    },
    queue::queue_pattern_expression::QueuePatternExpression,
};
use std::collections::BTreeMap;

pub const RECOVERY_PAIRS_PER_TASK: usize = 32;

pub(super) struct Domain {
    pub query: RecoveryBuildQuery,
    first: MaterializedPatternUniverse,
    second: MaterializedPatternUniverse,
    pub possible: u128,
}
impl Domain {
    pub fn new(query: RecoveryBuildQuery) -> Result<Self, RecoveryBuildError> {
        query.validate()?;
        let parse = |text: &str| {
            let parsed = QueuePatternExpression::parse(text, 0)
                .map_err(|_| RecoveryBuildError::InvalidSupplyPattern)?;
            PatternUniverseMaterializer::queue_pattern_expression(&parsed, 0)
                .map_err(|_| RecoveryBuildError::PatternDomainUnavailable)
        };
        let first = parse(&query.first_supply)?;
        let second = parse(&query.second_supply)?;
        let possible = (first.pattern_count() as u128)
            .checked_mul(second.pattern_count() as u128)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        Ok(Self {
            query,
            first,
            second,
            possible,
        })
    }
    fn indices(&self, ordinal: u128) -> Result<(usize, usize), RecoveryBuildError> {
        if ordinal >= self.possible {
            return Err(RecoveryBuildError::InvalidParallelWire);
        }
        let width = self.second.pattern_count() as u128;
        Ok((
            usize::try_from(ordinal / width).map_err(|_| RecoveryBuildError::CounterOverflow)?,
            usize::try_from(ordinal % width).map_err(|_| RecoveryBuildError::CounterOverflow)?,
        ))
    }
    pub(super) fn evaluate(
        &self,
        ordinal: u128,
        control: &ExecutionControl,
    ) -> Result<RecoveryBuildExample, RecoveryBuildError> {
        let (i, j) = self.indices(ordinal)?;
        let a = self.first.sequence_at(i).to_vec();
        let b = self.second.sequence_at(j).to_vec();
        let query = RecoveryBuildFixedQuery {
            fields: self.query.fields.clone(),
            first_supply: a.clone(),
            second_supply: b.clone(),
            early_limit: self.query.early_limit,
            allow_piece_exchange: self.query.allow_piece_exchange,
            hold_enabled: self.query.hold_enabled,
            preserve_b2b: self.query.preserve_b2b,
            initial_b2b: self.query.initial_b2b,
            rule_profile: self.query.rule_profile,
            spin_profile: self.query.spin_profile,
        };
        let path = query.search(control)?;
        Ok(RecoveryBuildExample {
            first_pattern: i,
            second_pattern: j,
            first_queue: a,
            second_queue: b,
            path,
        })
    }
    fn weight(&self, ordinal: u128) -> Result<f64, RecoveryBuildError> {
        let (i, j) = self.indices(ordinal)?;
        Ok(self.first.weight_at(i).get() * self.second.weight_at(j).get())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryBuildProduce {
    Pending,
    Batch,
    Completed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryBuildParallelProgress {
    pub possible: u128,
    pub dispatched: u128,
    pub evaluated: u128,
    pub states: u128,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Outcome {
    pub status: RecoveryBuildStatus,
    pub states: u64,
}

pub struct RecoveryBuildCoordinator {
    domain: Domain,
    initialization: Vec<u8>,
    next: u128,
    merged: u128,
    pending: BTreeMap<u128, usize>,
    buffered: BTreeMap<u128, Vec<Outcome>>,
    window: usize,
    counts: [u128; 3],
    states: u128,
    probabilities: [Sum; 3],
    examples: [Option<(u128, u64)>; 2],
}
impl RecoveryBuildCoordinator {
    pub fn new(query: RecoveryBuildQuery, workers: usize) -> Result<Self, RecoveryBuildError> {
        let initialization = wire::initialization(&query);
        Ok(Self {
            domain: Domain::new(query)?,
            initialization,
            next: 0,
            merged: 0,
            pending: BTreeMap::new(),
            buffered: BTreeMap::new(),
            window: workers.max(1).saturating_mul(2),
            counts: [0; 3],
            states: 0,
            probabilities: std::array::from_fn(|_| Sum::default()),
            examples: [None; 2],
        })
    }
    pub fn worker_initialization(&self) -> Vec<u8> {
        self.initialization.clone()
    }
    pub fn progress(&self) -> RecoveryBuildParallelProgress {
        RecoveryBuildParallelProgress {
            possible: self.domain.possible,
            dispatched: self.next,
            evaluated: self.merged,
            states: self.states,
        }
    }
    pub fn produce(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(RecoveryBuildProduce, Vec<u8>), RecoveryBuildError> {
        if control.is_cancelled() {
            return Ok((RecoveryBuildProduce::Cancelled, Vec::new()));
        }
        if self.merged == self.domain.possible {
            return Ok((RecoveryBuildProduce::Completed, Vec::new()));
        }
        if self.next == self.domain.possible
            || self.pending.len() + self.buffered.len() >= self.window
        {
            return Ok((RecoveryBuildProduce::Pending, Vec::new()));
        }
        let count = usize::try_from(
            (self.domain.possible - self.next).min(RECOVERY_PAIRS_PER_TASK as u128),
        )
        .map_err(|_| RecoveryBuildError::CounterOverflow)?;
        let start = self.next;
        let bytes = wire::task(&self.initialization, start, count);
        self.pending.insert(start, count);
        self.next += count as u128;
        Ok((RecoveryBuildProduce::Batch, bytes))
    }
    pub fn absorb(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<(), RecoveryBuildError> {
        if control.is_cancelled() {
            return Err(RecoveryBuildError::Cancelled);
        }
        let (start, outcomes) = wire::read_result(bytes, &self.initialization)?;
        if self.pending.get(&start).copied() != Some(outcomes.len()) {
            return Err(RecoveryBuildError::InvalidParallelState);
        }
        // Only a complete owned task can retire its lease; duplicates/stale or
        // foreign receipts cannot add probability mass a second time.
        self.pending.remove(&start);
        self.buffered.insert(start, outcomes);
        while let Some(outcomes) = self.buffered.remove(&self.merged) {
            for outcome in outcomes {
                let category = match outcome.status {
                    RecoveryBuildStatus::Normal => 0,
                    RecoveryBuildStatus::Recovery => 1,
                    RecoveryBuildStatus::NoPath => 2,
                };
                self.counts[category] = self.counts[category]
                    .checked_add(1)
                    .ok_or(RecoveryBuildError::CounterOverflow)?;
                self.states = self
                    .states
                    .checked_add(u128::from(outcome.states))
                    .ok_or(RecoveryBuildError::CounterOverflow)?;
                self.probabilities[category].add(self.domain.weight(self.merged)?);
                if category < 2 && self.examples[category].is_none() {
                    self.examples[category] = Some((self.merged, outcome.states));
                }
                self.merged += 1;
            }
        }
        let safe = 9_007_199_254_740_991;
        if self.merged <= safe {
            control.report_progress(
                "recovery-build",
                self.merged as u64,
                (self.domain.possible <= safe).then_some(self.domain.possible as u64),
            );
        }
        Ok(())
    }
    pub fn finish(
        self,
        control: &ExecutionControl,
    ) -> Result<RecoveryBuildPopulation, RecoveryBuildError> {
        if control.is_cancelled() {
            return Err(RecoveryBuildError::Cancelled);
        }
        if self.merged != self.domain.possible
            || !self.pending.is_empty()
            || !self.buffered.is_empty()
        {
            return Err(RecoveryBuildError::InvalidParallelState);
        }
        // Transfer small receipts, not traces for millions of discarded paths.
        // At most two canonical witnesses are materialized after exact reduction.
        let mut examples: [Option<RecoveryBuildExample>; 2] = [None, None];
        for (category, identity) in self.examples.into_iter().enumerate() {
            if let Some((ordinal, states)) = identity {
                let example = self.domain.evaluate(ordinal, control)?;
                let expected = if category == 0 {
                    RecoveryBuildStatus::Normal
                } else {
                    RecoveryBuildStatus::Recovery
                };
                if example.path.status != expected
                    || example.path.states as u128 != u128::from(states)
                {
                    return Err(RecoveryBuildError::InvalidParallelState);
                }
                examples[category] = Some(example);
            }
        }
        let [normal_example, recovery_example] = examples;
        control.report_progress("postprocess", 0, None);
        Ok(RecoveryBuildPopulation {
            possible: self.domain.possible,
            evaluated: self.merged,
            normal_count: self.counts[0],
            recovery_count: self.counts[1],
            no_path_count: self.counts[2],
            states: self.states,
            normal_probability: self.probabilities[0].value.clamp(0.0, 1.0),
            recovery_probability: self.probabilities[1].value.clamp(0.0, 1.0),
            no_path_probability: self.probabilities[2].value.clamp(0.0, 1.0),
            normal_example,
            recovery_example,
        })
    }
}

pub struct RecoveryBuildWorker {
    domain: Domain,
    initialization: Vec<u8>,
    pub(crate) completed: u128,
    pub(crate) states: u128,
}
impl RecoveryBuildWorker {
    pub fn accepts_initialization(bytes: &[u8]) -> bool {
        wire::accepts(bytes)
    }
    pub fn new(bytes: &[u8]) -> Result<Self, RecoveryBuildError> {
        let query = wire::read_initialization(bytes)?;
        Ok(Self {
            domain: Domain::new(query)?,
            initialization: bytes.to_vec(),
            completed: 0,
            states: 0,
        })
    }
    pub fn progress(&self) -> RecoveryBuildParallelProgress {
        RecoveryBuildParallelProgress {
            possible: self.domain.possible,
            evaluated: self.completed,
            states: self.states,
            dispatched: self.completed,
        }
    }
    pub fn consume(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<(usize, Vec<u8>), RecoveryBuildError> {
        let (start, count) = wire::read_task(bytes, &self.initialization)?;
        if start
            .checked_add(count as u128)
            .is_none_or(|end| end > self.domain.possible)
        {
            return Err(RecoveryBuildError::InvalidParallelWire);
        }
        let mut outcomes = Vec::new();
        outcomes
            .try_reserve_exact(count)
            .map_err(|_| RecoveryBuildError::MemoryUnavailable)?;
        for n in 0..count {
            if control.is_cancelled() {
                return Err(RecoveryBuildError::Cancelled);
            }
            let example = self.domain.evaluate(start + n as u128, control)?;
            let states = u64::try_from(example.path.states)
                .map_err(|_| RecoveryBuildError::CounterOverflow)?;
            outcomes.push(Outcome {
                status: example.path.status,
                states,
            });
            self.completed = self
                .completed
                .checked_add(1)
                .ok_or(RecoveryBuildError::CounterOverflow)?;
            self.states = self
                .states
                .checked_add(u128::from(states))
                .ok_or(RecoveryBuildError::CounterOverflow)?;
        }
        Ok((count, wire::result(&self.initialization, start, &outcomes)))
    }
}
