// SRP rationale: observation consumers page either a real hidden bag family
// or the single deterministic no-draw outcome. A fixed queue never invents
// bag state, performs a draw, or duplicates probability across hold choices.
use core::num::NonZeroUsize;
use std::sync::Arc;

use crate::{
    Pc4BagRevealCursor, Pc4BagRevealFamily, Pc4BagRevealGuard, Pc4BagRevealPageBudgetKind,
    Pc4BagRevealPageError, Pc4BagRevealSequence, Pc4BagState, Pc4ExactProbability, Pc4GraphPiece,
};

#[derive(Clone, Debug)]
pub(crate) enum ObservationRevealFamily {
    Fixed { token: Arc<()>, page_limit: usize },
    Bag(Pc4BagRevealFamily),
}

#[derive(Clone, Debug)]
pub(crate) enum ObservationRevealCursor {
    Fixed { token: Arc<()>, consumed: bool },
    Bag(Pc4BagRevealCursor),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ObservationRevealSequence {
    Fixed,
    Bag(Pc4BagRevealSequence),
}

impl ObservationRevealFamily {
    pub(crate) fn fixed(page_limit: NonZeroUsize) -> Self {
        Self::Fixed {
            token: Arc::new(()),
            page_limit: page_limit.get(),
        }
    }

    pub(crate) const fn total_sequences(&self) -> u128 {
        match self {
            Self::Fixed { .. } => 1,
            Self::Bag(family) => family.total_sequences(),
        }
    }

    pub(crate) fn cursor(&self) -> ObservationRevealCursor {
        match self {
            Self::Fixed { token, .. } => ObservationRevealCursor::Fixed {
                token: Arc::clone(token),
                consumed: false,
            },
            Self::Bag(family) => ObservationRevealCursor::Bag(family.cursor()),
        }
    }

    pub(crate) fn next_page<G: Pc4BagRevealGuard>(
        &self,
        cursor: &mut ObservationRevealCursor,
        limit: NonZeroUsize,
        guard: &G,
    ) -> Result<Vec<ObservationRevealSequence>, Pc4BagRevealPageError> {
        if guard.is_cancelled() {
            return Err(Pc4BagRevealPageError::Cancelled);
        }
        let mut transaction = cursor.clone();
        let mut result = Vec::new();
        match (self, &mut transaction) {
            (
                Self::Fixed { token, page_limit },
                ObservationRevealCursor::Fixed {
                    token: cursor_token,
                    consumed,
                },
            ) => {
                if !Arc::ptr_eq(token, cursor_token) {
                    return Err(Pc4BagRevealPageError::CursorMismatch);
                }
                if limit.get() > *page_limit {
                    return Err(Pc4BagRevealPageError::BudgetExceeded {
                        kind: Pc4BagRevealPageBudgetKind::PageSequences,
                        limit: *page_limit,
                        attempted: limit.get(),
                    });
                }
                if !*consumed {
                    result
                        .try_reserve_exact(1)
                        .map_err(|_| Pc4BagRevealPageError::AllocationFailed)?;
                    result.push(ObservationRevealSequence::Fixed);
                    *consumed = true;
                }
            }
            (Self::Bag(family), ObservationRevealCursor::Bag(cursor)) => {
                let page = family.next_page(cursor, limit, guard)?;
                result
                    .try_reserve_exact(page.len())
                    .map_err(|_| Pc4BagRevealPageError::AllocationFailed)?;
                result.extend(page.into_iter().map(ObservationRevealSequence::Bag));
            }
            _ => return Err(Pc4BagRevealPageError::CursorMismatch),
        }
        if guard.is_cancelled() {
            return Err(Pc4BagRevealPageError::Cancelled);
        }
        *cursor = transaction;
        Ok(result)
    }
}

impl ObservationRevealCursor {
    pub(crate) const fn next_rank(&self) -> u128 {
        match self {
            Self::Fixed { consumed, .. } => *consumed as u128,
            Self::Bag(cursor) => cursor.next_rank(),
        }
    }

    pub(crate) const fn is_exhausted(&self) -> bool {
        match self {
            Self::Fixed { consumed, .. } => *consumed,
            Self::Bag(cursor) => cursor.is_exhausted(),
        }
    }
}

impl ObservationRevealSequence {
    pub(crate) const fn rank(&self) -> u128 {
        match self {
            Self::Fixed => 0,
            Self::Bag(sequence) => sequence.rank(),
        }
    }

    pub(crate) fn pieces(&self) -> &[Pc4GraphPiece] {
        match self {
            Self::Fixed => &[],
            Self::Bag(sequence) => sequence.pieces(),
        }
    }

    pub(crate) const fn probability(&self) -> Pc4ExactProbability {
        match self {
            Self::Fixed => Pc4ExactProbability::one(),
            Self::Bag(sequence) => sequence.probability(),
        }
    }

    pub(crate) const fn terminal_state(&self) -> Option<Pc4BagState> {
        match self {
            Self::Fixed => None,
            Self::Bag(sequence) => Some(sequence.terminal_state()),
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        u128,
        Vec<Pc4GraphPiece>,
        Pc4ExactProbability,
        Option<Pc4BagState>,
    ) {
        match self {
            Self::Fixed => (0, Vec::new(), Pc4ExactProbability::one(), None),
            Self::Bag(sequence) => {
                let (rank, pieces, probability, state) = sequence.into_parts();
                (rank, pieces, probability, Some(state))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use super::*;
    use crate::{prepare_pc4_bag_reveal_family, Pc4BagProfile, Pc4BagRevealBudgets};

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("positive test budget")
    }

    #[test]
    fn fixed_outcome_is_probability_one_without_a_bag_or_draw() {
        let family = ObservationRevealFamily::fixed(nonzero(4));
        let mut cursor = family.cursor();
        assert_eq!(family.total_sequences(), 1);
        assert_eq!(cursor.next_rank(), 0);
        assert!(!cursor.is_exhausted());
        let page = family
            .next_page(&mut cursor, nonzero(4), &|| false)
            .expect("fixed page");
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].rank(), 0);
        assert!(page[0].pieces().is_empty());
        assert_eq!(page[0].probability(), Pc4ExactProbability::one());
        assert_eq!(page[0].terminal_state(), None);
        assert_eq!(
            page.into_iter().next().expect("one outcome").into_parts(),
            (0, vec![], Pc4ExactProbability::one(), None)
        );
        assert_eq!(cursor.next_rank(), 1);
        assert!(cursor.is_exhausted());
        assert!(family
            .next_page(&mut cursor, nonzero(1), &|| false)
            .expect("exhausted")
            .is_empty());
    }

    #[test]
    fn fixed_pages_reject_foreign_cursors_and_roll_back_budget_or_late_cancellation() {
        let family = ObservationRevealFamily::fixed(nonzero(1));
        let other = ObservationRevealFamily::fixed(nonzero(1));
        let mut cursor = family.cursor();
        assert_eq!(
            other.next_page(&mut cursor, nonzero(1), &|| false),
            Err(Pc4BagRevealPageError::CursorMismatch)
        );
        assert!(matches!(
            family.next_page(&mut cursor, nonzero(2), &|| false),
            Err(Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::PageSequences,
                limit: 1,
                attempted: 2,
            })
        ));
        let calls = Cell::new(0);
        assert_eq!(
            family.next_page(&mut cursor, nonzero(1), &|| {
                calls.set(calls.get() + 1);
                calls.get() == 2
            }),
            Err(Pc4BagRevealPageError::Cancelled)
        );
        assert_eq!(cursor.next_rank(), 0);
        assert!(!cursor.is_exhausted());
        assert_eq!(
            family
                .next_page(&mut cursor, nonzero(1), &|| false)
                .expect("retry")
                .len(),
            1
        );
    }

    #[test]
    fn bag_pages_keep_real_draws_and_cannot_consume_a_fixed_cursor() {
        let profile = Pc4BagProfile::new([1, 1, 0, 0, 0, 0, 0]).expect("two-piece bag");
        let state = Pc4BagState::new(profile, [1, 1, 0, 0, 0, 0, 0], 3).expect("bag state");
        let budget = Pc4BagRevealBudgets::new(
            nonzero(1),
            nonzero(32),
            nonzero(32),
            nonzero(1),
            nonzero(32),
            nonzero(32),
        );
        let family = ObservationRevealFamily::Bag(
            prepare_pc4_bag_reveal_family(state, 1, budget, &|| false).expect("bag family"),
        );
        let fixed = ObservationRevealFamily::fixed(nonzero(1));
        assert_eq!(
            family.next_page(&mut fixed.cursor(), nonzero(1), &|| false),
            Err(Pc4BagRevealPageError::CursorMismatch)
        );
        let mut cursor = family.cursor();
        assert_eq!(
            fixed.next_page(&mut cursor, nonzero(1), &|| false),
            Err(Pc4BagRevealPageError::CursorMismatch)
        );
        assert_eq!(family.total_sequences(), 2);
        let mut probability = Pc4ExactProbability::zero();
        for (rank, piece) in [Pc4GraphPiece::I, Pc4GraphPiece::O].into_iter().enumerate() {
            let page = family
                .next_page(&mut cursor, nonzero(1), &|| false)
                .expect("bag page");
            assert_eq!(page.len(), 1);
            assert_eq!(page[0].rank(), rank as u128);
            assert_eq!(page[0].pieces(), &[piece]);
            assert!(page[0].terminal_state().is_some());
            probability = probability
                .checked_add(page[0].probability())
                .expect("exact sum");
        }
        assert_eq!(probability, Pc4ExactProbability::one());
        assert!(cursor.is_exhausted());
    }
}
