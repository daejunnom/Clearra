//! Typed physical adapters for the one finite visible-language engine.
//! These traits are private: a public graph constructor cannot grant a PC
//! product's query/family authority. Core/App still have to bind that proof.
use std::{fmt::Debug, hash::Hash};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_replay::{
    ExactScoringExecutionBatch, ExactScoringExecutionGraph, HoldDecision, PieceDecision,
    ScoringExecutionEdge, ScoringExecutionNode, SpinCoverageExecutionGraph,
};

use super::{
    exact_replay_language::Label, execution_supply::ExecutionSupplyBatch,
    ExactReplayMaterializationError as Error, ExactReplayMaterializationLimits as Limits,
};

pub(super) trait ReplayLanguageGraph {
    fn root(&self) -> u32;
    fn node(&self, index: u32) -> Option<ScoringExecutionNode>;
    fn checked_edges(&self, node: ScoringExecutionNode) -> Option<&[ScoringExecutionEdge]>;
    fn same_candidate(&self, other: &Self) -> bool;
}

impl ReplayLanguageGraph for ExactScoringExecutionGraph {
    fn root(&self) -> u32 {
        self.root()
    }
    fn node(&self, index: u32) -> Option<ScoringExecutionNode> {
        self.node(index)
    }
    fn checked_edges(&self, node: ScoringExecutionNode) -> Option<&[ScoringExecutionEdge]> {
        self.checked_edges(node)
    }
    fn same_candidate(&self, other: &Self) -> bool {
        self.candidate_id() == other.candidate_id() && self.identity() == other.identity()
    }
}

impl ReplayLanguageGraph for SpinCoverageExecutionGraph {
    fn root(&self) -> u32 {
        self.root()
    }
    fn node(&self, index: u32) -> Option<ScoringExecutionNode> {
        self.node(index)
    }
    fn checked_edges(&self, node: ScoringExecutionNode) -> Option<&[ScoringExecutionEdge]> {
        self.checked_edges(node)
    }
    fn same_candidate(&self, other: &Self) -> bool {
        self.candidate_id() == other.candidate_id() && self.candidate_key() == other.candidate_key()
    }
}

pub(super) trait ReplayLanguageBatch: Debug + ExecutionSupplyBatch {
    type Graph: ReplayLanguageGraph;
    fn graph(&self, index: usize) -> Option<&Self::Graph>;
    fn patterns(&self) -> &[Vec<PieceKind>];
    fn initial_cursor(&self) -> u16;
    fn initial_hold(&self) -> Option<PieceKind>;
    fn complete(&self) -> bool;
    fn kick_table_id(&self) -> u64;
    fn rule_profile_id(&self) -> u64;
}

impl ReplayLanguageBatch for ExactScoringExecutionBatch {
    type Graph = ExactScoringExecutionGraph;
    fn graph(&self, index: usize) -> Option<&Self::Graph> {
        self.graphs().get(index)
    }
    fn patterns(&self) -> &[Vec<PieceKind>] {
        self.patterns()
    }
    fn initial_cursor(&self) -> u16 {
        self.initial_cursor()
    }
    fn initial_hold(&self) -> Option<PieceKind> {
        self.initial_hold()
    }
    fn complete(&self) -> bool {
        self.complete()
    }
    fn kick_table_id(&self) -> u64 {
        self.kick_table_id()
    }
    fn rule_profile_id(&self) -> u64 {
        self.rule_profile_id()
    }
}

pub(super) trait ReplayLanguageOutput {
    fn identity(&self) -> &str;
    fn checked_owned_bytes(&self) -> Option<u128>;
}

pub(super) trait ReplayLanguageDomain: Debug {
    type Batch: ReplayLanguageBatch;
    type State: Copy + Debug + Eq + Hash;
    type Mask: Copy;
    type Output: ReplayLanguageOutput;

    fn same_frame(batch: &Self::Batch, other: &Self::Batch) -> bool;
    fn initial(
        batch: &Self::Batch,
        graph: &<Self::Batch as ReplayLanguageBatch>::Graph,
    ) -> Result<Self::State, Error>;
    fn transition(
        batch: &Self::Batch,
        state: Self::State,
        edge: ScoringExecutionEdge,
    ) -> Result<(Self::Mask, Self::State), Error>;
    fn terminal(state: Self::State, depth: usize) -> Result<(), Error>;
    fn label(
        edge: ScoringExecutionEdge,
        decision: PieceDecision,
        mask: Self::Mask,
    ) -> Result<Label, Error>;
    fn step_suffix<'a>(batch: &Self::Batch, identity: &'a str) -> Result<&'a str, Error>;

    #[allow(clippy::too_many_arguments)]
    fn project_selected(
        batch: &Self::Batch,
        graph: &<Self::Batch as ReplayLanguageBatch>::Graph,
        pattern_id: usize,
        path: &[ScoringExecutionEdge],
        holds: &[HoldDecision],
        control: &ExecutionControl,
        baseline: u128,
        limits: Limits,
        guard: &mut impl FnMut(u128) -> Result<(), Error>,
    ) -> Result<Self::Output, Error>;
}
