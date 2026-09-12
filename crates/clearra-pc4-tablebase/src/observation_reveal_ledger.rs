// SRP rationale: this module has one behavior-level change reason: retain one
// exact probability row for every canonical reveal in an observation graph,
// independently of controllable hold branches and graph success.
use core::{fmt, num::NonZeroUsize};
use std::sync::Arc;

use crate::{
    FixedQueueTraversalGuard, Pc4BagRevealCursor, Pc4BagRevealFamily, Pc4BagRevealGuard,
    Pc4BagRevealPageError, Pc4BagState, Pc4ExactProbability, Pc4GraphPiece,
    Pc4ObservationQueueScope, QualifiedPc4TargetIdentity,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ObservationRevealOutcome {
    rank: u128,
    pieces: Vec<Pc4GraphPiece>,
    probability: Pc4ExactProbability,
    terminal_bag_state: Pc4BagState,
}

impl Pc4ObservationRevealOutcome {
    pub const fn rank(&self) -> u128 {
        self.rank
    }

    pub fn pieces(&self) -> &[Pc4GraphPiece] {
        &self.pieces
    }

    pub const fn probability(&self) -> Pc4ExactProbability {
        self.probability
    }

    pub const fn terminal_bag_state(&self) -> Pc4BagState {
        self.terminal_bag_state
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4ObservationRevealLedgerPageError {
    Cancelled,
    StaleSnapshot,
    CursorMismatch,
    PageLimitExceeded { limit: usize, attempted: usize },
    NonCanonicalRevealRank { expected: u128, actual: u128 },
    OutcomeCountMismatch { expected: u128, actual: u128 },
    ProbabilityOverflow,
    ProbabilityNotNormalized { actual: Pc4ExactProbability },
    Reveal(Pc4BagRevealPageError),
}

impl Pc4ObservationRevealLedgerPageError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_observation_reveal_ledger_cancelled",
            Self::StaleSnapshot => "pc4_observation_reveal_ledger_stale_snapshot",
            Self::CursorMismatch => "pc4_observation_reveal_ledger_cursor_mismatch",
            Self::PageLimitExceeded { .. } => "pc4_observation_reveal_ledger_page_limit_exceeded",
            Self::NonCanonicalRevealRank { .. } => {
                "pc4_observation_reveal_ledger_noncanonical_reveal_rank"
            }
            Self::OutcomeCountMismatch { .. } => {
                "pc4_observation_reveal_ledger_outcome_count_mismatch"
            }
            Self::ProbabilityOverflow => "pc4_observation_reveal_ledger_probability_overflow",
            Self::ProbabilityNotNormalized { .. } => {
                "pc4_observation_reveal_ledger_probability_not_normalized"
            }
            Self::Reveal(error) => error.reason(),
        }
    }
}

impl fmt::Display for Pc4ObservationRevealLedgerPageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4ObservationRevealLedgerPageError {}

#[derive(Clone, Debug)]
pub struct Pc4ObservationRevealLedgerCursor {
    family_token: Arc<()>,
    reveal_cursor: Pc4BagRevealCursor,
    emitted_outcomes: u128,
    accumulated_probability: Pc4ExactProbability,
    exhausted: bool,
}

impl Pc4ObservationRevealLedgerCursor {
    pub const fn emitted_outcomes(&self) -> u128 {
        self.emitted_outcomes
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ObservationRevealLedgerPage {
    outcomes: Vec<Pc4ObservationRevealOutcome>,
    complete_probability: Option<Pc4ExactProbability>,
    exhausted: bool,
}

impl Pc4ObservationRevealLedgerPage {
    pub fn outcomes(&self) -> &[Pc4ObservationRevealOutcome] {
        &self.outcomes
    }

    /// Exact total mass is intentionally unavailable until the ledger cursor
    /// has exhausted every canonical reveal rank.
    pub const fn complete_probability(&self) -> Option<Pc4ExactProbability> {
        self.complete_probability
    }

    pub const fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

/// Graph-derived, provider-neutral reveal ledger. It enumerates the graph's
/// own bag family directly; hold siblings therefore cannot duplicate random
/// mass and a reveal remains present even if every sibling has zero paths.
#[derive(Clone, Debug)]
pub struct Pc4ObservationRevealLedgerFamily {
    target: Arc<QualifiedPc4TargetIdentity>,
    source_field_id: u32,
    queue_scope: Arc<Pc4ObservationQueueScope>,
    reveal_family: Pc4BagRevealFamily,
    page_limit: usize,
    cursor_token: Arc<()>,
}

impl Pc4ObservationRevealLedgerFamily {
    pub(crate) fn from_graph_parts(
        target: Arc<QualifiedPc4TargetIdentity>,
        source_field_id: u32,
        queue_scope: Arc<Pc4ObservationQueueScope>,
        reveal_family: Pc4BagRevealFamily,
        page_limit: usize,
    ) -> Self {
        Self {
            target,
            source_field_id,
            queue_scope,
            reveal_family,
            page_limit,
            cursor_token: Arc::new(()),
        }
    }

    pub fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn source_field_id(&self) -> u32 {
        self.source_field_id
    }

    pub fn queue_scope(&self) -> &Pc4ObservationQueueScope {
        &self.queue_scope
    }

    pub const fn total_outcomes(&self) -> u128 {
        self.reveal_family.total_sequences()
    }

    pub const fn page_limit(&self) -> usize {
        self.page_limit
    }

    pub fn cursor(&self) -> Pc4ObservationRevealLedgerCursor {
        Pc4ObservationRevealLedgerCursor {
            family_token: Arc::clone(&self.cursor_token),
            reveal_cursor: self.reveal_family.cursor(),
            emitted_outcomes: 0,
            accumulated_probability: Pc4ExactProbability::zero(),
            exhausted: false,
        }
    }

    pub fn next_page<G>(
        &self,
        cursor: &mut Pc4ObservationRevealLedgerCursor,
        limit: NonZeroUsize,
        guard: &G,
    ) -> Result<Pc4ObservationRevealLedgerPage, Pc4ObservationRevealLedgerPageError>
    where
        G: FixedQueueTraversalGuard,
    {
        if !Arc::ptr_eq(&cursor.family_token, &self.cursor_token) {
            return Err(Pc4ObservationRevealLedgerPageError::CursorMismatch);
        }
        if limit.get() > self.page_limit {
            return Err(Pc4ObservationRevealLedgerPageError::PageLimitExceeded {
                limit: self.page_limit,
                attempted: limit.get(),
            });
        }
        check_guard(self.target.snapshot(), guard)?;
        if cursor.exhausted {
            return Ok(Pc4ObservationRevealLedgerPage {
                outcomes: Vec::new(),
                complete_probability: Some(cursor.accumulated_probability),
                exhausted: true,
            });
        }

        let mut transaction = cursor.clone();
        let sequences = self
            .reveal_family
            .next_page(&mut transaction.reveal_cursor, limit, &GuardAdapter(guard))
            .map_err(map_reveal_error)?;
        let mut outcomes = Vec::new();
        outcomes.try_reserve_exact(sequences.len()).map_err(|_| {
            Pc4ObservationRevealLedgerPageError::Reveal(Pc4BagRevealPageError::AllocationFailed)
        })?;
        for sequence in sequences {
            let (rank, pieces, probability, terminal_bag_state) = sequence.into_parts();
            if rank != transaction.emitted_outcomes {
                return Err(
                    Pc4ObservationRevealLedgerPageError::NonCanonicalRevealRank {
                        expected: transaction.emitted_outcomes,
                        actual: rank,
                    },
                );
            }
            transaction.accumulated_probability = transaction
                .accumulated_probability
                .checked_add(probability)
                .ok_or(Pc4ObservationRevealLedgerPageError::ProbabilityOverflow)?;
            transaction.emitted_outcomes = transaction
                .emitted_outcomes
                .checked_add(1)
                .ok_or(Pc4ObservationRevealLedgerPageError::ProbabilityOverflow)?;
            outcomes.push(Pc4ObservationRevealOutcome {
                rank,
                pieces,
                probability,
                terminal_bag_state,
            });
        }
        transaction.exhausted = transaction.reveal_cursor.is_exhausted();
        if transaction.exhausted && transaction.emitted_outcomes != self.total_outcomes() {
            return Err(Pc4ObservationRevealLedgerPageError::OutcomeCountMismatch {
                expected: self.total_outcomes(),
                actual: transaction.emitted_outcomes,
            });
        }
        if transaction.exhausted
            && transaction.accumulated_probability != Pc4ExactProbability::one()
        {
            return Err(
                Pc4ObservationRevealLedgerPageError::ProbabilityNotNormalized {
                    actual: transaction.accumulated_probability,
                },
            );
        }
        check_guard(self.target.snapshot(), guard)?;
        let complete_probability = transaction
            .exhausted
            .then_some(transaction.accumulated_probability);
        let exhausted = transaction.exhausted;
        *cursor = transaction;
        Ok(Pc4ObservationRevealLedgerPage {
            outcomes,
            complete_probability,
            exhausted,
        })
    }
}

struct GuardAdapter<'a, G>(&'a G);

impl<G> Pc4BagRevealGuard for GuardAdapter<'_, G>
where
    G: FixedQueueTraversalGuard,
{
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

fn check_guard<G>(
    snapshot: &crate::QualifiedSnapshotIdentity,
    guard: &G,
) -> Result<(), Pc4ObservationRevealLedgerPageError>
where
    G: FixedQueueTraversalGuard,
{
    if guard.is_cancelled() {
        Err(Pc4ObservationRevealLedgerPageError::Cancelled)
    } else if !guard.is_current_snapshot(snapshot) {
        Err(Pc4ObservationRevealLedgerPageError::StaleSnapshot)
    } else {
        Ok(())
    }
}

fn map_reveal_error(error: Pc4BagRevealPageError) -> Pc4ObservationRevealLedgerPageError {
    match error {
        Pc4BagRevealPageError::Cancelled => Pc4ObservationRevealLedgerPageError::Cancelled,
        error => Pc4ObservationRevealLedgerPageError::Reveal(error),
    }
}
