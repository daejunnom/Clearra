//! Physical replay projection for the four-word path. This validates selected
//! locks and supply transitions; it neither searches kicks nor proves that a
//! representative path is a complete replay family. Board64/trk1 stay unchanged.

use std::fmt;

use clearra_core_domain::{
    board::standard_pc_board::{Board256Mask, STANDARD_PC_MAX_LINES},
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use crate::{HoldDecision, PieceDecision, ScoringExecutionEdge};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FullHeightReplayError {
    InvalidHeight,
    InvalidInitialBoard,
    OutOfBounds,
    Collision,
    ClearedLinesMismatch,
    PerfectClearMismatch,
    SupplyTransitionMismatch,
    InvalidPathLength,
    DuplicateOperation,
    InvalidOperation,
    AllocationFailed,
    ProjectionOverflow,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FullHeightTransition {
    placement: Board256Mask,
    after_placement: Board256Mask,
    after_line_clear: Board256Mask,
    cleared_row_mask: u32,
}

impl FullHeightTransition {
    pub const fn placement(self) -> Board256Mask {
        self.placement
    }
    pub const fn after_placement(self) -> Board256Mask {
        self.after_placement
    }
    pub const fn after_line_clear(self) -> Board256Mask {
        self.after_line_clear
    }
    pub const fn cleared_row_mask(self) -> u32 {
        self.cleared_row_mask
    }
    pub const fn cleared_lines(self) -> u8 {
        self.cleared_row_mask.count_ones() as u8
    }
    pub const fn perfect_clear(self) -> bool {
        self.cleared_row_mask != 0 && self.after_line_clear.is_empty()
    }
}

pub struct FullHeightReplayProjector;

impl FullHeightReplayProjector {
    pub fn validate_board(height: u8, occupied: Board256Mask) -> Result<(), FullHeightReplayError> {
        if !(1..=STANDARD_PC_MAX_LINES).contains(&height) {
            return Err(FullHeightReplayError::InvalidHeight);
        }
        if !occupied
            .fits_cell_count(u16::from(height) * 10)
            .unwrap_or(false)
        {
            return Err(FullHeightReplayError::InvalidInitialBoard);
        }
        Ok(())
    }

    /// The same shape origin as PlacementMask and the ILC realization. Signed
    /// origins are permitted only when all actual cells are inside the field.
    /// No heap allocation and no low-word conversion occurs in this projection.
    pub fn project_lock(
        height: u8,
        occupied: Board256Mask,
        piece: PieceKind,
        rotation: RotationState,
        x: i32,
        y: i32,
    ) -> Result<FullHeightTransition, FullHeightReplayError> {
        Self::validate_board(height, occupied)?;
        let definition = standard_tetromino_registry()
            .get(piece)
            .ok_or(FullHeightReplayError::OutOfBounds)?;
        let mut placement = Board256Mask::EMPTY;
        for cell in definition.shape(rotation).cells() {
            let cell_x = x
                .checked_add(i32::from(cell.x()))
                .ok_or(FullHeightReplayError::OutOfBounds)?;
            let cell_y = y
                .checked_add(i32::from(cell.y()))
                .ok_or(FullHeightReplayError::OutOfBounds)?;
            if !(0..10).contains(&cell_x) || !(0..i32::from(height)).contains(&cell_y) {
                return Err(FullHeightReplayError::OutOfBounds);
            }
            let index = (cell_y * 10 + cell_x) as u16;
            placement = placement.union(
                Board256Mask::singleton(index).map_err(|_| FullHeightReplayError::OutOfBounds)?,
            );
        }
        if placement.count_ones() != 4 {
            return Err(FullHeightReplayError::OutOfBounds);
        }
        if occupied.intersects(placement) {
            return Err(FullHeightReplayError::Collision);
        }
        let after_placement = occupied.union(placement);
        let mut after_line_clear = Board256Mask::EMPTY;
        let mut cleared_row_mask = 0_u32;
        let mut destination_row = 0_u16;
        for row in 0..u16::from(height) {
            let row_mask = Board256Mask::row(10, u16::from(height), row)
                .expect("validated ten-column replay layout");
            if after_placement.union(row_mask) == after_placement {
                cleared_row_mask |= 1_u32 << row;
            } else {
                for column in 0..10_u16 {
                    if after_placement.contains_index(row * 10 + column) {
                        after_line_clear = after_line_clear.union(
                            Board256Mask::singleton(destination_row * 10 + column)
                                .expect("compaction stays inside validated layout"),
                        );
                    }
                }
                destination_row += 1;
            }
        }
        Ok(FullHeightTransition {
            placement,
            after_placement,
            after_line_clear,
            cleared_row_mask,
        })
    }

    pub fn project_scoring_step(
        height: u8,
        occupied: Board256Mask,
        edge: ScoringExecutionEdge,
    ) -> Result<FullHeightTransition, FullHeightReplayError> {
        let transition = Self::project_lock(
            height,
            occupied,
            edge.piece(),
            edge.rotation(),
            i32::from(edge.x()),
            i32::from(edge.y()),
        )?;
        if transition.cleared_lines() != edge.cleared_lines() {
            return Err(FullHeightReplayError::ClearedLinesMismatch);
        }
        if transition.perfect_clear() != edge.perfect_clear() {
            return Err(FullHeightReplayError::PerfectClearMismatch);
        }
        Ok(transition)
    }
}

/// A selected supply edge plus the exact physical board chain. Kick evidence
/// remains on the edge and is not invented by reconstructing a picture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FullHeightReplayStep {
    edge: ScoringExecutionEdge,
    decision: PieceDecision,
    before: Board256Mask,
    transition: FullHeightTransition,
}

impl FullHeightReplayStep {
    pub const fn edge(self) -> ScoringExecutionEdge {
        self.edge
    }
    pub const fn decision(self) -> PieceDecision {
        self.decision
    }
    pub const fn before(self) -> Board256Mask {
        self.before
    }
    pub const fn transition(self) -> FullHeightTransition {
        self.transition
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullHeightReplayTrace {
    height: u8,
    initial: Board256Mask,
    steps: Vec<FullHeightReplayStep>,
}

impl FullHeightReplayTrace {
    /// Caller retains the supply/graph authority. Memory is admitted before
    /// reserve and again with the actual allocator-visible capacity. A failure
    /// returns no partial trace. Cancellation is checked at every lock.
    pub fn from_selected_path<E>(
        height: u8,
        initial: Board256Mask,
        initial_cursor: usize,
        initial_hold: Option<PieceKind>,
        path: &[(ScoringExecutionEdge, HoldDecision)],
        control: &ExecutionControl,
        mut memory_guard: impl FnMut(u128) -> Result<(), E>,
    ) -> Result<Self, FullHeightReplayBuildError<E>> {
        use FullHeightReplayBuildError::{MemoryGuard, Replay};
        if control.is_cancelled() {
            return Err(Replay(FullHeightReplayError::Cancelled));
        }
        FullHeightReplayProjector::validate_board(height, initial).map_err(Replay)?;
        if path.len() > 60 || path.is_empty() {
            return Err(Replay(FullHeightReplayError::InvalidPathLength));
        }
        let requested = Self::checked_step_buffer_bytes(path.len())
            .ok_or(Replay(FullHeightReplayError::ProjectionOverflow))?;
        memory_guard(requested).map_err(MemoryGuard)?;
        let mut steps = Vec::new();
        steps
            .try_reserve_exact(path.len())
            .map_err(|_| Replay(FullHeightReplayError::AllocationFailed))?;
        let actual = Self::checked_step_buffer_bytes(steps.capacity())
            .ok_or(Replay(FullHeightReplayError::ProjectionOverflow))?;
        memory_guard(actual).map_err(MemoryGuard)?;
        let mut board = initial;
        let mut cursor = initial_cursor;
        let mut hold = initial_hold;
        let mut used_operations = 0_u64;
        for &(edge, selected_hold) in path {
            if control.is_cancelled() {
                return Err(Replay(FullHeightReplayError::Cancelled));
            }
            if edge.operation_index() >= 60 {
                return Err(Replay(FullHeightReplayError::InvalidOperation));
            }
            let bit = 1_u64 << edge.operation_index();
            if used_operations & bit != 0 {
                return Err(Replay(FullHeightReplayError::DuplicateOperation));
            }
            used_operations |= bit;
            let decision =
                PieceDecision::from_selected_hold(edge.piece(), cursor, hold, selected_hold)
                    .ok_or(Replay(FullHeightReplayError::SupplyTransitionMismatch))?;
            let transition = FullHeightReplayProjector::project_scoring_step(height, board, edge)
                .map_err(Replay)?;
            steps.push(FullHeightReplayStep {
                edge,
                decision,
                before: board,
                transition,
            });
            board = transition.after_line_clear();
            cursor = decision.output_cursor();
            hold = decision.output_hold_piece();
        }
        Ok(Self {
            height,
            initial,
            steps,
        })
    }

    pub const fn height(&self) -> u8 {
        self.height
    }
    pub const fn initial(&self) -> Board256Mask {
        self.initial
    }
    pub fn steps(&self) -> &[FullHeightReplayStep] {
        &self.steps
    }
    pub fn final_board(&self) -> Board256Mask {
        self.steps
            .last()
            .map_or(self.initial, |step| step.transition.after_line_clear())
    }
    pub fn checked_step_buffer_bytes(capacity: usize) -> Option<u128> {
        (capacity as u128).checked_mul(core::mem::size_of::<FullHeightReplayStep>() as u128)
    }
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        Self::checked_step_buffer_bytes(self.steps.capacity())
    }

    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        Self::checked_step_buffer_bytes(self.steps.len())
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        self.checked_nested_retained_bytes()?
            .checked_add(self.checked_clone_nested_bytes()?)
    }

    /// trk2 carries four words; it cannot collide with compact trk1 or discard
    /// high rows. The operation ordering, not diagnostic producer IDs, is the
    /// trace identity. No result-family completeness is asserted by this key.
    pub fn write_canonical_key(&self, writer: &mut impl fmt::Write) -> fmt::Result {
        Self::write_canonical_prefix(writer, self.height, self.initial)?;
        for step in &self.steps {
            writer.write_char('~')?;
            Self::write_step_key_with_decision(
                writer,
                step.edge,
                step.decision,
                step.transition.placement(),
            )?;
        }
        Ok(())
    }

    /// The exact same writers serve counting labels and selected trace keys.
    /// They serialize identity only; neither helper grants execution authority.
    pub fn write_canonical_prefix(
        writer: &mut impl fmt::Write,
        height: u8,
        initial: Board256Mask,
    ) -> fmt::Result {
        write!(writer, "trk2:h{height}:")?;
        write_mask(writer, initial)
    }

    pub fn write_step_key_with_decision(
        writer: &mut impl fmt::Write,
        edge: ScoringExecutionEdge,
        decision: PieceDecision,
        placement: Board256Mask,
    ) -> fmt::Result {
        write!(
            writer,
            "a{}i{}o{}ih",
            decision.active_piece().as_ascii(),
            decision.input_cursor(),
            decision.output_cursor()
        )?;
        write_hold_piece(writer, decision.input_hold_piece())?;
        writer.write_str("oh")?;
        write_hold_piece(writer, decision.output_hold_piece())?;
        writer.write_char('d')?;
        write_hold_decision(writer, decision.hold_decision())?;
        write!(
            writer,
            "p{}r{}x{}y{}m",
            edge.piece().as_ascii(),
            edge.rotation().quarter_turns(),
            edge.x(),
            edge.y()
        )?;
        write_mask(writer, placement)
    }

    /// No allocation before the caller's memory admission. The selected key is
    /// later checked again using allocator-visible String capacity.
    pub fn checked_canonical_key_requested_bytes(&self) -> Option<u128> {
        struct Counter(u128);
        impl fmt::Write for Counter {
            fn write_str(&mut self, value: &str) -> fmt::Result {
                self.0 = self.0.checked_add(value.len() as u128).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let mut counter = Counter(0);
        self.write_canonical_key(&mut counter).ok()?;
        Some(counter.0)
    }

    /// Compare one selected canonical identity without a temporary key owner.
    /// This does not prove family completeness or membership.
    pub fn canonical_key_matches(&self, identity: &str) -> bool {
        struct MatchWriter<'a>(&'a str);
        impl fmt::Write for MatchWriter<'_> {
            fn write_str(&mut self, value: &str) -> fmt::Result {
                self.0 = self.0.strip_prefix(value).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let mut writer = MatchWriter(identity);
        self.write_canonical_key(&mut writer).is_ok() && writer.0.is_empty()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum FullHeightReplayBuildError<E> {
    Replay(FullHeightReplayError),
    MemoryGuard(E),
}

fn write_mask(writer: &mut impl fmt::Write, mask: Board256Mask) -> fmt::Result {
    let words = mask.words();
    write!(
        writer,
        "{:016x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    )
}

fn write_hold_piece(writer: &mut impl fmt::Write, piece: Option<PieceKind>) -> fmt::Result {
    match piece {
        Some(piece) => writer.write_char(piece.as_ascii()),
        None => writer.write_str("none"),
    }
}

fn write_hold_decision(writer: &mut impl fmt::Write, decision: HoldDecision) -> fmt::Result {
    match decision {
        HoldDecision::None => writer.write_str("none"),
        HoldDecision::SwapWithHold {
            incoming_piece,
            held_piece,
        } => write!(
            writer,
            "swap{}{}",
            incoming_piece.as_ascii(),
            held_piece.as_ascii()
        ),
        HoldDecision::StoreIncoming {
            stored_piece,
            drawn_piece,
        } => write!(
            writer,
            "store{}{}",
            stored_piece.as_ascii(),
            drawn_piece.as_ascii()
        ),
        HoldDecision::ReleaseHeldAtTerminal { held_piece } => {
            write!(writer, "terminal{}", held_piece.as_ascii())
        }
    }
}

#[cfg(test)]
#[path = "full_height_replay_tests.rs"]
mod tests;
