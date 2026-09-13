// SRP rationale: lazily page an already-defined finite uniform queue family.
// Syntax, compiler ownership, graph completeness and I/O remain outside this
// primitive. A nominal family token prevents a same-size source substitution.
use core::{fmt, num::NonZeroUsize};
use std::sync::Arc;

use crate::{
    Pc4BagRevealGuard, Pc4BagRevealPageBudgetKind, Pc4BagRevealPageError, Pc4ExactProbability,
    Pc4GraphPiece,
};

pub trait Pc4FiniteQueueReader: fmt::Debug + Send + Sync {
    fn read_queue(&self, ordinal: usize) -> Result<Vec<Pc4GraphPiece>, Pc4FiniteQueueReadError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4FiniteQueueReadError {
    QueueUnavailable,
    QueueLengthMismatch,
    AllocationFailed,
}

impl Pc4FiniteQueueReadError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::QueueUnavailable => "pc4_finite_queue_unavailable",
            Self::QueueLengthMismatch => "pc4_finite_queue_length_mismatch",
            Self::AllocationFailed => "pc4_finite_queue_allocation_failed",
        }
    }
}

#[derive(Debug)]
struct FamilyData {
    reader: Arc<dyn Pc4FiniteQueueReader>,
    count: NonZeroUsize,
    sequence_pieces: NonZeroUsize,
}

/// Uniform means one equal-weight outcome per original ordinal, not per
/// distinct projected queue or controllable hold branch. The App must bind
/// this exact reader to the audited compiler-owned source before granting
/// candidate authority. Construction alone does not prove completeness.
#[derive(Clone, Debug)]
pub struct Pc4FiniteQueueFamily(Arc<FamilyData>);

impl PartialEq for Pc4FiniteQueueFamily {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Pc4FiniteQueueFamily {}

impl Pc4FiniteQueueFamily {
    pub fn uniform(
        reader: Arc<dyn Pc4FiniteQueueReader>,
        count: NonZeroUsize,
        sequence_pieces: NonZeroUsize,
    ) -> Self {
        Self(Arc::new(FamilyData {
            reader,
            count,
            sequence_pieces,
        }))
    }
    pub fn count(&self) -> usize {
        self.0.count.get()
    }
    pub fn sequence_pieces(&self) -> usize {
        self.0.sequence_pieces.get()
    }
    pub(crate) fn cursor(&self) -> FiniteQueueCursor {
        FiniteQueueCursor {
            family: self.clone(),
            next_ordinal: 0,
        }
    }
    pub(crate) fn next_page<G: Pc4BagRevealGuard>(
        &self,
        cursor: &mut FiniteQueueCursor,
        limit: NonZeroUsize,
        page_limit: usize,
        allocated_pieces_limit: usize,
        guard: &G,
    ) -> Result<Vec<FiniteQueueOutcome>, Pc4BagRevealPageError> {
        if self != &cursor.family {
            return Err(Pc4BagRevealPageError::CursorMismatch);
        }
        if guard.is_cancelled() {
            return Err(Pc4BagRevealPageError::Cancelled);
        }
        if limit.get() > page_limit {
            return Err(Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::PageSequences,
                limit: page_limit,
                attempted: limit.get(),
            });
        }
        let count = limit.get().min(self.count() - cursor.next_ordinal);
        let allocated = count.checked_mul(self.sequence_pieces()).ok_or(
            Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::AllocatedPieces,
                limit: allocated_pieces_limit,
                attempted: usize::MAX,
            },
        )?;
        if allocated > allocated_pieces_limit {
            return Err(Pc4BagRevealPageError::BudgetExceeded {
                kind: Pc4BagRevealPageBudgetKind::AllocatedPieces,
                limit: allocated_pieces_limit,
                attempted: allocated,
            });
        }
        let mut result = Vec::new();
        result
            .try_reserve_exact(count)
            .map_err(|_| Pc4BagRevealPageError::AllocationFailed)?;
        for ordinal in cursor.next_ordinal..cursor.next_ordinal + count {
            if guard.is_cancelled() {
                return Err(Pc4BagRevealPageError::Cancelled);
            }
            let pieces = self
                .0
                .reader
                .read_queue(ordinal)
                .map_err(Pc4BagRevealPageError::FiniteQueue)?;
            if pieces.len() != self.sequence_pieces() {
                return Err(Pc4BagRevealPageError::FiniteQueue(
                    Pc4FiniteQueueReadError::QueueLengthMismatch,
                ));
            }
            result.push(FiniteQueueOutcome {
                rank: ordinal as u128,
                pieces,
                probability: Pc4ExactProbability::uniform(self.0.count),
            });
        }
        if guard.is_cancelled() {
            return Err(Pc4BagRevealPageError::Cancelled);
        }
        cursor.next_ordinal += count;
        Ok(result)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FiniteQueueCursor {
    family: Pc4FiniteQueueFamily,
    next_ordinal: usize,
}
impl FiniteQueueCursor {
    pub(crate) const fn next_rank(&self) -> u128 {
        self.next_ordinal as u128
    }
    pub(crate) fn is_exhausted(&self) -> bool {
        self.next_ordinal == self.family.count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FiniteQueueOutcome {
    pub(crate) rank: u128,
    pub(crate) pieces: Vec<Pc4GraphPiece>,
    pub(crate) probability: Pc4ExactProbability,
}

#[cfg(test)]
#[path = "finite_queue_family_tests.rs"]
mod tests;
