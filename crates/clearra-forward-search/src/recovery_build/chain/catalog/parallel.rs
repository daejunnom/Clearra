//! Bounded logical-plan workers. Every worker verifies the complete continuous
//! source/hold/board execution; results carry exact input-language DAGs, never
//! stage probabilities to multiply. The existing host owns worker threads.
use super::*;
use crate::recovery_build::staged::diagram::DiagramPacket;
mod wire;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Task {
    ordinal: u128,
    plan: Plan,
}
struct ResultPacket {
    task: Task,
    languages: DiagramPacket,
    states: u128,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryChainProduce {
    Pending,
    Batch(Vec<u8>),
    Completed,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryChainProgress {
    pub possible_patterns: u128,
    /// Work units are complete tiling plans, not input tuples or probabilities.
    pub issued_plans: u128,
    pub completed_plans: u128,
    pub states: u128,
}

pub struct RecoveryChainCoordinator {
    session: RecoveryChainCatalogSession,
    initialization: Vec<u8>,
    pending: BTreeMap<u128, Plan>,
    next: u128,
    completed: u128,
    capacity: usize,
}
impl RecoveryChainCoordinator {
    pub fn new(
        query: RecoveryChainQuery,
        workers: usize,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        if workers == 0 {
            return Err(Error::InvalidState("zero chain workers"));
        }
        let capacity = workers.checked_mul(4).ok_or(Core::CounterOverflow)?;
        let initialization = wire::initialization(&query);
        Ok(Self {
            session: RecoveryChainCatalogSession::new(query, control)?,
            initialization,
            pending: BTreeMap::new(),
            next: 0,
            completed: 0,
            capacity,
        })
    }
    pub fn worker_initialization(&self) -> Vec<u8> {
        self.initialization.clone()
    }
    pub fn progress(&self) -> RecoveryChainProgress {
        RecoveryChainProgress {
            possible_patterns: self.session.sources.possible,
            issued_plans: self.next,
            completed_plans: self.completed,
            states: self.session.states,
        }
    }
    pub fn has_pending_preparation(&self) -> bool {
        !self.session.producer.done
    }
    pub fn produce(
        &mut self,
        fuel: usize,
        control: &ExecutionControl,
    ) -> Result<RecoveryChainProduce, Error> {
        cancelled(control)?;
        if self.session.producer.done {
            self.session.done = self.pending.is_empty();
            return Ok(if self.session.done {
                RecoveryChainProduce::Completed
            } else {
                RecoveryChainProduce::Pending
            });
        }
        if self.pending.len() >= self.capacity {
            return Ok(RecoveryChainProduce::Pending);
        }
        for _ in 0..fuel.max(1) {
            cancelled(control)?;
            if let Some(plan) = self.session.producer.advance(1, control)? {
                let key = plan.key();
                if self.session.seen.contains(&key) {
                    continue;
                }
                self.session
                    .seen
                    .try_reserve(1)
                    .map_err(|_| Core::MemoryUnavailable)?;
                self.session.seen.insert(key);
                let task = Task {
                    ordinal: self.next,
                    plan,
                };
                let packet = wire::task(&self.initialization, &task);
                self.next = self.next.checked_add(1).ok_or(Core::CounterOverflow)?;
                self.pending.insert(task.ordinal, task.plan);
                return Ok(RecoveryChainProduce::Batch(packet));
            }
            if self.session.producer.done {
                self.session.done = self.pending.is_empty();
                return Ok(if self.session.done {
                    RecoveryChainProduce::Completed
                } else {
                    RecoveryChainProduce::Pending
                });
            }
        }
        Ok(RecoveryChainProduce::Pending)
    }
    pub fn absorb(&mut self, bytes: &[u8], control: &ExecutionControl) -> Result<(), Error> {
        cancelled(control)?;
        let packet = wire::read_result(bytes, &self.initialization, &self.session.query)?;
        let expected = self
            .pending
            .get(&packet.task.ordinal)
            .ok_or(Error::InvalidState("unknown or completed chain task"))?;
        if expected != &packet.task.plan {
            return Err(Error::InvalidPacket("chain task plan changed"));
        }
        let [normal, all] = self
            .session
            .diagram
            .import(&packet.languages, self.session.sources.end())?;
        if self.session.diagram.difference(normal, all)? != NONE
            || self
                .session
                .diagram
                .difference(all, self.session.sources.universe)?
                != NONE
            || (self.session.query.early_limit == CrossStageEarlyLimit::AtMost(0) && normal != all)
        {
            return Err(Error::InvalidPacket(
                "chain result language outside its bound universe",
            ));
        }
        let repair = self.session.diagram.difference(all, normal)?;
        // Recover only concrete, already-proved examples on the coordinator.
        // No untrusted worker replay is promoted and no full queue is expanded.
        let normal_example = self.witness(&packet.task.plan, normal, false, control)?;
        let recovery_example = self.witness(&packet.task.plan, repair, true, control)?;
        let states = self
            .session
            .states
            .checked_add(packet.states)
            .ok_or(Core::CounterOverflow)?;
        let merged_normal = self.session.diagram.union(self.session.normal, normal)?;
        let merged_all = self.session.diagram.union(self.session.all, all)?;
        if all != NONE {
            let key = packet.task.plan.key();
            if self.session.records.contains_key(&key) {
                return Err(Error::InvalidState("duplicate chain solution"));
            }
            self.session.records.insert(
                key,
                Record {
                    plan: packet.task.plan,
                    all,
                    normal_example,
                    recovery_example,
                },
            );
        }
        self.session.normal = merged_normal;
        self.session.all = merged_all;
        self.session.states = states;
        self.completed = self.completed.checked_add(1).ok_or(Core::CounterOverflow)?;
        self.pending.remove(&packet.task.ordinal);
        self.session.done = self.session.producer.done && self.pending.is_empty();
        control.report_progress(
            "recovery-chain-plans",
            u64::try_from(self.completed).unwrap_or(u64::MAX),
            None,
        );
        Ok(())
    }
    fn witness(
        &mut self,
        plan: &Plan,
        language: Id,
        repair: bool,
        control: &ExecutionControl,
    ) -> Result<Option<RecoveryChainWitness>, Error> {
        if language == NONE {
            return Ok(None);
        }
        let queues = self
            .session
            .sources
            .first(&self.session.diagram, language)?;
        let mut query = self.session.query.clone();
        query.targets = plan.targets.clone();
        query.supplies = queues
            .iter()
            .map(|q| q.iter().map(|p| p.as_ascii()).collect())
            .collect();
        let mut diagram = Diagram::default();
        let sources = Sources::compile(&query, &mut diagram, control)?;
        let mut solver = Solver::for_plan(&query, plan.clone(), repair, &sources, control)?;
        while !solver.advance(256, &mut diagram, &sources, control)? {}
        solver
            .witness(
                &mut diagram,
                &sources,
                queues,
                if repair {
                    Status::Recovery
                } else {
                    Status::Normal
                },
                control,
            )
            .map(Some)
    }
    pub fn finish(self, control: &ExecutionControl) -> Result<RecoveryChainCatalog, Error> {
        cancelled(control)?;
        if !self.pending.is_empty() || self.completed != self.next {
            return Err(Error::Incomplete);
        }
        self.session.finish(control)
    }
}

struct Work {
    task: Task,
    diagram: Diagram,
    sources: Sources,
    solver: Option<Solver>,
    normal: Id,
    states: u128,
}
impl Work {
    fn new(
        query: &RecoveryChainQuery,
        task: Task,
        control: &ExecutionControl,
    ) -> Result<Self, Error> {
        task.plan.validate(query)?;
        let mut diagram = Diagram::default();
        let sources = Sources::compile(query, &mut diagram, control)?;
        let solver = Some(Solver::for_plan(
            query,
            task.plan.clone(),
            false,
            &sources,
            control,
        )?);
        Ok(Self {
            task,
            diagram,
            sources,
            solver,
            normal: NONE,
            states: 0,
        })
    }
    fn advance(
        &mut self,
        query: &RecoveryChainQuery,
        fuel: usize,
        control: &ExecutionControl,
    ) -> Result<Option<ResultPacket>, Error> {
        for _ in 0..fuel.max(1) {
            cancelled(control)?;
            let solver = self
                .solver
                .as_mut()
                .ok_or(Error::InvalidState("completed chain work reused"))?;
            if !solver.advance(1, &mut self.diagram, &self.sources, control)? {
                continue;
            }
            let solver = self.solver.take().ok_or(Error::Incomplete)?;
            let language = solver.result().ok_or(Error::Incomplete)?;
            self.states = self
                .states
                .checked_add(solver.states)
                .ok_or(Core::CounterOverflow)?;
            if !solver.repair {
                self.normal = language;
                if query.early_limit != CrossStageEarlyLimit::AtMost(0) {
                    self.solver = Some(Solver::for_plan(
                        query,
                        self.task.plan.clone(),
                        true,
                        &self.sources,
                        control,
                    )?);
                    continue;
                }
            }
            let all = self.diagram.union(self.normal, language)?;
            let languages = self.diagram.export([self.normal, all])?;
            return Ok(Some(ResultPacket {
                task: self.task.clone(),
                languages,
                states: self.states,
            }));
        }
        Ok(None)
    }
}
/// Reusable value-packet worker. A completed task drops its DAG and memo arena;
/// cache reclamation never converts an unfinished task into a negative result.
pub struct RecoveryChainWorker {
    query: RecoveryChainQuery,
    initialization: Vec<u8>,
    work: Option<Work>,
}
impl RecoveryChainWorker {
    pub fn new(initialization: &[u8]) -> Result<Self, Error> {
        Ok(Self {
            query: wire::read_initialization(initialization)?,
            initialization: initialization.to_vec(),
            work: None,
        })
    }
    pub fn has_pending_work(&self) -> bool {
        self.work.is_some()
    }
    pub fn consume(
        &mut self,
        bytes: &[u8],
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, Error> {
        cancelled(control)?;
        if self.work.is_some() {
            return Err(Error::InvalidState("busy chain worker"));
        }
        let task = wire::read_task(bytes, &self.initialization, &self.query)?;
        self.work = Some(Work::new(&self.query, task, control)?);
        self.advance(256, control)
    }
    pub fn advance(
        &mut self,
        fuel: usize,
        control: &ExecutionControl,
    ) -> Result<Option<Vec<u8>>, Error> {
        let mut work = self
            .work
            .take()
            .ok_or(Error::InvalidState("idle chain worker"))?;
        // Taking the work before cancellation/error gives an unambiguous owner
        // and releases all task-local memory on either abnormal exit.
        cancelled(control)?;
        match work.advance(&self.query, fuel, control)? {
            Some(packet) => Ok(Some(wire::result(&self.initialization, &packet))),
            None => {
                self.work = Some(work);
                Ok(None)
            }
        }
    }
}
