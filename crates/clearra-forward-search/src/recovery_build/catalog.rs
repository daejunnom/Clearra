//! Solution-first stage composition. Queue-independent ILC tilings are joined
//! by conserved inventories, then each exact physical verifier accepts a
//! symbolic language of BOTH supplies. Tasks partition tilings, never claim to
//! partition probabilities: the coordinator ORs their input languages.
mod plan;
#[cfg(test)]
mod tests;
mod wire;
use super::{
    population::{PreparedPopulation, RecoveryBuildSolution},
    staged::{
        diagram::{Diagram, DiagramPacket, Id, NONE},
        geometry::Geometry,
        solver::Solver,
        source::{cancelled, Source},
    },
    RecoveryBuildError as Error, RecoveryBuildExample, RecoveryBuildParallelError as ParallelError,
    RecoveryBuildParallelProduce as Produce, RecoveryBuildParallelProgress as Progress,
    RecoveryBuildPopulation, RecoveryBuildQuery, RecoveryBuildStatus as Status,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
pub(in crate::recovery_build) use plan::Plan;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone)]
struct Task {
    ordinal: u128,
    plan: Plan,
}
struct ResultPacket {
    task: Task,
    languages: DiagramPacket,
    states: u128,
    normal: Option<RecoveryBuildExample>,
    recovery: Option<RecoveryBuildExample>,
}
struct Record {
    plan: Plan,
    languages: [Id; 2],
    normal: Option<RecoveryBuildExample>,
    recovery: Option<RecoveryBuildExample>,
}
#[derive(Default)]
struct Sum {
    sum: f64,
    correction: f64,
}
impl Sum {
    fn add(&mut self, value: f64) {
        let y = value - self.correction;
        let t = self.sum + y;
        self.correction = (t - self.sum) - y;
        self.sum = t;
    }
}

pub(super) struct Coordinator {
    prepared: PreparedPopulation,
    init: Vec<u8>,
    source: Source,
    diagram: Diagram,
    producer: plan::Producer,
    issued: BTreeMap<u128, Plan>,
    records: BTreeMap<u128, Record>,
    capacity: usize,
    next: u128,
    finished: u128,
    states: u128,
    normal: Id,
    recovery: Id,
}
impl Coordinator {
    pub fn new(
        query: RecoveryBuildQuery,
        init: Vec<u8>,
        workers: usize,
    ) -> Result<Self, ParallelError> {
        let control = ExecutionControl::default();
        let prepared = PreparedPopulation::new(query)?;
        let mut diagram = Diagram::default();
        let source = Source::for_population(&mut diagram, &prepared, &control)?;
        let producer = plan::Producer::new(&prepared.query, &source, &control)?;
        Ok(Self {
            prepared,
            init,
            source,
            diagram,
            producer,
            issued: BTreeMap::new(),
            records: BTreeMap::new(),
            capacity: workers.saturating_mul(4).max(1),
            next: 0,
            finished: 0,
            states: 0,
            normal: NONE,
            recovery: NONE,
        })
    }
    pub fn enumeration_done(&self) -> bool {
        self.producer.done
    }
    pub fn waiting_for_results(&self) -> bool {
        !self.issued.is_empty() && (self.producer.done || self.issued.len() >= self.capacity)
    }
    pub fn progress(&self) -> Progress {
        // The catalog work unit is a geometric candidate. Pair probabilities
        // stay in the final report; a candidate is never counted as a queue.
        Progress {
            possible: if self.producer.done { self.next } else { 0 },
            issued: self.next,
            completed: self.finished,
            states: self.states,
        }
    }
    pub fn produce(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(Produce, Vec<u8>), ParallelError> {
        if control.is_cancelled() {
            return Ok((Produce::Cancelled, Vec::new()));
        }
        if self.issued.len() >= self.capacity {
            return Ok((Produce::Pending, Vec::new()));
        }
        if let Some(plan) = self.producer.advance(control)? {
            let ordinal = self.next;
            self.next = self.next.checked_add(1).ok_or(Error::CounterOverflow)?;
            self.issued.insert(ordinal, plan.clone());
            return Ok((
                Produce::Batch,
                wire::task(&self.init, &Task { ordinal, plan }),
            ));
        }
        Ok((
            if self.producer.done && self.issued.is_empty() {
                Produce::Completed
            } else {
                Produce::Pending
            },
            Vec::new(),
        ))
    }
    pub fn absorb(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<(), ParallelError> {
        cancelled(control)?;
        let mut packet = wire::read_result(bytes, &self.init)?;
        let expected = self
            .issued
            .get(&packet.task.ordinal)
            .ok_or(ParallelError::InvalidState("unknown catalog task"))?;
        if *expected != packet.task.plan {
            return Err(ParallelError::InvalidWire("catalog plan binding mismatch"));
        }
        let languages = self.diagram.import(&packet.languages, self.source.end)?;
        for (id, example, status) in [
            (languages[0], &mut packet.normal, Status::Normal),
            (languages[1], &mut packet.recovery, Status::Recovery),
        ] {
            if self.diagram.difference(id, self.source.universe)? != NONE
                || (id != NONE) != example.is_some()
            {
                return Err(ParallelError::InvalidWire(
                    "catalog language outside source or missing witness",
                ));
            }
            if let Some(e) = example {
                let stage = self
                    .producer
                    .stage(packet.task.plan.orientation)
                    .ok_or(Error::PatternDomainUnavailable)?;
                let lengths = super::chain::lengths(&self.prepared.query)?;
                let expected_targets = stage
                    .chain_targets
                    .iter()
                    .map(|m| m.words())
                    .collect::<Vec<_>>();
                if e.path.middle_target != stage.fields.middle.words()
                    || e.path.result_target != stage.fields.result.words()
                    || e.path.stage_targets != expected_targets
                    || e.path.stage_source_lengths != lengths
                {
                    return Err(ParallelError::InvalidWire(
                        "catalog witness stage identity mismatch",
                    ));
                }
                let future = if lengths.is_empty() {
                    stage.prepared.result_pieces
                } else {
                    stage.chain_targets[1..]
                        .iter()
                        .map(|t| t.count_ones() as usize / 4)
                        .sum()
                };
                if e.path.effective_max_early
                    != self
                        .prepared
                        .query
                        .early_limit
                        .effective_max(future, future)
                {
                    return Err(ParallelError::InvalidWire("catalog early policy mismatch"));
                }
                if e.first_pattern >= self.prepared.first.pattern_count()
                    || e.second_pattern >= self.prepared.second.pattern_count()
                    || e.path.status != status
                    || e.path.steps.len()
                        != packet.task.plan.middle.len() + packet.task.plan.result.len()
                {
                    return Err(ParallelError::InvalidWire("catalog example outside source"));
                }
                let ranked_first = self.prepared.first.sequence_at(e.first_pattern);
                let ranked_second = self.prepared.second.sequence_at(e.second_pattern);
                if e.first_queue.len() != usize::from(self.source.first_len)
                    || e.second_queue.as_slice() != ranked_second.as_ref()
                    || (self.prepared.stages.is_empty()
                        && e.first_queue.as_slice() != ranked_first.as_ref())
                    || (!self.prepared.stages.is_empty()
                        && !e.first_queue.starts_with(&ranked_first))
                {
                    return Err(ParallelError::InvalidWire(
                        "catalog witness source ranks differ",
                    ));
                }
                let rest = self.source.follow_first(&self.diagram, id, &e.first_queue);
                if !self
                    .source
                    .accepts_second(&self.diagram, rest, &e.second_queue)
                {
                    return Err(ParallelError::InvalidWire(
                        "catalog example outside accepted language",
                    ));
                }
                let mut before = true;
                let mut early = 0;
                let combined = e
                    .first_queue
                    .iter()
                    .chain(&e.second_queue)
                    .copied()
                    .collect::<Vec<_>>();
                for step in &e.path.steps {
                    if combined.get(step.source_index) != Some(&step.piece) {
                        return Err(ParallelError::InvalidWire("catalog example token mismatch"));
                    }
                    early += usize::from(before && step.result_target);
                    before &= !step.middle_complete;
                }
                if !lengths.is_empty() {
                    use clearra_core_domain::board::standard_pc_board::Board256Mask as Mask;
                    let mut boundaries = vec![0u8; lengths.len() - 1];
                    let mut used = Mask::EMPTY;
                    let mut source_used = vec![0usize; lengths.len()];
                    let mut balances = vec![[0i16; 7]; lengths.len()];
                    let mut tokens = std::collections::BTreeSet::new();
                    for step in &e.path.steps {
                        if !tokens.insert(step.source_index) {
                            return Err(ParallelError::InvalidWire("reused stage token"));
                        }
                        let mut cells = Mask::EMPTY;
                        // The established replay contract stores one 10-bit
                        // row mask per logical row, NOT a list of cell indices.
                        // Decode ownership in the shared pre-clear frame before
                        // matching a middle stage or counting boundary crossings.
                        for (y, &row) in step.logical_cells.iter().enumerate() {
                            if row & !1023 != 0
                                || (y >= usize::from(stage.fields.height) && row != 0)
                            {
                                return Err(ParallelError::InvalidWire(
                                    "catalog logical row outside chain field",
                                ));
                            }
                            for x in 0..10 {
                                if row & (1 << x) != 0 {
                                    cells = cells.union(
                                        Mask::singleton((y * 10 + x) as u16)
                                            .map_err(|_| Error::BoardOutsideField)?,
                                    );
                                }
                            }
                        }
                        let owner = stage
                            .chain_targets
                            .iter()
                            .position(|target| cells.without(*target).is_empty())
                            .ok_or(Error::PatternDomainUnavailable)?;
                        if cells.count_ones() != 4
                            || cells.intersects(used)
                            || step.result_target != (owner + 1 == lengths.len())
                        {
                            return Err(ParallelError::InvalidWire(
                                "catalog stage cell ownership mismatch",
                            ));
                        }
                        let mut end = 0usize;
                        let source = lengths
                            .iter()
                            .position(|len| {
                                end += usize::from(*len);
                                step.source_index < end
                            })
                            .ok_or(Error::PatternDomainUnavailable)?;
                        source_used[source] += 1;
                        let piece = super::search::piece_index(step.piece);
                        balances[source][piece] += 1;
                        balances[owner][piece] -= 1;
                        let mut prefix = Mask::EMPTY;
                        for (boundary, early_count) in boundaries.iter_mut().enumerate().take(owner)
                        {
                            prefix = prefix.union(stage.chain_targets[boundary]);
                            if !prefix.without(used).is_empty() {
                                *early_count =
                                    early_count.checked_add(1).ok_or(Error::CounterOverflow)?;
                            }
                        }
                        used = used.union(cells);
                    }
                    early = usize::from(boundaries.iter().copied().max().unwrap_or(0));
                    if boundaries != e.path.stage_early_counts
                        || early != e.path.actual_early
                        || ((status == Status::Normal) != (early == 0))
                        || source_used
                            .iter()
                            .zip(&stage.chain_targets)
                            .take(lengths.len() - 1)
                            .any(|(&n, t)| n != t.count_ones() as usize / 4)
                        || (!self.prepared.query.allow_piece_exchange
                            && balances.iter().any(|b| *b != [0; 7]))
                    {
                        return Err(ParallelError::InvalidWire(
                            "catalog stage boundary proof mismatch",
                        ));
                    }
                } else if !e.path.stage_early_counts.is_empty() {
                    return Err(ParallelError::InvalidWire("unexpected stage boundaries"));
                }
                if (self.prepared.stages.is_empty() && early != e.path.actual_early)
                    || e.path.actual_early > e.path.effective_max_early
                {
                    return Err(ParallelError::InvalidWire(
                        "catalog example violates early limit",
                    ));
                }
            }
        }
        if self.diagram.intersect(languages[0], languages[1])? != NONE {
            return Err(ParallelError::InvalidWire(
                "normal and repair overlap inside a catalog plan",
            ));
        }
        self.normal = self.diagram.union(self.normal, languages[0])?;
        self.recovery = self.diagram.union(self.recovery, languages[1])?;
        self.states = self
            .states
            .checked_add(packet.states)
            .ok_or(Error::CounterOverflow)?;
        self.finished += 1;
        self.issued.remove(&packet.task.ordinal);
        self.records.insert(
            packet.task.ordinal,
            Record {
                plan: packet.task.plan,
                languages,
                normal: packet.normal,
                recovery: packet.recovery,
            },
        );
        control.report_progress(
            "recovery-build-solutions",
            u64::try_from(self.finished).unwrap_or(u64::MAX),
            self.producer
                .done
                .then(|| u64::try_from(self.next).unwrap_or(u64::MAX)),
        );
        Ok(())
    }
    pub fn finish(
        mut self,
        control: &ExecutionControl,
    ) -> Result<RecoveryBuildPopulation, ParallelError> {
        cancelled(control)?;
        if !self.producer.done || !self.issued.is_empty() || self.finished != self.next {
            return Err(ParallelError::InvalidState(
                "catalog enumeration incomplete",
            ));
        }
        let recovery = self.diagram.difference(self.recovery, self.normal)?;
        let total = self.diagram.union(self.normal, recovery)?;
        let missing = self.diagram.difference(self.source.universe, total)?;
        let mut measures = [(0_u128, 0.0); 3];
        for (out, id) in measures.iter_mut().zip([self.normal, recovery, missing]) {
            *out = measure(&self.prepared, &self.source, &mut self.diagram, id, control)?;
        }
        if measures.iter().map(|m| m.0).sum::<u128>() != self.prepared.possible {
            return Err(Error::PatternDomainUnavailable.into());
        }
        let mut normal_example = None;
        let mut recovery_example = None;
        let mut recovery_plan = None;
        let mut solutions = Vec::new();
        let mut solution_languages = Vec::new();
        for record in self.records.values() {
            cancelled(control)?;
            if normal_example.is_none() {
                normal_example = record.normal.clone();
            }
            if recovery_plan.is_none()
                && self.diagram.intersect(record.languages[1], recovery)? != NONE
            {
                recovery_plan = Some(record.plan.clone());
                if let Some(e) = &record.recovery {
                    let rest = self
                        .source
                        .follow_first(&self.diagram, recovery, &e.first_queue);
                    if self
                        .source
                        .accepts_second(&self.diagram, rest, &e.second_queue)
                    {
                        recovery_example = Some(e.clone());
                    }
                }
            }
            let covered = self
                .diagram
                .union(record.languages[0], record.languages[1])?;
            if covered == NONE {
                continue;
            }
            let (covered_count, probability) = measure(
                &self.prepared,
                &self.source,
                &mut self.diagram,
                covered,
                control,
            )?;
            let example = record
                .normal
                .as_ref()
                .or(record.recovery.as_ref())
                .ok_or(Error::PatternDomainUnavailable)?
                .clone();
            solutions
                .try_reserve(1)
                .map_err(|_| Error::MemoryUnavailable)?;
            solution_languages.push(covered);
            solutions.push(RecoveryBuildSolution {
                key: record.plan.key(),
                covered_count,
                probability,
                example,
            });
        }
        // A per-plan repair example can be normal under another tiling. Only
        // the global set difference is an additional recovery probability.
        if recovery != NONE && recovery_example.is_none() {
            let plan = recovery_plan.ok_or(Error::PatternDomainUnavailable)?;
            let language = self
                .records
                .values()
                .find(|r| r.plan == plan)
                .ok_or(Error::PatternDomainUnavailable)?
                .languages[1];
            let language = self.diagram.intersect(language, recovery)?;
            let (i, j, first, second) = first_pair(
                &self.prepared,
                &self.source,
                &self.diagram,
                language,
                control,
            )?;
            let geometry = Geometry::new(&self.prepared.query, control)?;
            let mut solver = Solver::new(
                self.prepared.query.clone(),
                self.source.clone(),
                self.diagram.clone(),
                geometry,
            )
            .with_plan(plan);
            while !solver.advance(256, control)? {}
            let path = solver.witness(&first, &second, Status::Recovery, control)?;
            if path.status != Status::Recovery {
                return Err(Error::PatternDomainUnavailable.into());
            }
            recovery_example = Some(RecoveryBuildExample {
                first_pattern: i,
                second_pattern: j,
                first_queue: first,
                second_queue: second,
                path,
            });
        }
        Ok(RecoveryBuildPopulation {
            possible: self.prepared.possible,
            evaluated: self.prepared.possible,
            normal_count: measures[0].0,
            recovery_count: measures[1].0,
            no_path_count: measures[2].0,
            normal_probability: measures[0].1,
            recovery_probability: measures[1].1,
            no_path_probability: measures[2].1,
            states: self.states,
            normal_example,
            recovery_example,
            solutions,
            solutions_complete: true,
            coverage_classes: if self.prepared.query.minimum_solutions {
                Some(self.diagram.support_classes(&solution_languages, control)?)
            } else {
                None
            },
        })
    }
}
fn first_pair(
    prepared: &PreparedPopulation,
    source: &Source,
    diagram: &Diagram,
    mut language: Id,
    control: &ExecutionControl,
) -> Result<
    (
        usize,
        usize,
        Vec<clearra_core_domain::piece::piece_kind::PieceKind>,
        Vec<clearra_core_domain::piece::piece_kind::PieceKind>,
    ),
    Error,
> {
    use super::staged::source::PIECES;
    let mut queue = Vec::with_capacity(usize::from(source.end));
    for level in 0..source.end {
        cancelled(control)?;
        let piece = (0..7)
            .find(|&p| diagram.follow(language, level, p) != NONE)
            .ok_or(Error::PatternDomainUnavailable)?;
        language = diagram.follow(language, level, piece);
        queue.push(PIECES[piece]);
    }
    if language != super::staged::diagram::ALL {
        return Err(Error::PatternDomainUnavailable);
    }
    let first = queue[..usize::from(source.first_len)].to_vec();
    let second = queue[usize::from(source.first_len)..].to_vec();
    let first_rank_queue = if prepared.stages.is_empty() {
        first.as_slice()
    } else {
        &first[..prepared.first.sequence_len_at(0)]
    };
    let rank = |u: &clearra_supply::pattern_universe::MaterializedPatternUniverse,
                q: &[clearra_core_domain::piece::piece_kind::PieceKind]| {
        // This enumerates ONE source, never the Cartesian stage product.
        (0..u.pattern_count())
            .find(|&i| u.sequence_at(i).as_ref() == q)
            .ok_or(Error::PatternDomainUnavailable)
    };
    Ok((
        rank(&prepared.first, first_rank_queue)?,
        rank(&prepared.second, &second)?,
        first,
        second,
    ))
}
fn measure(
    prepared: &PreparedPopulation,
    source: &Source,
    diagram: &mut Diagram,
    language: Id,
    control: &ExecutionControl,
) -> Result<(u128, f64), Error> {
    if language == NONE {
        return Ok((0, 0.0));
    }
    if !prepared.stages.is_empty() {
        cancelled(control)?;
        let valid = diagram.intersect(language, source.universe)?;
        let count = diagram.count(valid, 0, source.end)?;
        return Ok((count, count as f64 / prepared.possible as f64));
    }
    let mut cache = HashMap::<Id, (u128, f64)>::new();
    let mut count = 0_u128;
    let mut sum = Sum::default();
    for i in 0..prepared.first.pattern_count() {
        cancelled(control)?;
        let rest = source.follow_first(diagram, language, &prepared.first.sequence_at(i));
        let value = if let Some(value) = cache.get(&rest) {
            *value
        } else {
            let value = if rest == NONE {
                (0, 0.0)
            } else if source.compact_second {
                let valid = diagram.intersect(rest, source.second)?;
                let n = diagram.count(valid, source.first_len, source.end)?;
                (n, n as f64 / prepared.second.pattern_count() as f64)
            } else {
                let mut n = 0;
                let mut weight = Sum::default();
                for j in 0..prepared.second.pattern_count() {
                    cancelled(control)?;
                    if source.accepts_second(diagram, rest, &prepared.second.sequence_at(j)) {
                        n += 1;
                        weight.add(prepared.second.weight_at(j).get());
                    }
                }
                (n, weight.sum)
            };
            cache.try_reserve(1).map_err(|_| Error::MemoryUnavailable)?;
            cache.insert(rest, value);
            value
        };
        count = count.checked_add(value.0).ok_or(Error::CounterOverflow)?;
        sum.add(prepared.first.weight_at(i).get() * value.1);
    }
    Ok((count, sum.sum.clamp(0.0, 1.0)))
}
struct Work {
    task: Task,
    solver: Solver,
}
pub(super) struct Worker {
    prepared: PreparedPopulation,
    init: Vec<u8>,
    blueprint: Option<(Source, Diagram)>,
    geometry: Option<Geometry>,
    pending: Option<Work>,
    progress: Progress,
}
impl Worker {
    pub fn new(query: RecoveryBuildQuery, init: Vec<u8>) -> Result<Self, Error> {
        Ok(Self {
            prepared: PreparedPopulation::new(query)?,
            init,
            blueprint: None,
            geometry: None,
            pending: None,
            progress: Progress::default(),
        })
    }
    pub fn has_pending_work(&self) -> bool {
        self.pending.is_some()
    }
    pub fn progress(&self) -> Progress {
        let mut p = self.progress;
        if let Some(work) = &self.pending {
            p.states = p.states.saturating_add(work.solver.states);
        }
        p
    }
    pub fn consume(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, ParallelError> {
        cancelled(control)?;
        if self.pending.is_some() {
            return Err(ParallelError::InvalidState("catalog worker busy"));
        }
        let task = wire::read_task(bytes, &self.init)?;
        if self.blueprint.is_none() {
            let mut d = Diagram::default();
            let s = Source::for_population(&mut d, &self.prepared, control)?;
            self.blueprint = Some((s, d));
        }
        let geometry = match self.geometry.take() {
            Some(g) => g,
            None => Geometry::new(&self.prepared.query, control)?,
        };
        task.plan.validate(&geometry)?;
        let (source, diagram) = self
            .blueprint
            .as_ref()
            .ok_or(Error::PatternDomainUnavailable)?;
        let solver = Solver::new(
            self.prepared.query.clone(),
            source.clone(),
            diagram.clone(),
            geometry,
        )
        .with_plan(task.plan.clone());
        self.pending = Some(Work { task, solver });
        self.progress.issued += 1;
        Ok(None)
    }
    pub fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, ParallelError> {
        if control.is_cancelled() {
            self.pending = None;
            return Err(Error::Cancelled.into());
        }
        let Some(work) = &mut self.pending else {
            return Ok(None);
        };
        if !work.solver.advance(256, control)? {
            return Ok(None);
        }
        let Work { task, mut solver } =
            self.pending.take().ok_or(Error::PatternDomainUnavailable)?;
        let mut examples = [None, None];
        for (slot, (language, status)) in examples.iter_mut().zip([
            (solver.normal, Status::Normal),
            (solver.recovery, Status::Recovery),
        ]) {
            if language == NONE {
                continue;
            }
            let (i, j, first, second) = first_pair(
                &self.prepared,
                &solver.source,
                &solver.diagram,
                language,
                control,
            )?;
            let path = solver.witness(&first, &second, status, control)?;
            *slot = Some(RecoveryBuildExample {
                first_pattern: i,
                second_pattern: j,
                first_queue: first,
                second_queue: second,
                path,
            });
        }
        let states = solver.states;
        let languages = solver.diagram.export([solver.normal, solver.recovery])?;
        let mut geometry = solver.geometry;
        geometry.retire_shard_cache()?;
        self.geometry = Some(geometry);
        self.progress.states = self
            .progress
            .states
            .checked_add(states)
            .ok_or(Error::CounterOverflow)?;
        self.progress.completed += 1;
        let [normal, recovery] = examples;
        Ok(Some(wire::result(
            &self.init,
            &ResultPacket {
                task,
                languages,
                states,
                normal,
                recovery,
            },
        )))
    }
}
