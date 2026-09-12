// SRP rationale: this module owns only the bounded expansion of a concrete
// queue and initial hold state into exact placement-piece/hold-decision paths.
use core::{fmt, num::NonZeroUsize};

use crate::Pc4GraphPiece;

/// Hold state before or after one fixed-queue supply transition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FixedQueueHoldState {
    Disabled,
    Empty,
    Occupied(Pc4GraphPiece),
}

/// Canonical semantic branch used to supply one placement piece.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FixedQueueHoldDecision {
    UseCurrent,
    SwapHeld,
    StoreCurrentUseNext,
}

/// Replayable supply evidence for one piece in an expanded placement queue.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FixedQueueHoldStep {
    used_piece: Pc4GraphPiece,
    queue_current_piece: Pc4GraphPiece,
    queue_next_piece: Option<Pc4GraphPiece>,
    cursor_before: usize,
    cursor_after: usize,
    hold_before: FixedQueueHoldState,
    hold_after: FixedQueueHoldState,
    decision: FixedQueueHoldDecision,
}

impl FixedQueueHoldStep {
    pub const fn used_piece(self) -> Pc4GraphPiece {
        self.used_piece
    }

    pub const fn queue_current_piece(self) -> Pc4GraphPiece {
        self.queue_current_piece
    }

    pub const fn queue_next_piece(self) -> Option<Pc4GraphPiece> {
        self.queue_next_piece
    }

    pub const fn cursor_before(self) -> usize {
        self.cursor_before
    }

    pub const fn cursor_after(self) -> usize {
        self.cursor_after
    }

    pub const fn hold_before(self) -> FixedQueueHoldState {
        self.hold_before
    }

    pub const fn hold_after(self) -> FixedQueueHoldState {
        self.hold_after
    }

    pub const fn decision(self) -> FixedQueueHoldDecision {
        self.decision
    }
}

/// One exact supply path. `placement_queue` can be passed to fixed graph
/// traversal; `steps` retains the hold evidence required by a later replay
/// adapter. Internal graph field IDs are deliberately absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedQueueHoldPath {
    placement_queue: Vec<Pc4GraphPiece>,
    steps: Vec<FixedQueueHoldStep>,
    terminal_cursor: usize,
    terminal_hold: FixedQueueHoldState,
}

impl FixedQueueHoldPath {
    pub fn placement_queue(&self) -> &[Pc4GraphPiece] {
        &self.placement_queue
    }

    pub fn steps(&self) -> &[FixedQueueHoldStep] {
        &self.steps
    }

    pub const fn terminal_cursor(&self) -> usize {
        self.terminal_cursor
    }

    pub const fn terminal_hold(&self) -> FixedQueueHoldState {
        self.terminal_hold
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixedQueueHoldBudgetKind {
    VisitedStateOccurrences,
    FrontierPaths,
    GeneratedPathSteps,
    OutputPaths,
}

/// Deterministic work/output limits for fixed-queue hold expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedQueueHoldBudgets {
    visited_state_occurrences: NonZeroUsize,
    frontier_paths: NonZeroUsize,
    generated_path_steps: NonZeroUsize,
    output_paths: NonZeroUsize,
}

impl FixedQueueHoldBudgets {
    pub const fn new(
        visited_state_occurrences: NonZeroUsize,
        frontier_paths: NonZeroUsize,
        generated_path_steps: NonZeroUsize,
        output_paths: NonZeroUsize,
    ) -> Self {
        Self {
            visited_state_occurrences,
            frontier_paths,
            generated_path_steps,
            output_paths,
        }
    }

    pub const fn visited_state_occurrences(self) -> usize {
        self.visited_state_occurrences.get()
    }

    pub const fn frontier_paths(self) -> usize {
        self.frontier_paths.get()
    }

    pub const fn generated_path_steps(self) -> usize {
        self.generated_path_steps.get()
    }

    pub const fn output_paths(self) -> usize {
        self.output_paths.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedQueueHoldBudgetExceeded {
    kind: FixedQueueHoldBudgetKind,
    limit: usize,
}

impl FixedQueueHoldBudgetExceeded {
    pub const fn kind(self) -> FixedQueueHoldBudgetKind {
        self.kind
    }

    pub const fn limit(self) -> usize {
        self.limit
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixedQueueHoldExpansionError {
    Cancelled,
    BudgetExceeded(FixedQueueHoldBudgetExceeded),
    AllocationFailed,
}

impl FixedQueueHoldExpansionError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_fixed_queue_hold_cancelled",
            Self::BudgetExceeded(_) => "pc4_fixed_queue_hold_budget_exceeded",
            Self::AllocationFailed => "pc4_fixed_queue_hold_allocation_failed",
        }
    }
}

impl fmt::Display for FixedQueueHoldExpansionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for FixedQueueHoldExpansionError {}

pub trait FixedQueueHoldExpansionGuard {
    fn is_cancelled(&self) -> bool;
}

pub struct FixedQueueHoldExpansionRequest<'a> {
    queue: &'a [Pc4GraphPiece],
    initial_hold: FixedQueueHoldState,
    placement_count: usize,
    budgets: FixedQueueHoldBudgets,
}

impl<'a> FixedQueueHoldExpansionRequest<'a> {
    pub const fn new(
        queue: &'a [Pc4GraphPiece],
        initial_hold: FixedQueueHoldState,
        placement_count: usize,
        budgets: FixedQueueHoldBudgets,
    ) -> Self {
        Self {
            queue,
            initial_hold,
            placement_count,
            budgets,
        }
    }

    pub const fn queue(&self) -> &'a [Pc4GraphPiece] {
        self.queue
    }

    pub const fn initial_hold(&self) -> FixedQueueHoldState {
        self.initial_hold
    }

    pub const fn placement_count(&self) -> usize {
        self.placement_count
    }

    pub const fn budgets(&self) -> FixedQueueHoldBudgets {
        self.budgets
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedQueueHoldExpansionResult<'a> {
    source_queue: &'a [Pc4GraphPiece],
    initial_hold: FixedQueueHoldState,
    requested_placement_count: usize,
    paths: Vec<FixedQueueHoldPath>,
    visited_state_occurrences: usize,
    generated_path_steps: usize,
}

impl<'a> FixedQueueHoldExpansionResult<'a> {
    pub const fn source_queue(&self) -> &'a [Pc4GraphPiece] {
        self.source_queue
    }

    pub const fn initial_hold(&self) -> FixedQueueHoldState {
        self.initial_hold
    }

    pub const fn requested_placement_count(&self) -> usize {
        self.requested_placement_count
    }

    pub fn paths(&self) -> &[FixedQueueHoldPath] {
        &self.paths
    }

    pub const fn visited_state_occurrences(&self) -> usize {
        self.visited_state_occurrences
    }

    pub const fn generated_path_steps(&self) -> usize {
        self.generated_path_steps
    }
}

#[derive(Clone, Debug)]
struct PendingPath {
    cursor: usize,
    hold: FixedQueueHoldState,
    placement_queue: Vec<Pc4GraphPiece>,
    steps: Vec<FixedQueueHoldStep>,
}

#[derive(Clone, Copy)]
struct Transition {
    used_piece: Pc4GraphPiece,
    queue_current_piece: Pc4GraphPiece,
    queue_next_piece: Option<Pc4GraphPiece>,
    cursor_after: usize,
    hold_after: FixedQueueHoldState,
    decision: FixedQueueHoldDecision,
}

/// Expands every semantic hold choice for one already concrete queue.
///
/// Equal current/held pieces do not create a duplicate swap branch because
/// both transitions have the same used piece, cursor, and hold state. An empty
/// hold consumes current plus next when the store branch is chosen. A held
/// piece is never released after the concrete queue is exhausted: doing so
/// would require an unprovided next active piece. Pattern/bag revelation is a
/// separate future state machine.
pub fn expand_fixed_queue_hold<'a, G: FixedQueueHoldExpansionGuard>(
    request: FixedQueueHoldExpansionRequest<'a>,
    guard: &G,
) -> Result<FixedQueueHoldExpansionResult<'a>, FixedQueueHoldExpansionError> {
    require_active(guard)?;
    let mut frontier = Vec::new();
    frontier
        .try_reserve_exact(1)
        .map_err(|_| FixedQueueHoldExpansionError::AllocationFailed)?;
    frontier.push(PendingPath {
        cursor: 0,
        hold: request.initial_hold,
        placement_queue: Vec::new(),
        steps: Vec::new(),
    });
    let mut visited_state_occurrences = 1_usize;
    let mut generated_path_steps = 0_usize;

    for _ in 0..request.placement_count {
        require_active(guard)?;
        let mut next = Vec::new();
        for path in &frontier {
            require_active(guard)?;
            for transition in transitions(request.queue, path).into_iter().flatten() {
                let next_visited = visited_state_occurrences
                    .checked_add(1)
                    .ok_or(FixedQueueHoldExpansionError::AllocationFailed)?;
                require_budget(
                    next_visited,
                    request.budgets.visited_state_occurrences(),
                    FixedQueueHoldBudgetKind::VisitedStateOccurrences,
                )?;
                let next_frontier = next
                    .len()
                    .checked_add(1)
                    .ok_or(FixedQueueHoldExpansionError::AllocationFailed)?;
                require_budget(
                    next_frontier,
                    request.budgets.frontier_paths(),
                    FixedQueueHoldBudgetKind::FrontierPaths,
                )?;
                let path_steps = path
                    .steps
                    .len()
                    .checked_add(1)
                    .ok_or(FixedQueueHoldExpansionError::AllocationFailed)?;
                let next_generated = generated_path_steps
                    .checked_add(path_steps)
                    .ok_or(FixedQueueHoldExpansionError::AllocationFailed)?;
                require_budget(
                    next_generated,
                    request.budgets.generated_path_steps(),
                    FixedQueueHoldBudgetKind::GeneratedPathSteps,
                )?;
                next.try_reserve(1)
                    .map_err(|_| FixedQueueHoldExpansionError::AllocationFailed)?;
                next.push(extend_path(path, transition)?);
                visited_state_occurrences = next_visited;
                generated_path_steps = next_generated;
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }

    require_active(guard)?;
    require_budget(
        frontier.len(),
        request.budgets.output_paths(),
        FixedQueueHoldBudgetKind::OutputPaths,
    )?;
    let mut paths = Vec::new();
    paths
        .try_reserve_exact(frontier.len())
        .map_err(|_| FixedQueueHoldExpansionError::AllocationFailed)?;
    for path in frontier {
        paths.push(FixedQueueHoldPath {
            placement_queue: path.placement_queue,
            steps: path.steps,
            terminal_cursor: path.cursor,
            terminal_hold: path.hold,
        });
    }
    require_active(guard)?;
    Ok(FixedQueueHoldExpansionResult {
        source_queue: request.queue,
        initial_hold: request.initial_hold,
        requested_placement_count: request.placement_count,
        paths,
        visited_state_occurrences,
        generated_path_steps,
    })
}

fn transitions(queue: &[Pc4GraphPiece], path: &PendingPath) -> [Option<Transition>; 3] {
    let Some(&current) = queue.get(path.cursor) else {
        return [None, None, None];
    };
    let current_transition = Some(Transition {
        used_piece: current,
        queue_current_piece: current,
        queue_next_piece: None,
        cursor_after: path.cursor + 1,
        hold_after: path.hold,
        decision: FixedQueueHoldDecision::UseCurrent,
    });
    let swap_transition = match path.hold {
        FixedQueueHoldState::Occupied(held) if held != current => Some(Transition {
            used_piece: held,
            queue_current_piece: current,
            queue_next_piece: None,
            cursor_after: path.cursor + 1,
            hold_after: FixedQueueHoldState::Occupied(current),
            decision: FixedQueueHoldDecision::SwapHeld,
        }),
        _ => None,
    };
    let store_transition = match path.hold {
        FixedQueueHoldState::Empty => queue.get(path.cursor + 1).copied().map(|next| Transition {
            used_piece: next,
            queue_current_piece: current,
            queue_next_piece: Some(next),
            cursor_after: path.cursor + 2,
            hold_after: FixedQueueHoldState::Occupied(current),
            decision: FixedQueueHoldDecision::StoreCurrentUseNext,
        }),
        _ => None,
    };
    [current_transition, swap_transition, store_transition]
}

fn extend_path(
    path: &PendingPath,
    transition: Transition,
) -> Result<PendingPath, FixedQueueHoldExpansionError> {
    let mut placement_queue = Vec::new();
    placement_queue
        .try_reserve_exact(path.placement_queue.len() + 1)
        .map_err(|_| FixedQueueHoldExpansionError::AllocationFailed)?;
    placement_queue.extend_from_slice(&path.placement_queue);
    placement_queue.push(transition.used_piece);

    let mut steps = Vec::new();
    steps
        .try_reserve_exact(path.steps.len() + 1)
        .map_err(|_| FixedQueueHoldExpansionError::AllocationFailed)?;
    steps.extend_from_slice(&path.steps);
    steps.push(FixedQueueHoldStep {
        used_piece: transition.used_piece,
        queue_current_piece: transition.queue_current_piece,
        queue_next_piece: transition.queue_next_piece,
        cursor_before: path.cursor,
        cursor_after: transition.cursor_after,
        hold_before: path.hold,
        hold_after: transition.hold_after,
        decision: transition.decision,
    });
    Ok(PendingPath {
        cursor: transition.cursor_after,
        hold: transition.hold_after,
        placement_queue,
        steps,
    })
}

fn require_active<G: FixedQueueHoldExpansionGuard>(
    guard: &G,
) -> Result<(), FixedQueueHoldExpansionError> {
    if guard.is_cancelled() {
        Err(FixedQueueHoldExpansionError::Cancelled)
    } else {
        Ok(())
    }
}

fn require_budget(
    value: usize,
    limit: usize,
    kind: FixedQueueHoldBudgetKind,
) -> Result<(), FixedQueueHoldExpansionError> {
    if value > limit {
        Err(FixedQueueHoldExpansionError::BudgetExceeded(
            FixedQueueHoldBudgetExceeded { kind, limit },
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::{cell::Cell, num::NonZeroUsize};

    use super::*;

    struct Guard {
        checks_before_cancel: Cell<usize>,
    }

    impl FixedQueueHoldExpansionGuard for Guard {
        fn is_cancelled(&self) -> bool {
            let remaining = self.checks_before_cancel.get();
            self.checks_before_cancel.set(remaining.saturating_sub(1));
            remaining == 0
        }
    }

    fn guard() -> Guard {
        Guard {
            checks_before_cancel: Cell::new(usize::MAX),
        }
    }

    fn budgets(limit: usize) -> FixedQueueHoldBudgets {
        let limit = NonZeroUsize::new(limit).expect("positive synthetic budget");
        FixedQueueHoldBudgets::new(limit, limit, limit, limit)
    }

    fn expand(
        queue: &[Pc4GraphPiece],
        initial_hold: FixedQueueHoldState,
        placement_count: usize,
    ) -> FixedQueueHoldExpansionResult<'_> {
        expand_fixed_queue_hold(
            FixedQueueHoldExpansionRequest::new(
                queue,
                initial_hold,
                placement_count,
                budgets(10_000),
            ),
            &guard(),
        )
        .expect("synthetic hold expansion")
    }

    #[test]
    fn disabled_hold_preserves_the_fixed_queue_exactly() {
        let queue = [Pc4GraphPiece::I, Pc4GraphPiece::O];
        let result = expand(&queue, FixedQueueHoldState::Disabled, 2);
        assert_eq!(result.source_queue(), &queue);
        assert_eq!(result.initial_hold(), FixedQueueHoldState::Disabled);
        assert_eq!(result.requested_placement_count(), 2);
        assert_eq!(result.paths().len(), 1);
        assert_eq!(
            result.paths()[0].placement_queue(),
            &[Pc4GraphPiece::I, Pc4GraphPiece::O]
        );
        assert!(result.paths()[0]
            .steps()
            .iter()
            .all(|step| step.decision() == FixedQueueHoldDecision::UseCurrent));
    }

    #[test]
    fn occupied_hold_emits_current_then_distinct_swap_in_canonical_order() {
        let result = expand(
            &[Pc4GraphPiece::I],
            FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
            1,
        );
        assert_eq!(result.paths().len(), 2);
        assert_eq!(result.paths()[0].placement_queue(), &[Pc4GraphPiece::I]);
        assert_eq!(result.paths()[1].placement_queue(), &[Pc4GraphPiece::T]);
        assert_eq!(
            result.paths()[1].terminal_hold(),
            FixedQueueHoldState::Occupied(Pc4GraphPiece::I)
        );
    }

    #[test]
    fn swapping_equal_current_and_hold_is_not_a_duplicate_semantic_path() {
        let result = expand(
            &[Pc4GraphPiece::I],
            FixedQueueHoldState::Occupied(Pc4GraphPiece::I),
            1,
        );
        assert_eq!(result.paths().len(), 1);
        assert_eq!(
            result.paths()[0].steps()[0].decision(),
            FixedQueueHoldDecision::UseCurrent
        );
    }

    #[test]
    fn empty_hold_stores_current_and_consumes_next_for_one_placement() {
        let result = expand(
            &[Pc4GraphPiece::I, Pc4GraphPiece::O],
            FixedQueueHoldState::Empty,
            1,
        );
        assert_eq!(result.paths().len(), 2);
        let stored = &result.paths()[1];
        assert_eq!(stored.placement_queue(), &[Pc4GraphPiece::O]);
        assert_eq!(stored.terminal_cursor(), 2);
        assert_eq!(
            stored.terminal_hold(),
            FixedQueueHoldState::Occupied(Pc4GraphPiece::I)
        );
        assert_eq!(
            stored.steps()[0].decision(),
            FixedQueueHoldDecision::StoreCurrentUseNext
        );
    }

    #[test]
    fn multi_step_paths_preserve_hold_evidence_and_canonical_branch_order() {
        let result = expand(
            &[Pc4GraphPiece::I, Pc4GraphPiece::O, Pc4GraphPiece::T],
            FixedQueueHoldState::Empty,
            2,
        );
        let queues = result
            .paths()
            .iter()
            .map(FixedQueueHoldPath::placement_queue)
            .collect::<Vec<_>>();
        assert_eq!(
            queues,
            vec![
                &[Pc4GraphPiece::I, Pc4GraphPiece::O][..],
                &[Pc4GraphPiece::I, Pc4GraphPiece::T][..],
                &[Pc4GraphPiece::O, Pc4GraphPiece::T][..],
                &[Pc4GraphPiece::O, Pc4GraphPiece::I][..],
            ]
        );
        assert_eq!(
            result.paths()[3].steps()[1].decision(),
            FixedQueueHoldDecision::SwapHeld
        );
    }

    #[test]
    fn zero_placement_request_has_one_empty_supply_path() {
        let result = expand(&[], FixedQueueHoldState::Empty, 0);
        assert_eq!(result.paths().len(), 1);
        assert!(result.paths()[0].placement_queue().is_empty());
        assert_eq!(
            result.paths()[0].terminal_hold(),
            FixedQueueHoldState::Empty
        );
    }

    #[test]
    fn concrete_queue_exhaustion_is_a_valid_empty_family_not_a_held_release() {
        let result = expand(
            &[Pc4GraphPiece::I],
            FixedQueueHoldState::Occupied(Pc4GraphPiece::O),
            2,
        );
        assert!(result.paths().is_empty());
    }

    #[test]
    fn cancellation_discards_the_in_progress_family() {
        let error = expand_fixed_queue_hold(
            FixedQueueHoldExpansionRequest::new(
                &[Pc4GraphPiece::I, Pc4GraphPiece::O],
                FixedQueueHoldState::Empty,
                2,
                budgets(100),
            ),
            &Guard {
                checks_before_cancel: Cell::new(2),
            },
        )
        .expect_err("cancelled transaction");
        assert_eq!(error, FixedQueueHoldExpansionError::Cancelled);
    }

    #[test]
    fn every_finite_budget_reports_its_exact_kind() {
        let cases = [
            (
                FixedQueueHoldBudgets::new(
                    NonZeroUsize::new(1).unwrap(),
                    NonZeroUsize::new(100).unwrap(),
                    NonZeroUsize::new(100).unwrap(),
                    NonZeroUsize::new(100).unwrap(),
                ),
                FixedQueueHoldBudgetKind::VisitedStateOccurrences,
            ),
            (
                FixedQueueHoldBudgets::new(
                    NonZeroUsize::new(100).unwrap(),
                    NonZeroUsize::new(1).unwrap(),
                    NonZeroUsize::new(100).unwrap(),
                    NonZeroUsize::new(100).unwrap(),
                ),
                FixedQueueHoldBudgetKind::FrontierPaths,
            ),
            (
                FixedQueueHoldBudgets::new(
                    NonZeroUsize::new(100).unwrap(),
                    NonZeroUsize::new(100).unwrap(),
                    NonZeroUsize::new(1).unwrap(),
                    NonZeroUsize::new(100).unwrap(),
                ),
                FixedQueueHoldBudgetKind::GeneratedPathSteps,
            ),
            (
                FixedQueueHoldBudgets::new(
                    NonZeroUsize::new(100).unwrap(),
                    NonZeroUsize::new(100).unwrap(),
                    NonZeroUsize::new(100).unwrap(),
                    NonZeroUsize::new(1).unwrap(),
                ),
                FixedQueueHoldBudgetKind::OutputPaths,
            ),
        ];
        for (budgets, expected) in cases {
            let error = expand_fixed_queue_hold(
                FixedQueueHoldExpansionRequest::new(
                    &[Pc4GraphPiece::I, Pc4GraphPiece::O],
                    FixedQueueHoldState::Empty,
                    1,
                    budgets,
                ),
                &guard(),
            )
            .expect_err("bounded synthetic expansion");
            let FixedQueueHoldExpansionError::BudgetExceeded(exceeded) = error else {
                panic!("unexpected error: {error:?}");
            };
            assert_eq!(exceeded.kind(), expected);
        }
    }

    #[test]
    fn exhaustive_short_queues_match_the_hold_recurrence_and_trace_invariants() {
        const PIECES: [Pc4GraphPiece; 7] = [
            Pc4GraphPiece::I,
            Pc4GraphPiece::O,
            Pc4GraphPiece::T,
            Pc4GraphPiece::S,
            Pc4GraphPiece::Z,
            Pc4GraphPiece::J,
            Pc4GraphPiece::L,
        ];
        let mut holds = vec![FixedQueueHoldState::Disabled, FixedQueueHoldState::Empty];
        holds.extend(PIECES.into_iter().map(FixedQueueHoldState::Occupied));

        for first in PIECES {
            for second in PIECES {
                for third in PIECES {
                    let queue = [first, second, third];
                    for &initial_hold in &holds {
                        for placement_count in 0..=3 {
                            let result = expand(&queue, initial_hold, placement_count);
                            assert_eq!(
                                result.paths().len(),
                                recurrence_count(&queue, 0, initial_hold, placement_count)
                            );
                            for path in result.paths() {
                                assert_eq!(path.steps().len(), placement_count);
                                assert_eq!(path.placement_queue().len(), placement_count);
                                let mut cursor = 0;
                                let mut hold = initial_hold;
                                for (index, step) in path.steps().iter().copied().enumerate() {
                                    assert_eq!(step.cursor_before(), cursor);
                                    assert_eq!(step.hold_before(), hold);
                                    assert_eq!(step.used_piece(), path.placement_queue()[index]);
                                    cursor = step.cursor_after();
                                    hold = step.hold_after();
                                }
                                assert_eq!(path.terminal_cursor(), cursor);
                                assert_eq!(path.terminal_hold(), hold);
                            }
                        }
                    }
                }
            }
        }
    }

    fn recurrence_count(
        queue: &[Pc4GraphPiece],
        cursor: usize,
        hold: FixedQueueHoldState,
        remaining: usize,
    ) -> usize {
        if remaining == 0 {
            return 1;
        }
        let Some(&current) = queue.get(cursor) else {
            return 0;
        };
        let mut count = recurrence_count(queue, cursor + 1, hold, remaining - 1);
        if let FixedQueueHoldState::Occupied(held) = hold {
            if held != current {
                count += recurrence_count(
                    queue,
                    cursor + 1,
                    FixedQueueHoldState::Occupied(current),
                    remaining - 1,
                );
            }
        }
        if hold == FixedQueueHoldState::Empty && queue.get(cursor + 1).is_some() {
            count += recurrence_count(
                queue,
                cursor + 2,
                FixedQueueHoldState::Occupied(current),
                remaining - 1,
            );
        }
        count
    }
}
