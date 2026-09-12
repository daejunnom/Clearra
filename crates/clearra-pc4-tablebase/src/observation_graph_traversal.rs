// SRP rationale: this module has one behavior-level change reason: page one
// target-qualified graph traversal across the existing observation frontier.
use core::{fmt, num::NonZeroUsize};
use std::sync::Arc;

use crate::{
    prepare_fixed_queue_traversal_family, FixedQueueGraphPath, FixedQueueTerminalPredicate,
    FixedQueueTraversalBudgets, FixedQueueTraversalCursor, FixedQueueTraversalFamily,
    FixedQueueTraversalFamilyRequest, FixedQueueTraversalGuard, FixedQueueTraversalPageBudgets,
    FixedQueueTraversalPageError, FixedQueueTraversalPrepareError, Pc4ExactProbability,
    Pc4ObservationFrontierCursor, Pc4ObservationFrontierEntry, Pc4ObservationFrontierFamily,
    Pc4ObservationFrontierGuard, Pc4ObservationFrontierPageError, Pc4ObservationQueueScope,
    Pc4ObservationRevealLedgerFamily, QualifiedCompleteAdjacencyProvider,
    QualifiedPc4TargetIdentity, QualifiedSnapshotIdentity, TerminalDepthContract,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ObservationGraphBudgetKind {
    FrontierEntries,
    VisitedStateOccurrences,
    AdjacencyQueries,
    OutputPaths,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4ObservationGraphBudgetExceeded {
    kind: Pc4ObservationGraphBudgetKind,
    limit: usize,
    attempted: usize,
}

impl Pc4ObservationGraphBudgetExceeded {
    pub const fn kind(self) -> Pc4ObservationGraphBudgetKind {
        self.kind
    }

    pub const fn limit(self) -> usize {
        self.limit
    }

    pub const fn attempted(self) -> usize {
        self.attempted
    }
}

/// Aggregate family limits and per-call work slices.
///
/// The nested frontier and graph families retain their own finer-grained
/// limits. These limits additionally bound work accumulated across every
/// reveal/hold branch so no caller can multiply a per-branch graph allowance
/// into an unbounded composite request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4ObservationGraphBudgets {
    frontier_entries: NonZeroUsize,
    visited_state_occurrences: NonZeroUsize,
    adjacency_queries: NonZeroUsize,
    output_paths: NonZeroUsize,
    page_frontier_entries: NonZeroUsize,
    page_graph_calls: NonZeroUsize,
    page_output_paths: NonZeroUsize,
}

impl Pc4ObservationGraphBudgets {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        frontier_entries: NonZeroUsize,
        visited_state_occurrences: NonZeroUsize,
        adjacency_queries: NonZeroUsize,
        output_paths: NonZeroUsize,
        page_frontier_entries: NonZeroUsize,
        page_graph_calls: NonZeroUsize,
        page_output_paths: NonZeroUsize,
    ) -> Self {
        Self {
            frontier_entries,
            visited_state_occurrences,
            adjacency_queries,
            output_paths,
            page_frontier_entries,
            page_graph_calls,
            page_output_paths,
        }
    }

    pub const fn frontier_entries(self) -> usize {
        self.frontier_entries.get()
    }

    pub const fn visited_state_occurrences(self) -> usize {
        self.visited_state_occurrences.get()
    }

    pub const fn adjacency_queries(self) -> usize {
        self.adjacency_queries.get()
    }

    pub const fn output_paths(self) -> usize {
        self.output_paths.get()
    }

    pub const fn page_frontier_entries(self) -> usize {
        self.page_frontier_entries.get()
    }

    pub const fn page_graph_calls(self) -> usize {
        self.page_graph_calls.get()
    }

    pub const fn page_output_paths(self) -> usize {
        self.page_output_paths.get()
    }
}

/// Owned preparation request for one source field and one qualified target.
/// Preparing the family performs no graph callback.
pub struct Pc4ObservationGraphRequest {
    target: QualifiedPc4TargetIdentity,
    source_field_id: u32,
    frontier: Pc4ObservationFrontierFamily,
    terminal_depth_contract: TerminalDepthContract,
    graph_traversal_budgets: FixedQueueTraversalBudgets,
    graph_page_budgets: FixedQueueTraversalPageBudgets,
    budgets: Pc4ObservationGraphBudgets,
}

impl Pc4ObservationGraphRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target: QualifiedPc4TargetIdentity,
        source_field_id: u32,
        frontier: Pc4ObservationFrontierFamily,
        terminal_depth_contract: TerminalDepthContract,
        graph_traversal_budgets: FixedQueueTraversalBudgets,
        graph_page_budgets: FixedQueueTraversalPageBudgets,
        budgets: Pc4ObservationGraphBudgets,
    ) -> Self {
        Self {
            target,
            source_field_id,
            frontier,
            terminal_depth_contract,
            graph_traversal_budgets,
            graph_page_budgets,
            budgets,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ObservationGraphPrepareError {
    Cancelled,
    StaleSnapshot,
}

impl Pc4ObservationGraphPrepareError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_observation_graph_prepare_cancelled",
            Self::StaleSnapshot => "pc4_observation_graph_prepare_stale_snapshot",
        }
    }
}

impl fmt::Display for Pc4ObservationGraphPrepareError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4ObservationGraphPrepareError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4ObservationGraphPageError<ProviderError, TerminalError> {
    Cancelled,
    StaleSnapshot,
    CursorMismatch,
    CursorInvariantViolation,
    PageLimitExceeded { limit: usize, attempted: usize },
    CounterOverflow,
    AllocationFailed,
    BudgetExceeded(Pc4ObservationGraphBudgetExceeded),
    Frontier(Pc4ObservationFrontierPageError),
    TraversalPrepare(FixedQueueTraversalPrepareError),
    Traversal(FixedQueueTraversalPageError<ProviderError, TerminalError>),
}

impl<ProviderError, TerminalError> Pc4ObservationGraphPageError<ProviderError, TerminalError> {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_observation_graph_page_cancelled",
            Self::StaleSnapshot => "pc4_observation_graph_page_stale_snapshot",
            Self::CursorMismatch => "pc4_observation_graph_cursor_mismatch",
            Self::CursorInvariantViolation => "pc4_observation_graph_cursor_invariant_violation",
            Self::PageLimitExceeded { .. } => "pc4_observation_graph_page_limit_exceeded",
            Self::CounterOverflow => "pc4_observation_graph_counter_overflow",
            Self::AllocationFailed => "pc4_observation_graph_allocation_failed",
            Self::BudgetExceeded(_) => "pc4_observation_graph_budget_exceeded",
            Self::Frontier(error) => error.reason(),
            Self::TraversalPrepare(error) => error.reason(),
            Self::Traversal(error) => error.reason(),
        }
    }
}

impl<ProviderError, TerminalError> fmt::Display
    for Pc4ObservationGraphPageError<ProviderError, TerminalError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

/// One exact graph result with its complete supply and authority provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ObservationGraphPath {
    target: Arc<QualifiedPc4TargetIdentity>,
    source_field_id: u32,
    frontier_entry: Arc<Pc4ObservationFrontierEntry>,
    graph_path: FixedQueueGraphPath,
}

impl Pc4ObservationGraphPath {
    pub fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn source_field_id(&self) -> u32 {
        self.source_field_id
    }

    pub fn frontier_entry(&self) -> &Pc4ObservationFrontierEntry {
        &self.frontier_entry
    }

    pub fn probability(&self) -> Pc4ExactProbability {
        self.frontier_entry.probability()
    }

    pub const fn graph_path(&self) -> &FixedQueueGraphPath {
        &self.graph_path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ObservationGraphPage {
    paths: Vec<Pc4ObservationGraphPath>,
    frontier_entries_started: usize,
    graph_calls: usize,
    visited_state_occurrences: usize,
    adjacency_queries: usize,
    stopped_by_work_budget: bool,
    exhausted: bool,
}

impl Pc4ObservationGraphPage {
    pub fn paths(&self) -> &[Pc4ObservationGraphPath] {
        &self.paths
    }

    pub const fn frontier_entries_started(&self) -> usize {
        self.frontier_entries_started
    }

    pub const fn graph_calls(&self) -> usize {
        self.graph_calls
    }

    pub const fn visited_state_occurrences(&self) -> usize {
        self.visited_state_occurrences
    }

    pub const fn adjacency_queries(&self) -> usize {
        self.adjacency_queries
    }

    pub const fn stopped_by_work_budget(&self) -> bool {
        self.stopped_by_work_budget
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

#[derive(Clone, Debug)]
struct ActiveObservationTraversal {
    frontier_entry: Arc<Pc4ObservationFrontierEntry>,
    family: FixedQueueTraversalFamily,
    cursor: FixedQueueTraversalCursor,
}

#[derive(Clone, Debug)]
pub struct Pc4ObservationGraphCursor {
    family_token: Arc<()>,
    frontier_cursor: Pc4ObservationFrontierCursor,
    active: Option<ActiveObservationTraversal>,
    frontier_entries_started: usize,
    visited_state_occurrences: usize,
    adjacency_queries: usize,
    emitted_paths: usize,
    exhausted: bool,
}

impl Pc4ObservationGraphCursor {
    pub const fn frontier_entries_started(&self) -> usize {
        self.frontier_entries_started
    }

    pub const fn visited_state_occurrences(&self) -> usize {
        self.visited_state_occurrences
    }

    pub const fn adjacency_queries(&self) -> usize {
        self.adjacency_queries
    }

    pub const fn emitted_paths(&self) -> usize {
        self.emitted_paths
    }

    pub fn frontier_next_entry_index(&self) -> u128 {
        self.frontier_cursor.next_entry_index()
    }

    pub fn active_frontier_entry_index(&self) -> Option<u128> {
        self.active
            .as_ref()
            .map(|active| active.frontier_entry.entry_index())
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

#[derive(Clone, Debug)]
pub struct Pc4ObservationGraphFamily {
    target: Arc<QualifiedPc4TargetIdentity>,
    source_field_id: u32,
    frontier: Pc4ObservationFrontierFamily,
    terminal_depth_contract: TerminalDepthContract,
    graph_traversal_budgets: FixedQueueTraversalBudgets,
    graph_page_budgets: FixedQueueTraversalPageBudgets,
    budgets: Pc4ObservationGraphBudgets,
    cursor_token: Arc<()>,
}

impl Pc4ObservationGraphFamily {
    pub fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn source_field_id(&self) -> u32 {
        self.source_field_id
    }

    pub const fn budgets(&self) -> Pc4ObservationGraphBudgets {
        self.budgets
    }

    pub const fn queue_scope(&self) -> &Pc4ObservationQueueScope {
        self.frontier.queue_scope()
    }

    /// Produces a reveal-rank ledger from this graph family's own canonical bag
    /// enumerator. No caller-provided reveal list can be substituted.
    pub fn reveal_ledger_family(&self) -> Pc4ObservationRevealLedgerFamily {
        Pc4ObservationRevealLedgerFamily::from_graph_parts(
            Arc::clone(&self.target),
            self.source_field_id,
            Arc::new(self.frontier.queue_scope().clone()),
            self.frontier.reveal_family(),
            self.frontier.budgets().reveal().page_sequences(),
        )
    }

    pub fn cursor(&self) -> Pc4ObservationGraphCursor {
        Pc4ObservationGraphCursor {
            family_token: Arc::clone(&self.cursor_token),
            frontier_cursor: self.frontier.cursor(),
            active: None,
            frontier_entries_started: 0,
            visited_state_occurrences: 0,
            adjacency_queries: 0,
            emitted_paths: 0,
            exhausted: false,
        }
    }

    /// Performs bounded work against at most the configured number of frontier
    /// entries and graph pages. All cursor/output changes are staged until the
    /// final guard check succeeds.
    pub fn next_page<P, T, G>(
        &self,
        cursor: &mut Pc4ObservationGraphCursor,
        limit: NonZeroUsize,
        provider: &mut P,
        terminal_predicate: &mut T,
        guard: &G,
    ) -> Result<Pc4ObservationGraphPage, Pc4ObservationGraphPageError<P::Error, T::Error>>
    where
        P: QualifiedCompleteAdjacencyProvider,
        T: FixedQueueTerminalPredicate,
        G: FixedQueueTraversalGuard,
    {
        if !Arc::ptr_eq(&cursor.family_token, &self.cursor_token) {
            return Err(Pc4ObservationGraphPageError::CursorMismatch);
        }
        if limit.get() > self.budgets.page_output_paths() {
            return Err(Pc4ObservationGraphPageError::PageLimitExceeded {
                limit: self.budgets.page_output_paths(),
                attempted: limit.get(),
            });
        }
        check_page_guard(self.target.snapshot(), guard)?;

        let mut transaction = cursor.clone();
        let mut paths = Vec::new();
        paths
            .try_reserve_exact(limit.get())
            .map_err(|_| Pc4ObservationGraphPageError::AllocationFailed)?;
        let mut page_frontier_entries = 0usize;
        let mut page_graph_calls = 0usize;
        let mut page_visited = 0usize;
        let mut page_adjacency_queries = 0usize;
        let mut stopped_by_work_budget = false;

        while paths.len() < limit.get() && !transaction.exhausted {
            check_page_guard(self.target.snapshot(), guard)?;
            if transaction.active.is_none() {
                if page_frontier_entries == self.budgets.page_frontier_entries() {
                    stopped_by_work_budget = true;
                    break;
                }
                let frontier_page = self
                    .frontier
                    .next_page(
                        &mut transaction.frontier_cursor,
                        NonZeroUsize::MIN,
                        &CancellationAdapter(guard),
                    )
                    .map_err(map_frontier_error)?;
                let Some(frontier_entry) = frontier_page.entries().first().cloned() else {
                    transaction.exhausted = frontier_page.is_exhausted();
                    stopped_by_work_budget = !transaction.exhausted;
                    break;
                };
                transaction.frontier_entries_started = consume_budget(
                    transaction.frontier_entries_started,
                    self.budgets.frontier_entries(),
                    Pc4ObservationGraphBudgetKind::FrontierEntries,
                )?;
                page_frontier_entries = checked_increment(page_frontier_entries)?;
                let family = prepare_fixed_queue_traversal_family(
                    FixedQueueTraversalFamilyRequest::new(
                        &self.target,
                        self.source_field_id,
                        frontier_entry.hold_path().placement_queue(),
                        self.terminal_depth_contract,
                        self.graph_traversal_budgets,
                        self.graph_page_budgets,
                    ),
                    guard,
                )
                .map_err(map_traversal_prepare_error)?;
                let graph_cursor = family.cursor();
                transaction.active = Some(ActiveObservationTraversal {
                    frontier_entry: Arc::new(frontier_entry),
                    family,
                    cursor: graph_cursor,
                });
            }

            if page_graph_calls == self.budgets.page_graph_calls() {
                stopped_by_work_budget = true;
                break;
            }
            let lifetime_output_remaining = self
                .budgets
                .output_paths()
                .checked_sub(transaction.emitted_paths)
                .ok_or(Pc4ObservationGraphPageError::CursorInvariantViolation)?;
            if lifetime_output_remaining == 0 {
                return Err(Pc4ObservationGraphPageError::BudgetExceeded(
                    Pc4ObservationGraphBudgetExceeded {
                        kind: Pc4ObservationGraphBudgetKind::OutputPaths,
                        limit: self.budgets.output_paths(),
                        attempted: transaction
                            .emitted_paths
                            .checked_add(1)
                            .ok_or(Pc4ObservationGraphPageError::CounterOverflow)?,
                    },
                ));
            }
            let requested_output_remaining = limit.get() - paths.len();
            let graph_page_limit = requested_output_remaining
                .min(lifetime_output_remaining)
                .min(self.graph_page_budgets.output_paths());
            let active = transaction
                .active
                .as_mut()
                .ok_or(Pc4ObservationGraphPageError::CursorInvariantViolation)?;
            let graph_page = active
                .family
                .next_page(
                    &mut active.cursor,
                    NonZeroUsize::new(graph_page_limit)
                        .ok_or(Pc4ObservationGraphPageError::CursorInvariantViolation)?,
                    provider,
                    terminal_predicate,
                    guard,
                )
                .map_err(map_traversal_page_error)?;
            page_graph_calls = checked_increment(page_graph_calls)?;
            page_visited = checked_add(page_visited, graph_page.visited_state_occurrences())?;
            page_adjacency_queries =
                checked_add(page_adjacency_queries, graph_page.adjacency_queries())?;
            transaction.visited_state_occurrences = consume_batch_budget(
                transaction.visited_state_occurrences,
                graph_page.visited_state_occurrences(),
                self.budgets.visited_state_occurrences(),
                Pc4ObservationGraphBudgetKind::VisitedStateOccurrences,
            )?;
            transaction.adjacency_queries = consume_batch_budget(
                transaction.adjacency_queries,
                graph_page.adjacency_queries(),
                self.budgets.adjacency_queries(),
                Pc4ObservationGraphBudgetKind::AdjacencyQueries,
            )?;

            let frontier_entry = Arc::clone(&active.frontier_entry);
            for graph_path in graph_page.paths() {
                transaction.emitted_paths = consume_budget(
                    transaction.emitted_paths,
                    self.budgets.output_paths(),
                    Pc4ObservationGraphBudgetKind::OutputPaths,
                )?;
                paths.push(Pc4ObservationGraphPath {
                    target: Arc::clone(&self.target),
                    source_field_id: self.source_field_id,
                    frontier_entry: Arc::clone(&frontier_entry),
                    graph_path: graph_path.clone(),
                });
            }

            if graph_page.is_exhausted() {
                transaction.active = None;
                transaction.exhausted = transaction.frontier_cursor.is_exhausted();
            } else if graph_page.paths().is_empty() && graph_page.visited_state_occurrences() == 0 {
                return Err(Pc4ObservationGraphPageError::CursorInvariantViolation);
            }
        }

        check_page_guard(self.target.snapshot(), guard)?;
        let exhausted = transaction.exhausted;
        *cursor = transaction;
        Ok(Pc4ObservationGraphPage {
            paths,
            frontier_entries_started: page_frontier_entries,
            graph_calls: page_graph_calls,
            visited_state_occurrences: page_visited,
            adjacency_queries: page_adjacency_queries,
            stopped_by_work_budget,
            exhausted,
        })
    }
}

pub fn prepare_pc4_observation_graph_family<G>(
    request: Pc4ObservationGraphRequest,
    guard: &G,
) -> Result<Pc4ObservationGraphFamily, Pc4ObservationGraphPrepareError>
where
    G: FixedQueueTraversalGuard,
{
    check_prepare_guard(request.target.snapshot(), guard)?;
    let family = Pc4ObservationGraphFamily {
        target: Arc::new(request.target),
        source_field_id: request.source_field_id,
        frontier: request.frontier,
        terminal_depth_contract: request.terminal_depth_contract,
        graph_traversal_budgets: request.graph_traversal_budgets,
        graph_page_budgets: request.graph_page_budgets,
        budgets: request.budgets,
        cursor_token: Arc::new(()),
    };
    check_prepare_guard(family.target.snapshot(), guard)?;
    Ok(family)
}

struct CancellationAdapter<'a, G>(&'a G);

impl<G> Pc4ObservationFrontierGuard for CancellationAdapter<'_, G>
where
    G: FixedQueueTraversalGuard,
{
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

fn check_prepare_guard<G>(
    snapshot: &QualifiedSnapshotIdentity,
    guard: &G,
) -> Result<(), Pc4ObservationGraphPrepareError>
where
    G: FixedQueueTraversalGuard,
{
    if guard.is_cancelled() {
        Err(Pc4ObservationGraphPrepareError::Cancelled)
    } else if !guard.is_current_snapshot(snapshot) {
        Err(Pc4ObservationGraphPrepareError::StaleSnapshot)
    } else {
        Ok(())
    }
}

fn check_page_guard<ProviderError, TerminalError, G>(
    snapshot: &QualifiedSnapshotIdentity,
    guard: &G,
) -> Result<(), Pc4ObservationGraphPageError<ProviderError, TerminalError>>
where
    G: FixedQueueTraversalGuard,
{
    if guard.is_cancelled() {
        Err(Pc4ObservationGraphPageError::Cancelled)
    } else if !guard.is_current_snapshot(snapshot) {
        Err(Pc4ObservationGraphPageError::StaleSnapshot)
    } else {
        Ok(())
    }
}

fn map_frontier_error<ProviderError, TerminalError>(
    error: Pc4ObservationFrontierPageError,
) -> Pc4ObservationGraphPageError<ProviderError, TerminalError> {
    match error {
        Pc4ObservationFrontierPageError::Cancelled => Pc4ObservationGraphPageError::Cancelled,
        error => Pc4ObservationGraphPageError::Frontier(error),
    }
}

fn map_traversal_prepare_error<ProviderError, TerminalError>(
    error: FixedQueueTraversalPrepareError,
) -> Pc4ObservationGraphPageError<ProviderError, TerminalError> {
    match error {
        FixedQueueTraversalPrepareError::Cancelled => Pc4ObservationGraphPageError::Cancelled,
        FixedQueueTraversalPrepareError::StaleSnapshot => {
            Pc4ObservationGraphPageError::StaleSnapshot
        }
        error => Pc4ObservationGraphPageError::TraversalPrepare(error),
    }
}

fn map_traversal_page_error<ProviderError, TerminalError>(
    error: FixedQueueTraversalPageError<ProviderError, TerminalError>,
) -> Pc4ObservationGraphPageError<ProviderError, TerminalError> {
    match error {
        FixedQueueTraversalPageError::Cancelled => Pc4ObservationGraphPageError::Cancelled,
        FixedQueueTraversalPageError::StaleSnapshot => Pc4ObservationGraphPageError::StaleSnapshot,
        error => Pc4ObservationGraphPageError::Traversal(error),
    }
}

fn consume_budget<ProviderError, TerminalError>(
    current: usize,
    limit: usize,
    kind: Pc4ObservationGraphBudgetKind,
) -> Result<usize, Pc4ObservationGraphPageError<ProviderError, TerminalError>> {
    consume_batch_budget(current, 1, limit, kind)
}

fn consume_batch_budget<ProviderError, TerminalError>(
    current: usize,
    amount: usize,
    limit: usize,
    kind: Pc4ObservationGraphBudgetKind,
) -> Result<usize, Pc4ObservationGraphPageError<ProviderError, TerminalError>> {
    let attempted = current
        .checked_add(amount)
        .ok_or(Pc4ObservationGraphPageError::CounterOverflow)?;
    if attempted > limit {
        Err(Pc4ObservationGraphPageError::BudgetExceeded(
            Pc4ObservationGraphBudgetExceeded {
                kind,
                limit,
                attempted,
            },
        ))
    } else {
        Ok(attempted)
    }
}

fn checked_increment<ProviderError, TerminalError>(
    value: usize,
) -> Result<usize, Pc4ObservationGraphPageError<ProviderError, TerminalError>> {
    value
        .checked_add(1)
        .ok_or(Pc4ObservationGraphPageError::CounterOverflow)
}

fn checked_add<ProviderError, TerminalError>(
    left: usize,
    right: usize,
) -> Result<usize, Pc4ObservationGraphPageError<ProviderError, TerminalError>> {
    left.checked_add(right)
        .ok_or(Pc4ObservationGraphPageError::CounterOverflow)
}

#[cfg(test)]
#[path = "observation_graph_traversal_tests.rs"]
mod tests;
