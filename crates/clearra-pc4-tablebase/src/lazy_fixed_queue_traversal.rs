// SRP rationale: resumable paging of one qualified fixed-queue graph family is this module's single change reason.
use core::{fmt, num::NonZeroUsize};
use std::sync::Arc;

use crate::fixed_queue_suffix_memo::{FixedQueueSuffixMemo, SuffixFact, SuffixKey, SuffixMemoPage};
use crate::lazy_graph_prefix::PendingGraphPath;
use crate::{
    FixedQueueAdjacencyQuery, FixedQueueBudgetExceeded, FixedQueueBudgetKind, FixedQueueGraphPath,
    FixedQueueTerminalPredicate, FixedQueueTerminalQuery, FixedQueueTraversalBudgets,
    FixedQueueTraversalGuard, FixedQueueTraversalSemanticError, Pc4GraphPiece, Pc4RuleProfile,
    QualifiedCompleteAdjacency, QualifiedCompleteAdjacencyProvider, QualifiedPc4GraphEdge,
    QualifiedPc4TargetIdentity, QualifiedSnapshotIdentity, TerminalDepthContract,
};

#[cfg(test)]
#[path = "lazy_fixed_queue_suffix_tests.rs"]
mod suffix_tests;

/// Per-call limits for resumable traversal work and emitted graph paths.
///
/// The lifetime budgets in [`FixedQueueTraversalBudgets`] still bound the
/// complete cursor. These limits prevent one page request from walking the
/// whole graph family before returning control to its caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedQueueTraversalPageBudgets {
    state_occurrences: NonZeroUsize,
    output_paths: NonZeroUsize,
}

impl FixedQueueTraversalPageBudgets {
    pub const fn new(state_occurrences: NonZeroUsize, output_paths: NonZeroUsize) -> Self {
        Self {
            state_occurrences,
            output_paths,
        }
    }

    pub const fn state_occurrences(self) -> usize {
        self.state_occurrences.get()
    }

    pub const fn output_paths(self) -> usize {
        self.output_paths.get()
    }
}

/// Immutable request used to prepare a feature-off lazy traversal family.
/// A verified profile/use-case/target identity is mandatory even before a
/// product adapter exists, so paging cannot bypass target completeness.
pub struct FixedQueueTraversalFamilyRequest<'a> {
    target: &'a QualifiedPc4TargetIdentity,
    start_field_id: u32,
    queue: &'a [Pc4GraphPiece],
    terminal_depth_contract: TerminalDepthContract,
    traversal_budgets: FixedQueueTraversalBudgets,
    page_budgets: FixedQueueTraversalPageBudgets,
}

impl<'a> FixedQueueTraversalFamilyRequest<'a> {
    pub const fn new(
        target: &'a QualifiedPc4TargetIdentity,
        start_field_id: u32,
        queue: &'a [Pc4GraphPiece],
        terminal_depth_contract: TerminalDepthContract,
        traversal_budgets: FixedQueueTraversalBudgets,
        page_budgets: FixedQueueTraversalPageBudgets,
    ) -> Self {
        Self {
            target,
            start_field_id,
            queue,
            terminal_depth_contract,
            traversal_budgets,
            page_budgets,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixedQueueTraversalPrepareError {
    Cancelled,
    StaleSnapshot,
    AllocationFailed,
}

impl FixedQueueTraversalPrepareError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_fixed_queue_traversal_prepare_cancelled",
            Self::StaleSnapshot => "pc4_fixed_queue_traversal_prepare_stale_snapshot",
            Self::AllocationFailed => "pc4_fixed_queue_traversal_prepare_allocation_failed",
        }
    }
}

impl fmt::Display for FixedQueueTraversalPrepareError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for FixedQueueTraversalPrepareError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FixedQueueTraversalPageError<ProviderError, TerminalError> {
    Cancelled,
    StaleSnapshot,
    CursorMismatch,
    CursorInvariantViolation,
    PageLimitExceeded { limit: usize, attempted: usize },
    CounterOverflow,
    AllocationFailed,
    BudgetExceeded(FixedQueueBudgetExceeded),
    Provider(ProviderError),
    TerminalPredicate(TerminalError),
    Semantic(FixedQueueTraversalSemanticError),
}

impl<ProviderError, TerminalError> FixedQueueTraversalPageError<ProviderError, TerminalError> {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_fixed_queue_traversal_page_cancelled",
            Self::StaleSnapshot => "pc4_fixed_queue_traversal_page_stale_snapshot",
            Self::CursorMismatch => "pc4_fixed_queue_traversal_page_cursor_mismatch",
            Self::CursorInvariantViolation => {
                "pc4_fixed_queue_traversal_page_cursor_invariant_violation"
            }
            Self::PageLimitExceeded { .. } => "pc4_fixed_queue_traversal_page_limit_exceeded",
            Self::CounterOverflow => "pc4_fixed_queue_traversal_page_counter_overflow",
            Self::AllocationFailed => "pc4_fixed_queue_traversal_page_allocation_failed",
            Self::BudgetExceeded(_) => "pc4_fixed_queue_traversal_page_budget_exceeded",
            Self::Provider(_) => "pc4_fixed_queue_adjacency_provider_failed",
            Self::TerminalPredicate(_) => "pc4_fixed_queue_terminal_predicate_failed",
            Self::Semantic(error) => error.reason(),
        }
    }
}

impl<ProviderError, TerminalError> fmt::Display
    for FixedQueueTraversalPageError<ProviderError, TerminalError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedQueueTraversalPage {
    paths: Vec<FixedQueueGraphPath>,
    visited_state_occurrences: usize,
    adjacency_queries: usize,
    duplicate_edges_suppressed: usize,
    suffix_memo_hits: usize,
    empty_suffixes_skipped: usize,
    stopped_by_work_budget: bool,
    exhausted: bool,
}

impl FixedQueueTraversalPage {
    pub fn paths(&self) -> &[FixedQueueGraphPath] {
        &self.paths
    }

    pub const fn visited_state_occurrences(&self) -> usize {
        self.visited_state_occurrences
    }

    pub const fn adjacency_queries(&self) -> usize {
        self.adjacency_queries
    }

    pub const fn duplicate_edges_suppressed(&self) -> usize {
        self.duplicate_edges_suppressed
    }

    pub const fn suffix_memo_hits(&self) -> usize {
        self.suffix_memo_hits
    }

    pub const fn empty_suffixes_skipped(&self) -> usize {
        self.empty_suffixes_skipped
    }

    pub const fn stopped_by_work_budget(&self) -> bool {
        self.stopped_by_work_budget
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

#[derive(Clone, Debug)]
pub struct FixedQueueTraversalCursor {
    family_token: Arc<()>,
    pending_paths: Vec<PendingGraphPath>,
    suffix_memo: FixedQueueSuffixMemo,
    suffix_completions: Vec<SuffixCompletion>,
    manifest_terminal_mode: Option<bool>,
    visited_state_occurrences: usize,
    adjacency_queries: usize,
    emitted_paths: usize,
    duplicate_edges_suppressed: usize,
    suffix_memo_hits: usize,
    empty_suffixes_skipped: usize,
    exhausted: bool,
}

#[derive(Clone, Debug)]
struct SuffixCompletion {
    key: SuffixKey,
    pending_outside_subtree: usize,
    emitted_before_subtree: usize,
}

impl FixedQueueTraversalCursor {
    pub const fn visited_state_occurrences(&self) -> usize {
        self.visited_state_occurrences
    }

    pub const fn adjacency_queries(&self) -> usize {
        self.adjacency_queries
    }

    pub const fn emitted_paths(&self) -> usize {
        self.emitted_paths
    }

    pub const fn duplicate_edges_suppressed(&self) -> usize {
        self.duplicate_edges_suppressed
    }

    pub const fn suffix_memo_hits(&self) -> usize {
        self.suffix_memo_hits
    }

    pub const fn empty_suffixes_skipped(&self) -> usize {
        self.empty_suffixes_skipped
    }

    // Only the observation-family owner can share facts across its own reveal
    // and hold entries. Source provenance and cursor positions stay separate.
    pub(crate) fn share_suffix_memo(&mut self, memo: &FixedQueueSuffixMemo) {
        self.suffix_memo = memo.clone();
    }

    fn finish_suffixes<PE, TE>(
        &mut self,
        memo_page: &mut SuffixMemoPage,
    ) -> Result<(), FixedQueueTraversalPageError<PE, TE>> {
        while let Some(completion) = self.suffix_completions.last() {
            match self
                .pending_paths
                .len()
                .cmp(&completion.pending_outside_subtree)
            {
                core::cmp::Ordering::Greater => break,
                core::cmp::Ordering::Less => {
                    return Err(FixedQueueTraversalPageError::CursorInvariantViolation);
                }
                core::cmp::Ordering::Equal => {}
            }
            let completion = self
                .suffix_completions
                .pop()
                .ok_or(FixedQueueTraversalPageError::CursorInvariantViolation)?;
            if self.emitted_paths == completion.emitted_before_subtree {
                memo_page.stage_empty(completion.key);
            }
        }
        Ok(())
    }

    pub fn pending_path_count(&self) -> usize {
        self.pending_paths.len()
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

/// Prepared immutable feature-off traversal identity. No graph lookup occurs
/// until a page is requested, and this type is not product activation or
/// target-completeness evidence.
#[derive(Clone, Debug)]
pub struct FixedQueueTraversalFamily {
    target: QualifiedPc4TargetIdentity,
    start_field_id: u32,
    queue: Vec<Pc4GraphPiece>,
    terminal_depth_contract: TerminalDepthContract,
    traversal_budgets: FixedQueueTraversalBudgets,
    page_budgets: FixedQueueTraversalPageBudgets,
    cursor_token: Arc<()>,
    #[cfg(test)]
    copied_prefix_for_test: bool,
}

impl FixedQueueTraversalFamily {
    pub const fn snapshot(&self) -> &QualifiedSnapshotIdentity {
        self.target.snapshot()
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.target.profile()
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn start_field_id(&self) -> u32 {
        self.start_field_id
    }

    pub fn queue(&self) -> &[Pc4GraphPiece] {
        &self.queue
    }

    pub const fn terminal_depth_contract(&self) -> TerminalDepthContract {
        self.terminal_depth_contract
    }

    pub const fn traversal_budgets(&self) -> FixedQueueTraversalBudgets {
        self.traversal_budgets
    }

    pub const fn page_budgets(&self) -> FixedQueueTraversalPageBudgets {
        self.page_budgets
    }

    pub fn cursor(&self) -> FixedQueueTraversalCursor {
        let root = PendingGraphPath::root(self.start_field_id);
        #[cfg(test)]
        let root = if self.copied_prefix_for_test {
            PendingGraphPath::copied_root(self.start_field_id)
        } else {
            root
        };
        FixedQueueTraversalCursor {
            family_token: Arc::clone(&self.cursor_token),
            pending_paths: vec![root],
            suffix_memo: FixedQueueSuffixMemo::new(
                &self.target,
                self.traversal_budgets.visited_state_occurrences(),
            ),
            suffix_completions: Vec::new(),
            manifest_terminal_mode: None,
            visited_state_occurrences: 0,
            adjacency_queries: 0,
            emitted_paths: 0,
            duplicate_edges_suppressed: 0,
            suffix_memo_hits: 0,
            empty_suffixes_skipped: 0,
            exhausted: false,
        }
    }

    /// Emits a canonical bounded page without collecting the complete graph
    /// family. The cursor is committed only after the whole page succeeds.
    pub fn next_page<P, T, G>(
        &self,
        cursor: &mut FixedQueueTraversalCursor,
        limit: NonZeroUsize,
        provider: &mut P,
        terminal_predicate: &mut T,
        guard: &G,
    ) -> Result<FixedQueueTraversalPage, FixedQueueTraversalPageError<P::Error, T::Error>>
    where
        P: QualifiedCompleteAdjacencyProvider,
        T: FixedQueueTerminalPredicate,
        G: FixedQueueTraversalGuard,
    {
        if !Arc::ptr_eq(&cursor.family_token, &self.cursor_token) {
            return Err(FixedQueueTraversalPageError::CursorMismatch);
        }
        if limit.get() > self.page_budgets.output_paths() {
            return Err(FixedQueueTraversalPageError::PageLimitExceeded {
                limit: self.page_budgets.output_paths(),
                attempted: limit.get(),
            });
        }
        let snapshot = self.target.snapshot();
        check_page_guard(snapshot, guard)?;
        validate_provider_binding(&self.target, provider)?;

        let manifest_terminal_mode = match terminal_predicate.qualified_field_terminal() {
            Some(target) if target == &self.target => true,
            Some(_) => return Err(FixedQueueTraversalPageError::CursorInvariantViolation),
            None => false,
        };
        if cursor
            .manifest_terminal_mode
            .is_some_and(|mode| mode != manifest_terminal_mode)
            || !cursor.suffix_memo.matches_target(&self.target)
        {
            return Err(FixedQueueTraversalPageError::CursorInvariantViolation);
        }

        let mut transaction = cursor.clone();
        transaction.manifest_terminal_mode = Some(manifest_terminal_mode);
        let mut memo_page = transaction.suffix_memo.page();
        let mut paths = Vec::new();
        paths
            .try_reserve_exact(limit.get())
            .map_err(|_| FixedQueueTraversalPageError::AllocationFailed)?;
        let mut page_visited = 0usize;
        let mut page_adjacency_queries = 0usize;
        let mut page_duplicate_edges = 0usize;
        let mut page_memo_hits = 0usize;
        let mut page_empty_suffixes = 0usize;

        while paths.len() < limit.get() && !transaction.exhausted {
            check_page_guard(snapshot, guard)?;
            transaction.finish_suffixes(&mut memo_page)?;
            if page_visited == self.page_budgets.state_occurrences() {
                break;
            }
            let path = transaction
                .pending_paths
                .pop()
                .ok_or(FixedQueueTraversalPageError::CursorInvariantViolation)?;
            transaction.visited_state_occurrences = consume_lifetime_budget(
                transaction.visited_state_occurrences,
                self.traversal_budgets.visited_state_occurrences(),
                FixedQueueBudgetKind::VisitedStateOccurrences,
            )?;
            page_visited = checked_increment(page_visited)?;

            let consumed_pieces = path.consumed_pieces();
            let field_id = path.terminal_field_id();
            let suffix_key = if manifest_terminal_mode {
                memo_page.key(
                    field_id,
                    consumed_pieces,
                    &self.queue[consumed_pieces..],
                    self.terminal_depth_contract,
                )
            } else {
                None
            };
            let memo_fact = suffix_key.as_ref().and_then(|key| memo_page.get(key));
            if memo_fact.is_some() {
                validate_provider_binding(&self.target, provider)?;
                page_memo_hits = checked_increment(page_memo_hits)?;
                transaction.suffix_memo_hits = checked_increment(transaction.suffix_memo_hits)?;
            }
            if matches!(memo_fact, Some(SuffixFact::Empty)) {
                page_empty_suffixes = checked_increment(page_empty_suffixes)?;
                transaction.empty_suffixes_skipped =
                    checked_increment(transaction.empty_suffixes_skipped)?;
                transaction.exhausted = transaction.pending_paths.is_empty();
                continue;
            }
            let terminal_query = FixedQueueTerminalQuery::from_parts(
                &self.target,
                field_id,
                &self.queue,
                consumed_pieces,
            );
            let terminal_result = if manifest_terminal_mode {
                Ok(field_id == self.target.terminal_field().field_id())
            } else {
                terminal_predicate.is_terminal(&terminal_query)
            };
            check_page_guard(snapshot, guard)?;
            let predicate_matches =
                terminal_result.map_err(FixedQueueTraversalPageError::TerminalPredicate)?;
            let depth_permits_terminal = consumed_pieces == self.queue.len()
                || self.terminal_depth_contract
                    == TerminalDepthContract::PredicateMayTerminateEarly;
            if predicate_matches && depth_permits_terminal {
                transaction.emitted_paths = consume_lifetime_budget(
                    transaction.emitted_paths,
                    self.traversal_budgets.output_paths(),
                    FixedQueueBudgetKind::OutputPaths,
                )?;
                paths.push(
                    path.into_graph_path()
                        .map_err(|_| FixedQueueTraversalPageError::AllocationFailed)?,
                );
                transaction.exhausted = transaction.pending_paths.is_empty();
                continue;
            }
            if consumed_pieces == self.queue.len() {
                if let Some(key) = suffix_key {
                    memo_page.stage_empty(key);
                }
                transaction.exhausted = transaction.pending_paths.is_empty();
                continue;
            }

            let attempted_path_edges = consumed_pieces
                .checked_add(1)
                .ok_or(FixedQueueTraversalPageError::CounterOverflow)?;
            if attempted_path_edges > self.traversal_budgets.path_edges() {
                return Err(FixedQueueTraversalPageError::BudgetExceeded(
                    FixedQueueBudgetExceeded {
                        kind: FixedQueueBudgetKind::PathEdges,
                        limit: self.traversal_budgets.path_edges(),
                        attempted: attempted_path_edges,
                    },
                ));
            }

            let piece = self.queue[consumed_pieces];
            let adjacency_query = FixedQueueAdjacencyQuery::from_parts(
                &self.target,
                field_id,
                piece,
                consumed_pieces,
            );
            let edges = if let Some(SuffixFact::Adjacency(targets)) = memo_fact {
                let mut edges = Vec::new();
                edges
                    .try_reserve_exact(targets.len())
                    .map_err(|_| FixedQueueTraversalPageError::AllocationFailed)?;
                edges.extend(targets.iter().map(|&target_field_id| {
                    QualifiedPc4GraphEdge::from_qualified_record(
                        &self.target,
                        field_id,
                        piece,
                        target_field_id,
                    )
                }));
                edges
            } else {
                transaction.adjacency_queries = checked_increment(transaction.adjacency_queries)?;
                page_adjacency_queries = checked_increment(page_adjacency_queries)?;
                let adjacency_result = provider.complete_outgoing_edges(&adjacency_query);
                check_page_guard(snapshot, guard)?;
                validate_provider_binding(&self.target, provider)?;
                let adjacency = match adjacency_result {
                    Ok(adjacency) => adjacency,
                    Err(error) => {
                        // Keep the failed transaction uncommitted. Expose only
                        // a bounded prefix of its already queued siblings, so a
                        // host can group future byte reads without changing DFS
                        // order or pretending missing adjacency is an empty set.
                        let mut fields = [0_u32; 32];
                        fields[0] = field_id;
                        let mut count = 1;
                        for pending in transaction.pending_paths.iter().rev().take(128) {
                            if count == fields.len() {
                                break;
                            }
                            let pending_id = pending.terminal_field_id();
                            if pending.consumed_pieces() < self.queue.len()
                                && !fields[..count].contains(&pending_id)
                            {
                                fields[count] = pending_id;
                                count += 1;
                            }
                        }
                        provider.observe_unresolved_frontier(&self.target, &fields[..count]);
                        check_page_guard(snapshot, guard)?;
                        validate_provider_binding(&self.target, provider)?;
                        return Err(FixedQueueTraversalPageError::Provider(error));
                    }
                };
                validate_adjacency(&adjacency_query, &adjacency)?;

                let mut edges = adjacency.into_edges();
                edges.sort_unstable_by_key(QualifiedPc4GraphEdge::target_field_id);
                let original_edge_count = edges.len();
                edges.dedup_by_key(|edge| edge.target_field_id());
                let duplicate_count = original_edge_count - edges.len();
                page_duplicate_edges = checked_add(page_duplicate_edges, duplicate_count)?;
                transaction.duplicate_edges_suppressed =
                    checked_add(transaction.duplicate_edges_suppressed, duplicate_count)?;
                if let Some(key) = &suffix_key {
                    let mut targets = Vec::new();
                    if targets.try_reserve_exact(edges.len()).is_ok() {
                        targets.extend(edges.iter().map(QualifiedPc4GraphEdge::target_field_id));
                        memo_page.stage_adjacency(key.clone(), targets);
                    }
                }
                edges
            };

            let attempted_frontier = transaction
                .pending_paths
                .len()
                .checked_add(edges.len())
                .ok_or(FixedQueueTraversalPageError::CounterOverflow)?;
            if attempted_frontier > self.traversal_budgets.frontier_paths() {
                return Err(FixedQueueTraversalPageError::BudgetExceeded(
                    FixedQueueBudgetExceeded {
                        kind: FixedQueueBudgetKind::FrontierPaths,
                        limit: self.traversal_budgets.frontier_paths(),
                        attempted: attempted_frontier,
                    },
                ));
            }
            transaction
                .pending_paths
                .try_reserve_exact(edges.len())
                .map_err(|_| FixedQueueTraversalPageError::AllocationFailed)?;
            if let Some(key) = suffix_key {
                if edges.is_empty() {
                    memo_page.stage_empty(key);
                } else if transaction.suffix_completions.try_reserve(1).is_ok() {
                    transaction.suffix_completions.push(SuffixCompletion {
                        key,
                        pending_outside_subtree: transaction.pending_paths.len(),
                        emitted_before_subtree: transaction.emitted_paths,
                    });
                }
            }
            for edge in edges.into_iter().rev() {
                transaction.pending_paths.push(path.extended(edge));
            }
            transaction.exhausted = transaction.pending_paths.is_empty();
        }

        transaction.finish_suffixes(&mut memo_page)?;
        check_page_guard(snapshot, guard)?;
        let stopped_by_work_budget = !transaction.exhausted
            && page_visited == self.page_budgets.state_occurrences()
            && paths.len() < limit.get();
        let exhausted = transaction.exhausted;
        memo_page.commit();
        *cursor = transaction;
        Ok(FixedQueueTraversalPage {
            paths,
            visited_state_occurrences: page_visited,
            adjacency_queries: page_adjacency_queries,
            duplicate_edges_suppressed: page_duplicate_edges,
            suffix_memo_hits: page_memo_hits,
            empty_suffixes_skipped: page_empty_suffixes,
            stopped_by_work_budget,
            exhausted,
        })
    }
}

/// Owns only immutable request data. Graph callbacks remain lazy.
pub fn prepare_fixed_queue_traversal_family<G>(
    request: FixedQueueTraversalFamilyRequest<'_>,
    guard: &G,
) -> Result<FixedQueueTraversalFamily, FixedQueueTraversalPrepareError>
where
    G: FixedQueueTraversalGuard,
{
    check_prepare_guard(request.target.snapshot(), guard)?;
    let mut queue = Vec::new();
    queue
        .try_reserve_exact(request.queue.len())
        .map_err(|_| FixedQueueTraversalPrepareError::AllocationFailed)?;
    queue.extend_from_slice(request.queue);
    check_prepare_guard(request.target.snapshot(), guard)?;
    Ok(FixedQueueTraversalFamily {
        target: request.target.clone(),
        start_field_id: request.start_field_id,
        queue,
        terminal_depth_contract: request.terminal_depth_contract,
        traversal_budgets: request.traversal_budgets,
        page_budgets: request.page_budgets,
        cursor_token: Arc::new(()),
        #[cfg(test)]
        copied_prefix_for_test: false,
    })
}

fn check_prepare_guard<G>(
    snapshot: &QualifiedSnapshotIdentity,
    guard: &G,
) -> Result<(), FixedQueueTraversalPrepareError>
where
    G: FixedQueueTraversalGuard,
{
    if guard.is_cancelled() {
        Err(FixedQueueTraversalPrepareError::Cancelled)
    } else if !guard.is_current_snapshot(snapshot) {
        Err(FixedQueueTraversalPrepareError::StaleSnapshot)
    } else {
        Ok(())
    }
}

fn check_page_guard<ProviderError, TerminalError, G>(
    snapshot: &QualifiedSnapshotIdentity,
    guard: &G,
) -> Result<(), FixedQueueTraversalPageError<ProviderError, TerminalError>>
where
    G: FixedQueueTraversalGuard,
{
    if guard.is_cancelled() {
        Err(FixedQueueTraversalPageError::Cancelled)
    } else if !guard.is_current_snapshot(snapshot) {
        Err(FixedQueueTraversalPageError::StaleSnapshot)
    } else {
        Ok(())
    }
}

fn validate_provider_binding<ProviderError, TerminalError, P>(
    target: &QualifiedPc4TargetIdentity,
    provider: &P,
) -> Result<(), FixedQueueTraversalPageError<ProviderError, TerminalError>>
where
    P: QualifiedCompleteAdjacencyProvider,
{
    if provider.snapshot() != target.snapshot() {
        return Err(FixedQueueTraversalPageError::Semantic(
            FixedQueueTraversalSemanticError::ProviderSnapshotMismatch,
        ));
    }
    if provider.profile() != target.profile() {
        return Err(FixedQueueTraversalPageError::Semantic(
            FixedQueueTraversalSemanticError::ProviderProfileMismatch {
                expected: target.profile(),
                actual: provider.profile(),
            },
        ));
    }
    if provider.target() != target {
        return Err(FixedQueueTraversalPageError::Semantic(
            FixedQueueTraversalSemanticError::ProviderTargetMismatch,
        ));
    }
    Ok(())
}

/// Read one complete piece adjacency with the same qualification checks as
/// fixed-queue traversal. This is an I/O-free orchestration port: the provider
/// owns lookup/cache misses and must never label a missing record as empty.
pub fn read_qualified_pc4_adjacency<P, G>(
    target: &QualifiedPc4TargetIdentity,
    source_field_id: u32,
    piece: Pc4GraphPiece,
    placed_pieces: usize,
    provider: &mut P,
    guard: &G,
) -> Result<
    Vec<QualifiedPc4GraphEdge>,
    FixedQueueTraversalPageError<P::Error, core::convert::Infallible>,
>
where
    P: QualifiedCompleteAdjacencyProvider,
    G: FixedQueueTraversalGuard,
{
    check_page_guard(target.snapshot(), guard)?;
    validate_provider_binding(target, provider)?;
    let query = FixedQueueAdjacencyQuery::from_parts(target, source_field_id, piece, placed_pieces);
    let response = provider.complete_outgoing_edges(&query);
    check_page_guard(target.snapshot(), guard)?;
    validate_provider_binding(target, provider)?;
    let adjacency = response.map_err(FixedQueueTraversalPageError::Provider)?;
    validate_adjacency(&query, &adjacency)?;
    let mut edges = adjacency.into_edges();
    edges.sort_unstable_by_key(QualifiedPc4GraphEdge::target_field_id);
    edges.dedup_by_key(|edge| edge.target_field_id());
    check_page_guard(target.snapshot(), guard)?;
    Ok(edges)
}

fn validate_adjacency<ProviderError, TerminalError>(
    query: &FixedQueueAdjacencyQuery<'_>,
    adjacency: &QualifiedCompleteAdjacency,
) -> Result<(), FixedQueueTraversalPageError<ProviderError, TerminalError>> {
    let mismatch = if adjacency.snapshot() != query.snapshot() {
        Some(FixedQueueTraversalSemanticError::AdjacencySnapshotMismatch)
    } else if adjacency.profile() != query.profile() {
        Some(FixedQueueTraversalSemanticError::AdjacencyProfileMismatch {
            expected: query.profile(),
            actual: adjacency.profile(),
        })
    } else if adjacency.target() != query.target() {
        Some(FixedQueueTraversalSemanticError::AdjacencyTargetMismatch)
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
        return Err(FixedQueueTraversalPageError::Semantic(mismatch));
    }
    for edge in adjacency.edges() {
        let mismatch = if edge.snapshot() != query.snapshot() {
            Some(FixedQueueTraversalSemanticError::EdgeSnapshotMismatch)
        } else if edge.profile() != query.profile() {
            Some(FixedQueueTraversalSemanticError::EdgeProfileMismatch {
                expected: query.profile(),
                actual: edge.profile(),
            })
        } else if edge.target() != query.target() {
            Some(FixedQueueTraversalSemanticError::EdgeTargetMismatch)
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
            return Err(FixedQueueTraversalPageError::Semantic(mismatch));
        }
    }
    Ok(())
}

fn consume_lifetime_budget<ProviderError, TerminalError>(
    value: usize,
    limit: usize,
    kind: FixedQueueBudgetKind,
) -> Result<usize, FixedQueueTraversalPageError<ProviderError, TerminalError>> {
    let attempted = value
        .checked_add(1)
        .ok_or(FixedQueueTraversalPageError::CounterOverflow)?;
    if attempted > limit {
        Err(FixedQueueTraversalPageError::BudgetExceeded(
            FixedQueueBudgetExceeded {
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
) -> Result<usize, FixedQueueTraversalPageError<ProviderError, TerminalError>> {
    value
        .checked_add(1)
        .ok_or(FixedQueueTraversalPageError::CounterOverflow)
}

fn checked_add<ProviderError, TerminalError>(
    left: usize,
    right: usize,
) -> Result<usize, FixedQueueTraversalPageError<ProviderError, TerminalError>> {
    left.checked_add(right)
        .ok_or(FixedQueueTraversalPageError::CounterOverflow)
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, collections::BTreeMap, convert::Infallible, rc::Rc};

    use super::*;
    use crate::manifest::tests::qualified_target_identity;
    use crate::{
        prepare_fixed_queue_concrete_family, ClearraPlacementIdentity,
        ConcretePathMaterializationBudgets, FixedQueuePathMaterializationRequest,
        MaterializationGuard, MaterializationOutput, Pc4PlacementMaterializer, Pc4TerminalUseCase,
        PlacementRotation,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ProviderError {
        Rejected,
    }

    struct Provider {
        target: QualifiedPc4TargetIdentity,
        graph: BTreeMap<(u32, Pc4GraphPiece), Vec<u32>>,
        calls: Vec<(u32, Pc4GraphPiece, usize)>,
        fail: bool,
        cancel_during_call: Option<Rc<Cell<bool>>>,
    }

    impl QualifiedCompleteAdjacencyProvider for Provider {
        type Error = ProviderError;

        fn target(&self) -> &QualifiedPc4TargetIdentity {
            &self.target
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
            if self.fail {
                return Err(ProviderError::Rejected);
            }
            let edges = self
                .graph
                .get(&(query.source_field_id(), query.piece()))
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|target| {
                    QualifiedPc4GraphEdge::from_qualified_record(
                        query.target(),
                        query.source_field_id(),
                        query.piece(),
                        target,
                    )
                })
                .collect();
            Ok(QualifiedCompleteAdjacency::from_qualified_provider(
                query.target(),
                query.source_field_id(),
                query.piece(),
                query.queue_index(),
                edges,
            ))
        }
    }

    struct Guard {
        cancelled: Rc<Cell<bool>>,
        stale: Rc<Cell<bool>>,
    }

    impl FixedQueueTraversalGuard for Guard {
        fn is_cancelled(&self) -> bool {
            self.cancelled.get()
        }

        fn is_current_snapshot(&self, _expected: &QualifiedSnapshotIdentity) -> bool {
            !self.stale.get()
        }
    }

    impl MaterializationGuard for Guard {
        fn is_cancelled(&self) -> bool {
            self.cancelled.get()
        }

        fn is_current_snapshot(&self, _expected: &QualifiedSnapshotIdentity) -> bool {
            !self.stale.get()
        }
    }

    fn target() -> QualifiedPc4TargetIdentity {
        qualified_target_identity(
            "lazy-traversal-generation",
            "lazy-traversal-manifest",
            Pc4RuleProfile::Srs,
            Pc4TerminalUseCase::PcSearch,
            4,
        )
    }

    fn provider(graph: &[((u32, Pc4GraphPiece), &[u32])]) -> Provider {
        Provider {
            target: target(),
            graph: graph
                .iter()
                .map(|(key, targets)| (*key, targets.to_vec()))
                .collect(),
            calls: Vec::new(),
            fail: false,
            cancel_during_call: None,
        }
    }

    fn guard() -> Guard {
        Guard {
            cancelled: Rc::new(Cell::new(false)),
            stale: Rc::new(Cell::new(false)),
        }
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("positive synthetic budget")
    }

    fn family(queue: &[Pc4GraphPiece], limits: [usize; 6]) -> FixedQueueTraversalFamily {
        let [visited, frontier, path, output, page_work, page_output] = limits;
        let target = target();
        prepare_fixed_queue_traversal_family(
            FixedQueueTraversalFamilyRequest::new(
                &target,
                0,
                queue,
                TerminalDepthContract::QueueExhaustedOnly,
                FixedQueueTraversalBudgets::new(
                    nonzero(visited),
                    nonzero(frontier),
                    nonzero(path),
                    nonzero(output),
                ),
                FixedQueueTraversalPageBudgets::new(nonzero(page_work), nonzero(page_output)),
            ),
            &guard(),
        )
        .expect("prepared lazy traversal")
    }

    fn exhausted(query: &FixedQueueTerminalQuery<'_>) -> Result<bool, Infallible> {
        Ok(query.queue_is_exhausted())
    }

    fn target_paths(page: &FixedQueueTraversalPage) -> Vec<Vec<u32>> {
        page.paths()
            .iter()
            .map(|path| {
                path.edges()
                    .iter()
                    .map(QualifiedPc4GraphEdge::target_field_id)
                    .collect()
            })
            .collect()
    }

    fn append_target_paths(output: &mut Vec<Vec<u32>>, page: &FixedQueueTraversalPage) {
        output.extend(target_paths(page));
    }

    #[test]
    fn unresolved_frontier_is_bounded_and_cannot_commit_a_failed_page() {
        struct Waiting {
            base: Provider,
            blocked: bool,
            hints: Vec<u32>,
        }
        impl QualifiedCompleteAdjacencyProvider for Waiting {
            type Error = ProviderError;
            fn target(&self) -> &QualifiedPc4TargetIdentity {
                self.base.target()
            }
            fn complete_outgoing_edges(
                &mut self,
                query: &FixedQueueAdjacencyQuery<'_>,
            ) -> Result<QualifiedCompleteAdjacency, Self::Error> {
                if self.blocked && query.source_field_id() != 0 {
                    return Err(ProviderError::Rejected);
                }
                self.base.complete_outgoing_edges(query)
            }
            fn observe_unresolved_frontier(
                &mut self,
                target: &QualifiedPc4TargetIdentity,
                fields: &[u32],
            ) {
                assert_eq!(target, self.base.target());
                self.hints = fields.to_vec();
            }
        }
        let family = family(
            &[Pc4GraphPiece::I, Pc4GraphPiece::O],
            [128, 64, 2, 64, 128, 64],
        );
        let mut waiting = Waiting {
            base: provider(&[
                ((0, Pc4GraphPiece::I), &[3, 1, 2]),
                ((1, Pc4GraphPiece::O), &[11]),
                ((2, Pc4GraphPiece::O), &[12]),
                ((3, Pc4GraphPiece::O), &[13]),
            ]),
            blocked: true,
            hints: Vec::new(),
        };
        let mut cursor = family.cursor();
        assert!(matches!(
            family.next_page(
                &mut cursor,
                nonzero(64),
                &mut waiting,
                &mut exhausted,
                &guard()
            ),
            Err(FixedQueueTraversalPageError::Provider(
                ProviderError::Rejected
            ))
        ));
        assert_eq!(waiting.hints, [1, 2, 3]);
        assert_eq!(cursor.visited_state_occurrences(), 0);
        assert_eq!(cursor.emitted_paths(), 0);
        assert_eq!(cursor.pending_path_count(), 1);
        waiting.blocked = false;
        let page = family
            .next_page(
                &mut cursor,
                nonzero(64),
                &mut waiting,
                &mut exhausted,
                &guard(),
            )
            .unwrap();
        assert_eq!(
            target_paths(&page),
            vec![vec![1, 11], vec![2, 12], vec![3, 13]]
        );
        assert!(page.is_exhausted());

        waiting.blocked = true;
        waiting
            .base
            .graph
            .insert((0, Pc4GraphPiece::I), (1..=60).collect());
        let mut cursor = family.cursor();
        assert!(family
            .next_page(
                &mut cursor,
                nonzero(64),
                &mut waiting,
                &mut exhausted,
                &guard()
            )
            .is_err());
        assert_eq!(waiting.hints, (1..=32).collect::<Vec<_>>());
        assert_eq!(cursor.visited_state_occurrences(), 0);
    }

    #[test]
    fn first_page_does_not_enumerate_or_store_the_whole_family() {
        let queue = [Pc4GraphPiece::I, Pc4GraphPiece::O];
        let family = family(&queue, [32, 8, 2, 8, 8, 1]);
        let mut provider = provider(&[
            ((0, Pc4GraphPiece::I), &[3, 1, 2]),
            ((1, Pc4GraphPiece::O), &[11]),
            ((2, Pc4GraphPiece::O), &[12]),
            ((3, Pc4GraphPiece::O), &[13]),
        ]);
        let mut cursor = family.cursor();
        let page = family
            .next_page(
                &mut cursor,
                nonzero(1),
                &mut provider,
                &mut exhausted,
                &guard(),
            )
            .expect("first lazy page");

        assert_eq!(target_paths(&page), vec![vec![1, 11]]);
        assert_eq!(
            provider.calls,
            vec![(0, Pc4GraphPiece::I, 0), (1, Pc4GraphPiece::O, 1)]
        );
        assert_eq!(cursor.pending_path_count(), 2);
        assert!(!page.is_exhausted());
    }

    #[test]
    fn duplicate_transition_is_suppressed_but_converging_prefixes_survive() {
        let queue = [Pc4GraphPiece::I, Pc4GraphPiece::O];
        let family = family(&queue, [32, 8, 2, 8, 32, 8]);
        let mut provider = provider(&[
            ((0, Pc4GraphPiece::I), &[2, 1, 1]),
            ((1, Pc4GraphPiece::O), &[3, 3]),
            ((2, Pc4GraphPiece::O), &[3]),
        ]);
        let mut cursor = family.cursor();
        let page = family
            .next_page(
                &mut cursor,
                nonzero(8),
                &mut provider,
                &mut exhausted,
                &guard(),
            )
            .expect("canonical duplicate handling");

        assert_eq!(target_paths(&page), vec![vec![1, 3], vec![2, 3]]);
        assert_eq!(page.duplicate_edges_suppressed(), 2);
        assert_eq!(cursor.duplicate_edges_suppressed(), 2);
        assert!(page.is_exhausted());
    }

    #[test]
    fn canonical_dfs_matches_lexicographic_batch_order_with_early_terminals() {
        let target = target();
        let queue = [Pc4GraphPiece::I, Pc4GraphPiece::O, Pc4GraphPiece::T];
        let family = prepare_fixed_queue_traversal_family(
            FixedQueueTraversalFamilyRequest::new(
                &target,
                0,
                &queue,
                TerminalDepthContract::PredicateMayTerminateEarly,
                FixedQueueTraversalBudgets::new(nonzero(32), nonzero(8), nonzero(3), nonzero(8)),
                FixedQueueTraversalPageBudgets::new(nonzero(32), nonzero(2)),
            ),
            &guard(),
        )
        .expect("early-terminal family");
        let mut provider = provider(&[
            ((0, Pc4GraphPiece::I), &[2, 1]),
            ((2, Pc4GraphPiece::O), &[4, 3]),
            ((4, Pc4GraphPiece::T), &[5]),
        ]);
        let mut terminal = |query: &FixedQueueTerminalQuery<'_>| {
            Ok::<bool, Infallible>(matches!(query.field_id(), 1 | 3 | 5))
        };
        let mut cursor = family.cursor();
        let first = family
            .next_page(
                &mut cursor,
                nonzero(2),
                &mut provider,
                &mut terminal,
                &guard(),
            )
            .expect("first canonical page");
        let second = family
            .next_page(
                &mut cursor,
                nonzero(2),
                &mut provider,
                &mut terminal,
                &guard(),
            )
            .expect("second canonical page");
        let mut paths = Vec::new();
        append_target_paths(&mut paths, &first);
        append_target_paths(&mut paths, &second);

        assert_eq!(paths, vec![vec![1], vec![2, 3], vec![2, 4, 5]]);
        assert_eq!(
            provider.calls,
            vec![
                (0, Pc4GraphPiece::I, 0),
                (2, Pc4GraphPiece::O, 1),
                (4, Pc4GraphPiece::T, 2),
            ]
        );
        assert!(second.is_exhausted());
    }

    struct Materializer {
        calls: usize,
    }

    impl Pc4PlacementMaterializer for Materializer {
        type Error = Infallible;

        fn profile(&self) -> Pc4RuleProfile {
            Pc4RuleProfile::Srs
        }

        fn enumerate(
            &mut self,
            edge: &QualifiedPc4GraphEdge,
        ) -> Result<MaterializationOutput, Self::Error> {
            self.calls += 1;
            Ok(MaterializationOutput {
                snapshot: edge.snapshot().clone(),
                profile: edge.profile(),
                source_field_id: edge.source_field_id(),
                piece: edge.piece(),
                target_field_id: edge.target_field_id(),
                placements: vec![
                    ClearraPlacementIdentity::new(
                        edge.piece(),
                        PlacementRotation::Zero,
                        0,
                        0,
                        0b1111,
                    )
                    .expect("first placement"),
                    ClearraPlacementIdentity::new(
                        edge.piece(),
                        PlacementRotation::Two,
                        1,
                        0,
                        0b1_1110,
                    )
                    .expect("second placement"),
                ],
            })
        }
    }

    #[test]
    fn duplicate_graph_edge_materializes_one_complete_concrete_family() {
        let queue = [Pc4GraphPiece::I];
        let graph_family = family(&queue, [8, 4, 1, 4, 8, 4]);
        let mut provider = provider(&[((0, Pc4GraphPiece::I), &[1, 1])]);
        let guard = guard();
        let mut cursor = graph_family.cursor();
        let graph_page = graph_family
            .next_page(
                &mut cursor,
                nonzero(4),
                &mut provider,
                &mut exhausted,
                &guard,
            )
            .expect("deduplicated graph page");
        assert_eq!(graph_page.paths().len(), 1);

        let mut materializer = Materializer { calls: 0 };
        let concrete_family = prepare_fixed_queue_concrete_family(
            FixedQueuePathMaterializationRequest::new(
                graph_family.target(),
                &graph_page.paths()[0],
                ConcretePathMaterializationBudgets::new(
                    nonzero(1),
                    nonzero(2),
                    nonzero(2),
                    nonzero(2),
                ),
            ),
            &mut materializer,
            &guard,
        )
        .expect("one complete materialized transition");
        let concrete_page = concrete_family
            .next_page(&mut concrete_family.cursor(), nonzero(2), &guard)
            .expect("concrete page");

        assert_eq!(materializer.calls, 1);
        assert_eq!(concrete_page.len(), 2);
    }

    #[test]
    fn work_slice_can_commit_empty_progress_and_resume_canonically() {
        let queue = [Pc4GraphPiece::I];
        let family = family(&queue, [8, 4, 1, 4, 1, 2]);
        let mut provider = provider(&[((0, Pc4GraphPiece::I), &[2, 1])]);
        let mut cursor = family.cursor();

        let first = family
            .next_page(
                &mut cursor,
                nonzero(2),
                &mut provider,
                &mut exhausted,
                &guard(),
            )
            .expect("root work slice");
        assert!(first.paths().is_empty());
        assert!(first.stopped_by_work_budget());
        assert_eq!(cursor.pending_path_count(), 2);

        let second = family
            .next_page(
                &mut cursor,
                nonzero(2),
                &mut provider,
                &mut exhausted,
                &guard(),
            )
            .expect("first output slice");
        assert_eq!(target_paths(&second), vec![vec![1]]);
        assert!(second.stopped_by_work_budget());

        let third = family
            .next_page(
                &mut cursor,
                nonzero(2),
                &mut provider,
                &mut exhausted,
                &guard(),
            )
            .expect("final output slice");
        assert_eq!(target_paths(&third), vec![vec![2]]);
        assert!(third.is_exhausted());
    }

    #[test]
    fn cursor_is_bound_to_exact_family_request() {
        let first_queue = [Pc4GraphPiece::I];
        let second_queue = [Pc4GraphPiece::O];
        let first = family(&first_queue, [4, 2, 1, 2, 4, 2]);
        let second = family(&second_queue, [4, 2, 1, 2, 4, 2]);
        let mut cursor = first.cursor();
        let mut provider = provider(&[]);
        let error = second
            .next_page(
                &mut cursor,
                nonzero(1),
                &mut provider,
                &mut exhausted,
                &guard(),
            )
            .expect_err("foreign family cursor");

        assert_eq!(error, FixedQueueTraversalPageError::CursorMismatch);
        assert!(provider.calls.is_empty());
        assert_eq!(cursor.visited_state_occurrences(), 0);
    }

    #[test]
    fn cancellation_is_transactional_and_retry_starts_at_same_cursor() {
        let queue = [Pc4GraphPiece::I];
        let family = family(&queue, [4, 2, 1, 2, 4, 2]);
        let guard = guard();
        let mut provider = provider(&[((0, Pc4GraphPiece::I), &[1])]);
        provider.cancel_during_call = Some(Rc::clone(&guard.cancelled));
        let mut cursor = family.cursor();
        let error = family
            .next_page(
                &mut cursor,
                nonzero(1),
                &mut provider,
                &mut exhausted,
                &guard,
            )
            .expect_err("cancelled provider callback");
        assert_eq!(error, FixedQueueTraversalPageError::Cancelled);
        assert_eq!(cursor.visited_state_occurrences(), 0);
        assert_eq!(cursor.adjacency_queries(), 0);
        assert_eq!(cursor.pending_path_count(), 1);

        guard.cancelled.set(false);
        provider.cancel_during_call = None;
        let retry = family
            .next_page(
                &mut cursor,
                nonzero(1),
                &mut provider,
                &mut exhausted,
                &guard,
            )
            .expect("retry unchanged cursor");
        assert_eq!(target_paths(&retry), vec![vec![1]]);
    }

    #[test]
    fn page_and_lifetime_budgets_fail_closed() {
        let queue = [Pc4GraphPiece::I];
        let family = family(&queue, [1, 1, 1, 1, 1, 1]);
        let mut cursor = family.cursor();
        let mut provider = provider(&[((0, Pc4GraphPiece::I), &[1, 2])]);
        assert_eq!(
            family.next_page(
                &mut cursor,
                nonzero(2),
                &mut provider,
                &mut exhausted,
                &guard(),
            ),
            Err(FixedQueueTraversalPageError::PageLimitExceeded {
                limit: 1,
                attempted: 2,
            })
        );
        assert!(provider.calls.is_empty());

        let error = family
            .next_page(
                &mut cursor,
                nonzero(1),
                &mut provider,
                &mut exhausted,
                &guard(),
            )
            .expect_err("unique frontier exceeds lifetime budget");
        assert!(matches!(
            error,
            FixedQueueTraversalPageError::BudgetExceeded(FixedQueueBudgetExceeded {
                kind: FixedQueueBudgetKind::FrontierPaths,
                limit: 1,
                attempted: 2,
            })
        ));
        assert_eq!(cursor.visited_state_occurrences(), 0);
        assert_eq!(cursor.pending_path_count(), 1);
    }

    #[test]
    fn stale_snapshot_and_provider_failure_return_no_partial_page() {
        let queue = [Pc4GraphPiece::I];
        let family = family(&queue, [4, 2, 1, 2, 4, 2]);
        let stale_guard = guard();
        stale_guard.stale.set(true);
        let mut never_called = provider(&[]);
        let mut cursor = family.cursor();
        assert_eq!(
            family.next_page(
                &mut cursor,
                nonzero(1),
                &mut never_called,
                &mut exhausted,
                &stale_guard,
            ),
            Err(FixedQueueTraversalPageError::StaleSnapshot)
        );
        assert!(never_called.calls.is_empty());

        let mut rejected = provider(&[]);
        rejected.fail = true;
        assert_eq!(
            family.next_page(
                &mut cursor,
                nonzero(1),
                &mut rejected,
                &mut exhausted,
                &guard(),
            ),
            Err(FixedQueueTraversalPageError::Provider(
                ProviderError::Rejected
            ))
        );
        assert_eq!(cursor.visited_state_occurrences(), 0);
    }
}
