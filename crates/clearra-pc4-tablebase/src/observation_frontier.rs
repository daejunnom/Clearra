// SRP rationale: this module has one behavior-level change reason: compose the
// already-bounded concrete hold expansion and lazy multiset reveal families
// into a graph-free observation frontier. Graph lookup, pattern syntax,
// product activation, and I/O remain outside this feature-off state machine.
use core::{fmt, num::NonZeroUsize};
use std::sync::Arc;

use crate::{
    expand_fixed_queue_hold, prepare_pc4_bag_reveal_family, FixedQueueHoldBudgets,
    FixedQueueHoldExpansionError, FixedQueueHoldExpansionGuard, FixedQueueHoldExpansionRequest,
    FixedQueueHoldPath, FixedQueueHoldState, Pc4BagRevealBudgets, Pc4BagRevealCursor,
    Pc4BagRevealFamily, Pc4BagRevealGuard, Pc4BagRevealPageError, Pc4BagRevealPrepareError,
    Pc4BagRevealSequence, Pc4BagState, Pc4ExactProbability, Pc4GraphPiece,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ObservationFrontierPrepareBudgetKind {
    InitialVisiblePieces,
    Placements,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ObservationFrontierPageBudgetKind {
    PageEntries,
    ComposedQueuePieces,
}

/// Finite limits for one prepared observation family and each page transaction.
///
/// Reveal and hold limits are delegated unchanged to their owning primitives.
/// `page_reveal_sequences` and `page_hold_expansions` are resumable work slices:
/// reaching either returns a successful partial page. `page_entries` is a hard
/// request limit. Composed queue pieces are also a resumable per-page work
/// slice after progress, but one queue larger than that limit fails closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4ObservationFrontierBudgets {
    reveal: Pc4BagRevealBudgets,
    hold: FixedQueueHoldBudgets,
    initial_visible_pieces: NonZeroUsize,
    placements: NonZeroUsize,
    page_entries: NonZeroUsize,
    page_reveal_sequences: NonZeroUsize,
    page_hold_expansions: NonZeroUsize,
    page_composed_queue_pieces: NonZeroUsize,
}

impl Pc4ObservationFrontierBudgets {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        reveal: Pc4BagRevealBudgets,
        hold: FixedQueueHoldBudgets,
        initial_visible_pieces: NonZeroUsize,
        placements: NonZeroUsize,
        page_entries: NonZeroUsize,
        page_reveal_sequences: NonZeroUsize,
        page_hold_expansions: NonZeroUsize,
        page_composed_queue_pieces: NonZeroUsize,
    ) -> Self {
        Self {
            reveal,
            hold,
            initial_visible_pieces,
            placements,
            page_entries,
            page_reveal_sequences,
            page_hold_expansions,
            page_composed_queue_pieces,
        }
    }

    pub const fn reveal(self) -> Pc4BagRevealBudgets {
        self.reveal
    }

    pub const fn hold(self) -> FixedQueueHoldBudgets {
        self.hold
    }

    pub const fn initial_visible_pieces(self) -> usize {
        self.initial_visible_pieces.get()
    }

    pub const fn placements(self) -> usize {
        self.placements.get()
    }

    pub const fn page_entries(self) -> usize {
        self.page_entries.get()
    }

    pub const fn page_reveal_sequences(self) -> usize {
        self.page_reveal_sequences.get()
    }

    pub const fn page_hold_expansions(self) -> usize {
        self.page_hold_expansions.get()
    }

    pub const fn page_composed_queue_pieces(self) -> usize {
        self.page_composed_queue_pieces.get()
    }
}

/// Host-owned cancellation observation. This pure checkpoint has no snapshot
/// or external resource whose freshness it could authorize.
pub trait Pc4ObservationFrontierGuard {
    fn is_cancelled(&self) -> bool;
}

impl<F> Pc4ObservationFrontierGuard for F
where
    F: Fn() -> bool,
{
    fn is_cancelled(&self) -> bool {
        self()
    }
}

pub struct Pc4ObservationFrontierRequest<'a> {
    initial_visible_queue: &'a [Pc4GraphPiece],
    preview_length: usize,
    hidden_source_state: Pc4BagState,
    hidden_draws: usize,
    initial_hold: FixedQueueHoldState,
    placement_count: usize,
    budgets: Pc4ObservationFrontierBudgets,
}

impl<'a> Pc4ObservationFrontierRequest<'a> {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        initial_visible_queue: &'a [Pc4GraphPiece],
        preview_length: usize,
        hidden_source_state: Pc4BagState,
        hidden_draws: usize,
        initial_hold: FixedQueueHoldState,
        placement_count: usize,
        budgets: Pc4ObservationFrontierBudgets,
    ) -> Self {
        Self {
            initial_visible_queue,
            preview_length,
            hidden_source_state,
            hidden_draws,
            initial_hold,
            placement_count,
            budgets,
        }
    }

    pub const fn initial_visible_queue(&self) -> &'a [Pc4GraphPiece] {
        self.initial_visible_queue
    }

    pub const fn preview_length(&self) -> usize {
        self.preview_length
    }

    pub const fn hidden_source_state(&self) -> Pc4BagState {
        self.hidden_source_state
    }

    pub const fn hidden_draws(&self) -> usize {
        self.hidden_draws
    }

    pub const fn initial_hold(&self) -> FixedQueueHoldState {
        self.initial_hold
    }

    pub const fn placement_count(&self) -> usize {
        self.placement_count
    }

    pub const fn budgets(&self) -> Pc4ObservationFrontierBudgets {
        self.budgets
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4ObservationFrontierPrepareError {
    Cancelled,
    InvalidVisibleQueueLength {
        expected: usize,
        actual: usize,
    },
    BudgetExceeded {
        kind: Pc4ObservationFrontierPrepareBudgetKind,
        limit: usize,
        attempted: usize,
    },
    LengthOverflow,
    AllocationFailed,
    Reveal(Pc4BagRevealPrepareError),
}

impl Pc4ObservationFrontierPrepareError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_observation_frontier_prepare_cancelled",
            Self::InvalidVisibleQueueLength { .. } => {
                "pc4_observation_frontier_invalid_visible_queue_length"
            }
            Self::BudgetExceeded { .. } => "pc4_observation_frontier_prepare_budget_exceeded",
            Self::LengthOverflow => "pc4_observation_frontier_length_overflow",
            Self::AllocationFailed => "pc4_observation_frontier_prepare_allocation_failed",
            Self::Reveal(error) => error.reason(),
        }
    }
}

impl fmt::Display for Pc4ObservationFrontierPrepareError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4ObservationFrontierPrepareError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4ObservationFrontierPageError {
    Cancelled,
    CursorMismatch,
    CursorInvariantViolation,
    EntryIndexOverflow,
    WorkCounterOverflow,
    BudgetExceeded {
        kind: Pc4ObservationFrontierPageBudgetKind,
        limit: usize,
        attempted: usize,
    },
    LengthOverflow,
    AllocationFailed,
    Reveal(Pc4BagRevealPageError),
    Hold(FixedQueueHoldExpansionError),
}

impl Pc4ObservationFrontierPageError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_observation_frontier_page_cancelled",
            Self::CursorMismatch => "pc4_observation_frontier_cursor_mismatch",
            Self::CursorInvariantViolation => "pc4_observation_frontier_cursor_invariant_violation",
            Self::EntryIndexOverflow => "pc4_observation_frontier_entry_index_overflow",
            Self::WorkCounterOverflow => "pc4_observation_frontier_work_counter_overflow",
            Self::BudgetExceeded { .. } => "pc4_observation_frontier_page_budget_exceeded",
            Self::LengthOverflow => "pc4_observation_frontier_length_overflow",
            Self::AllocationFailed => "pc4_observation_frontier_page_allocation_failed",
            Self::Reveal(error) => error.reason(),
            Self::Hold(error) => error.code(),
        }
    }
}

impl fmt::Display for Pc4ObservationFrontierPageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4ObservationFrontierPageError {}

/// One concrete supply branch at the requested placement frontier.
///
/// `probability` is the exact probability of the hidden reveal sequence. Hold
/// decisions are controllable alternatives, not random events, so sibling hold
/// branches intentionally retain the same reveal probability. Concrete hidden
/// suffixes are evidence for later lazy path reconstruction; a policy layer
/// must group by `terminal_observation` rather than inspect that suffix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ObservationFrontierEntry {
    reveal: Arc<Pc4BagRevealSequence>,
    hold_path_index: usize,
    concrete_supply_queue: Arc<[Pc4GraphPiece]>,
    hold_path: FixedQueueHoldPath,
    observation_width: usize,
    entry_index: u128,
}

impl Pc4ObservationFrontierEntry {
    pub const fn entry_index(&self) -> u128 {
        self.entry_index
    }

    pub fn reveal_rank(&self) -> u128 {
        self.reveal.rank()
    }

    pub const fn hold_path_index(&self) -> usize {
        self.hold_path_index
    }

    pub fn revealed_pieces(&self) -> &[Pc4GraphPiece] {
        self.reveal.pieces()
    }

    pub fn probability(&self) -> Pc4ExactProbability {
        self.reveal.probability()
    }

    pub fn terminal_bag_state(&self) -> Pc4BagState {
        self.reveal.terminal_state()
    }

    pub fn concrete_supply_queue(&self) -> &[Pc4GraphPiece] {
        &self.concrete_supply_queue
    }

    pub const fn hold_path(&self) -> &FixedQueueHoldPath {
        &self.hold_path
    }

    pub fn terminal_remaining_queue(&self) -> &[Pc4GraphPiece] {
        let start = self
            .hold_path
            .terminal_cursor()
            .min(self.concrete_supply_queue.len());
        &self.concrete_supply_queue[start..]
    }

    /// Current piece followed by at most the configured number of previews.
    pub fn terminal_observation(&self) -> &[Pc4GraphPiece] {
        let remaining = self.terminal_remaining_queue();
        &remaining[..remaining.len().min(self.observation_width)]
    }

    /// Concrete suffix beyond the observation window. This is retained only as
    /// lazy reconstruction evidence and must not influence a policy decision.
    pub fn terminal_hidden_queue(&self) -> &[Pc4GraphPiece] {
        let remaining = self.terminal_remaining_queue();
        &remaining[remaining.len().min(self.observation_width)..]
    }

    pub fn terminal_observation_is_complete(&self) -> bool {
        self.terminal_remaining_queue().len() >= self.observation_width
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ObservationFrontierPage {
    entries: Vec<Pc4ObservationFrontierEntry>,
    reveal_sequences_processed: usize,
    hold_expansions: usize,
    composed_queue_pieces: usize,
    stopped_by_work_budget: bool,
    exhausted: bool,
}

impl Pc4ObservationFrontierPage {
    pub fn entries(&self) -> &[Pc4ObservationFrontierEntry] {
        &self.entries
    }

    pub const fn reveal_sequences_processed(&self) -> usize {
        self.reveal_sequences_processed
    }

    pub const fn hold_expansions(&self) -> usize {
        self.hold_expansions
    }

    pub const fn composed_queue_pieces(&self) -> usize {
        self.composed_queue_pieces
    }

    pub const fn stopped_by_work_budget(&self) -> bool {
        self.stopped_by_work_budget
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

#[derive(Clone, Debug)]
pub struct Pc4ObservationFrontierCursor {
    family_token: Arc<()>,
    reveal_cursor: Pc4BagRevealCursor,
    pending_reveal: Option<Arc<Pc4BagRevealSequence>>,
    pending_hold_path_index: usize,
    next_entry_index: u128,
    exhausted: bool,
}

impl Pc4ObservationFrontierCursor {
    pub const fn next_reveal_rank(&self) -> u128 {
        self.reveal_cursor.next_rank()
    }

    pub const fn pending_hold_path_index(&self) -> Option<usize> {
        if self.pending_reveal.is_some() {
            Some(self.pending_hold_path_index)
        } else {
            None
        }
    }

    pub const fn next_entry_index(&self) -> u128 {
        self.next_entry_index
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

/// Prepared graph-free `hold x bag multiset x preview` supply family.
///
/// The caller supplies an exact current-plus-preview queue and the exact bag
/// state *after* those visible pieces were drawn. This layer deliberately does
/// not infer bag provenance or an ordered hidden sequence.
#[derive(Clone, Debug)]
pub struct Pc4ObservationFrontierFamily {
    initial_visible_queue: Vec<Pc4GraphPiece>,
    preview_length: usize,
    observation_width: usize,
    hidden_source_state: Pc4BagState,
    initial_hold: FixedQueueHoldState,
    placement_count: usize,
    reveal_family: Pc4BagRevealFamily,
    budgets: Pc4ObservationFrontierBudgets,
    cursor_token: Arc<()>,
}

impl Pc4ObservationFrontierFamily {
    pub fn initial_visible_queue(&self) -> &[Pc4GraphPiece] {
        &self.initial_visible_queue
    }

    pub const fn preview_length(&self) -> usize {
        self.preview_length
    }

    pub const fn hidden_source_state(&self) -> Pc4BagState {
        self.hidden_source_state
    }

    pub const fn initial_hold(&self) -> FixedQueueHoldState {
        self.initial_hold
    }

    pub const fn placement_count(&self) -> usize {
        self.placement_count
    }

    pub const fn hidden_draws(&self) -> usize {
        self.reveal_family.hidden_draws()
    }

    pub const fn total_reveal_sequences(&self) -> u128 {
        self.reveal_family.total_sequences()
    }

    pub const fn budgets(&self) -> Pc4ObservationFrontierBudgets {
        self.budgets
    }

    pub fn cursor(&self) -> Pc4ObservationFrontierCursor {
        Pc4ObservationFrontierCursor {
            family_token: Arc::clone(&self.cursor_token),
            reveal_cursor: self.reveal_family.cursor(),
            pending_reveal: None,
            pending_hold_path_index: 0,
            next_entry_index: 0,
            exhausted: false,
        }
    }

    /// Returns a bounded canonical page. Any error discards all staged output
    /// and leaves the caller's cursor byte-for-byte semantically unchanged.
    pub fn next_page<G>(
        &self,
        cursor: &mut Pc4ObservationFrontierCursor,
        limit: NonZeroUsize,
        guard: &G,
    ) -> Result<Pc4ObservationFrontierPage, Pc4ObservationFrontierPageError>
    where
        G: Pc4ObservationFrontierGuard,
    {
        if !Arc::ptr_eq(&cursor.family_token, &self.cursor_token) {
            return Err(Pc4ObservationFrontierPageError::CursorMismatch);
        }
        if limit.get() > self.budgets.page_entries() {
            return Err(Pc4ObservationFrontierPageError::BudgetExceeded {
                kind: Pc4ObservationFrontierPageBudgetKind::PageEntries,
                limit: self.budgets.page_entries(),
                attempted: limit.get(),
            });
        }
        check_page_guard(guard)?;

        let mut transaction = cursor.clone();
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(limit.get())
            .map_err(|_| Pc4ObservationFrontierPageError::AllocationFailed)?;
        let mut reveal_sequences_processed = 0_usize;
        let mut hold_expansions = 0_usize;
        let mut composed_queue_pieces = 0_usize;
        let mut stopped_by_work_budget = false;

        while entries.len() < limit.get() && !transaction.exhausted {
            check_page_guard(guard)?;
            if transaction.pending_reveal.is_none() {
                if reveal_sequences_processed >= self.budgets.page_reveal_sequences() {
                    stopped_by_work_budget = true;
                    break;
                }
                let mut reveal_page = self
                    .reveal_family
                    .next_page(
                        &mut transaction.reveal_cursor,
                        NonZeroUsize::MIN,
                        &GuardAdapter(guard),
                    )
                    .map_err(map_reveal_page_error)?;
                if reveal_page.is_empty() {
                    transaction.exhausted = true;
                    break;
                }
                let reveal = reveal_page
                    .pop()
                    .ok_or(Pc4ObservationFrontierPageError::CursorInvariantViolation)?;
                transaction.pending_reveal = Some(Arc::new(reveal));
                transaction.pending_hold_path_index = 0;
                reveal_sequences_processed = checked_increment(reveal_sequences_processed)?;
            }

            if hold_expansions >= self.budgets.page_hold_expansions() {
                stopped_by_work_budget = true;
                break;
            }
            let reveal = transaction
                .pending_reveal
                .as_ref()
                .cloned()
                .ok_or(Pc4ObservationFrontierPageError::CursorInvariantViolation)?;
            let queue_length = self
                .initial_visible_queue
                .len()
                .checked_add(reveal.pieces().len())
                .ok_or(Pc4ObservationFrontierPageError::LengthOverflow)?;
            let attempted_queue_pieces = composed_queue_pieces.checked_add(queue_length).ok_or(
                Pc4ObservationFrontierPageError::BudgetExceeded {
                    kind: Pc4ObservationFrontierPageBudgetKind::ComposedQueuePieces,
                    limit: self.budgets.page_composed_queue_pieces(),
                    attempted: usize::MAX,
                },
            )?;
            if attempted_queue_pieces > self.budgets.page_composed_queue_pieces() {
                if composed_queue_pieces != 0 {
                    stopped_by_work_budget = true;
                    break;
                }
                return Err(Pc4ObservationFrontierPageError::BudgetExceeded {
                    kind: Pc4ObservationFrontierPageBudgetKind::ComposedQueuePieces,
                    limit: self.budgets.page_composed_queue_pieces(),
                    attempted: attempted_queue_pieces,
                });
            }

            let mut queue = Vec::new();
            queue
                .try_reserve_exact(queue_length)
                .map_err(|_| Pc4ObservationFrontierPageError::AllocationFailed)?;
            queue.extend_from_slice(&self.initial_visible_queue);
            queue.extend_from_slice(reveal.pieces());
            let concrete_supply_queue: Arc<[Pc4GraphPiece]> = Arc::from(queue.into_boxed_slice());
            let expansion = expand_fixed_queue_hold(
                FixedQueueHoldExpansionRequest::new(
                    &concrete_supply_queue,
                    self.initial_hold,
                    self.placement_count,
                    self.budgets.hold(),
                ),
                &GuardAdapter(guard),
            )
            .map_err(map_hold_error)?;
            hold_expansions = checked_increment(hold_expansions)?;
            composed_queue_pieces = attempted_queue_pieces;

            let paths = expansion.paths();
            if transaction.pending_hold_path_index > paths.len() {
                return Err(Pc4ObservationFrontierPageError::CursorInvariantViolation);
            }
            if transaction.pending_hold_path_index == paths.len() {
                transaction.pending_reveal = None;
                transaction.pending_hold_path_index = 0;
                if transaction.reveal_cursor.is_exhausted() {
                    transaction.exhausted = true;
                }
                continue;
            }

            while transaction.pending_hold_path_index < paths.len() && entries.len() < limit.get() {
                check_page_guard(guard)?;
                let hold_path_index = transaction.pending_hold_path_index;
                entries.push(Pc4ObservationFrontierEntry {
                    reveal: Arc::clone(&reveal),
                    hold_path_index,
                    concrete_supply_queue: Arc::clone(&concrete_supply_queue),
                    hold_path: paths[hold_path_index].clone(),
                    observation_width: self.observation_width,
                    entry_index: transaction.next_entry_index,
                });
                transaction.pending_hold_path_index = transaction
                    .pending_hold_path_index
                    .checked_add(1)
                    .ok_or(Pc4ObservationFrontierPageError::CursorInvariantViolation)?;
                transaction.next_entry_index = transaction
                    .next_entry_index
                    .checked_add(1)
                    .ok_or(Pc4ObservationFrontierPageError::EntryIndexOverflow)?;
            }
            if transaction.pending_hold_path_index == paths.len() {
                transaction.pending_reveal = None;
                transaction.pending_hold_path_index = 0;
                if transaction.reveal_cursor.is_exhausted() {
                    transaction.exhausted = true;
                }
            }
        }

        check_page_guard(guard)?;
        let exhausted = transaction.exhausted;
        *cursor = transaction;
        Ok(Pc4ObservationFrontierPage {
            entries,
            reveal_sequences_processed,
            hold_expansions,
            composed_queue_pieces,
            stopped_by_work_budget,
            exhausted,
        })
    }
}

/// Validates the declared observation boundary, owns the visible prefix, and
/// prepares only the hidden reveal count/memo family. No hold path is expanded
/// until its page is requested.
pub fn prepare_pc4_observation_frontier<G>(
    request: Pc4ObservationFrontierRequest<'_>,
    guard: &G,
) -> Result<Pc4ObservationFrontierFamily, Pc4ObservationFrontierPrepareError>
where
    G: Pc4ObservationFrontierGuard,
{
    check_prepare_guard(guard)?;
    let observation_width = request
        .preview_length
        .checked_add(1)
        .ok_or(Pc4ObservationFrontierPrepareError::LengthOverflow)?;
    if request.initial_visible_queue.len() != observation_width {
        return Err(
            Pc4ObservationFrontierPrepareError::InvalidVisibleQueueLength {
                expected: observation_width,
                actual: request.initial_visible_queue.len(),
            },
        );
    }
    require_prepare_budget(
        request.initial_visible_queue.len(),
        request.budgets.initial_visible_pieces(),
        Pc4ObservationFrontierPrepareBudgetKind::InitialVisiblePieces,
    )?;
    require_prepare_budget(
        request.placement_count,
        request.budgets.placements(),
        Pc4ObservationFrontierPrepareBudgetKind::Placements,
    )?;

    let mut initial_visible_queue = Vec::new();
    initial_visible_queue
        .try_reserve_exact(request.initial_visible_queue.len())
        .map_err(|_| Pc4ObservationFrontierPrepareError::AllocationFailed)?;
    initial_visible_queue.extend_from_slice(request.initial_visible_queue);
    let reveal_family = prepare_pc4_bag_reveal_family(
        request.hidden_source_state,
        request.hidden_draws,
        request.budgets.reveal(),
        &GuardAdapter(guard),
    )
    .map_err(map_reveal_prepare_error)?;
    check_prepare_guard(guard)?;

    Ok(Pc4ObservationFrontierFamily {
        initial_visible_queue,
        preview_length: request.preview_length,
        observation_width,
        hidden_source_state: request.hidden_source_state,
        initial_hold: request.initial_hold,
        placement_count: request.placement_count,
        reveal_family,
        budgets: request.budgets,
        cursor_token: Arc::new(()),
    })
}

struct GuardAdapter<'a, G>(&'a G);

impl<G> Pc4BagRevealGuard for GuardAdapter<'_, G>
where
    G: Pc4ObservationFrontierGuard,
{
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

impl<G> FixedQueueHoldExpansionGuard for GuardAdapter<'_, G>
where
    G: Pc4ObservationFrontierGuard,
{
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

fn check_prepare_guard<G>(guard: &G) -> Result<(), Pc4ObservationFrontierPrepareError>
where
    G: Pc4ObservationFrontierGuard,
{
    if guard.is_cancelled() {
        Err(Pc4ObservationFrontierPrepareError::Cancelled)
    } else {
        Ok(())
    }
}

fn check_page_guard<G>(guard: &G) -> Result<(), Pc4ObservationFrontierPageError>
where
    G: Pc4ObservationFrontierGuard,
{
    if guard.is_cancelled() {
        Err(Pc4ObservationFrontierPageError::Cancelled)
    } else {
        Ok(())
    }
}

fn require_prepare_budget(
    attempted: usize,
    limit: usize,
    kind: Pc4ObservationFrontierPrepareBudgetKind,
) -> Result<(), Pc4ObservationFrontierPrepareError> {
    if attempted > limit {
        Err(Pc4ObservationFrontierPrepareError::BudgetExceeded {
            kind,
            limit,
            attempted,
        })
    } else {
        Ok(())
    }
}

fn map_reveal_prepare_error(error: Pc4BagRevealPrepareError) -> Pc4ObservationFrontierPrepareError {
    match error {
        Pc4BagRevealPrepareError::Cancelled => Pc4ObservationFrontierPrepareError::Cancelled,
        error => Pc4ObservationFrontierPrepareError::Reveal(error),
    }
}

fn map_reveal_page_error(error: Pc4BagRevealPageError) -> Pc4ObservationFrontierPageError {
    match error {
        Pc4BagRevealPageError::Cancelled => Pc4ObservationFrontierPageError::Cancelled,
        error => Pc4ObservationFrontierPageError::Reveal(error),
    }
}

fn map_hold_error(error: FixedQueueHoldExpansionError) -> Pc4ObservationFrontierPageError {
    match error {
        FixedQueueHoldExpansionError::Cancelled => Pc4ObservationFrontierPageError::Cancelled,
        error => Pc4ObservationFrontierPageError::Hold(error),
    }
}

fn checked_increment(value: usize) -> Result<usize, Pc4ObservationFrontierPageError> {
    value
        .checked_add(1)
        .ok_or(Pc4ObservationFrontierPageError::WorkCounterOverflow)
}

#[cfg(test)]
mod tests {
    use core::{cell::Cell, num::NonZeroUsize};

    use super::*;
    use crate::{
        FixedQueueHoldDecision, Pc4BagProfile, Pc4BagRevealPageBudgetKind,
        Pc4BagRevealPrepareBudgetKind,
    };

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("positive synthetic budget")
    }

    fn budgets() -> Pc4ObservationFrontierBudgets {
        Pc4ObservationFrontierBudgets::new(
            Pc4BagRevealBudgets::new(
                nonzero(32),
                nonzero(1_000_000),
                nonzero(100_000),
                nonzero(1),
                nonzero(10_000_000),
                nonzero(1_000_000),
            ),
            FixedQueueHoldBudgets::new(
                nonzero(100_000),
                nonzero(100_000),
                nonzero(1_000_000),
                nonzero(100_000),
            ),
            nonzero(16),
            nonzero(16),
            nonzero(64),
            nonzero(64),
            nonzero(64),
            nonzero(1_024),
        )
    }

    fn standard_state(remainder: [u32; 7]) -> Pc4BagState {
        Pc4BagState::new(Pc4BagProfile::standard_seven_bag(), remainder, 2)
            .expect("valid synthetic bag state")
    }

    fn family(
        visible: &[Pc4GraphPiece],
        preview_length: usize,
        hidden_state: Pc4BagState,
        hidden_draws: usize,
        hold: FixedQueueHoldState,
        placements: usize,
    ) -> Pc4ObservationFrontierFamily {
        prepare_pc4_observation_frontier(
            Pc4ObservationFrontierRequest::new(
                visible,
                preview_length,
                hidden_state,
                hidden_draws,
                hold,
                placements,
                budgets(),
            ),
            &|| false,
        )
        .expect("synthetic observation family")
    }

    fn drain(
        family: &Pc4ObservationFrontierFamily,
        page_size: usize,
    ) -> Vec<Pc4ObservationFrontierEntry> {
        let mut cursor = family.cursor();
        let mut entries = Vec::new();
        while !cursor.is_exhausted() {
            let page = family
                .next_page(&mut cursor, nonzero(page_size), &|| false)
                .expect("synthetic observation page");
            entries.extend_from_slice(page.entries());
        }
        entries
    }

    #[test]
    fn hidden_draw_refills_the_terminal_preview_without_exposing_it_to_hold_early() {
        let family = family(
            &[Pc4GraphPiece::T, Pc4GraphPiece::I],
            1,
            standard_state([0, 1, 0, 0, 0, 0, 0]),
            1,
            FixedQueueHoldState::Disabled,
            1,
        );
        let entries = drain(&family, 8);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].revealed_pieces(), &[Pc4GraphPiece::O]);
        assert_eq!(
            entries[0].concrete_supply_queue(),
            &[Pc4GraphPiece::T, Pc4GraphPiece::I, Pc4GraphPiece::O]
        );
        assert_eq!(
            entries[0].hold_path().placement_queue(),
            &[Pc4GraphPiece::T]
        );
        assert_eq!(
            entries[0].terminal_observation(),
            &[Pc4GraphPiece::I, Pc4GraphPiece::O]
        );
        assert!(entries[0].terminal_hidden_queue().is_empty());
        assert!(entries[0].terminal_observation_is_complete());
    }

    #[test]
    fn empty_hold_store_and_occupied_hold_swap_preserve_exact_decision_evidence() {
        let hidden_state = standard_state([0, 1, 0, 0, 0, 0, 0]);
        let empty = family(
            &[Pc4GraphPiece::I, Pc4GraphPiece::O],
            1,
            hidden_state,
            0,
            FixedQueueHoldState::Empty,
            1,
        );
        let empty_entries = drain(&empty, 8);
        assert_eq!(empty_entries.len(), 2);
        assert_eq!(
            empty_entries[0].hold_path().steps()[0].decision(),
            FixedQueueHoldDecision::UseCurrent
        );
        assert_eq!(
            empty_entries[1].hold_path().steps()[0].decision(),
            FixedQueueHoldDecision::StoreCurrentUseNext
        );
        assert_eq!(
            empty_entries[1].hold_path().terminal_hold(),
            FixedQueueHoldState::Occupied(Pc4GraphPiece::I)
        );

        let occupied = family(
            &[Pc4GraphPiece::I],
            0,
            hidden_state,
            0,
            FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
            1,
        );
        let occupied_entries = drain(&occupied, 8);
        assert_eq!(occupied_entries.len(), 2);
        assert_eq!(
            occupied_entries[1].hold_path().steps()[0].decision(),
            FixedQueueHoldDecision::SwapHeld
        );
        assert_eq!(
            occupied_entries[1].hold_path().placement_queue(),
            &[Pc4GraphPiece::T]
        );
    }

    #[test]
    fn duplicate_multiset_draws_have_canonical_sequences_and_exact_probability() {
        let profile = Pc4BagProfile::new([2, 1, 0, 0, 0, 0, 0]).expect("valid profile");
        let state = Pc4BagState::new(profile, [2, 1, 0, 0, 0, 0, 0], 7).expect("valid remainder");
        let family = family(
            &[Pc4GraphPiece::T],
            0,
            state,
            2,
            FixedQueueHoldState::Disabled,
            0,
        );
        let entries = drain(&family, 2);

        assert_eq!(
            entries
                .iter()
                .map(Pc4ObservationFrontierEntry::revealed_pieces)
                .collect::<Vec<_>>(),
            vec![
                &[Pc4GraphPiece::I, Pc4GraphPiece::I][..],
                &[Pc4GraphPiece::I, Pc4GraphPiece::O][..],
                &[Pc4GraphPiece::O, Pc4GraphPiece::I][..],
            ]
        );
        assert!(entries.iter().all(|entry| {
            entry.probability().numerator() == 1 && entry.probability().denominator() == 3
        }));
        assert_eq!(
            entries
                .iter()
                .map(Pc4ObservationFrontierEntry::reveal_rank)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn pages_resume_inside_one_reveal_then_continue_in_canonical_pair_order() {
        let family = family(
            &[Pc4GraphPiece::I],
            0,
            standard_state([0, 1, 1, 0, 0, 0, 0]),
            1,
            FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
            1,
        );
        let mut cursor = family.cursor();
        let first = family
            .next_page(&mut cursor, nonzero(1), &|| false)
            .expect("first page");
        assert_eq!(
            (
                first.entries()[0].reveal_rank(),
                first.entries()[0].hold_path_index()
            ),
            (0, 0)
        );
        assert_eq!(cursor.pending_hold_path_index(), Some(1));
        let rest = drain_from(&family, &mut cursor, 1);
        let pairs = rest
            .iter()
            .map(|entry| (entry.reveal_rank(), entry.hold_path_index()))
            .collect::<Vec<_>>();
        assert_eq!(pairs, vec![(0, 1), (1, 0), (1, 1)]);
        assert_eq!(cursor.next_entry_index(), 4);
    }

    #[test]
    fn canonical_family_is_independent_of_page_size() {
        let family = family(
            &[Pc4GraphPiece::I, Pc4GraphPiece::O],
            1,
            standard_state([0, 0, 1, 1, 0, 0, 0]),
            1,
            FixedQueueHoldState::Empty,
            2,
        );

        assert_eq!(drain(&family, 1), drain(&family, 64));
    }

    fn drain_from(
        family: &Pc4ObservationFrontierFamily,
        cursor: &mut Pc4ObservationFrontierCursor,
        page_size: usize,
    ) -> Vec<Pc4ObservationFrontierEntry> {
        let mut entries = Vec::new();
        while !cursor.is_exhausted() {
            let page = family
                .next_page(cursor, nonzero(page_size), &|| false)
                .expect("continuation page");
            entries.extend_from_slice(page.entries());
        }
        entries
    }

    #[test]
    fn work_slice_can_commit_an_empty_dead_reveal_and_resume_without_spinning() {
        let mut constrained = budgets();
        constrained.page_reveal_sequences = nonzero(1);
        constrained.page_hold_expansions = nonzero(1);
        let family = prepare_pc4_observation_frontier(
            Pc4ObservationFrontierRequest::new(
                &[Pc4GraphPiece::I],
                0,
                standard_state([0, 1, 1, 0, 0, 0, 0]),
                1,
                FixedQueueHoldState::Disabled,
                3,
                constrained,
            ),
            &|| false,
        )
        .expect("dead family");
        let mut cursor = family.cursor();
        let first = family
            .next_page(&mut cursor, nonzero(4), &|| false)
            .expect("bounded dead page");
        assert!(first.entries().is_empty());
        assert!(first.stopped_by_work_budget() || first.is_exhausted());
        assert!(cursor.next_reveal_rank() > 0 || cursor.is_exhausted());
        while !cursor.is_exhausted() {
            family
                .next_page(&mut cursor, nonzero(4), &|| false)
                .expect("remaining dead page");
        }
    }

    struct CancelAfter {
        checks: Cell<usize>,
        threshold: usize,
    }

    impl Pc4ObservationFrontierGuard for CancelAfter {
        fn is_cancelled(&self) -> bool {
            let next = self.checks.get() + 1;
            self.checks.set(next);
            next >= self.threshold
        }
    }

    #[test]
    fn cancellation_after_staged_entries_rolls_the_cursor_back_transactionally() {
        let family = family(
            &[Pc4GraphPiece::I],
            0,
            standard_state([0, 1, 1, 0, 0, 0, 0]),
            1,
            FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
            1,
        );
        let mut cursor = family.cursor();
        let before = (
            cursor.next_reveal_rank(),
            cursor.pending_hold_path_index(),
            cursor.next_entry_index(),
            cursor.is_exhausted(),
        );
        let guard = CancelAfter {
            checks: Cell::new(0),
            threshold: 25,
        };
        assert_eq!(
            family.next_page(&mut cursor, nonzero(4), &guard),
            Err(Pc4ObservationFrontierPageError::Cancelled)
        );
        assert_eq!(
            (
                cursor.next_reveal_rank(),
                cursor.pending_hold_path_index(),
                cursor.next_entry_index(),
                cursor.is_exhausted(),
            ),
            before
        );
        assert_eq!(guard.checks.get(), guard.threshold);
    }

    #[test]
    fn invalid_observation_cursor_and_hard_budgets_fail_closed() {
        let state = standard_state([0, 1, 0, 0, 0, 0, 0]);
        assert_eq!(
            prepare_pc4_observation_frontier(
                Pc4ObservationFrontierRequest::new(
                    &[Pc4GraphPiece::I],
                    1,
                    state,
                    0,
                    FixedQueueHoldState::Disabled,
                    0,
                    budgets(),
                ),
                &|| false,
            )
            .expect_err("mismatched visible observation must fail"),
            Pc4ObservationFrontierPrepareError::InvalidVisibleQueueLength {
                expected: 2,
                actual: 1,
            }
        );

        let first = family(
            &[Pc4GraphPiece::I],
            0,
            state,
            0,
            FixedQueueHoldState::Disabled,
            1,
        );
        let second = family(
            &[Pc4GraphPiece::I],
            0,
            state,
            0,
            FixedQueueHoldState::Disabled,
            1,
        );
        let mut foreign = first.cursor();
        assert_eq!(
            second.next_page(&mut foreign, nonzero(1), &|| false),
            Err(Pc4ObservationFrontierPageError::CursorMismatch)
        );

        let mut constrained = budgets();
        constrained.page_entries = nonzero(1);
        constrained.page_composed_queue_pieces = nonzero(1);
        let constrained_family = prepare_pc4_observation_frontier(
            Pc4ObservationFrontierRequest::new(
                &[Pc4GraphPiece::I],
                0,
                state,
                1,
                FixedQueueHoldState::Disabled,
                1,
                constrained,
            ),
            &|| false,
        )
        .expect("constrained family");
        let mut cursor = constrained_family.cursor();
        let before = cursor.next_reveal_rank();
        assert_eq!(
            constrained_family.next_page(&mut cursor, nonzero(2), &|| false),
            Err(Pc4ObservationFrontierPageError::BudgetExceeded {
                kind: Pc4ObservationFrontierPageBudgetKind::PageEntries,
                limit: 1,
                attempted: 2,
            })
        );
        assert_eq!(cursor.next_reveal_rank(), before);
        assert_eq!(
            constrained_family.next_page(&mut cursor, nonzero(1), &|| false),
            Err(Pc4ObservationFrontierPageError::BudgetExceeded {
                kind: Pc4ObservationFrontierPageBudgetKind::ComposedQueuePieces,
                limit: 1,
                attempted: 2,
            })
        );
        assert_eq!(cursor.next_reveal_rank(), before);
    }

    #[test]
    fn nested_reveal_and_hold_failures_remain_typed_and_transactional() {
        let state = standard_state([0, 1, 0, 0, 0, 0, 0]);
        let mut reveal_limited = budgets();
        reveal_limited.reveal = Pc4BagRevealBudgets::new(
            nonzero(1),
            nonzero(1),
            nonzero(1),
            nonzero(1),
            nonzero(1),
            nonzero(1),
        );
        assert!(matches!(
            prepare_pc4_observation_frontier(
                Pc4ObservationFrontierRequest::new(
                    &[Pc4GraphPiece::I],
                    0,
                    state,
                    2,
                    FixedQueueHoldState::Disabled,
                    0,
                    reveal_limited,
                ),
                &|| false,
            ),
            Err(Pc4ObservationFrontierPrepareError::Reveal(
                Pc4BagRevealPrepareError::BudgetExceeded {
                    kind: Pc4BagRevealPrepareBudgetKind::HiddenDraws,
                    ..
                }
            ))
        ));

        let mut page_reveal_limited = budgets();
        page_reveal_limited.reveal = Pc4BagRevealBudgets::new(
            nonzero(8),
            nonzero(100),
            nonzero(100),
            nonzero(1),
            nonzero(1),
            nonzero(8),
        );
        let reveal_family = prepare_pc4_observation_frontier(
            Pc4ObservationFrontierRequest::new(
                &[Pc4GraphPiece::I],
                0,
                standard_state([0, 1, 1, 0, 0, 0, 0]),
                2,
                FixedQueueHoldState::Disabled,
                0,
                page_reveal_limited,
            ),
            &|| false,
        )
        .expect("page-limited family");
        let mut cursor = reveal_family.cursor();
        assert!(matches!(
            reveal_family.next_page(&mut cursor, nonzero(1), &|| false),
            Err(Pc4ObservationFrontierPageError::Reveal(
                Pc4BagRevealPageError::BudgetExceeded {
                    kind: Pc4BagRevealPageBudgetKind::RankSteps,
                    ..
                }
            ))
        ));
        assert_eq!(cursor.next_reveal_rank(), 0);

        let mut hold_limited = budgets();
        hold_limited.hold =
            FixedQueueHoldBudgets::new(nonzero(1), nonzero(1), nonzero(1), nonzero(1));
        let hold_family = prepare_pc4_observation_frontier(
            Pc4ObservationFrontierRequest::new(
                &[Pc4GraphPiece::I],
                0,
                state,
                0,
                FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
                1,
                hold_limited,
            ),
            &|| false,
        )
        .expect("hold-limited family");
        let mut cursor = hold_family.cursor();
        assert!(matches!(
            hold_family.next_page(&mut cursor, nonzero(1), &|| false),
            Err(Pc4ObservationFrontierPageError::Hold(
                FixedQueueHoldExpansionError::BudgetExceeded(_)
            ))
        ));
        assert_eq!(cursor.next_reveal_rank(), 0);
    }

    #[test]
    fn exhausted_concrete_supply_never_releases_a_held_piece_without_a_current() {
        let family = family(
            &[Pc4GraphPiece::I],
            0,
            standard_state([0, 1, 0, 0, 0, 0, 0]),
            0,
            FixedQueueHoldState::Occupied(Pc4GraphPiece::T),
            2,
        );
        let mut cursor = family.cursor();
        let page = family
            .next_page(&mut cursor, nonzero(8), &|| false)
            .expect("dead supply page");
        assert!(page.entries().is_empty());
        assert!(page.is_exhausted());
        assert!(cursor.is_exhausted());
    }

    #[test]
    fn work_counters_fail_closed_instead_of_wrapping() {
        assert_eq!(
            checked_increment(usize::MAX),
            Err(Pc4ObservationFrontierPageError::WorkCounterOverflow)
        );
    }
}
