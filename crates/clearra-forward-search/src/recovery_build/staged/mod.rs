//! Stage-factorized coverage. The original Cartesian universe is counted, not
//! enumerated. First-source shards are disjoint; alternative executions and
//! mirrored targets contribute set union within a shard.
pub(super) mod diagram;
pub(super) mod geometry;
pub(super) mod solver;
pub(super) mod source;
#[cfg(test)]
mod tests;
use super::{
    population::PreparedPopulation, RecoveryBuildError as Error, RecoveryBuildExample,
    RecoveryBuildStatus,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use diagram::{Id, NONE};
pub(super) use geometry::Geometry;
use solver::Solver;
use source::{cancelled, Source};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(super) struct BlockResult {
    pub counts: [u128; 3],
    pub probabilities: [f64; 3],
    pub states: u128,
    pub normal: Option<RecoveryBuildExample>,
    pub recovery: Option<RecoveryBuildExample>,
}
#[derive(Default)]
struct Sum {
    value: f64,
    correction: f64,
}
impl Sum {
    fn add(&mut self, value: f64) {
        let adjusted = value - self.correction;
        let next = self.value + adjusted;
        self.correction = (next - self.value) - adjusted;
        self.value = next;
    }
}
pub(super) struct Block {
    solver: Solver,
    start: usize,
    count: usize,
    wanted: u8,
}
impl Block {
    pub fn new(
        prepared: &PreparedPopulation,
        geometry: Geometry,
        start: usize,
        count: usize,
        wanted: u8,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        let mut diagram = diagram::Diagram::default();
        let source = Source::compile(
            &mut diagram,
            &prepared.first,
            &prepared.second,
            start,
            count,
            control,
        )?;
        let solver = Solver::new(prepared.query.clone(), source, diagram, geometry);
        Ok(Self {
            solver,
            start,
            count,
            wanted,
        })
    }
    pub fn advance(&mut self, control: &ExecutionControl) -> Result<bool, Error> {
        self.solver.advance(256, control)
    }
    pub fn states(&self) -> u128 {
        self.solver.states
    }
    pub fn finish(
        mut self,
        prepared: &PreparedPopulation,
        control: &ExecutionControl,
    ) -> Result<(BlockResult, Geometry), Error> {
        if !self.solver.done {
            return Err(Error::PatternDomainUnavailable);
        }
        cancelled(control)?;
        let total = self
            .solver
            .diagram
            .union(self.solver.normal, self.solver.recovery)?;
        let no_path = self
            .solver
            .diagram
            .difference(self.solver.source.universe, total)?;
        let languages = [self.solver.normal, self.solver.recovery, no_path];
        let mut result = BlockResult {
            counts: [0; 3],
            probabilities: [0.0; 3],
            states: self.solver.states,
            normal: None,
            recovery: None,
        };
        let mut sums = [Sum::default(), Sum::default(), Sum::default()];
        let mut measure_cache = HashMap::<Id, (u128, f64)>::new();
        for i in self.start..self.start + self.count {
            cancelled(control)?;
            let first = prepared.first.sequence_at(i);
            for (category, &language) in languages.iter().enumerate() {
                let rest = self
                    .solver
                    .source
                    .follow_first(&self.solver.diagram, language, &first);
                let (count, probability) = if let Some(&measure) = measure_cache.get(&rest) {
                    measure
                } else {
                    let measure = self.measure_second(prepared, rest, control)?;
                    measure_cache
                        .try_reserve(1)
                        .map_err(|_| Error::MemoryUnavailable)?;
                    measure_cache.insert(rest, measure);
                    measure
                };
                result.counts[category] = result.counts[category]
                    .checked_add(count)
                    .ok_or(Error::CounterOverflow)?;
                sums[category].add(prepared.first.weight_at(i).get() * probability);
                let needed = count > 0
                    && match category {
                        0 => self.wanted & 1 != 0 && result.normal.is_none(),
                        1 => self.wanted & 2 != 0 && result.recovery.is_none(),
                        _ => false,
                    };
                if needed {
                    // Search only the representative's second-source ranks,
                    // after its full suffix class has already been certified.
                    let j = (0..prepared.second.pattern_count())
                        .find(|&j| {
                            self.solver.source.accepts_second(
                                &self.solver.diagram,
                                rest,
                                &prepared.second.sequence_at(j),
                            )
                        })
                        .ok_or(Error::PatternDomainUnavailable)?;
                    let second = prepared.second.sequence_at(j);
                    let status = if category == 0 {
                        RecoveryBuildStatus::Normal
                    } else {
                        RecoveryBuildStatus::Recovery
                    };
                    let path = self.solver.witness(&first, &second, status, control)?;
                    let example = RecoveryBuildExample {
                        first_pattern: i,
                        second_pattern: j,
                        first_queue: first.to_vec(),
                        second_queue: second.to_vec(),
                        path,
                    };
                    if category == 0 {
                        result.normal = Some(example);
                    } else {
                        result.recovery = Some(example);
                    }
                }
            }
        }
        result.probabilities = sums.map(|s| s.value);
        if result.counts.iter().sum::<u128>()
            != (self.count as u128) * prepared.second.pattern_count() as u128
        {
            return Err(Error::PatternDomainUnavailable);
        }
        Ok((result, self.solver.geometry))
    }
    fn measure_second(
        &mut self,
        prepared: &PreparedPopulation,
        rest: Id,
        control: &ExecutionControl,
    ) -> Result<(u128, f64), Error> {
        if rest == NONE {
            return Ok((0, 0.0));
        }
        if self.solver.source.compact_second {
            let language = self
                .solver
                .diagram
                .intersect(rest, self.solver.source.second)?;
            let count = self.solver.diagram.count(
                language,
                self.solver.source.first_len,
                self.solver.source.end,
            )?;
            return Ok((count, count as f64 / prepared.second.pattern_count() as f64));
        }
        let mut count = 0;
        let mut weight = Sum::default();
        for j in 0..prepared.second.pattern_count() {
            cancelled(control)?;
            if self.solver.source.accepts_second(
                &self.solver.diagram,
                rest,
                &prepared.second.sequence_at(j),
            ) {
                count += 1;
                weight.add(prepared.second.weight_at(j).get());
            }
        }
        Ok((count, weight.value))
    }
}
