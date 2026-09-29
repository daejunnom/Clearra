//! Bounded tiling tasks shared by the native pool and browser worker protocol.
//! Task results overlap in input space; only exact language unions are counted.
use super::super::{
    staged::diagram::{Diagram, Id, NONE},
    RecoveryBuildExample, RecoveryBuildFixedReport, RecoveryBuildParallelError as E,
    RecoveryBuildParallelProduce as Produce, RecoveryBuildParallelProgress as Progress,
    RecoveryBuildPopulation, RecoveryBuildSolution, RecoveryBuildStatus,
};
use super::*;
use super::{
    plan::{Plan, Producer},
    solver::Solver,
    source::Source,
    wire::{self, ResultPacket},
};
use std::collections::BTreeMap;
struct Record {
    plan: Plan,
    languages: [Id; 2],
    paths: [Option<RecoveryBuildFixedReport>; 2],
}
pub(in crate::recovery_build) struct Coordinator {
    query: RecoveryBuildQuery,
    init: Vec<u8>,
    diagram: Diagram,
    source: Source,
    producer: Producer,
    next: u128,
    finished: u128,
    states: u128,
    capacity: usize,
    issued: BTreeMap<u128, Vec<u8>>,
    records: BTreeMap<String, Record>,
    normal: Id,
    recovery: Id,
}
impl Coordinator {
    pub fn new(query: RecoveryBuildQuery, workers: usize) -> Result<Self, E> {
        let control = ExecutionControl::default();
        let mut diagram = Diagram::default();
        let source = Source::new(&query, &mut diagram, &control)?;
        let producer = Producer::new(query.clone(), &source);
        let init = super::super::parallel::wire::encode_initialization(&query);
        Ok(Self {
            query,
            init,
            diagram,
            source,
            producer,
            next: 0,
            finished: 0,
            states: 0,
            capacity: workers.saturating_mul(4).max(1),
            issued: BTreeMap::new(),
            records: BTreeMap::new(),
            normal: NONE,
            recovery: NONE,
        })
    }
    pub fn worker_initialization(&self) -> Vec<u8> {
        self.init.clone()
    }
    pub fn has_pending_preparation(&self) -> bool {
        !self.producer.done
    }
    pub fn progress(&self) -> Progress {
        Progress {
            possible: if self.producer.done { self.next } else { 0 },
            issued: self.next,
            completed: self.finished,
            states: self.states,
        }
    }
    pub fn produce(
        &mut self,
        _maximum: usize,
        control: &ExecutionControl,
    ) -> Result<(Produce, Vec<u8>), E> {
        if control.is_cancelled() {
            return Ok((Produce::Cancelled, Vec::new()));
        }
        if self.producer.done {
            return Ok((
                if self.issued.is_empty() {
                    Produce::Completed
                } else {
                    Produce::Pending
                },
                Vec::new(),
            ));
        }
        if self.issued.len() >= self.capacity {
            return Ok((Produce::Pending, Vec::new()));
        }
        if let Some(plan) = self.producer.advance(control)? {
            let task = wire::task(&self.init, self.next, &plan);
            self.issued.insert(self.next, task.clone());
            self.next = self.next.checked_add(1).ok_or(Error::CounterOverflow)?;
            return Ok((Produce::Batch, task));
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
    pub fn absorb(&mut self, bytes: &[u8], control: &ExecutionControl) -> Result<(), E> {
        cancelled(control)?;
        let packet = wire::read_result(bytes)?;
        let (ordinal, plan) = wire::read_task(&packet.task, &self.init)?;
        if self.issued.get(&ordinal) != Some(&packet.task) {
            return Err(E::InvalidWire("unknown, duplicate or modified chain task"));
        }
        plan.validate(&self.query)?;
        let ids = self.diagram.import(&packet.diagram, self.source.end())?;
        if self.diagram.intersect(ids[0], ids[1])? != NONE {
            return Err(E::InvalidWire("chain normal and recovery overlap"));
        }
        for (kind, (&id, path)) in ids.iter().zip(&packet.paths).enumerate() {
            if self.diagram.difference(id, self.source.universe)? != NONE
                || (id == NONE) != path.is_none()
            {
                return Err(E::InvalidWire(
                    "chain result outside source or missing witness",
                ));
            }
            if let Some(path) = path {
                let chain = path
                    .chain
                    .as_ref()
                    .ok_or(E::InvalidWire("missing chain witness"))?;
                if chain.targets != plan.targets.iter().map(|m| m.words()).collect::<Vec<_>>()
                    || chain.placement_stages.len() != path.steps.len()
                    || path.steps.len() != plan.pieces()
                    || !self.source.accepts(&self.diagram, id, &chain.queues)
                    || path.status
                        != if kind == 0 {
                            RecoveryBuildStatus::Normal
                        } else {
                            RecoveryBuildStatus::Recovery
                        }
                    || self.source.indices(&chain.queues, control)? != chain.pattern_indices
                {
                    return Err(E::InvalidWire("chain witness binding mismatch"));
                }
                verify_path(&self.query, &plan, path, control)?;
            }
        }
        self.normal = self.diagram.union(self.normal, ids[0])?;
        self.recovery = self.diagram.union(self.recovery, ids[1])?;
        self.states = self
            .states
            .checked_add(packet.states)
            .ok_or(Error::CounterOverflow)?;
        let key = plan.key();
        if let Some(r) = self.records.get_mut(&key) {
            for i in 0..2 {
                r.languages[i] = self.diagram.union(r.languages[i], ids[i])?;
                if r.paths[i].is_none() {
                    r.paths[i] = packet.paths[i].clone();
                }
            }
        } else {
            self.records.insert(
                key,
                Record {
                    plan,
                    languages: ids,
                    paths: packet.paths,
                },
            );
        }
        self.issued.remove(&ordinal);
        self.finished += 1;
        control.report_progress(
            "recovery-build-solutions",
            u64::try_from(self.finished).unwrap_or(u64::MAX),
            self.producer
                .done
                .then(|| u64::try_from(self.next).unwrap_or(u64::MAX)),
        );
        Ok(())
    }
    fn example(
        &mut self,
        kind: usize,
        language: Id,
        control: &ExecutionControl,
    ) -> Result<Option<RecoveryBuildExample>, E> {
        if language == NONE {
            return Ok(None);
        }
        let queues = self.source.queues(&self.diagram, language)?;
        let record = self
            .records
            .values()
            .find(|r| {
                self.source
                    .accepts(&self.diagram, r.languages[kind], &queues)
            })
            .ok_or(Error::PatternDomainUnavailable)?;
        let path = if let Some(path) = record.paths[kind]
            .as_ref()
            .filter(|p| p.chain.as_ref().is_some_and(|c| c.queues == queues))
        {
            path.clone()
        } else {
            let mut solver = Solver::new(self.query.clone(), record.plan.clone(), control)?;
            solver.restrict(&queues)?;
            while !solver.advance(control)? {}
            solver.witness(queues, kind, control)?
        };
        Ok(Some(example(path)?))
    }
    pub fn finish(mut self, control: &ExecutionControl) -> Result<RecoveryBuildPopulation, E> {
        cancelled(control)?;
        if !self.producer.done || !self.issued.is_empty() || self.next != self.finished {
            return Err(E::InvalidState("chain catalog incomplete"));
        }
        let recovery = self.diagram.difference(self.recovery, self.normal)?;
        let total = self.diagram.union(self.normal, recovery)?;
        let missing = self.diagram.difference(self.source.universe, total)?;
        let normal = self
            .source
            .measure(&mut self.diagram, self.normal, control)?;
        let repaired = self.source.measure(&mut self.diagram, recovery, control)?;
        let no_path = self.source.measure(&mut self.diagram, missing, control)?;
        if normal
            .0
            .checked_add(repaired.0)
            .and_then(|v| v.checked_add(no_path.0))
            != Some(self.source.possible)
            || (normal.1 + repaired.1 + no_path.1 - 1.0).abs() > 1e-9
        {
            return Err(Error::PatternDomainUnavailable.into());
        }
        let normal_example = self.example(0, self.normal, control)?;
        let recovery_example = self.example(1, recovery, control)?;
        let mut solutions = Vec::new();
        let mut languages = Vec::new();
        for (key, record) in self.records {
            let id = self
                .diagram
                .union(record.languages[0], record.languages[1])?;
            if id == NONE {
                continue;
            }
            let (count, probability) = self.source.measure(&mut self.diagram, id, control)?;
            if self.query.all_solutions {
                let path = record.paths[0]
                    .clone()
                    .or(record.paths[1].clone())
                    .ok_or(Error::PatternDomainUnavailable)?;
                solutions.push(RecoveryBuildSolution {
                    key,
                    covered_count: count,
                    probability,
                    example: example(path)?,
                });
                languages.push(id);
            }
        }
        let coverage_classes = if self.query.minimum_solutions {
            Some(self.diagram.support_classes(&languages, control)?)
        } else {
            None
        };
        Ok(RecoveryBuildPopulation {
            solutions,
            solutions_complete: self.query.all_solutions,
            coverage_classes,
            possible: self.source.possible,
            evaluated: self.source.possible,
            normal_count: normal.0,
            recovery_count: repaired.0,
            no_path_count: no_path.0,
            states: self.states,
            normal_probability: normal.1,
            recovery_probability: repaired.1,
            no_path_probability: no_path.1,
            normal_example,
            recovery_example,
        })
    }
}
fn example(path: RecoveryBuildFixedReport) -> Result<RecoveryBuildExample, Error> {
    let chain = path.chain.as_ref().ok_or(Error::PatternDomainUnavailable)?;
    Ok(RecoveryBuildExample {
        first_pattern: chain.pattern_indices[0],
        second_pattern: chain.pattern_indices[1],
        first_queue: chain.queues[0].clone(),
        second_queue: chain.queues[1].clone(),
        path,
    })
}
struct Pending {
    task: Vec<u8>,
    solver: Solver,
}
pub(in crate::recovery_build) struct Worker {
    query: RecoveryBuildQuery,
    init: Vec<u8>,
    pending: Option<Pending>,
    progress: Progress,
}
impl Worker {
    pub fn new(init: &[u8]) -> Result<Self, E> {
        let query = super::super::parallel::wire::decode_initialization(init)?;
        validate(&query)?;
        Ok(Self {
            query,
            init: init.to_vec(),
            pending: None,
            progress: Progress::default(),
        })
    }
    pub fn has_pending_work(&self) -> bool {
        self.pending.is_some()
    }
    pub fn progress(&self) -> Progress {
        let mut p = self.progress;
        if let Some(v) = &self.pending {
            p.states = p.states.saturating_add(v.solver.states);
        }
        p
    }
    pub fn consume(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, E> {
        cancelled(control)?;
        if self.pending.is_some() {
            return Err(E::InvalidState("chain worker busy"));
        }
        let (_, plan) = wire::read_task(bytes, &self.init)?;
        let solver = Solver::new(self.query.clone(), plan, control)?;
        self.progress.issued = self
            .progress
            .issued
            .checked_add(1)
            .ok_or(Error::CounterOverflow)?;
        self.pending = Some(Pending {
            task: bytes.to_vec(),
            solver,
        });
        Ok(None)
    }
    pub fn advance(&mut self, control: &ExecutionControl) -> Result<Option<Vec<u8>>, E> {
        if control.is_cancelled() {
            self.pending = None;
            return Err(Error::Cancelled.into());
        }
        let Some(pending) = &mut self.pending else {
            return Ok(None);
        };
        if !pending.solver.advance(control)? {
            return Ok(None);
        }
        let mut pending = self.pending.take().ok_or(Error::PatternDomainUnavailable)?;
        let ids = pending
            .solver
            .result
            .ok_or(Error::PatternDomainUnavailable)?;
        let ids = [ids[0], pending.solver.diagram.difference(ids[1], ids[0])?];
        let diagram = pending.solver.diagram.export(ids)?;
        let mut paths = [None, None];
        for (kind, &id) in ids.iter().enumerate() {
            if id != NONE {
                let queues = pending.solver.source.queues(&pending.solver.diagram, id)?;
                paths[kind] = Some(pending.solver.witness(queues, kind, control)?);
            }
        }
        self.progress.completed += 1;
        self.progress.states = self
            .progress
            .states
            .checked_add(pending.solver.states)
            .ok_or(Error::CounterOverflow)?;
        Ok(Some(wire::result(&ResultPacket {
            task: pending.task,
            states: pending.solver.states,
            diagram,
            paths,
        })))
    }
}
/// Replays the exported word, hold decisions, stage ownership and actual locks.
/// This validates the packet's witness only; it does not recompute coverage.
fn verify_path(
    q: &RecoveryBuildQuery,
    plan: &Plan,
    path: &RecoveryBuildFixedReport,
    control: &ExecutionControl,
) -> Result<(), E> {
    let bad = || E::InvalidWire("invalid chain replay");
    let c = path.chain.as_ref().ok_or_else(bad)?;
    let queue = c.queues.iter().flatten().copied().collect::<Vec<_>>();
    let mut offsets = vec![0];
    for v in &c.queues {
        offsets.push(offsets.last().copied().unwrap_or(0) + v.len());
    }
    let mut next = 0;
    let mut held = None;
    let mut used = vec![0_usize; plan.groups.len()];
    let mut inventory = vec![[0_u8; 7]; plan.groups.len()];
    let mut early = vec![0_u8; plan.groups.len() - 1];
    let mut placed = 0_u64;
    let mut b2b = q.initial_b2b;
    let mut g = super::geometry::Geometry::new(q.clone(), plan.clone())?;
    for (s, &stage) in path.steps.iter().zip(&c.placement_stages) {
        cancelled(control)?;
        let current = if next < queue.len() {
            let i = next;
            next += 1;
            Some(i)
        } else {
            None
        };
        let selected = match (s.hold_decision, current, held) {
            ("none", Some(i), _) => i,
            ("swap", Some(i), Some(h)) if q.hold_enabled => {
                held = Some(i);
                h
            }
            ("store", Some(i), None) if q.hold_enabled && next < queue.len() => {
                held = Some(i);
                let j = next;
                next += 1;
                j
            }
            ("release-held-at-terminal", None, Some(h)) => {
                held = None;
                h
            }
            _ => return Err(bad()),
        };
        if selected != s.source_index || queue.get(selected) != Some(&s.piece) {
            return Err(bad());
        }
        let origin = offsets.partition_point(|&v| v <= selected) - 1;
        used[origin] += 1;
        inventory[origin][super::super::search::piece_index(s.piece)] += 1;
        let pos = g.position(placed)?;
        let edge = g
            .edges(
                placed,
                super::super::search::piece_index(s.piece) as u8,
                control,
            )?
            .iter()
            .copied()
            .find(|e| {
                e.stage == usize::from(stage)
                    && e.lock.mask.words() == s.placement
                    && e.lock.rotation.quarter_turns() == s.rotation
                    && e.lock.x == s.x
                    && e.lock.y == s.y
            })
            .ok_or_else(bad)?;
        for b in pos.prefix..edge.stage {
            early[b] = early[b].checked_add(1).ok_or_else(bad)?;
            let n = plan.groups[b + 1..].iter().map(Vec::len).sum();
            if usize::from(early[b]) > q.early_limit.effective_max(n, n) {
                return Err(bad());
            }
        }
        if edge.lines > 0 {
            b2b = edge.lines == 4 || edge.board.is_empty() || edge.spin;
        }
        let expected = g.step(
            placed,
            edge,
            selected as u16,
            super::super::search::piece_index(s.piece) as u8,
            s.hold_decision,
            b2b,
        )?;
        if &expected != s {
            return Err(bad());
        }
        placed |= 1 << edge.tile;
    }
    if placed != g.all()
        || early != c.early_by_boundary
        || path.actual_early != usize::from(*early.iter().max().unwrap_or(&0))
        || used.iter().zip(&plan.groups).any(|(&n, g)| n != g.len())
        || (!q.allow_piece_exchange && inventory != plan.counts())
        || g.position(placed)?.board.words() != path.terminal_board
        || (path.status == RecoveryBuildStatus::Normal) != early.iter().all(|&n| n == 0)
    {
        return Err(bad());
    }
    Ok(())
}
