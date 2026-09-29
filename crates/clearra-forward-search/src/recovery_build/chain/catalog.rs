//! Complete per-tiling coverage, separate from aggregate witness search.
//! One plan fixes all target tiles while preserving every legal build order
//! and source/hold alternative that covers an input. Rendering is not an input.
use super::{plan::{Plan,Producer},solver::Solver,source::Sources,RecoveryChainQuery,
    RecoveryChainError as Error,RecoveryChainCoverage,RecoveryChainWitness};
use crate::{CrossStageEarlyLimit,recovery_build::{RecoveryBuildError as Core,RecoveryBuildStatus as Status,
    staged::{diagram::{Diagram,Id,NONE},source::cancelled}}};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use std::collections::BTreeMap;

#[derive(Clone,Debug,PartialEq)]
pub struct RecoveryChainSolution {
    pub key:String,
    pub covered_count:u128,
    pub probability:f64,
    pub example:RecoveryChainWitness,
}
/// The private language roots retain the full original input measure. A
/// quotient used by an objective must not replace these probability languages.
pub struct RecoveryChainCatalog {
    pub coverage:RecoveryChainCoverage,
    pub solutions:Vec<RecoveryChainSolution>,
    pub(crate) query:RecoveryChainQuery,
    pub(super) diagram:Diagram,
    pub(super) sources:Sources,
    pub(super) languages:Vec<Id>,
}
impl RecoveryChainCatalog {
    pub fn matches_query(&self,query:&RecoveryChainQuery)->bool { &self.query==query }
}
struct Record {plan:Plan,normal:Id,repair:Id}
pub struct RecoveryChainCatalogSession {
    query:RecoveryChainQuery,
    diagram:Diagram,
    sources:Sources,
    producer:Producer,
    current:Option<Plan>,
    solver:Option<Solver>,
    normal_pass:Option<Id>,
    records:BTreeMap<String,Record>,
    normal:Id,
    all:Id,
    states:u128,
    done:bool,
}
impl RecoveryChainQuery {
    pub fn catalog(&self,control:&ExecutionControl)->Result<RecoveryChainCatalog,Error> {
        let mut session=RecoveryChainCatalogSession::new(self.clone(),control)?;
        while !session.advance(256,control)? {}
        session.finish(control)
    }
}
impl RecoveryChainCatalogSession {
    pub fn new(query:RecoveryChainQuery,control:&ExecutionControl)->Result<Self,Error> {
        query.validate()?;cancelled(control)?;
        let mut diagram=Diagram::default();
        let sources=Sources::compile(&query,&mut diagram,control)?;
        let producer=Producer::new(&query,&sources)?;
        Ok(Self {query,diagram,sources,producer,current:None,solver:None,normal_pass:None,
            records:BTreeMap::new(),normal:NONE,all:NONE,states:0,done:false})
    }
    pub fn advance(&mut self,fuel:usize,control:&ExecutionControl)->Result<bool,Error> {
        for _ in 0..fuel.max(1) {
            cancelled(control)?;
            if self.done {return Ok(true);}
            if let Some(solver)=&mut self.solver {
                if !solver.advance(1,&mut self.diagram,&self.sources,control)? {continue;}
                let solver=self.solver.take().ok_or(Error::Incomplete)?;
                let language=solver.result().ok_or(Error::Incomplete)?;
                self.states=self.states.checked_add(solver.states).ok_or(Core::CounterOverflow)?;
                let plan=self.current.as_ref().ok_or(Error::Incomplete)?;
                if !solver.repair && self.query.early_limit!=CrossStageEarlyLimit::AtMost(0) {
                    self.normal_pass=Some(language);
                    self.solver=Some(Solver::new(&self.query,plan.targets.clone(),true,&self.sources,control)?.with_plan(plan.clone())?);
                    continue;
                }
                let normal=if solver.repair {self.normal_pass.take().ok_or(Error::Incomplete)?} else {language};
                let repair=if solver.repair {self.diagram.difference(language,normal)?} else {NONE};
                let all=self.diagram.union(normal,repair)?;
                if self.diagram.difference(all,self.sources.universe)?!=NONE {return Err(Core::PatternDomainUnavailable.into());}
                self.normal=self.diagram.union(self.normal,normal)?;
                self.all=self.diagram.union(self.all,all)?;
                let plan=self.current.take().ok_or(Error::Incomplete)?;
                if all!=NONE {
                    let key=plan.key();
                    if self.records.insert(key,Record {plan,normal,repair}).is_some() {
                        return Err(Core::PatternDomainUnavailable.into());
                    }
                }
                continue;
            }
            if let Some(plan)=self.producer.advance(&self.query,control)? {
                self.solver=Some(Solver::new(&self.query,plan.targets.clone(),false,&self.sources,control)?.with_plan(plan.clone())?);
                self.current=Some(plan);
            } else if self.producer.done {self.done=true;}
        }
        control.report_progress("recovery-chain-solutions",u64::try_from(self.records.len()).unwrap_or(u64::MAX),None);
        Ok(self.done)
    }
    pub fn finish(mut self,control:&ExecutionControl)->Result<RecoveryChainCatalog,Error> {
        cancelled(control)?;
        if !self.done || !self.producer.done || self.solver.is_some() || self.current.is_some() || self.normal_pass.is_some() {
            return Err(Error::Incomplete);
        }
        let recovery=self.diagram.difference(self.all,self.normal)?;
        let missing=self.diagram.difference(self.sources.universe,self.all)?;
        let n=self.sources.measure(&mut self.diagram,self.normal,control)?;
        let r=self.sources.measure(&mut self.diagram,recovery,control)?;
        let m=self.sources.measure(&mut self.diagram,missing,control)?;
        if n.0.checked_add(r.0).and_then(|c|c.checked_add(m.0))!=Some(self.sources.possible) {return Err(Core::PatternDomainUnavailable.into());}
        let mut normal_example=None;
        let mut recovery_example=None;
        let mut solutions=Vec::new();let mut languages=Vec::new();
        solutions.try_reserve_exact(self.records.len()).map_err(|_|Core::MemoryUnavailable)?;
        languages.try_reserve_exact(self.records.len()).map_err(|_|Core::MemoryUnavailable)?;
        for (key,record) in self.records {
            cancelled(control)?;
            let all=self.diagram.union(record.normal,record.repair)?;
            let (covered_count,probability)=self.sources.measure(&mut self.diagram,all,control)?;
            let (example_language,repair,status)=if record.normal!=NONE {(record.normal,false,Status::Normal)}
                else {(record.repair,true,Status::Recovery)};
            let example=witness(&self.query,&record.plan,&self.sources,&self.diagram,example_language,repair,status,control)?;
            if normal_example.is_none() && record.normal!=NONE {normal_example=Some(example.clone());}
            if recovery_example.is_none() {
                let extra=self.diagram.intersect(record.repair,recovery)?;
                if extra!=NONE {
                    recovery_example=Some(witness(&self.query,&record.plan,&self.sources,&self.diagram,extra,true,Status::Recovery,control)?);
                }
            }
            solutions.push(RecoveryChainSolution {key,covered_count,probability,example});
            languages.push(all);
        }
        if (n.0!=0)!=normal_example.is_some() || (r.0!=0)!=recovery_example.is_some() {return Err(Core::PatternDomainUnavailable.into());}
        let coverage=RecoveryChainCoverage {possible:self.sources.possible,normal_count:n.0,recovery_count:r.0,no_path_count:m.0,
            normal_probability:n.1,recovery_probability:r.1,no_path_probability:m.1,states:self.states,normal_example,recovery_example};
        Ok(RecoveryChainCatalog {coverage,solutions,query:self.query,diagram:self.diagram,sources:self.sources,languages})
    }
}
#[allow(clippy::too_many_arguments)]
fn witness(query:&RecoveryChainQuery,plan:&Plan,sources:&Sources,diagram:&Diagram,language:Id,
    repair:bool,status:Status,control:&ExecutionControl)->Result<RecoveryChainWitness,Error> {
    let queues=sources.first(diagram,language)?;
    let mut fixed=query.clone();fixed.targets=plan.targets.clone();
    fixed.supplies=queues.iter().map(|q|q.iter().map(|p|p.as_ascii()).collect()).collect();
    let mut d=Diagram::default();let s=Sources::compile(&fixed,&mut d,control)?;
    let mut solver=Solver::new(&fixed,plan.targets.clone(),repair,&s,control)?.with_plan(plan.clone())?;
    while !solver.advance(256,&mut d,&s,control)? {}
    solver.witness(&mut d,&s,queues,status,control)
}
