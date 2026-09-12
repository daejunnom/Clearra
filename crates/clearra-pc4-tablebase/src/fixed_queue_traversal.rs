// SRP rationale: bounded fixed-queue full outgoing-edge traversal is this module's single change reason.
use core::{fmt, num::NonZeroUsize};

use crate::{
    Pc4GraphPiece, Pc4RuleProfile, Pc4TargetLines, Pc4TerminalUseCase, QualifiedPc4GraphEdge,
    QualifiedPc4TargetIdentity, QualifiedSnapshotIdentity,
};

/// One separately qualified, complete adjacency response.
///
/// The redundant request binding is intentional. Traversal checks the response
/// and every edge before using a target. An empty `edges` vector is a valid
/// graph dead end; it is not a dataset lookup miss.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualifiedCompleteAdjacency {
    snapshot: QualifiedSnapshotIdentity,
    profile: Pc4RuleProfile,
    source_field_id: u32,
    piece: Pc4GraphPiece,
    queue_index: usize,
    edges: Vec<QualifiedPc4GraphEdge>,
}

impl QualifiedCompleteAdjacency {
    pub fn from_qualified_provider(
        snapshot: QualifiedSnapshotIdentity,
        profile: Pc4RuleProfile,
        source_field_id: u32,
        piece: Pc4GraphPiece,
        queue_index: usize,
        edges: Vec<QualifiedPc4GraphEdge>,
    ) -> Self {
        Self {
            snapshot,
            profile,
            source_field_id,
            piece,
            queue_index,
            edges,
        }
    }

    pub const fn snapshot(&self) -> &QualifiedSnapshotIdentity {
        &self.snapshot
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn source_field_id(&self) -> u32 {
        self.source_field_id
    }

    pub const fn piece(&self) -> Pc4GraphPiece {
        self.piece
    }

    pub const fn queue_index(&self) -> usize {
        self.queue_index
    }

    pub fn edges(&self) -> &[QualifiedPc4GraphEdge] {
        &self.edges
    }
}

/// Fully bound query passed to a separately qualified adjacency provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedQueueAdjacencyQuery<'a> {
    snapshot: &'a QualifiedSnapshotIdentity,
    profile: Pc4RuleProfile,
    source_field_id: u32,
    piece: Pc4GraphPiece,
    queue_index: usize,
}

impl<'a> FixedQueueAdjacencyQuery<'a> {
    pub const fn snapshot(&self) -> &'a QualifiedSnapshotIdentity {
        self.snapshot
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn source_field_id(&self) -> u32 {
        self.source_field_id
    }

    pub const fn piece(&self) -> Pc4GraphPiece {
        self.piece
    }

    pub const fn queue_index(&self) -> usize {
        self.queue_index
    }
}

/// Pure port for graph-record parsing/lookup owned outside this traversal.
///
/// Implementations must return all outgoing raw target occurrences for the
/// exact bound query. The qualified graph may repeat one target for multiple
/// placements. Traversal validates every occurrence, canonicalizes equal
/// source + piece + target transitions once, and leaves concrete placement
/// enumeration to the exact materializer. This crate does not parse, fetch,
/// qualify, or infer the provider's graph format.
pub trait QualifiedCompleteAdjacencyProvider {
    type Error;

    fn snapshot(&self) -> &QualifiedSnapshotIdentity;

    fn profile(&self) -> Pc4RuleProfile;

    fn complete_outgoing_edges(
        &mut self,
        query: &FixedQueueAdjacencyQuery<'_>,
    ) -> Result<QualifiedCompleteAdjacency, Self::Error>;
}

/// Host-owned cancellation and immutable-snapshot freshness observation.
///
/// The traversal samples the guard before and after every caller callback and
/// once more before returning results.
pub trait FixedQueueTraversalGuard {
    fn is_cancelled(&self) -> bool;

    fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool;
}

/// Context for the caller-owned terminal predicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedQueueTerminalQuery<'a> {
    target: &'a QualifiedPc4TargetIdentity,
    field_id: u32,
    queue: &'a [Pc4GraphPiece],
    consumed_pieces: usize,
}

impl<'a> FixedQueueTerminalQuery<'a> {
    pub const fn snapshot(&self) -> &'a QualifiedSnapshotIdentity {
        self.target.snapshot()
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.target.profile()
    }

    pub const fn target(&self) -> &'a QualifiedPc4TargetIdentity {
        self.target
    }

    pub const fn use_case(&self) -> Pc4TerminalUseCase {
        self.target.use_case()
    }

    pub const fn target_lines(&self) -> Pc4TargetLines {
        self.target.target_lines()
    }

    pub const fn field_id(&self) -> u32 {
        self.field_id
    }

    pub const fn queue(&self) -> &'a [Pc4GraphPiece] {
        self.queue
    }

    pub const fn consumed_pieces(&self) -> usize {
        self.consumed_pieces
    }

    pub const fn queue_is_exhausted(&self) -> bool {
        self.consumed_pieces == self.queue.len()
    }
}

pub trait FixedQueueTerminalPredicate {
    type Error;

    fn is_terminal(&mut self, query: &FixedQueueTerminalQuery<'_>) -> Result<bool, Self::Error>;
}

impl<F, E> FixedQueueTerminalPredicate for F
where
    F: for<'a> FnMut(&FixedQueueTerminalQuery<'a>) -> Result<bool, E>,
{
    type Error = E;

    fn is_terminal(&mut self, query: &FixedQueueTerminalQuery<'_>) -> Result<bool, Self::Error> {
        self(query)
    }
}

/// Controls whether the predicate may terminate a path before queue exhaustion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalDepthContract {
    QueueExhaustedOnly,
    PredicateMayTerminateEarly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixedQueueBudgetKind {
    VisitedStateOccurrences,
    FrontierPaths,
    PathEdges,
    OutputPaths,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedQueueTraversalBudgets {
    visited_state_occurrences: NonZeroUsize,
    frontier_paths: NonZeroUsize,
    path_edges: NonZeroUsize,
    output_paths: NonZeroUsize,
}

impl FixedQueueTraversalBudgets {
    pub const fn new(
        visited_state_occurrences: NonZeroUsize,
        frontier_paths: NonZeroUsize,
        path_edges: NonZeroUsize,
        output_paths: NonZeroUsize,
    ) -> Self {
        Self {
            visited_state_occurrences,
            frontier_paths,
            path_edges,
            output_paths,
        }
    }

    pub const fn visited_state_occurrences(self) -> usize {
        self.visited_state_occurrences.get()
    }

    pub const fn frontier_paths(self) -> usize {
        self.frontier_paths.get()
    }

    pub const fn path_edges(self) -> usize {
        self.path_edges.get()
    }

    pub const fn output_paths(self) -> usize {
        self.output_paths.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedQueueBudgetExceeded {
    pub kind: FixedQueueBudgetKind,
    pub limit: usize,
    pub attempted: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FixedQueueTraversalSemanticError {
    ProviderSnapshotMismatch,
    ProviderProfileMismatch {
        expected: Pc4RuleProfile,
        actual: Pc4RuleProfile,
    },
    AdjacencySnapshotMismatch,
    AdjacencyProfileMismatch {
        expected: Pc4RuleProfile,
        actual: Pc4RuleProfile,
    },
    AdjacencySourceMismatch {
        expected: u32,
        actual: u32,
    },
    AdjacencyPieceMismatch {
        expected: Pc4GraphPiece,
        actual: Pc4GraphPiece,
    },
    AdjacencyQueueIndexMismatch {
        expected: usize,
        actual: usize,
    },
    EdgeSnapshotMismatch,
    EdgeProfileMismatch {
        expected: Pc4RuleProfile,
        actual: Pc4RuleProfile,
    },
    EdgeSourceMismatch {
        expected: u32,
        actual: u32,
    },
    EdgePieceMismatch {
        expected: Pc4GraphPiece,
        actual: Pc4GraphPiece,
    },
}

impl FixedQueueTraversalSemanticError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::ProviderSnapshotMismatch => "pc4_fixed_queue_provider_snapshot_mismatch",
            Self::ProviderProfileMismatch { .. } => "pc4_fixed_queue_provider_profile_mismatch",
            Self::AdjacencySnapshotMismatch => "pc4_fixed_queue_adjacency_snapshot_mismatch",
            Self::AdjacencyProfileMismatch { .. } => "pc4_fixed_queue_adjacency_profile_mismatch",
            Self::AdjacencySourceMismatch { .. } => "pc4_fixed_queue_adjacency_source_mismatch",
            Self::AdjacencyPieceMismatch { .. } => "pc4_fixed_queue_adjacency_piece_mismatch",
            Self::AdjacencyQueueIndexMismatch { .. } => {
                "pc4_fixed_queue_adjacency_queue_index_mismatch"
            }
            Self::EdgeSnapshotMismatch => "pc4_fixed_queue_edge_snapshot_mismatch",
            Self::EdgeProfileMismatch { .. } => "pc4_fixed_queue_edge_profile_mismatch",
            Self::EdgeSourceMismatch { .. } => "pc4_fixed_queue_edge_source_mismatch",
            Self::EdgePieceMismatch { .. } => "pc4_fixed_queue_edge_piece_mismatch",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FixedQueueTraversalError<ProviderError, TerminalError> {
    Cancelled,
    StaleSnapshot,
    BudgetExceeded(FixedQueueBudgetExceeded),
    Provider(ProviderError),
    TerminalPredicate(TerminalError),
    Semantic(FixedQueueTraversalSemanticError),
}

impl<ProviderError, TerminalError> FixedQueueTraversalError<ProviderError, TerminalError> {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_fixed_queue_traversal_cancelled",
            Self::StaleSnapshot => "pc4_fixed_queue_traversal_stale_snapshot",
            Self::BudgetExceeded(_) => "pc4_fixed_queue_traversal_budget_exceeded",
            Self::Provider(_) => "pc4_fixed_queue_adjacency_provider_failed",
            Self::TerminalPredicate(_) => "pc4_fixed_queue_terminal_predicate_failed",
            Self::Semantic(error) => error.reason(),
        }
    }
}

impl<ProviderError, TerminalError> fmt::Display
    for FixedQueueTraversalError<ProviderError, TerminalError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedQueueGraphPath {
    start_field_id: u32,
    edges: Vec<QualifiedPc4GraphEdge>,
}

impl FixedQueueGraphPath {
    #[cfg(test)]
    pub(crate) fn from_test_edges(start_field_id: u32, edges: Vec<QualifiedPc4GraphEdge>) -> Self {
        Self {
            start_field_id,
            edges,
        }
    }

    pub const fn start_field_id(&self) -> u32 {
        self.start_field_id
    }

    pub fn edges(&self) -> &[QualifiedPc4GraphEdge] {
        &self.edges
    }

    pub fn terminal_field_id(&self) -> u32 {
        self.edges
            .last()
            .map_or(self.start_field_id, QualifiedPc4GraphEdge::target_field_id)
    }

    pub fn consumed_pieces(&self) -> usize {
        self.edges.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedQueueTraversalResult {
    paths: Vec<FixedQueueGraphPath>,
    visited_state_occurrences: usize,
    adjacency_queries: usize,
}

impl FixedQueueTraversalResult {
    pub fn paths(&self) -> &[FixedQueueGraphPath] {
        &self.paths
    }

    pub const fn visited_state_occurrences(&self) -> usize {
        self.visited_state_occurrences
    }

    pub const fn adjacency_queries(&self) -> usize {
        self.adjacency_queries
    }
}

pub struct FixedQueueTraversalRequest<'a> {
    target: &'a QualifiedPc4TargetIdentity,
    start_field_id: u32,
    queue: &'a [Pc4GraphPiece],
    terminal_depth_contract: TerminalDepthContract,
    budgets: FixedQueueTraversalBudgets,
}

impl<'a> FixedQueueTraversalRequest<'a> {
    pub const fn new(
        target: &'a QualifiedPc4TargetIdentity,
        start_field_id: u32,
        queue: &'a [Pc4GraphPiece],
        terminal_depth_contract: TerminalDepthContract,
        budgets: FixedQueueTraversalBudgets,
    ) -> Self {
        Self {
            target,
            start_field_id,
            queue,
            terminal_depth_contract,
            budgets,
        }
    }

    pub const fn target(&self) -> &'a QualifiedPc4TargetIdentity {
        self.target
    }
}

/// Traverses every distinct outgoing target transition for one fixed queue
/// without global state deduplication.
///
/// Repeated raw target occurrences in one adjacency are one field transition:
/// the exact materializer later recovers all concrete placements for that
/// source + piece + target. Reaching the same state through different prior
/// paths still preserves every path. Canonical ordering is derived solely from
/// qualified edge values. Any callback, binding, cancellation, freshness, or
/// budget failure returns an error and discards the in-progress result.
pub fn traverse_fixed_queue<P, T, G>(
    request: FixedQueueTraversalRequest<'_>,
    provider: &mut P,
    terminal_predicate: &mut T,
    guard: &G,
) -> Result<FixedQueueTraversalResult, FixedQueueTraversalError<P::Error, T::Error>>
where
    P: QualifiedCompleteAdjacencyProvider,
    T: FixedQueueTerminalPredicate,
    G: FixedQueueTraversalGuard,
{
    let snapshot = request.target.snapshot();
    let profile = request.target.profile();
    check_guard(snapshot, guard)?;
    validate_provider_binding(snapshot, profile, provider)?;

    let mut frontier = vec![FixedQueueGraphPath {
        start_field_id: request.start_field_id,
        edges: Vec::new(),
    }];
    let mut outputs = Vec::new();
    let mut visited_state_occurrences = 0usize;
    let mut adjacency_queries = 0usize;

    while !frontier.is_empty() {
        let mut next_frontier = Vec::new();
        for path in frontier {
            check_guard(snapshot, guard)?;
            consume_budget(
                &mut visited_state_occurrences,
                request.budgets.visited_state_occurrences(),
                FixedQueueBudgetKind::VisitedStateOccurrences,
            )?;

            let consumed_pieces = path.consumed_pieces();
            let field_id = path.terminal_field_id();
            let terminal_query = FixedQueueTerminalQuery {
                target: request.target,
                field_id,
                queue: request.queue,
                consumed_pieces,
            };
            let terminal_result = terminal_predicate.is_terminal(&terminal_query);
            check_guard(snapshot, guard)?;
            let predicate_matches =
                terminal_result.map_err(FixedQueueTraversalError::TerminalPredicate)?;
            let depth_permits_terminal = consumed_pieces == request.queue.len()
                || request.terminal_depth_contract
                    == TerminalDepthContract::PredicateMayTerminateEarly;
            if predicate_matches && depth_permits_terminal {
                let attempted = outputs.len().saturating_add(1);
                if attempted > request.budgets.output_paths() {
                    return Err(FixedQueueTraversalError::BudgetExceeded(
                        FixedQueueBudgetExceeded {
                            kind: FixedQueueBudgetKind::OutputPaths,
                            limit: request.budgets.output_paths(),
                            attempted,
                        },
                    ));
                }
                outputs.push(path);
                continue;
            }
            if consumed_pieces == request.queue.len() {
                continue;
            }
            let attempted_path_edges = consumed_pieces.saturating_add(1);
            if attempted_path_edges > request.budgets.path_edges() {
                return Err(FixedQueueTraversalError::BudgetExceeded(
                    FixedQueueBudgetExceeded {
                        kind: FixedQueueBudgetKind::PathEdges,
                        limit: request.budgets.path_edges(),
                        attempted: attempted_path_edges,
                    },
                ));
            }

            let piece = request.queue[consumed_pieces];
            let adjacency_query = FixedQueueAdjacencyQuery {
                snapshot,
                profile,
                source_field_id: field_id,
                piece,
                queue_index: consumed_pieces,
            };
            consume_unbounded_counter(&mut adjacency_queries);
            let adjacency_result = provider.complete_outgoing_edges(&adjacency_query);
            check_guard(snapshot, guard)?;
            validate_provider_binding(snapshot, profile, provider)?;
            let mut adjacency = adjacency_result.map_err(FixedQueueTraversalError::Provider)?;
            validate_adjacency(&adjacency_query, &adjacency)?;

            adjacency
                .edges
                .sort_unstable_by_key(QualifiedPc4GraphEdge::target_field_id);
            adjacency.edges.dedup_by_key(|edge| edge.target_field_id());
            for edge in adjacency.edges {
                check_guard(snapshot, guard)?;
                let attempted = next_frontier.len().saturating_add(1);
                if attempted > request.budgets.frontier_paths() {
                    return Err(FixedQueueTraversalError::BudgetExceeded(
                        FixedQueueBudgetExceeded {
                            kind: FixedQueueBudgetKind::FrontierPaths,
                            limit: request.budgets.frontier_paths(),
                            attempted,
                        },
                    ));
                }
                let mut next_path = path.clone();
                next_path.edges.push(edge);
                next_frontier.push(next_path);
            }
        }
        frontier = next_frontier;
    }

    check_guard(snapshot, guard)?;
    outputs.sort_by(|left, right| {
        left.start_field_id
            .cmp(&right.start_field_id)
            .then_with(|| {
                left.edges
                    .iter()
                    .map(QualifiedPc4GraphEdge::target_field_id)
                    .cmp(
                        right
                            .edges
                            .iter()
                            .map(QualifiedPc4GraphEdge::target_field_id),
                    )
            })
    });
    Ok(FixedQueueTraversalResult {
        paths: outputs,
        visited_state_occurrences,
        adjacency_queries,
    })
}

fn consume_budget<ProviderError, TerminalError>(
    value: &mut usize,
    limit: usize,
    kind: FixedQueueBudgetKind,
) -> Result<(), FixedQueueTraversalError<ProviderError, TerminalError>> {
    let attempted = value.saturating_add(1);
    if attempted > limit {
        return Err(FixedQueueTraversalError::BudgetExceeded(
            FixedQueueBudgetExceeded {
                kind,
                limit,
                attempted,
            },
        ));
    }
    *value = attempted;
    Ok(())
}

fn consume_unbounded_counter(value: &mut usize) {
    *value = value.saturating_add(1);
}

fn check_guard<ProviderError, TerminalError, G>(
    snapshot: &QualifiedSnapshotIdentity,
    guard: &G,
) -> Result<(), FixedQueueTraversalError<ProviderError, TerminalError>>
where
    G: FixedQueueTraversalGuard,
{
    if guard.is_cancelled() {
        return Err(FixedQueueTraversalError::Cancelled);
    }
    if !guard.is_current_snapshot(snapshot) {
        return Err(FixedQueueTraversalError::StaleSnapshot);
    }
    Ok(())
}

fn validate_provider_binding<ProviderError, TerminalError, P>(
    snapshot: &QualifiedSnapshotIdentity,
    profile: Pc4RuleProfile,
    provider: &P,
) -> Result<(), FixedQueueTraversalError<ProviderError, TerminalError>>
where
    P: QualifiedCompleteAdjacencyProvider,
{
    if provider.snapshot() != snapshot {
        return Err(FixedQueueTraversalError::Semantic(
            FixedQueueTraversalSemanticError::ProviderSnapshotMismatch,
        ));
    }
    if provider.profile() != profile {
        return Err(FixedQueueTraversalError::Semantic(
            FixedQueueTraversalSemanticError::ProviderProfileMismatch {
                expected: profile,
                actual: provider.profile(),
            },
        ));
    }
    Ok(())
}

fn validate_adjacency<ProviderError, TerminalError>(
    query: &FixedQueueAdjacencyQuery<'_>,
    adjacency: &QualifiedCompleteAdjacency,
) -> Result<(), FixedQueueTraversalError<ProviderError, TerminalError>> {
    let mismatch = if adjacency.snapshot() != query.snapshot() {
        Some(FixedQueueTraversalSemanticError::AdjacencySnapshotMismatch)
    } else if adjacency.profile() != query.profile() {
        Some(FixedQueueTraversalSemanticError::AdjacencyProfileMismatch {
            expected: query.profile(),
            actual: adjacency.profile(),
        })
    } else if adjacency.source_field_id() != query.source_field_id() {
        Some(FixedQueueTraversalSemanticError::AdjacencySourceMismatch {
            expected: query.source_field_id(),
            actual: adjacency.source_field_id(),
        })
    } else if adjacency.piece() != query.piece() {
        Some(FixedQueueTraversalSemanticError::AdjacencyPieceMismatch {
            expected: query.piece(),
            actual: adjacency.piece(),
        })
    } else if adjacency.queue_index() != query.queue_index() {
        Some(
            FixedQueueTraversalSemanticError::AdjacencyQueueIndexMismatch {
                expected: query.queue_index(),
                actual: adjacency.queue_index(),
            },
        )
    } else {
        None
    };
    if let Some(mismatch) = mismatch {
        return Err(FixedQueueTraversalError::Semantic(mismatch));
    }

    for edge in adjacency.edges() {
        let mismatch = if edge.snapshot() != query.snapshot() {
            Some(FixedQueueTraversalSemanticError::EdgeSnapshotMismatch)
        } else if edge.profile() != query.profile() {
            Some(FixedQueueTraversalSemanticError::EdgeProfileMismatch {
                expected: query.profile(),
                actual: edge.profile(),
            })
        } else if edge.source_field_id() != query.source_field_id() {
            Some(FixedQueueTraversalSemanticError::EdgeSourceMismatch {
                expected: query.source_field_id(),
                actual: edge.source_field_id(),
            })
        } else if edge.piece() != query.piece() {
            Some(FixedQueueTraversalSemanticError::EdgePieceMismatch {
                expected: query.piece(),
                actual: edge.piece(),
            })
        } else {
            None
        };
        if let Some(mismatch) = mismatch {
            return Err(FixedQueueTraversalError::Semantic(mismatch));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, collections::BTreeMap, convert::Infallible, rc::Rc};

    use super::*;
    use crate::manifest::tests::{qualified_snapshot_identity, qualified_target_identity};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum SyntheticProviderError {
        Rejected,
    }

    #[derive(Clone, Copy)]
    enum ResponseFault {
        None,
        WrongSnapshot,
        WrongProfile,
        WrongSource,
        WrongPiece,
        WrongQueueIndex,
        WrongEdgeSnapshot,
        WrongEdgeProfile,
        WrongEdgeSource,
        WrongEdgePiece,
    }

    struct SyntheticProvider {
        snapshot: QualifiedSnapshotIdentity,
        profile: Pc4RuleProfile,
        graph: BTreeMap<(u32, Pc4GraphPiece), Vec<u32>>,
        calls: Vec<(u32, Pc4GraphPiece, usize)>,
        response_fault: ResponseFault,
        fail: bool,
        cancel_during_call: Option<Rc<Cell<bool>>>,
        stale_during_call: Option<Rc<Cell<bool>>>,
    }

    impl QualifiedCompleteAdjacencyProvider for SyntheticProvider {
        type Error = SyntheticProviderError;

        fn snapshot(&self) -> &QualifiedSnapshotIdentity {
            &self.snapshot
        }

        fn profile(&self) -> Pc4RuleProfile {
            self.profile
        }

        fn complete_outgoing_edges(
            &mut self,
            query: &FixedQueueAdjacencyQuery<'_>,
        ) -> Result<QualifiedCompleteAdjacency, Self::Error> {
            self.calls
                .push((query.source_field_id(), query.piece(), query.queue_index()));
            if let Some(cancelled) = &self.cancel_during_call {
                cancelled.set(true);
            }
            if let Some(stale) = &self.stale_during_call {
                stale.set(true);
            }
            if self.fail {
                return Err(SyntheticProviderError::Rejected);
            }

            let mut snapshot = query.snapshot().clone();
            let mut profile = query.profile();
            let mut source = query.source_field_id();
            let mut piece = query.piece();
            let mut queue_index = query.queue_index();
            match self.response_fault {
                ResponseFault::WrongSnapshot => snapshot = other_snapshot(),
                ResponseFault::WrongProfile => profile = Pc4RuleProfile::NoKick,
                ResponseFault::WrongSource => source = source.saturating_add(1),
                ResponseFault::WrongPiece => piece = Pc4GraphPiece::L,
                ResponseFault::WrongQueueIndex => queue_index = queue_index.saturating_add(1),
                _ => {}
            }
            let targets = self
                .graph
                .get(&(query.source_field_id(), query.piece()))
                .cloned()
                .unwrap_or_default();
            let edges = targets
                .into_iter()
                .map(|target| {
                    let mut edge_snapshot = query.snapshot().clone();
                    let mut edge_profile = query.profile();
                    let mut edge_source = query.source_field_id();
                    let mut edge_piece = query.piece();
                    match self.response_fault {
                        ResponseFault::WrongEdgeSnapshot => edge_snapshot = other_snapshot(),
                        ResponseFault::WrongEdgeProfile => {
                            edge_profile = Pc4RuleProfile::NoKick;
                        }
                        ResponseFault::WrongEdgeSource => {
                            edge_source = edge_source.saturating_add(1);
                        }
                        ResponseFault::WrongEdgePiece => edge_piece = Pc4GraphPiece::L,
                        _ => {}
                    }
                    QualifiedPc4GraphEdge::from_qualified_record(
                        edge_snapshot,
                        edge_profile,
                        edge_source,
                        edge_piece,
                        target,
                    )
                })
                .collect();
            Ok(QualifiedCompleteAdjacency::from_qualified_provider(
                snapshot,
                profile,
                source,
                piece,
                queue_index,
                edges,
            ))
        }
    }

    struct SyntheticGuard {
        cancelled: Rc<Cell<bool>>,
        stale: Rc<Cell<bool>>,
    }

    impl FixedQueueTraversalGuard for SyntheticGuard {
        fn is_cancelled(&self) -> bool {
            self.cancelled.get()
        }

        fn is_current_snapshot(&self, _expected: &QualifiedSnapshotIdentity) -> bool {
            !self.stale.get()
        }
    }

    fn snapshot() -> QualifiedPc4TargetIdentity {
        qualified_target_identity(
            "traversal-generation-a",
            "synthetic-traversal-manifest-a",
            Pc4RuleProfile::Srs,
            Pc4TerminalUseCase::PcSearch,
            4,
        )
    }

    fn other_snapshot() -> QualifiedSnapshotIdentity {
        qualified_snapshot_identity("traversal-generation-a", "synthetic-traversal-manifest-b")
    }

    fn provider(graph: &[((u32, Pc4GraphPiece), &[u32])]) -> SyntheticProvider {
        SyntheticProvider {
            snapshot: snapshot().snapshot().clone(),
            profile: Pc4RuleProfile::Srs,
            graph: graph
                .iter()
                .map(|(key, targets)| (*key, targets.to_vec()))
                .collect(),
            calls: Vec::new(),
            response_fault: ResponseFault::None,
            fail: false,
            cancel_during_call: None,
            stale_during_call: None,
        }
    }

    fn guard() -> SyntheticGuard {
        SyntheticGuard {
            cancelled: Rc::new(Cell::new(false)),
            stale: Rc::new(Cell::new(false)),
        }
    }

    fn budgets(
        visited: usize,
        frontier: usize,
        path: usize,
        output: usize,
    ) -> FixedQueueTraversalBudgets {
        FixedQueueTraversalBudgets::new(
            NonZeroUsize::new(visited).expect("positive visited budget"),
            NonZeroUsize::new(frontier).expect("positive frontier budget"),
            NonZeroUsize::new(path).expect("positive path budget"),
            NonZeroUsize::new(output).expect("positive output budget"),
        )
    }

    fn request<'a>(
        target: &'a QualifiedPc4TargetIdentity,
        queue: &'a [Pc4GraphPiece],
        depth: TerminalDepthContract,
        budgets: FixedQueueTraversalBudgets,
    ) -> FixedQueueTraversalRequest<'a> {
        FixedQueueTraversalRequest::new(target, 0, queue, depth, budgets)
    }

    fn exhausted_terminal(query: &FixedQueueTerminalQuery<'_>) -> Result<bool, Infallible> {
        Ok(query.queue_is_exhausted())
    }

    fn target_paths(result: &FixedQueueTraversalResult) -> Vec<Vec<u32>> {
        result
            .paths()
            .iter()
            .map(|path| {
                path.edges()
                    .iter()
                    .map(QualifiedPc4GraphEdge::target_field_id)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn kat_traverses_every_outgoing_edge_in_canonical_order() {
        let snapshot = snapshot();
        let queue = [Pc4GraphPiece::I, Pc4GraphPiece::O];
        let mut provider = provider(&[
            ((0, Pc4GraphPiece::I), &[2, 1]),
            ((1, Pc4GraphPiece::O), &[4, 3]),
            ((2, Pc4GraphPiece::O), &[6, 5]),
        ]);
        let result = traverse_fixed_queue(
            request(
                &snapshot,
                &queue,
                TerminalDepthContract::QueueExhaustedOnly,
                budgets(16, 8, 2, 8),
            ),
            &mut provider,
            &mut exhausted_terminal,
            &guard(),
        )
        .expect("synthetic all-edge traversal");

        assert_eq!(
            target_paths(&result),
            vec![vec![1, 3], vec![1, 4], vec![2, 5], vec![2, 6]]
        );
        assert_eq!(
            provider.calls,
            vec![
                (0, Pc4GraphPiece::I, 0),
                (1, Pc4GraphPiece::O, 1),
                (2, Pc4GraphPiece::O, 1),
            ]
        );
        assert_eq!(result.adjacency_queries(), 3);
    }

    #[test]
    fn converging_paths_are_not_collapsed_by_visited_state() {
        let snapshot = snapshot();
        let queue = [Pc4GraphPiece::I, Pc4GraphPiece::O];
        let mut provider = provider(&[
            ((0, Pc4GraphPiece::I), &[2, 1]),
            ((1, Pc4GraphPiece::O), &[3]),
            ((2, Pc4GraphPiece::O), &[3]),
        ]);
        let result = traverse_fixed_queue(
            request(
                &snapshot,
                &queue,
                TerminalDepthContract::QueueExhaustedOnly,
                budgets(16, 8, 2, 8),
            ),
            &mut provider,
            &mut exhausted_terminal,
            &guard(),
        )
        .expect("converging path traversal");

        assert_eq!(target_paths(&result), vec![vec![1, 3], vec![2, 3]]);
        assert_eq!(result.visited_state_occurrences(), 5);
    }

    #[test]
    fn repeated_raw_targets_are_one_transition_before_exact_materialization() {
        let snapshot = snapshot();
        let queue = [Pc4GraphPiece::I];
        let mut provider = provider(&[((0, Pc4GraphPiece::I), &[2, 1, 2, 1])]);
        let result = traverse_fixed_queue(
            request(
                &snapshot,
                &queue,
                TerminalDepthContract::QueueExhaustedOnly,
                budgets(8, 4, 1, 4),
            ),
            &mut provider,
            &mut exhausted_terminal,
            &guard(),
        )
        .expect("duplicate raw targets are canonical transitions");

        assert_eq!(target_paths(&result), vec![vec![1], vec![2]]);
        assert_eq!(result.adjacency_queries(), 1);
        assert_eq!(result.visited_state_occurrences(), 3);
    }

    #[test]
    fn repeated_states_are_bounded_by_fixed_queue_without_losing_the_path() {
        let snapshot = snapshot();
        let queue = [Pc4GraphPiece::I, Pc4GraphPiece::I, Pc4GraphPiece::I];
        let mut provider = provider(&[((0, Pc4GraphPiece::I), &[0])]);
        let result = traverse_fixed_queue(
            request(
                &snapshot,
                &queue,
                TerminalDepthContract::QueueExhaustedOnly,
                budgets(4, 1, 3, 1),
            ),
            &mut provider,
            &mut exhausted_terminal,
            &guard(),
        )
        .expect("finite repeated-state traversal");

        assert_eq!(target_paths(&result), vec![vec![0, 0, 0]]);
        assert_eq!(provider.calls.len(), 3);
    }

    #[test]
    fn zero_outgoing_edges_are_a_valid_dead_end() {
        let snapshot = snapshot();
        let queue = [Pc4GraphPiece::T];
        let mut provider = provider(&[]);
        let result = traverse_fixed_queue(
            request(
                &snapshot,
                &queue,
                TerminalDepthContract::QueueExhaustedOnly,
                budgets(2, 1, 1, 1),
            ),
            &mut provider,
            &mut exhausted_terminal,
            &guard(),
        )
        .expect("empty adjacency is not a provider miss");

        assert!(result.paths().is_empty());
        assert_eq!(result.adjacency_queries(), 1);
    }

    #[test]
    fn terminal_at_wrong_depth_requires_explicit_early_contract() {
        let snapshot = snapshot();
        let queue = [Pc4GraphPiece::I, Pc4GraphPiece::O];
        let graph = [
            ((0, Pc4GraphPiece::I), &[1][..]),
            ((1, Pc4GraphPiece::O), &[2][..]),
        ];
        let terminal_at_one = |query: &FixedQueueTerminalQuery<'_>| {
            assert_eq!(query.use_case(), Pc4TerminalUseCase::PcSearch);
            assert_eq!(query.target_lines().get(), 4);
            Ok::<bool, Infallible>(query.field_id() == 1)
        };

        let mut strict_provider = provider(&graph);
        let strict = traverse_fixed_queue(
            request(
                &snapshot,
                &queue,
                TerminalDepthContract::QueueExhaustedOnly,
                budgets(4, 1, 2, 1),
            ),
            &mut strict_provider,
            &mut { terminal_at_one },
            &guard(),
        )
        .expect("strict depth traversal");
        assert!(strict.paths().is_empty());
        assert_eq!(strict_provider.calls.len(), 2);

        let mut early_provider = provider(&graph);
        let early = traverse_fixed_queue(
            request(
                &snapshot,
                &queue,
                TerminalDepthContract::PredicateMayTerminateEarly,
                budgets(4, 1, 2, 1),
            ),
            &mut early_provider,
            &mut { terminal_at_one },
            &guard(),
        )
        .expect("explicit early-terminal traversal");
        assert_eq!(target_paths(&early), vec![vec![1]]);
        assert_eq!(early_provider.calls.len(), 1);
    }

    #[test]
    fn cancellation_and_stale_snapshot_fail_before_or_during_callbacks() {
        let snapshot = snapshot();
        let queue = [Pc4GraphPiece::I];

        let cancelled_guard = guard();
        cancelled_guard.cancelled.set(true);
        let mut never_called = provider(&[((0, Pc4GraphPiece::I), &[1])]);
        assert_eq!(
            traverse_fixed_queue(
                request(
                    &snapshot,
                    &queue,
                    TerminalDepthContract::QueueExhaustedOnly,
                    budgets(2, 1, 1, 1),
                ),
                &mut never_called,
                &mut exhausted_terminal,
                &cancelled_guard,
            ),
            Err(FixedQueueTraversalError::Cancelled)
        );
        assert!(never_called.calls.is_empty());

        let cancelled_during_predicate_guard = guard();
        let predicate_cancelled = Rc::clone(&cancelled_during_predicate_guard.cancelled);
        let mut cancels_during_predicate = move |_: &FixedQueueTerminalQuery<'_>| {
            predicate_cancelled.set(true);
            Ok::<bool, Infallible>(false)
        };
        let mut not_reached = provider(&[((0, Pc4GraphPiece::I), &[1])]);
        assert_eq!(
            traverse_fixed_queue(
                request(
                    &snapshot,
                    &queue,
                    TerminalDepthContract::QueueExhaustedOnly,
                    budgets(2, 1, 1, 1),
                ),
                &mut not_reached,
                &mut cancels_during_predicate,
                &cancelled_during_predicate_guard,
            ),
            Err(FixedQueueTraversalError::Cancelled)
        );
        assert!(not_reached.calls.is_empty());

        let cancelled_during_provider_guard = guard();
        let mut cancelled_during_provider = provider(&[((0, Pc4GraphPiece::I), &[1])]);
        cancelled_during_provider.cancel_during_call =
            Some(Rc::clone(&cancelled_during_provider_guard.cancelled));
        assert_eq!(
            traverse_fixed_queue(
                request(
                    &snapshot,
                    &queue,
                    TerminalDepthContract::QueueExhaustedOnly,
                    budgets(2, 1, 1, 1),
                ),
                &mut cancelled_during_provider,
                &mut exhausted_terminal,
                &cancelled_during_provider_guard,
            ),
            Err(FixedQueueTraversalError::Cancelled)
        );

        let stale_guard = guard();
        let mut becomes_stale = provider(&[((0, Pc4GraphPiece::I), &[1])]);
        becomes_stale.stale_during_call = Some(Rc::clone(&stale_guard.stale));
        assert_eq!(
            traverse_fixed_queue(
                request(
                    &snapshot,
                    &queue,
                    TerminalDepthContract::QueueExhaustedOnly,
                    budgets(2, 1, 1, 1),
                ),
                &mut becomes_stale,
                &mut exhausted_terminal,
                &stale_guard,
            ),
            Err(FixedQueueTraversalError::StaleSnapshot)
        );
    }

    #[test]
    fn provider_profile_and_every_response_binding_fail_closed() {
        let snapshot = snapshot();
        let differently_qualified = other_snapshot();
        assert_eq!(
            snapshot.snapshot().snapshot_identity(),
            differently_qualified.snapshot_identity()
        );
        assert_ne!(
            snapshot.snapshot().manifest_content_identity(),
            differently_qualified.manifest_content_identity()
        );
        let queue = [Pc4GraphPiece::I];
        let traversal_request = || {
            request(
                &snapshot,
                &queue,
                TerminalDepthContract::QueueExhaustedOnly,
                budgets(2, 1, 1, 1),
            )
        };

        let mut wrong_profile_provider = provider(&[]);
        wrong_profile_provider.profile = Pc4RuleProfile::NoKick;
        assert!(matches!(
            traverse_fixed_queue(
                traversal_request(),
                &mut wrong_profile_provider,
                &mut exhausted_terminal,
                &guard(),
            ),
            Err(FixedQueueTraversalError::Semantic(
                FixedQueueTraversalSemanticError::ProviderProfileMismatch { .. }
            ))
        ));

        let faults = [
            ResponseFault::WrongSnapshot,
            ResponseFault::WrongProfile,
            ResponseFault::WrongSource,
            ResponseFault::WrongPiece,
            ResponseFault::WrongQueueIndex,
            ResponseFault::WrongEdgeSnapshot,
            ResponseFault::WrongEdgeProfile,
            ResponseFault::WrongEdgeSource,
            ResponseFault::WrongEdgePiece,
        ];
        for fault in faults {
            let mut faulty = provider(&[((0, Pc4GraphPiece::I), &[1])]);
            faulty.response_fault = fault;
            assert!(matches!(
                traverse_fixed_queue(
                    traversal_request(),
                    &mut faulty,
                    &mut exhausted_terminal,
                    &guard(),
                ),
                Err(FixedQueueTraversalError::Semantic(_))
            ));
        }
    }

    #[test]
    fn provider_failure_is_typed_and_cannot_return_partial_paths() {
        let snapshot = snapshot();
        let queue = [Pc4GraphPiece::I];
        let mut rejected = provider(&[]);
        rejected.fail = true;
        assert_eq!(
            traverse_fixed_queue(
                request(
                    &snapshot,
                    &queue,
                    TerminalDepthContract::QueueExhaustedOnly,
                    budgets(2, 1, 1, 1),
                ),
                &mut rejected,
                &mut exhausted_terminal,
                &guard(),
            ),
            Err(FixedQueueTraversalError::Provider(
                SyntheticProviderError::Rejected
            ))
        );
    }

    #[test]
    fn every_budget_exhaustion_is_typed() {
        let snapshot = snapshot();

        let cases = [
            (
                [Pc4GraphPiece::I, Pc4GraphPiece::O].as_slice(),
                &[((0, Pc4GraphPiece::I), &[1][..])][..],
                budgets(1, 2, 2, 2),
                FixedQueueBudgetKind::VisitedStateOccurrences,
            ),
            (
                [Pc4GraphPiece::I].as_slice(),
                &[((0, Pc4GraphPiece::I), &[1, 2][..])][..],
                budgets(3, 1, 1, 2),
                FixedQueueBudgetKind::FrontierPaths,
            ),
            (
                [Pc4GraphPiece::I, Pc4GraphPiece::O].as_slice(),
                &[((0, Pc4GraphPiece::I), &[1][..])][..],
                budgets(3, 1, 1, 2),
                FixedQueueBudgetKind::PathEdges,
            ),
            (
                [Pc4GraphPiece::I].as_slice(),
                &[((0, Pc4GraphPiece::I), &[1, 2][..])][..],
                budgets(3, 2, 1, 1),
                FixedQueueBudgetKind::OutputPaths,
            ),
        ];

        for (queue, graph, budget, expected_kind) in cases {
            let mut provider = provider(graph);
            let error = traverse_fixed_queue(
                request(
                    &snapshot,
                    queue,
                    TerminalDepthContract::QueueExhaustedOnly,
                    budget,
                ),
                &mut provider,
                &mut exhausted_terminal,
                &guard(),
            )
            .expect_err("budget must fail closed");
            assert!(matches!(
                error,
                FixedQueueTraversalError::BudgetExceeded(FixedQueueBudgetExceeded {
                    kind,
                    ..
                }) if kind == expected_kind
            ));
        }
    }
}
