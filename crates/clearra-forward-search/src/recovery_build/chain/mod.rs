//! Ordered, shared-frame multi-boundary coverage. This library path deliberately
//! has its own result type: aggregate coverage is not a complete tiling catalog.
//! The two-target product route stays unchanged until host/catalog integration.
mod catalog;
mod fields;
mod plan;
pub use catalog::{RecoveryChainCatalog, RecoveryChainCatalogSession, RecoveryChainSolution};
mod solver;
mod source;
#[cfg(test)]
mod tests;

use super::{RecoveryBuildError, RecoveryBuildStatus};
use crate::CrossStageEarlyLimit;
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;

/// Every target owns distinct cells in the ORIGINAL, common logical frame.
/// Supplies belong to the ordered destinations, including the final target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryChainQuery {
    pub height: u8,
    pub initial: Board256Mask,
    pub targets: Vec<Board256Mask>,
    pub supplies: Vec<String>,
    /// Applied independently at every intermediate boundary, not once per token.
    pub early_limit: CrossStageEarlyLimit,
    pub allow_piece_exchange: bool,
    pub hold_enabled: bool,
    pub preserve_b2b: bool,
    pub initial_b2b: bool,
    pub rule_profile: RuleProfileId,
    pub spin_profile: SpinProfileId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryChainError {
    StageCount,
    SupplyCount,
    SupplyWindowTooLong { maximum: usize },
    OverlappingStage(usize),
    InvalidStageArea(usize),
    Incomplete,
    Core(RecoveryBuildError),
}
impl From<RecoveryBuildError> for RecoveryChainError {
    fn from(error: RecoveryBuildError) -> Self {
        Self::Core(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryChainStep {
    pub target_stage: usize,
    pub source_stage: usize,
    pub source_index: usize,
    pub piece: PieceKind,
    pub rotation: u8,
    pub x: i8,
    pub y: i8,
    pub hold_decision: &'static str,
    pub board_before: [u64; 4],
    pub placement: [u64; 4],
    pub board_after: [u64; 4],
    pub logical_placement: [u64; 4],
    pub cleared_rows: u32,
    pub cleared_lines: u8,
    pub recognized_spin: bool,
    pub b2b_active: bool,
    pub completed_stages: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryChainWitness {
    pub status: RecoveryBuildStatus,
    pub queues: Vec<Vec<PieceKind>>,
    pub targets: Vec<[u64; 4]>,
    pub early_by_boundary: Vec<usize>,
    pub exchange_by_stage: Vec<[i16; 7]>,
    pub steps: Vec<RecoveryChainStep>,
    pub terminal_board: [u64; 4],
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryChainCoverage {
    pub possible: u128,
    pub normal_count: u128,
    pub recovery_count: u128,
    pub no_path_count: u128,
    pub normal_probability: f64,
    pub recovery_probability: f64,
    pub no_path_probability: f64,
    pub states: u128,
    pub normal_example: Option<RecoveryChainWitness>,
    pub recovery_example: Option<RecoveryChainWitness>,
}

impl RecoveryChainQuery {
    pub fn validate(&self) -> Result<(), RecoveryChainError> {
        fields::validate(self)?;
        source::validate(self)
    }
    /// Exact aggregate coverage. This API does not claim all-solution/minimum
    /// support, and cannot be substituted for the product catalog output.
    pub fn coverage(
        &self,
        control: &ExecutionControl,
    ) -> Result<RecoveryChainCoverage, RecoveryChainError> {
        let mut session = RecoveryChainSession::new(self.clone(), control)?;
        while !session.advance(256, control)? {}
        session.finish(control)
    }
}

use super::staged::diagram::{Diagram, Id, NONE};
use fields::Orientations;
use solver::Solver;
use source::Sources;

/// Cooperative library session. The caller owns workers and resource limits.
/// No existing worker request is redirected into this serial reference path.
pub struct RecoveryChainSession {
    query: RecoveryChainQuery,
    diagram: Diagram,
    sources: Sources,
    orientations: Orientations,
    pending_targets: Option<Vec<Board256Mask>>,
    solver: Option<Solver>,
    normal: Id,
    all: Id,
    states: u128,
    // Each root language has an exact physical witness owner. An alternative
    // orientation may turn a per-orientation repair into a global normal path.
    witnesses: Vec<(Vec<Board256Mask>, bool, Id)>,
    done: bool,
}
impl RecoveryChainSession {
    pub fn new(
        query: RecoveryChainQuery,
        control: &ExecutionControl,
    ) -> Result<Self, RecoveryChainError> {
        query.validate()?;
        super::staged::source::cancelled(control)?;
        let mut diagram = Diagram::default();
        let sources = Sources::compile(&query, &mut diagram, control)?;
        let orientations = Orientations::new(&query);
        Ok(Self {
            query,
            diagram,
            sources,
            orientations,
            pending_targets: None,
            solver: None,
            normal: NONE,
            all: NONE,
            states: 0,
            witnesses: Vec::new(),
            done: false,
        })
    }
    pub fn is_complete(&self) -> bool {
        self.done
    }
    pub fn visited_states(&self) -> u128 {
        self.states + self.solver.as_ref().map_or(0, |s| s.states)
    }
    pub fn advance(
        &mut self,
        fuel: usize,
        control: &ExecutionControl,
    ) -> Result<bool, RecoveryChainError> {
        for _ in 0..fuel.max(1) {
            super::staged::source::cancelled(control)?;
            if self.done {
                return Ok(true);
            }
            if let Some(solver) = &mut self.solver {
                if !solver.advance(1, &mut self.diagram, &self.sources, control)? {
                    continue;
                }
                let solver = self.solver.take().ok_or(RecoveryChainError::Incomplete)?;
                let language = solver.result().ok_or(RecoveryChainError::Incomplete)?;
                self.states = self
                    .states
                    .checked_add(solver.states)
                    .ok_or(RecoveryBuildError::CounterOverflow)?;
                self.all = self.diagram.union(self.all, language)?;
                if !solver.repair {
                    self.normal = self.diagram.union(self.normal, language)?;
                }
                if language != NONE {
                    self.witnesses
                        .try_reserve(1)
                        .map_err(|_| RecoveryBuildError::MemoryUnavailable)?;
                    self.witnesses
                        .push((solver.targets.clone(), solver.repair, language));
                }
                if !solver.repair && self.query.early_limit != CrossStageEarlyLimit::AtMost(0) {
                    self.pending_targets = Some(solver.targets);
                }
                continue;
            }
            if let Some(targets) = self.pending_targets.take() {
                self.solver = Some(Solver::new(
                    &self.query,
                    targets,
                    true,
                    &self.sources,
                    control,
                )?);
                continue;
            }
            match self.orientations.advance(&self.query, control)? {
                fields::OrientationAdvance::Pending => {}
                fields::OrientationAdvance::Found(targets) => {
                    self.solver = Some(Solver::new(
                        &self.query,
                        targets,
                        false,
                        &self.sources,
                        control,
                    )?);
                }
                fields::OrientationAdvance::Done => self.done = true,
            }
        }
        control.report_progress(
            "recovery-chain-states",
            u64::try_from(self.visited_states()).unwrap_or(u64::MAX),
            None,
        );
        Ok(self.done)
    }
    pub fn finish(
        mut self,
        control: &ExecutionControl,
    ) -> Result<RecoveryChainCoverage, RecoveryChainError> {
        super::staged::source::cancelled(control)?;
        if !self.done || self.solver.is_some() || self.pending_targets.is_some() {
            return Err(RecoveryChainError::Incomplete);
        }
        let recovery = self.diagram.difference(self.all, self.normal)?;
        let missing = self.diagram.difference(self.sources.universe, self.all)?;
        let normal = self
            .sources
            .measure(&mut self.diagram, self.normal, control)?;
        let repaired = self.sources.measure(&mut self.diagram, recovery, control)?;
        let no_path = self.sources.measure(&mut self.diagram, missing, control)?;
        if normal
            .0
            .checked_add(repaired.0)
            .and_then(|n| n.checked_add(no_path.0))
            != Some(self.sources.possible)
        {
            return Err(RecoveryBuildError::PatternDomainUnavailable.into());
        }
        let normal_example = self.witness_for(self.normal, false, control)?;
        let recovery_example = self.witness_for(recovery, true, control)?;
        Ok(RecoveryChainCoverage {
            possible: self.sources.possible,
            normal_count: normal.0,
            recovery_count: repaired.0,
            no_path_count: no_path.0,
            normal_probability: normal.1,
            recovery_probability: repaired.1,
            no_path_probability: no_path.1,
            states: self.states,
            normal_example,
            recovery_example,
        })
    }
    fn witness_for(
        &mut self,
        language: Id,
        repair: bool,
        control: &ExecutionControl,
    ) -> Result<Option<RecoveryChainWitness>, RecoveryChainError> {
        if language == NONE {
            return Ok(None);
        }
        for (targets, path_repair, root) in &self.witnesses {
            if *path_repair != repair {
                continue;
            }
            let accepted = self.diagram.intersect(*root, language)?;
            if accepted == NONE {
                continue;
            }
            let queues = self.sources.first(&self.diagram, accepted)?;
            // Re-run only the already-proved concrete witness. This search is
            // neither a source of probability nor an all-solution catalog.
            let mut fixed = self.query.clone();
            fixed.targets = targets.clone();
            fixed.supplies = queues
                .iter()
                .map(|q| q.iter().map(|p| p.as_ascii()).collect())
                .collect();
            let mut diagram = Diagram::default();
            let sources = Sources::compile(&fixed, &mut diagram, control)?;
            let mut solver = Solver::new(&fixed, targets.clone(), repair, &sources, control)?;
            while !solver.advance(256, &mut diagram, &sources, control)? {}
            let status = if repair {
                RecoveryBuildStatus::Recovery
            } else {
                RecoveryBuildStatus::Normal
            };
            return solver
                .witness(&mut diagram, &sources, queues, status, control)
                .map(Some);
        }
        Err(RecoveryBuildError::PatternDomainUnavailable.into())
    }
}
