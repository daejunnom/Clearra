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
    RecoveryBuildError as Error, RecoveryBuildExample, RecoveryBuildFixedQuery,
    RecoveryBuildParallelError as ParallelError, RecoveryBuildParallelProduce as Produce,
    RecoveryBuildParallelProgress as Progress, RecoveryBuildPopulation, RecoveryBuildQuery,
    RecoveryBuildStatus as Status,
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
        let source =
            Source::compile_all(&mut diagram, &prepared.first, &prepared.second, &control)?;
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
        let variants = super::mirror::orientations(&self.prepared.query.fields)?;
        let target = variants
            .get(usize::from(packet.task.plan.orientation))
            .ok_or(ParallelError::InvalidWire("unknown catalog orientation"))?;
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
                if e.first_pattern >= self.prepared.first.pattern_count()
                    || e.second_pattern >= self.prepared.second.pattern_count()
                    || e.path.status != status
                    || e.path.middle_target != target.middle.words()
                    || e.path.result_target != target.result.words()
                    || e.path.steps.len()
                        != packet.task.plan.middle.len() + packet.task.plan.result.len()
                {
                    return Err(ParallelError::InvalidWire("catalog example outside source"));
                }
                e.first_queue = self.prepared.first.sequence_at(e.first_pattern).to_vec();
                e.second_queue = self.prepared.second.sequence_at(e.second_pattern).to_vec();
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
                if early != e.path.actual_early || early > e.path.effective_max_early {
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
            let (i, j) = first_pair(
                &self.prepared,
                &self.source,
                &self.diagram,
                language,
                control,
            )?;
            let fields = Geometry::new(&self.prepared.query, control)?.stages
                [usize::from(plan.orientation)]
            .fields
            .clone();
            let first = self.prepared.first.sequence_at(i).to_vec();
            let second = self.prepared.second.sequence_at(j).to_vec();
            let q = &self.prepared.query;
            let fixed = RecoveryBuildFixedQuery {
                fields: fields.clone(),
                first_supply: first.clone(),
                second_supply: second.clone(),
                early_limit: q.early_limit,
                allow_piece_exchange: q.allow_piece_exchange,
                hold_enabled: q.hold_enabled,
                preserve_b2b: q.preserve_b2b,
                initial_b2b: q.initial_b2b,
                rule_profile: q.rule_profile,
                spin_profile: q.spin_profile,
            };
            // Reconstruct one already-proved witness under this plan. This is
            // not used to manufacture or sample coverage.
            let prepared = fields.prepare()?;
            let mut physical_to_logical = Vec::new();
            let mut logical = 0usize;
            let full = crate::board::ForwardBoard::from_mask(fields.initial.union(fields.middle));
            for _ in 0..fields.height {
                while logical < usize::from(fields.height)
                    && full.row_bits(10, logical as u8) == 1023
                {
                    logical += 1;
                }
                physical_to_logical.push(logical);
                logical += 1;
            }
            let accepts = |piece: clearra_core_domain::piece::piece_kind::PieceKind,
                           result: bool,
                           rows: &[u16]| {
                let tiles = if result { &plan.result } else { &plan.middle };
                tiles.iter().any(|t| {
                    t.piece as usize == super::search::piece_index(piece) && {
                        let mask = crate::board::ForwardBoard::from_words(t.cells);
                        let mut expected = vec![0_u16; prepared.middle.len()];
                        for y in 0..fields.height {
                            let logical = if result {
                                physical_to_logical[usize::from(y)]
                            } else {
                                usize::from(y)
                            };
                            expected[logical] = mask.row_bits(10, y);
                        }
                        expected == rows
                    }
                })
            };
            let path = fixed.search_with_filter(control, &accepts)?;
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
    language: Id,
    control: &ExecutionControl,
) -> Result<(usize, usize), Error> {
    for i in 0..prepared.first.pattern_count() {
        cancelled(control)?;
        let rest = source.follow_first(diagram, language, &prepared.first.sequence_at(i));
        if rest == NONE {
            continue;
        }
        for j in 0..prepared.second.pattern_count() {
            if source.accepts_second(diagram, rest, &prepared.second.sequence_at(j)) {
                return Ok((i, j));
            }
        }
    }
    Err(Error::PatternDomainUnavailable)
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
            let s =
                Source::compile_all(&mut d, &self.prepared.first, &self.prepared.second, control)?;
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
            let (i, j) = first_pair(
                &self.prepared,
                &solver.source,
                &solver.diagram,
                language,
                control,
            )?;
            let first = self.prepared.first.sequence_at(i).to_vec();
            let second = self.prepared.second.sequence_at(j).to_vec();
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
