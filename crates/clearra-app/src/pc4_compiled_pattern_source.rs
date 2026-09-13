// SRP rationale: bind a finite, compiled Core pattern source to its actual
// ordered queues and probability weights without expanding a second universe.
// This grants input identity only, never graph completeness or online authority.
use core::{fmt, num::NonZeroUsize};
use std::{borrow::Cow, sync::Arc};

use clearra_core_domain::{
    piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
};
use clearra_pc4_tablebase::Pc4GraphPiece;
use clearra_pc_graph::request::PcQueueInput;
use clearra_problem::{SearchProblem, SearchProblemKind};
use clearra_supply::pattern_universe::MaterializedPatternUniverse;
use sha2::{Digest, Sha256};

pub const PC4_COMPILED_PATTERN_SOURCE_CONTRACT: &str = "pc4-compiled-pattern-source.v1";
const IDENTITY_DOMAIN: &[u8] = b"clearra.pc4-compiled-pattern-source.v1\0";

/// Preparation work limits, not Core execution or retained-memory authority.
/// The caller already owns the immutable problem; only an Arc and at most one
/// lazily unranked queue are retained here. Each advance is cooperatively bounded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4CompiledPatternLimits {
    patterns: NonZeroUsize,
    sequence_pieces: NonZeroUsize,
    patterns_per_advance: NonZeroUsize,
}

impl Pc4CompiledPatternLimits {
    pub const fn new(
        patterns: NonZeroUsize,
        sequence_pieces: NonZeroUsize,
        patterns_per_advance: NonZeroUsize,
    ) -> Self {
        Self {
            patterns,
            sequence_pieces,
            patterns_per_advance,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4CompiledPatternError {
    Cancelled,
    PreparationTerminated,
    PreparationIncomplete,
    UnsupportedProblem,
    UnsupportedQueueSource,
    UnsupportedBoard,
    MissingUniverse,
    IncompleteUniverse,
    InconsistentUniverse,
    PatternLimit { limit: usize, attempted: usize },
    SequencePieceLimit { limit: usize, attempted: usize },
    AdvanceLimit { limit: usize, attempted: usize },
    PatternIndexOutOfBounds,
    AllocationFailed,
    CanonicalLengthOverflow,
}

impl Pc4CompiledPatternError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_compiled_pattern_cancelled",
            Self::PreparationTerminated => "pc4_compiled_pattern_preparation_terminated",
            Self::PreparationIncomplete => "pc4_compiled_pattern_preparation_incomplete",
            Self::UnsupportedProblem => "pc4_compiled_pattern_problem_unsupported",
            Self::UnsupportedQueueSource => "pc4_compiled_pattern_queue_source_unsupported",
            Self::UnsupportedBoard => "pc4_compiled_pattern_board_unsupported",
            Self::MissingUniverse => "pc4_compiled_pattern_universe_missing",
            Self::IncompleteUniverse => "pc4_compiled_pattern_universe_incomplete",
            Self::InconsistentUniverse => "pc4_compiled_pattern_universe_inconsistent",
            Self::PatternLimit { .. } => "pc4_compiled_pattern_count_limit",
            Self::SequencePieceLimit { .. } => "pc4_compiled_pattern_sequence_piece_limit",
            Self::AdvanceLimit { .. } => "pc4_compiled_pattern_advance_limit",
            Self::PatternIndexOutOfBounds => "pc4_compiled_pattern_index_out_of_bounds",
            Self::AllocationFailed => "pc4_compiled_pattern_allocation_failed",
            Self::CanonicalLengthOverflow => "pc4_compiled_pattern_canonical_length_overflow",
        }
    }
}

impl fmt::Display for Pc4CompiledPatternError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}
impl std::error::Error for Pc4CompiledPatternError {}

/// Content identity is intentionally distinct from SearchProblemId,
/// PieceSourceId and PatternUniverseId. Those IDs are not content proofs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Pc4CompiledPatternIdentity([u8; 32]);

impl Pc4CompiledPatternIdentity {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A queue's original ordinal and weight are preserved even when prefix
/// projection makes several queues equal. Controllable hold choices must not
/// multiply this weight; a zero-hit ordinal must remain in the denominator.
#[derive(Clone, Debug, PartialEq)]
pub struct Pc4CompiledPatternQueue {
    pattern_index: usize,
    pieces: Vec<Pc4GraphPiece>,
    weight: ProbabilityValue,
}

impl Pc4CompiledPatternQueue {
    pub const fn pattern_index(&self) -> usize {
        self.pattern_index
    }
    pub fn pieces(&self) -> &[Pc4GraphPiece] {
        &self.pieces
    }
    pub const fn weight(&self) -> ProbabilityValue {
        self.weight
    }
}

/// An input-only certificate minted only after every original queue and weight
/// has been audited. Reading queues stays lazy and uses the same immutable
/// problem, not a reconstructed seven-bag or an unweighted candidate union.
#[derive(Clone, Debug)]
pub struct Pc4CompiledPatternSource {
    problem: Arc<SearchProblem>,
    identity: Pc4CompiledPatternIdentity,
    pattern_count: usize,
    sequence_pieces: usize,
}

impl Pc4CompiledPatternSource {
    pub const fn identity(&self) -> Pc4CompiledPatternIdentity {
        self.identity
    }
    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }
    pub const fn sequence_pieces(&self) -> usize {
        self.sequence_pieces
    }
    pub fn problem(&self) -> &SearchProblem {
        &self.problem
    }

    pub fn read_queue(
        &self,
        pattern_index: usize,
    ) -> Result<Pc4CompiledPatternQueue, Pc4CompiledPatternError> {
        if pattern_index >= self.pattern_count {
            return Err(Pc4CompiledPatternError::PatternIndexOutOfBounds);
        }
        let universe = universe(&self.problem)?;
        let queue = checked_queue(universe, pattern_index, self.sequence_pieces)?;
        let mut pieces = Vec::new();
        pieces
            .try_reserve_exact(queue.len())
            .map_err(|_| Pc4CompiledPatternError::AllocationFailed)?;
        pieces.extend(queue.iter().copied().map(graph_piece));
        Ok(Pc4CompiledPatternQueue {
            pattern_index,
            pieces,
            weight: universe.weight_at(pattern_index),
        })
    }
}

/// No graph lookup, native search, source clone, or bulk queue allocation occurs
/// in begin. Hashing is O(total source pieces), sliced by advance; source count
/// is checked before the first lazy unrank. Cancellation poisons preparation,
/// so an observed cancellation cannot be followed by a late successful seal.
pub struct Pc4CompiledPatternPreparation {
    problem: Arc<SearchProblem>,
    limits: Pc4CompiledPatternLimits,
    pattern_count: usize,
    sequence_pieces: usize,
    next_pattern: usize,
    hasher: Option<Sha256>,
}

impl Pc4CompiledPatternPreparation {
    pub fn begin(
        problem: Arc<SearchProblem>,
        limits: Pc4CompiledPatternLimits,
    ) -> Result<Self, Pc4CompiledPatternError> {
        if !matches!(
            problem.problem_kind(),
            SearchProblemKind::OpeningPc | SearchProblemKind::ScenarioPc
        ) {
            return Err(Pc4CompiledPatternError::UnsupportedProblem);
        }
        let board = problem.initial_board();
        if board.width() != 10 || !(1..=4).contains(&board.visible_height()) {
            return Err(Pc4CompiledPatternError::UnsupportedBoard);
        }
        let mut hasher = Sha256::new();
        hasher.update(IDENTITY_DOMAIN);
        match problem.core_query().remaining_queue() {
            PcQueueInput::PatternExpression(expression) => {
                hasher.update([0]);
                hash_len(&mut hasher, expression.source().len())?;
                hasher.update(expression.source().as_bytes());
            }
            PcQueueInput::Standard7Bag => hasher.update([1]),
            // Fixed queues and hidden observations have separate disclosure
            // contracts; they must not acquire pattern authority by relabeling.
            _ => return Err(Pc4CompiledPatternError::UnsupportedQueueSource),
        }
        let universe = universe(&problem)?;
        if !problem.piece_source().complete()
            || !universe.complete()
            || problem.piece_source().truncation_reason().is_some()
            || universe.truncation_reason().is_some()
        {
            return Err(Pc4CompiledPatternError::IncompleteUniverse);
        }
        let pattern_count = universe.pattern_count();
        if pattern_count == 0 || universe.total_possible_pattern_count() != pattern_count as u128 {
            return Err(Pc4CompiledPatternError::InconsistentUniverse);
        }
        if pattern_count > limits.patterns.get() {
            return Err(Pc4CompiledPatternError::PatternLimit {
                limit: limits.patterns.get(),
                attempted: pattern_count,
            });
        }
        let sequence_pieces = problem.supply().source_sequence_length();
        if sequence_pieces > limits.sequence_pieces.get() {
            return Err(Pc4CompiledPatternError::SequencePieceLimit {
                limit: limits.sequence_pieces.get(),
                attempted: sequence_pieces,
            });
        }
        if sequence_pieces == 0 {
            return Err(Pc4CompiledPatternError::InconsistentUniverse);
        }
        hash_len(&mut hasher, pattern_count)?;
        hasher.update(universe.total_possible_pattern_count().to_be_bytes());
        hash_len(&mut hasher, sequence_pieces)?;
        hasher.update([u8::from(problem.supply().projects_unplaced_lookahead())]);
        Ok(Self {
            problem,
            limits,
            pattern_count,
            sequence_pieces,
            next_pattern: 0,
            hasher: Some(hasher),
        })
    }

    pub const fn audited_patterns(&self) -> usize {
        self.next_pattern
    }
    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }
    pub fn is_complete(&self) -> bool {
        self.hasher.is_some() && self.next_pattern == self.pattern_count
    }

    /// A failed page commits no partial hash or cursor. Limit errors are
    /// retryable with a smaller page; cancellation or invalid source is terminal.
    pub fn advance<G: Fn() -> bool>(
        &mut self,
        limit: NonZeroUsize,
        cancelled: &G,
    ) -> Result<bool, Pc4CompiledPatternError> {
        let hasher = self
            .hasher
            .as_ref()
            .ok_or(Pc4CompiledPatternError::PreparationTerminated)?;
        if cancelled() {
            self.hasher = None;
            return Err(Pc4CompiledPatternError::Cancelled);
        }
        if limit.get() > self.limits.patterns_per_advance.get() {
            return Err(Pc4CompiledPatternError::AdvanceLimit {
                limit: self.limits.patterns_per_advance.get(),
                attempted: limit.get(),
            });
        }
        let mut staged = hasher.clone();
        let end = self
            .next_pattern
            .saturating_add(limit.get())
            .min(self.pattern_count);
        let result = (|| {
            let universe = universe(&self.problem)?;
            for index in self.next_pattern..end {
                if cancelled() {
                    return Err(Pc4CompiledPatternError::Cancelled);
                }
                hash_record(&mut staged, universe, index, self.sequence_pieces)?;
            }
            if cancelled() {
                return Err(Pc4CompiledPatternError::Cancelled);
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.hasher = None;
            return Err(error);
        }
        self.next_pattern = end;
        self.hasher = Some(staged);
        Ok(self.is_complete())
    }

    pub fn finish(self) -> Result<Pc4CompiledPatternSource, Pc4CompiledPatternError> {
        let hasher = self
            .hasher
            .ok_or(Pc4CompiledPatternError::PreparationTerminated)?;
        if self.next_pattern != self.pattern_count {
            return Err(Pc4CompiledPatternError::PreparationIncomplete);
        }
        Ok(Pc4CompiledPatternSource {
            problem: self.problem,
            identity: Pc4CompiledPatternIdentity(hasher.finalize().into()),
            pattern_count: self.pattern_count,
            sequence_pieces: self.sequence_pieces,
        })
    }
}

fn universe(
    problem: &SearchProblem,
) -> Result<&MaterializedPatternUniverse, Pc4CompiledPatternError> {
    problem
        .piece_source()
        .materialized_universe()
        .ok_or(Pc4CompiledPatternError::MissingUniverse)
}

fn checked_queue(
    universe: &MaterializedPatternUniverse,
    index: usize,
    length: usize,
) -> Result<Cow<'_, [PieceKind]>, Pc4CompiledPatternError> {
    // Check the lazy row size BEFORE unranking, not after allocating its output.
    if universe.sequence_len_at(index) != length {
        return Err(Pc4CompiledPatternError::InconsistentUniverse);
    }
    let queue = universe
        .try_sequence_at(index)
        .ok_or(Pc4CompiledPatternError::InconsistentUniverse)?;
    if queue.len() != length {
        return Err(Pc4CompiledPatternError::InconsistentUniverse);
    }
    Ok(queue)
}

fn hash_record(
    hasher: &mut Sha256,
    universe: &MaterializedPatternUniverse,
    index: usize,
    length: usize,
) -> Result<(), Pc4CompiledPatternError> {
    let queue = checked_queue(universe, index, length)?;
    hash_len(hasher, index)?;
    hash_len(hasher, queue.len())?;
    // Preserve the original Core f64 weight bits. No inferred rational bag,
    // renormalization, or successful-outcome-only denominator is introduced.
    hasher.update(universe.weight_at(index).get().to_bits().to_be_bytes());
    for piece in queue.iter().copied() {
        hasher.update([match piece {
            PieceKind::I => b'I',
            PieceKind::J => b'J',
            PieceKind::L => b'L',
            PieceKind::O => b'O',
            PieceKind::S => b'S',
            PieceKind::T => b'T',
            PieceKind::Z => b'Z',
        }]);
    }
    Ok(())
}

fn hash_len(hasher: &mut Sha256, value: usize) -> Result<(), Pc4CompiledPatternError> {
    hasher.update(
        u64::try_from(value)
            .map_err(|_| Pc4CompiledPatternError::CanonicalLengthOverflow)?
            .to_be_bytes(),
    );
    Ok(())
}

fn graph_piece(piece: PieceKind) -> Pc4GraphPiece {
    match piece {
        PieceKind::I => Pc4GraphPiece::I,
        PieceKind::J => Pc4GraphPiece::J,
        PieceKind::L => Pc4GraphPiece::L,
        PieceKind::O => Pc4GraphPiece::O,
        PieceKind::S => Pc4GraphPiece::S,
        PieceKind::T => Pc4GraphPiece::T,
        PieceKind::Z => Pc4GraphPiece::Z,
    }
}

#[cfg(test)]
#[path = "pc4_compiled_pattern_source_tests.rs"]
mod tests;
