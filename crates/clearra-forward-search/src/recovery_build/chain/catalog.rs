//! All logical tilings with exact symbolic source coverage. This is deliberately
//! separate from the aggregate reference solver: first-witness coverage cannot
//! be used to enumerate the solution catalog or prove a minimum portfolio.
mod parallel;
use super::{
    plan::{Plan, Producer},
    solver::Solver,
    source::Sources,
    RecoveryChainCoverage, RecoveryChainError as Error, RecoveryChainQuery, RecoveryChainWitness,
};
use crate::{
    recovery_build::{
        staged::{
            diagram::{Diagram, Id, NONE},
            source::cancelled,
        },
        RecoveryBuildError as Core, RecoveryBuildStatus as Status,
    },
    CrossStageEarlyLimit,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
pub use parallel::{
    RecoveryChainCoordinator, RecoveryChainProduce, RecoveryChainProgress, RecoveryChainWorker,
};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryChainSolution {
    pub key: String,
    pub covered_count: u128,
    pub probability: f64,
    pub example: RecoveryChainWitness,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryChainCatalog {
    pub input: RecoveryChainQuery,
    pub coverage: RecoveryChainCoverage,
    pub solutions: Vec<RecoveryChainSolution>,
    /// Exact, nonempty supporter sets over the COMPLETE source universe. These
    /// are minimum-cover constraints, not equally weighted probability events.
    pub coverage_classes: Vec<Vec<usize>>,
    pub complete: bool,
}
struct Record {
    plan: Plan,
    all: Id,
    normal_example: Option<RecoveryChainWitness>,
    recovery_example: Option<RecoveryChainWitness>,
}
struct Current {
    plan: Plan,
    normal: Id,
    normal_example: Option<RecoveryChainWitness>,
}

/// Cooperative catalog preparation. Every geometry candidate receives its own
/// exact execution-language verification. No queue product is materialized.
pub struct RecoveryChainCatalogSession {
    query: RecoveryChainQuery,
    diagram: Diagram,
    sources: Sources,
    producer: Producer,
    seen: HashSet<String>,
    records: BTreeMap<String, Record>,
    current: Option<Current>,
    solver: Option<Solver>,
    normal: Id,
    all: Id,
    states: u128,
    done: bool,
}
impl RecoveryChainCatalogSession {
    pub fn new(query: RecoveryChainQuery, control: &ExecutionControl) -> Result<Self, Error> {
        query.validate()?;
        cancelled(control)?;
        let mut diagram = Diagram::default();
        let sources = Sources::compile(&query, &mut diagram, control)?;
        let producer = Producer::new(&query, &sources)?;
        Ok(Self {
            query,
            diagram,
            sources,
            producer,
            seen: HashSet::new(),
            records: BTreeMap::new(),
            current: None,
            solver: None,
            normal: NONE,
            all: NONE,
            states: 0,
            done: false,
        })
    }
    pub fn is_complete(&self) -> bool {
        self.done
    }
    pub fn visited_states(&self) -> u128 {
        self.states + self.solver.as_ref().map_or(0, |s| s.states)
    }
    pub fn advance(&mut self, fuel: usize, control: &ExecutionControl) -> Result<bool, Error> {
        for _ in 0..fuel.max(1) {
            cancelled(control)?;
            if self.done {
                return Ok(true);
            }
            if let Some(solver) = &mut self.solver {
                if !solver.advance(1, &mut self.diagram, &self.sources, control)? {
                    continue;
                }
                let mut solver = self.solver.take().ok_or(Error::Incomplete)?;
                let language = solver.result().ok_or(Error::Incomplete)?;
                self.states = self
                    .states
                    .checked_add(solver.states)
                    .ok_or(Core::CounterOverflow)?;
                let mut current = self.current.take().ok_or(Error::Incomplete)?;
                if !solver.repair {
                    current.normal = language;
                    if language != NONE {
                        let queues = self.sources.first(&self.diagram, language)?;
                        current.normal_example = Some(solver.witness(
                            &mut self.diagram,
                            &self.sources,
                            queues,
                            Status::Normal,
                            control,
                        )?);
                    }
                    if self.query.early_limit != CrossStageEarlyLimit::AtMost(0) {
                        self.solver = Some(Solver::for_plan(
                            &self.query,
                            current.plan.clone(),
                            true,
                            &self.sources,
                            control,
                        )?);
                        self.current = Some(current);
                        continue;
                    }
                }
                let all = self.diagram.union(current.normal, language)?;
                let recovery = self.diagram.difference(all, current.normal)?;
                let recovery_example = if recovery != NONE {
                    let queues = self.sources.first(&self.diagram, recovery)?;
                    Some(solver.witness(
                        &mut self.diagram,
                        &self.sources,
                        queues,
                        Status::Recovery,
                        control,
                    )?)
                } else {
                    None
                };
                if all != NONE {
                    self.normal = self.diagram.union(self.normal, current.normal)?;
                    self.all = self.diagram.union(self.all, all)?;
                    self.records.insert(
                        current.plan.key(),
                        Record {
                            plan: current.plan,
                            all,
                            normal_example: current.normal_example,
                            recovery_example,
                        },
                    );
                }
                continue;
            }
            if let Some(plan) = self.producer.advance(1, control)? {
                let key = plan.key();
                if self.seen.contains(&key) {
                    continue;
                }
                self.seen
                    .try_reserve(1)
                    .map_err(|_| Core::MemoryUnavailable)?;
                self.seen.insert(key);
                self.solver = Some(Solver::for_plan(
                    &self.query,
                    plan.clone(),
                    false,
                    &self.sources,
                    control,
                )?);
                self.current = Some(Current {
                    plan,
                    normal: NONE,
                    normal_example: None,
                });
            } else if self.producer.done {
                self.done = true;
            }
        }
        control.report_progress(
            "recovery-chain-catalog",
            u64::try_from(self.visited_states()).unwrap_or(u64::MAX),
            None,
        );
        Ok(self.done)
    }
    pub fn finish(mut self, control: &ExecutionControl) -> Result<RecoveryChainCatalog, Error> {
        cancelled(control)?;
        if !self.done || self.current.is_some() || self.solver.is_some() {
            return Err(Error::Incomplete);
        }
        let repair = self.diagram.difference(self.all, self.normal)?;
        let missing = self.diagram.difference(self.sources.universe, self.all)?;
        let normal = self
            .sources
            .measure(&mut self.diagram, self.normal, control)?;
        let recovery = self.sources.measure(&mut self.diagram, repair, control)?;
        let no_path = self.sources.measure(&mut self.diagram, missing, control)?;
        if normal
            .0
            .checked_add(recovery.0)
            .and_then(|n| n.checked_add(no_path.0))
            != Some(self.sources.possible)
        {
            return Err(Core::PatternDomainUnavailable.into());
        }
        let normal_example = self.records.values().find_map(|r| r.normal_example.clone());
        let recovery_example = self.global_recovery_witness(repair, control)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(self.records.len())
            .map_err(|_| Core::MemoryUnavailable)?;
        let mut roots = Vec::new();
        roots
            .try_reserve_exact(self.records.len())
            .map_err(|_| Core::MemoryUnavailable)?;
        for (key, record) in self.records {
            cancelled(control)?;
            let (count, probability) =
                self.sources
                    .measure(&mut self.diagram, record.all, control)?;
            if count == 0 {
                return Err(Core::PatternDomainUnavailable.into());
            }
            let example = record
                .normal_example
                .or(record.recovery_example)
                .ok_or(Core::PatternDomainUnavailable)?;
            roots.push(record.all);
            rows.push(RecoveryChainSolution {
                key,
                covered_count: count,
                probability,
                example,
            });
        }
        let classes = self.diagram.support_classes(&roots, control)?;
        Ok(RecoveryChainCatalog {
            input: self.query.clone(),
            coverage: RecoveryChainCoverage {
                possible: self.sources.possible,
                normal_count: normal.0,
                recovery_count: recovery.0,
                no_path_count: no_path.0,
                normal_probability: normal.1,
                recovery_probability: recovery.1,
                no_path_probability: no_path.1,
                states: self.states,
                normal_example,
                recovery_example,
            },
            solutions: rows,
            coverage_classes: classes,
            complete: true,
        })
    }
    fn global_recovery_witness(
        &mut self,
        language: Id,
        control: &ExecutionControl,
    ) -> Result<Option<RecoveryChainWitness>, Error> {
        if language == NONE {
            return Ok(None);
        }
        for record in self.records.values() {
            let accepted = self.diagram.intersect(record.all, language)?;
            if accepted == NONE {
                continue;
            }
            let queues = self.sources.first(&self.diagram, accepted)?;
            let mut fixed = self.query.clone();
            fixed.targets = record.plan.targets.clone();
            fixed.supplies = queues
                .iter()
                .map(|q| q.iter().map(|p| p.as_ascii()).collect())
                .collect();
            let mut diagram = Diagram::default();
            let sources = Sources::compile(&fixed, &mut diagram, control)?;
            let mut solver =
                Solver::for_plan(&fixed, record.plan.clone(), true, &sources, control)?;
            while !solver.advance(256, &mut diagram, &sources, control)? {}
            return solver
                .witness(&mut diagram, &sources, queues, Status::Recovery, control)
                .map(Some);
        }
        Err(Core::PatternDomainUnavailable.into())
    }
}
impl RecoveryChainQuery {
    pub fn catalog(&self, control: &ExecutionControl) -> Result<RecoveryChainCatalog, Error> {
        let mut session = RecoveryChainCatalogSession::new(self.clone(), control)?;
        while !session.advance(256, control)? {}
        session.finish(control)
    }
}
