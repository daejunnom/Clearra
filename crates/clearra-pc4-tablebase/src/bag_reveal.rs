// SRP rationale: this module owns only bounded lazy enumeration of hidden draws
// from one exact multiset-bag state. Observation, hold, graph, and product policy
// remain outside this family.
use core::{fmt, num::NonZeroUsize};
use std::{collections::HashMap, sync::Arc};

use crate::{draw_pc4_bag, Pc4BagDrawError, Pc4BagState, Pc4GraphPiece};

/// Absolute recursion-safety ceiling. PC4 callers need far fewer draws, while
/// this limit also keeps hostile caller budgets from turning counting into an
/// unbounded stack request.
pub const PC4_BAG_REVEAL_ABSOLUTE_DRAW_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4BagRevealPrepareBudgetKind {
    HiddenDraws,
    CountSteps,
    MemoEntries,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4BagRevealPageBudgetKind {
    PageSequences,
    RankSteps,
    AllocatedPieces,
}

/// Caller-selected finite limits for preparing and paging one reveal family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4BagRevealBudgets {
    hidden_draws: NonZeroUsize,
    count_steps: NonZeroUsize,
    memo_entries: NonZeroUsize,
    page_sequences: NonZeroUsize,
    rank_steps: NonZeroUsize,
    allocated_pieces: NonZeroUsize,
}

impl Pc4BagRevealBudgets {
    pub const fn new(
        hidden_draws: NonZeroUsize,
        count_steps: NonZeroUsize,
        memo_entries: NonZeroUsize,
        page_sequences: NonZeroUsize,
        rank_steps: NonZeroUsize,
        allocated_pieces: NonZeroUsize,
    ) -> Self {
        Self {
            hidden_draws,
            count_steps,
            memo_entries,
            page_sequences,
            rank_steps,
            allocated_pieces,
        }
    }

    pub const fn hidden_draws(self) -> usize {
        self.hidden_draws.get()
    }

    pub const fn count_steps(self) -> usize {
        self.count_steps.get()
    }

    pub const fn memo_entries(self) -> usize {
        self.memo_entries.get()
    }

    pub const fn page_sequences(self) -> usize {
        self.page_sequences.get()
    }

    pub const fn rank_steps(self) -> usize {
        self.rank_steps.get()
    }

    pub const fn allocated_pieces(self) -> usize {
        self.allocated_pieces.get()
    }
}

/// Host-owned cancellation observation. The pure family has no external
/// snapshot to refresh; cursor/family binding protects its local identity.
pub trait Pc4BagRevealGuard {
    fn is_cancelled(&self) -> bool;
}

impl<F> Pc4BagRevealGuard for F
where
    F: Fn() -> bool,
{
    fn is_cancelled(&self) -> bool {
        self()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4ExactProbability {
    numerator: u128,
    denominator: u128,
}

impl Pc4ExactProbability {
    const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    pub const fn zero() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }

    pub const fn one() -> Self {
        Self::ONE
    }

    pub const fn numerator(self) -> u128 {
        self.numerator
    }

    pub const fn denominator(self) -> u128 {
        self.denominator
    }

    /// Adds two canonical probabilities without first multiplying both full
    /// denominators. `None` is a representational overflow, never rounding.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let denominator_gcd = gcd(self.denominator, other.denominator);
        let left_multiplier = other.denominator / denominator_gcd;
        let right_multiplier = self.denominator / denominator_gcd;
        let numerator = self
            .numerator
            .checked_mul(left_multiplier)?
            .checked_add(other.numerator.checked_mul(right_multiplier)?)?;
        let denominator = self.denominator.checked_mul(left_multiplier)?;
        let divisor = gcd(numerator, denominator);
        Some(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    fn multiply(self, numerator: u32, denominator: u32) -> Option<Self> {
        let mut numerator = u128::from(numerator);
        let mut denominator = u128::from(denominator);
        // A distinct-piece bag outcome carries its raw multiplicity over the
        // current bag size, which need not itself be reduced (for example,
        // 2/4). Reduce that factor before cross-cancelling with the accumulated
        // probability so the public result remains canonical.
        let factor_divisor = gcd(numerator, denominator);
        numerator /= factor_divisor;
        denominator /= factor_divisor;
        let cancel_left = gcd(self.numerator, denominator);
        let cancel_right = gcd(numerator, self.denominator);
        let reduced_left_numerator = self.numerator / cancel_left;
        let reduced_right_denominator = denominator / cancel_left;
        let reduced_right_numerator = numerator / cancel_right;
        let reduced_left_denominator = self.denominator / cancel_right;
        Some(Self {
            numerator: reduced_left_numerator.checked_mul(reduced_right_numerator)?,
            denominator: reduced_left_denominator.checked_mul(reduced_right_denominator)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4BagRevealPrepareError {
    Cancelled,
    AbsoluteDrawLimitExceeded {
        limit: usize,
        requested: usize,
    },
    BudgetExceeded {
        kind: Pc4BagRevealPrepareBudgetKind,
        limit: usize,
        attempted: usize,
    },
    SequenceCountOverflow,
    AllocationFailed,
    Draw(Pc4BagDrawError),
}

impl Pc4BagRevealPrepareError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_bag_reveal_prepare_cancelled",
            Self::AbsoluteDrawLimitExceeded { .. } => "pc4_bag_reveal_absolute_draw_limit_exceeded",
            Self::BudgetExceeded { .. } => "pc4_bag_reveal_prepare_budget_exceeded",
            Self::SequenceCountOverflow => "pc4_bag_reveal_sequence_count_overflow",
            Self::AllocationFailed => "pc4_bag_reveal_prepare_allocation_failed",
            Self::Draw(error) => error.code(),
        }
    }
}

impl fmt::Display for Pc4BagRevealPrepareError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4BagRevealPrepareError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4BagRevealPageError {
    Cancelled,
    CursorMismatch,
    BudgetExceeded {
        kind: Pc4BagRevealPageBudgetKind,
        limit: usize,
        attempted: usize,
    },
    ProbabilityOverflow,
    AllocationFailed,
    MemoInvariantViolation,
    Draw(Pc4BagDrawError),
}

impl Pc4BagRevealPageError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_bag_reveal_page_cancelled",
            Self::CursorMismatch => "pc4_bag_reveal_cursor_mismatch",
            Self::BudgetExceeded { .. } => "pc4_bag_reveal_page_budget_exceeded",
            Self::ProbabilityOverflow => "pc4_bag_reveal_probability_overflow",
            Self::AllocationFailed => "pc4_bag_reveal_page_allocation_failed",
            Self::MemoInvariantViolation => "pc4_bag_reveal_memo_invariant_violation",
            Self::Draw(error) => error.code(),
        }
    }
}

impl fmt::Display for Pc4BagRevealPageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4BagRevealPageError {}

/// One lexicographically ranked concrete hidden-piece sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4BagRevealSequence {
    rank: u128,
    pieces: Vec<Pc4GraphPiece>,
    probability: Pc4ExactProbability,
    terminal_state: Pc4BagState,
}

impl Pc4BagRevealSequence {
    pub const fn rank(&self) -> u128 {
        self.rank
    }

    pub fn pieces(&self) -> &[Pc4GraphPiece] {
        &self.pieces
    }

    pub const fn probability(&self) -> Pc4ExactProbability {
        self.probability
    }

    pub const fn terminal_state(&self) -> Pc4BagState {
        self.terminal_state
    }

    pub(crate) fn into_parts(self) -> (u128, Vec<Pc4GraphPiece>, Pc4ExactProbability, Pc4BagState) {
        (
            self.rank,
            self.pieces,
            self.probability,
            self.terminal_state,
        )
    }
}

/// Prepared count/memo family. Concrete sequences are reconstructed by rank
/// only when a bounded page is requested; the full family is never allocated.
#[derive(Clone, Debug)]
pub struct Pc4BagRevealFamily {
    source_state: Pc4BagState,
    hidden_draws: usize,
    total_sequences: u128,
    suffix_counts: HashMap<(Pc4BagState, usize), u128>,
    budgets: Pc4BagRevealBudgets,
    cursor_token: Arc<()>,
}

impl Pc4BagRevealFamily {
    pub const fn source_state(&self) -> Pc4BagState {
        self.source_state
    }

    pub const fn hidden_draws(&self) -> usize {
        self.hidden_draws
    }

    pub const fn total_sequences(&self) -> u128 {
        self.total_sequences
    }

    pub fn cursor(&self) -> Pc4BagRevealCursor {
        Pc4BagRevealCursor {
            family_token: Arc::clone(&self.cursor_token),
            next_rank: 0,
            exhausted: false,
        }
    }

    /// Produces the next bounded lexicographic page transactionally. On any
    /// error the caller's cursor is unchanged and no partial page is returned.
    pub fn next_page<G>(
        &self,
        cursor: &mut Pc4BagRevealCursor,
        limit: NonZeroUsize,
        guard: &G,
    ) -> Result<Vec<Pc4BagRevealSequence>, Pc4BagRevealPageError>
    where
        G: Pc4BagRevealGuard,
    {
        if !Arc::ptr_eq(&cursor.family_token, &self.cursor_token) {
            return Err(Pc4BagRevealPageError::CursorMismatch);
        }
        if limit.get() > self.budgets.page_sequences() {
            return Err(Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::PageSequences,
                limit: self.budgets.page_sequences(),
                attempted: limit.get(),
            });
        }
        check_page_guard(guard)?;
        if cursor.exhausted {
            return Ok(Vec::new());
        }

        let remaining = self.total_sequences - cursor.next_rank;
        let page_length = usize::try_from(remaining.min(limit.get() as u128))
            .map_err(|_| Pc4BagRevealPageError::AllocationFailed)?;
        let allocated_pieces = page_length.checked_mul(self.hidden_draws).ok_or(
            Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::AllocatedPieces,
                limit: self.budgets.allocated_pieces(),
                attempted: usize::MAX,
            },
        )?;
        if allocated_pieces > self.budgets.allocated_pieces() {
            return Err(Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::AllocatedPieces,
                limit: self.budgets.allocated_pieces(),
                attempted: allocated_pieces,
            });
        }

        let mut page = Vec::new();
        page.try_reserve_exact(page_length)
            .map_err(|_| Pc4BagRevealPageError::AllocationFailed)?;
        let mut next_rank = cursor.next_rank;
        let mut rank_steps = 0_usize;
        for _ in 0..page_length {
            check_page_guard(guard)?;
            page.push(self.sequence_at_rank(next_rank, &mut rank_steps, guard)?);
            next_rank += 1;
        }
        check_page_guard(guard)?;

        cursor.next_rank = next_rank;
        cursor.exhausted = next_rank == self.total_sequences;
        Ok(page)
    }

    fn sequence_at_rank<G>(
        &self,
        original_rank: u128,
        rank_steps: &mut usize,
        guard: &G,
    ) -> Result<Pc4BagRevealSequence, Pc4BagRevealPageError>
    where
        G: Pc4BagRevealGuard,
    {
        let mut pieces = Vec::new();
        pieces
            .try_reserve_exact(self.hidden_draws)
            .map_err(|_| Pc4BagRevealPageError::AllocationFailed)?;
        let mut state = self.source_state;
        let mut remaining_draws = self.hidden_draws;
        let mut residual_rank = original_rank;
        let mut probability = Pc4ExactProbability::ONE;

        while remaining_draws != 0 {
            check_page_guard(guard)?;
            let batch = draw_pc4_bag(state).map_err(Pc4BagRevealPageError::Draw)?;
            let mut selected = None;
            for transition in batch.transitions().iter().copied() {
                consume_page_budget(
                    rank_steps,
                    self.budgets.rank_steps(),
                    Pc4BagRevealPageBudgetKind::RankSteps,
                )?;
                let suffix_count = if remaining_draws == 1 {
                    1
                } else {
                    *self
                        .suffix_counts
                        .get(&(transition.next_state(), remaining_draws - 1))
                        .ok_or(Pc4BagRevealPageError::MemoInvariantViolation)?
                };
                if residual_rank < suffix_count {
                    selected = Some(transition);
                    break;
                }
                residual_rank -= suffix_count;
            }
            let transition = selected.ok_or(Pc4BagRevealPageError::MemoInvariantViolation)?;
            pieces.push(transition.piece());
            probability = probability
                .multiply(
                    transition.weight().multiplicity(),
                    transition.weight().denominator(),
                )
                .ok_or(Pc4BagRevealPageError::ProbabilityOverflow)?;
            state = transition.next_state();
            remaining_draws -= 1;
        }

        Ok(Pc4BagRevealSequence {
            rank: original_rank,
            pieces,
            probability,
            terminal_state: state,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Pc4BagRevealCursor {
    family_token: Arc<()>,
    next_rank: u128,
    exhausted: bool,
}

impl Pc4BagRevealCursor {
    pub const fn next_rank(&self) -> u128 {
        self.next_rank
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

/// Counts one exact-state reveal family and retains only suffix counts required
/// for lazy lexicographic unranking.
pub fn prepare_pc4_bag_reveal_family<G>(
    source_state: Pc4BagState,
    hidden_draws: usize,
    budgets: Pc4BagRevealBudgets,
    guard: &G,
) -> Result<Pc4BagRevealFamily, Pc4BagRevealPrepareError>
where
    G: Pc4BagRevealGuard,
{
    if hidden_draws > PC4_BAG_REVEAL_ABSOLUTE_DRAW_LIMIT {
        return Err(Pc4BagRevealPrepareError::AbsoluteDrawLimitExceeded {
            limit: PC4_BAG_REVEAL_ABSOLUTE_DRAW_LIMIT,
            requested: hidden_draws,
        });
    }
    if hidden_draws > budgets.hidden_draws() {
        return Err(Pc4BagRevealPrepareError::BudgetExceeded {
            kind: Pc4BagRevealPrepareBudgetKind::HiddenDraws,
            limit: budgets.hidden_draws(),
            attempted: hidden_draws,
        });
    }
    check_prepare_guard(guard)?;

    let mut context = CountContext {
        suffix_counts: HashMap::new(),
        count_steps: 0,
        budgets,
        guard,
    };
    let total_sequences = context.count(source_state, hidden_draws)?;
    check_prepare_guard(guard)?;
    Ok(Pc4BagRevealFamily {
        source_state,
        hidden_draws,
        total_sequences,
        suffix_counts: context.suffix_counts,
        budgets,
        cursor_token: Arc::new(()),
    })
}

struct CountContext<'a, G> {
    suffix_counts: HashMap<(Pc4BagState, usize), u128>,
    count_steps: usize,
    budgets: Pc4BagRevealBudgets,
    guard: &'a G,
}

impl<G> CountContext<'_, G>
where
    G: Pc4BagRevealGuard,
{
    fn count(
        &mut self,
        state: Pc4BagState,
        remaining_draws: usize,
    ) -> Result<u128, Pc4BagRevealPrepareError> {
        check_prepare_guard(self.guard)?;
        consume_prepare_budget(
            &mut self.count_steps,
            self.budgets.count_steps(),
            Pc4BagRevealPrepareBudgetKind::CountSteps,
        )?;
        if remaining_draws == 0 {
            return Ok(1);
        }
        if let Some(&count) = self.suffix_counts.get(&(state, remaining_draws)) {
            return Ok(count);
        }

        let batch = draw_pc4_bag(state).map_err(Pc4BagRevealPrepareError::Draw)?;
        let mut count = 0_u128;
        for transition in batch.transitions().iter().copied() {
            let suffix = self.count(transition.next_state(), remaining_draws - 1)?;
            count = count
                .checked_add(suffix)
                .ok_or(Pc4BagRevealPrepareError::SequenceCountOverflow)?;
        }
        check_prepare_guard(self.guard)?;
        if self.suffix_counts.len() >= self.budgets.memo_entries() {
            return Err(Pc4BagRevealPrepareError::BudgetExceeded {
                kind: Pc4BagRevealPrepareBudgetKind::MemoEntries,
                limit: self.budgets.memo_entries(),
                attempted: self.suffix_counts.len().saturating_add(1),
            });
        }
        self.suffix_counts
            .try_reserve(1)
            .map_err(|_| Pc4BagRevealPrepareError::AllocationFailed)?;
        self.suffix_counts.insert((state, remaining_draws), count);
        Ok(count)
    }
}

fn check_prepare_guard<G>(guard: &G) -> Result<(), Pc4BagRevealPrepareError>
where
    G: Pc4BagRevealGuard,
{
    if guard.is_cancelled() {
        Err(Pc4BagRevealPrepareError::Cancelled)
    } else {
        Ok(())
    }
}

fn check_page_guard<G>(guard: &G) -> Result<(), Pc4BagRevealPageError>
where
    G: Pc4BagRevealGuard,
{
    if guard.is_cancelled() {
        Err(Pc4BagRevealPageError::Cancelled)
    } else {
        Ok(())
    }
}

fn consume_prepare_budget(
    used: &mut usize,
    limit: usize,
    kind: Pc4BagRevealPrepareBudgetKind,
) -> Result<(), Pc4BagRevealPrepareError> {
    let attempted = used
        .checked_add(1)
        .ok_or(Pc4BagRevealPrepareError::BudgetExceeded {
            kind,
            limit,
            attempted: usize::MAX,
        })?;
    if attempted > limit {
        return Err(Pc4BagRevealPrepareError::BudgetExceeded {
            kind,
            limit,
            attempted,
        });
    }
    *used = attempted;
    Ok(())
}

fn consume_page_budget(
    used: &mut usize,
    limit: usize,
    kind: Pc4BagRevealPageBudgetKind,
) -> Result<(), Pc4BagRevealPageError> {
    let attempted = used
        .checked_add(1)
        .ok_or(Pc4BagRevealPageError::BudgetExceeded {
            kind,
            limit,
            attempted: usize::MAX,
        })?;
    if attempted > limit {
        return Err(Pc4BagRevealPageError::BudgetExceeded {
            kind,
            limit,
            attempted,
        });
    }
    *used = attempted;
    Ok(())
}

const fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pc4BagProfile;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct EagerSequence {
        pieces: Vec<Pc4GraphPiece>,
        probability: Pc4ExactProbability,
        terminal_state: Pc4BagState,
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("nonzero test budget")
    }

    fn budgets() -> Pc4BagRevealBudgets {
        Pc4BagRevealBudgets::new(
            nonzero(32),
            nonzero(1_000_000),
            nonzero(100_000),
            nonzero(100_000),
            nonzero(10_000_000),
            nonzero(1_000_000),
        )
    }

    fn eager_sequences(state: Pc4BagState, draw_count: usize) -> Vec<EagerSequence> {
        if draw_count == 0 {
            return vec![EagerSequence {
                pieces: Vec::new(),
                probability: Pc4ExactProbability::ONE,
                terminal_state: state,
            }];
        }

        let batch = draw_pc4_bag(state).expect("small eager draw");
        let mut output = Vec::new();
        for transition in batch.transitions().iter().copied() {
            for suffix in eager_sequences(transition.next_state(), draw_count - 1) {
                let mut pieces = Vec::with_capacity(draw_count);
                pieces.push(transition.piece());
                pieces.extend_from_slice(&suffix.pieces);
                output.push(EagerSequence {
                    pieces,
                    probability: suffix
                        .probability
                        .multiply(
                            transition.weight().multiplicity(),
                            transition.weight().denominator(),
                        )
                        .expect("small exact probability"),
                    terminal_state: suffix.terminal_state,
                });
            }
        }
        output
    }

    fn collect_family(
        state: Pc4BagState,
        draw_count: usize,
        page_size: usize,
    ) -> (Pc4BagRevealFamily, Vec<Pc4BagRevealSequence>) {
        let family = prepare_pc4_bag_reveal_family(state, draw_count, budgets(), &|| false)
            .expect("small family");
        let mut cursor = family.cursor();
        let mut output = Vec::new();
        while !cursor.is_exhausted() {
            output.extend(
                family
                    .next_page(&mut cursor, nonzero(page_size), &|| false)
                    .expect("small page"),
            );
        }
        (family, output)
    }

    fn assert_matches_eager(state: Pc4BagState, draw_count: usize, page_size: usize) {
        let eager = eager_sequences(state, draw_count);
        let (family, lazy) = collect_family(state, draw_count, page_size);
        assert_eq!(family.total_sequences(), eager.len() as u128);
        assert_eq!(lazy.len(), eager.len());
        for (rank, (actual, expected)) in lazy.iter().zip(&eager).enumerate() {
            assert_eq!(actual.rank(), rank as u128);
            assert_eq!(actual.pieces(), expected.pieces);
            assert_eq!(actual.probability(), expected.probability);
            assert_eq!(actual.terminal_state(), expected.terminal_state);
        }
        let total_probability = lazy
            .iter()
            .try_fold(Pc4ExactProbability::zero(), |sum, sequence| {
                sum.checked_add(sequence.probability())
            })
            .expect("small reveal probability sum remains representable");
        assert_eq!(total_probability, Pc4ExactProbability::one());
    }

    #[test]
    fn zero_draws_is_one_empty_sequence() {
        let source =
            Pc4BagState::new(Pc4BagProfile::standard_seven_bag(), [0; 7], 8).expect("valid state");
        let (family, sequences) = collect_family(source, 0, 3);
        assert_eq!(family.total_sequences(), 1);
        assert_eq!(sequences.len(), 1);
        assert_eq!(sequences[0].rank(), 0);
        assert!(sequences[0].pieces().is_empty());
        assert_eq!(sequences[0].probability(), Pc4ExactProbability::ONE);
        assert_eq!(sequences[0].terminal_state(), source);
    }

    #[test]
    fn standard_seven_bag_matches_small_eager_reference_exhaustively() {
        let profile = Pc4BagProfile::standard_seven_bag();
        for remainder_mask in 0_u8..=0x7f {
            let mut remainder = [0_u32; 7];
            for (index, count) in remainder.iter_mut().enumerate() {
                *count = u32::from((remainder_mask >> index) & 1);
            }
            let source = Pc4BagState::new(profile, remainder, 5).expect("valid remainder");
            for draw_count in 0..=3 {
                assert_matches_eager(source, draw_count, 5);
            }
        }
    }

    #[test]
    fn repeated_multiplicity_profiles_match_small_eager_reference_exhaustively() {
        let profile = Pc4BagProfile::new([2, 1, 2, 0, 0, 0, 0]).expect("profile");
        for i_count in 0..=2 {
            for o_count in 0..=1 {
                for t_count in 0..=2 {
                    let source =
                        Pc4BagState::new(profile, [i_count, o_count, t_count, 0, 0, 0, 0], 11)
                            .expect("valid repeated remainder");
                    for draw_count in 0..=5 {
                        assert_matches_eager(source, draw_count, 4);
                    }
                }
            }
        }
    }

    #[test]
    fn repeated_multiplicity_probability_is_returned_in_reduced_form() {
        let profile = Pc4BagProfile::new([2, 1, 1, 0, 0, 0, 0]).expect("profile");
        let source = Pc4BagState::new(profile, [2, 1, 1, 0, 0, 0, 0], 2).expect("valid remainder");
        let (_, sequences) = collect_family(source, 1, 7);

        assert_eq!(sequences[0].pieces(), [Pc4GraphPiece::I]);
        assert_eq!(
            sequences[0].probability(),
            Pc4ExactProbability {
                numerator: 1,
                denominator: 2,
            }
        );
        assert_eq!(sequences[1].probability().denominator(), 4);
        assert_eq!(sequences[2].probability().denominator(), 4);
    }

    #[test]
    fn checked_probability_addition_is_reduced_exact_and_overflow_checked() {
        let one_sixth = Pc4ExactProbability {
            numerator: 1,
            denominator: 6,
        };
        let one_third = Pc4ExactProbability {
            numerator: 1,
            denominator: 3,
        };

        assert_eq!(
            Pc4ExactProbability::zero().checked_add(one_sixth),
            Some(one_sixth)
        );
        assert_eq!(
            one_sixth.checked_add(one_third),
            Some(Pc4ExactProbability {
                numerator: 1,
                denominator: 2,
            })
        );
        assert_eq!(
            Pc4ExactProbability {
                numerator: u128::MAX,
                denominator: 1,
            }
            .checked_add(Pc4ExactProbability::one()),
            None
        );
    }

    #[test]
    fn page_is_lexicographic_and_cursor_is_family_bound() {
        let profile = Pc4BagProfile::new([2, 1, 1, 0, 0, 0, 0]).expect("profile");
        let source = Pc4BagState::new(profile, [2, 1, 1, 0, 0, 0, 0], 0).expect("valid state");
        let first =
            prepare_pc4_bag_reveal_family(source, 3, budgets(), &|| false).expect("first family");
        let second =
            prepare_pc4_bag_reveal_family(source, 3, budgets(), &|| false).expect("second family");
        let mut foreign_cursor = first.cursor();
        assert_eq!(
            second.next_page(&mut foreign_cursor, nonzero(1), &|| false),
            Err(Pc4BagRevealPageError::CursorMismatch)
        );
        assert_eq!(foreign_cursor.next_rank(), 0);

        let mut cursor = first.cursor();
        let page = first
            .next_page(&mut cursor, nonzero(100), &|| false)
            .expect("page");
        assert!(page
            .windows(2)
            .all(|pair| pair[0].pieces() < pair[1].pieces()));
        assert_eq!(cursor.next_rank(), first.total_sequences());
        assert!(cursor.is_exhausted());
    }

    #[test]
    fn prepare_budgets_cancellation_and_epoch_overflow_fail_closed() {
        let profile = Pc4BagProfile::standard_seven_bag();
        let source = Pc4BagState::new(profile, [0; 7], 0).expect("valid state");
        assert!(matches!(
            prepare_pc4_bag_reveal_family(source, 1, budgets(), &|| true),
            Err(Pc4BagRevealPrepareError::Cancelled)
        ));

        let tiny_count = Pc4BagRevealBudgets::new(
            nonzero(4),
            nonzero(1),
            nonzero(100),
            nonzero(10),
            nonzero(100),
            nonzero(100),
        );
        assert!(matches!(
            prepare_pc4_bag_reveal_family(source, 2, tiny_count, &|| false),
            Err(Pc4BagRevealPrepareError::BudgetExceeded {
                kind: Pc4BagRevealPrepareBudgetKind::CountSteps,
                ..
            })
        ));

        let tiny_memo = Pc4BagRevealBudgets::new(
            nonzero(4),
            nonzero(100),
            nonzero(1),
            nonzero(10),
            nonzero(100),
            nonzero(100),
        );
        assert!(matches!(
            prepare_pc4_bag_reveal_family(source, 2, tiny_memo, &|| false),
            Err(Pc4BagRevealPrepareError::BudgetExceeded {
                kind: Pc4BagRevealPrepareBudgetKind::MemoEntries,
                ..
            })
        ));

        let empty_at_max_epoch =
            Pc4BagState::new(profile, [0; 7], u64::MAX).expect("valid max epoch state");
        assert!(matches!(
            prepare_pc4_bag_reveal_family(empty_at_max_epoch, 1, budgets(), &|| false),
            Err(Pc4BagRevealPrepareError::Draw(
                Pc4BagDrawError::EpochOverflow { epoch: u64::MAX }
            ))
        ));
    }

    #[test]
    fn page_failures_do_not_advance_cursor() {
        let source =
            Pc4BagState::new(Pc4BagProfile::standard_seven_bag(), [0; 7], 0).expect("valid state");
        let constrained = Pc4BagRevealBudgets::new(
            nonzero(4),
            nonzero(10_000),
            nonzero(10_000),
            nonzero(2),
            nonzero(1),
            nonzero(2),
        );
        let family = prepare_pc4_bag_reveal_family(source, 2, constrained, &|| false)
            .expect("prepared family");
        let mut cursor = family.cursor();

        assert!(matches!(
            family.next_page(&mut cursor, nonzero(3), &|| false),
            Err(Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::PageSequences,
                ..
            })
        ));
        assert_eq!(cursor.next_rank(), 0);
        assert!(matches!(
            family.next_page(&mut cursor, nonzero(2), &|| false),
            Err(Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::AllocatedPieces,
                ..
            })
        ));
        assert_eq!(cursor.next_rank(), 0);
        assert!(matches!(
            family.next_page(&mut cursor, nonzero(1), &|| false),
            Err(Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::RankSteps,
                ..
            })
        ));
        assert_eq!(cursor.next_rank(), 0);
        assert_eq!(
            family.next_page(&mut cursor, nonzero(1), &|| true),
            Err(Pc4BagRevealPageError::Cancelled)
        );
        assert_eq!(cursor.next_rank(), 0);
    }

    #[test]
    fn cancellation_after_partial_unranking_does_not_advance_cursor() {
        let source =
            Pc4BagState::new(Pc4BagProfile::standard_seven_bag(), [0; 7], 0).expect("valid state");
        let family = prepare_pc4_bag_reveal_family(source, 2, budgets(), &|| false)
            .expect("prepared family");
        let mut cursor = family.cursor();
        let checks = std::cell::Cell::new(0_usize);
        let cancel_during_first_sequence = || {
            let next = checks.get() + 1;
            checks.set(next);
            next >= 4
        };

        assert_eq!(
            family.next_page(&mut cursor, nonzero(2), &cancel_during_first_sequence,),
            Err(Pc4BagRevealPageError::Cancelled)
        );
        assert_eq!(cursor.next_rank(), 0);
        assert!(!cursor.is_exhausted());
    }

    #[test]
    fn sequence_count_overflow_is_typed() {
        let source =
            Pc4BagState::new(Pc4BagProfile::standard_seven_bag(), [0; 7], 0).expect("valid state");
        let large = Pc4BagRevealBudgets::new(
            nonzero(100),
            nonzero(10_000_000),
            nonzero(1_000_000),
            nonzero(1),
            nonzero(1_000),
            nonzero(100),
        );
        assert!(matches!(
            prepare_pc4_bag_reveal_family(source, 77, large, &|| false),
            Err(Pc4BagRevealPrepareError::SequenceCountOverflow)
        ));
    }

    #[test]
    fn budget_counters_fail_closed_at_usize_max() {
        let mut prepare_used = usize::MAX;
        assert!(matches!(
            consume_prepare_budget(
                &mut prepare_used,
                usize::MAX,
                Pc4BagRevealPrepareBudgetKind::CountSteps,
            ),
            Err(Pc4BagRevealPrepareError::BudgetExceeded {
                attempted: usize::MAX,
                ..
            })
        ));
        assert_eq!(prepare_used, usize::MAX);

        let mut page_used = usize::MAX;
        assert!(matches!(
            consume_page_budget(
                &mut page_used,
                usize::MAX,
                Pc4BagRevealPageBudgetKind::RankSteps,
            ),
            Err(Pc4BagRevealPageError::BudgetExceeded {
                attempted: usize::MAX,
                ..
            })
        ));
        assert_eq!(page_used, usize::MAX);
    }
}
