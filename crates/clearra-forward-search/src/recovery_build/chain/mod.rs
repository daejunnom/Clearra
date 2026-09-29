//! Ordered logical target chains. Stage domains generate complete tilings;
//! one physical/hold automaton verifies them against an ordered input language.
//! No Cartesian queue list, probability multiplication, or pairwise witness join.
mod geometry;
mod solver;
mod source;
#[cfg(test)]
mod tests;
use super::staged::diagram::{Diagram, Id, NONE};
use super::{RecoveryBuildError as Error, RecoveryBuildStep};
use crate::CrossStageEarlyLimit;
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask as Mask, execution_cancellation::ExecutionControl,
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryChainQuery {
    pub height: u8,
    pub initial: Mask,
    /// Every destination is in the original shared logical frame. The final
    /// element is the result; preceding elements are ordered middle targets.
    pub targets: Vec<Mask>,
    pub supplies: Vec<String>,
    pub early_limit: CrossStageEarlyLimit,
    pub allow_piece_exchange: bool,
    pub hold_enabled: bool,
    pub preserve_b2b: bool,
    pub initial_b2b: bool,
    pub rule_profile: RuleProfileId,
    pub spin_profile: SpinProfileId,
    pub minimum_solutions: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryChainWitness {
    pub queues: Vec<String>,
    pub patterns: Vec<usize>,
    pub targets: Vec<Mask>,
    pub steps: Vec<RecoveryBuildStep>,
    /// One index per placement. The Boolean legacy result flag is not used to
    /// distinguish destinations of a chain with more than two targets.
    pub step_stages: Vec<usize>,
    pub early_counts: Vec<usize>,
    pub exchange: Vec<[i16; 7]>,
    pub terminal_board: [u64; 4],
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryChainSolution {
    pub key: String,
    pub covered_count: u128,
    pub probability: f64,
    pub witness: RecoveryChainWitness,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryChainReport {
    pub possible: u128,
    pub normal_count: u128,
    pub recovery_count: u128,
    pub no_path_count: u128,
    pub normal_probability: f64,
    pub recovery_probability: f64,
    pub no_path_probability: f64,
    pub solutions: Vec<RecoveryChainSolution>,
    pub coverage_classes: Option<Vec<Vec<usize>>>,
    pub states: u128,
}
impl RecoveryChainQuery {
    pub fn validate(&self) -> Result<(), Error> {
        if !(1..=24).contains(&self.height) {
            return Err(Error::InvalidHeight);
        }
        if self.targets.len() < 2 || self.targets.len() != self.supplies.len() {
            return Err(Error::InvalidSupplyPattern);
        }
        let mut filled = self.initial;
        if filled.fits_cell_count(u16::from(self.height) * 10) != Ok(true) {
            return Err(Error::BoardOutsideField);
        }
        for target in &self.targets {
            if target.fits_cell_count(u16::from(self.height) * 10) != Ok(true) {
                return Err(Error::BoardOutsideField);
            }
            if target.is_empty() || target.count_ones() % 4 != 0 {
                return Err(Error::TargetAreaNotTetrominoes);
            }
            if filled.intersects(*target) {
                return Err(Error::MiddleOverlapsStart);
            }
            filled = filled.union(*target);
        }
        for text in &self.supplies {
            let expression =
                clearra_supply::queue::queue_pattern_expression::QueuePatternExpression::parse(
                    text, 0,
                )
                .map_err(|_| Error::InvalidSupplyPattern)?;
            if expression.sequence_len() == 0 {
                return Err(Error::EmptySupply);
            }
        }
        Ok(())
    }
    pub fn search(&self, control: &ExecutionControl) -> Result<RecoveryChainReport, Error> {
        let mut search = RecoveryChainSearch::new(self.clone(), control)?;
        while !search.advance(256, control)? {}
        search.finish(control)
    }
}

/// Host-quantum state machine. Complete tilings partition work, not probability:
/// all certified queue languages are unioned before counting or minimization.
pub struct RecoveryChainSearch {
    query: RecoveryChainQuery,
    source: source::Source,
    diagram: Diagram,
    producer: geometry::Producer,
    pending: Option<solver::Solver>,
    normal: Id,
    recovery: Id,
    rows: Vec<(Id, RecoveryChainSolution)>,
    states: u128,
    done: bool,
}
impl RecoveryChainSearch {
    pub fn new(query: RecoveryChainQuery, control: &ExecutionControl) -> Result<Self, Error> {
        query.validate()?;
        let mut diagram = Diagram::default();
        let source = source::Source::new(&query, &mut diagram, control)?;
        let producer = geometry::Producer::new(&query, &source, control)?;
        Ok(Self {
            query,
            source,
            diagram,
            producer,
            pending: None,
            normal: NONE,
            recovery: NONE,
            rows: Vec::new(),
            states: 0,
            done: false,
        })
    }
    pub fn advance(&mut self, fuel: usize, control: &ExecutionControl) -> Result<bool, Error> {
        for _ in 0..fuel.max(1) {
            super::staged::source::cancelled(control)?;
            if self.done {
                return Ok(true);
            }
            if let Some(solver) = &mut self.pending {
                if !solver.advance(1, control)? {
                    continue;
                }
                let mut solver = self.pending.take().ok_or(Error::PatternDomainUnavailable)?;
                let packet = solver.diagram.export([solver.normal, solver.recovery])?;
                let [normal, recovery] = self.diagram.import(&packet, self.source.end)?;
                self.normal = self.diagram.union(self.normal, normal)?;
                self.recovery = self.diagram.union(self.recovery, recovery)?;
                self.states = self
                    .states
                    .checked_add(solver.states)
                    .ok_or(Error::CounterOverflow)?;
                let language = self.diagram.union(normal, recovery)?;
                if language != NONE {
                    let (patterns, queues) = self.source.first(&self.diagram, language, control)?;
                    let local = solver.diagram.union(solver.normal, solver.recovery)?;
                    let witness = solver.witness(local, patterns, queues, control)?;
                    let (covered_count, probability) =
                        self.source.measure(&mut self.diagram, language, control)?;
                    self.rows
                        .try_reserve(1)
                        .map_err(|_| Error::MemoryUnavailable)?;
                    self.rows.push((
                        language,
                        RecoveryChainSolution {
                            key: solver.plan.key(),
                            covered_count,
                            probability,
                            witness,
                        },
                    ));
                }
                control.report_progress("recovery-build-solutions", self.rows.len() as u64, None);
            } else {
                match self.producer.advance(control)? {
                    geometry::Produced::Pending => {}
                    geometry::Produced::Done => self.done = true,
                    geometry::Produced::Plan(plan) => {
                        self.pending = Some(solver::Solver::new(
                            self.query.clone(),
                            plan,
                            self.source.clone(),
                            self.source.blueprint.clone(),
                        )?);
                    }
                }
            }
        }
        Ok(self.done)
    }
    pub fn finish(mut self, control: &ExecutionControl) -> Result<RecoveryChainReport, Error> {
        super::staged::source::cancelled(control)?;
        if !self.done || self.pending.is_some() {
            return Err(Error::PatternDomainUnavailable);
        }
        let recovery = self.diagram.difference(self.recovery, self.normal)?;
        let covered = self.diagram.union(self.normal, recovery)?;
        let missing = self.diagram.difference(self.source.universe, covered)?;
        let n = self
            .source
            .measure(&mut self.diagram, self.normal, control)?;
        let r = self.source.measure(&mut self.diagram, recovery, control)?;
        let m = self.source.measure(&mut self.diagram, missing, control)?;
        if n.0.checked_add(r.0).and_then(|v| v.checked_add(m.0)) != Some(self.source.possible) {
            return Err(Error::PatternDomainUnavailable);
        }
        self.rows.sort_by(|a, b| a.1.key.cmp(&b.1.key));
        let languages = self.rows.iter().map(|row| row.0).collect::<Vec<_>>();
        let coverage_classes = if self.query.minimum_solutions {
            Some(self.diagram.support_classes(&languages, control)?)
        } else {
            None
        };
        Ok(RecoveryChainReport {
            possible: self.source.possible,
            normal_count: n.0,
            recovery_count: r.0,
            no_path_count: m.0,
            normal_probability: n.1,
            recovery_probability: r.1,
            no_path_probability: m.1,
            solutions: self.rows.into_iter().map(|r| r.1).collect(),
            coverage_classes,
            states: self.states,
        })
    }
}
