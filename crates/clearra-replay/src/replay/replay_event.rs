mod board_snapshot_event {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ReplayBoardSnapshotPhase {
        Initial,
        BeforePlacement,
        AfterPlacement,
        AfterLineClear,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayBoardSnapshotEvent {
        step_index: usize,
        phase: ReplayBoardSnapshotPhase,
        occupied: u64,
    }

    impl ReplayBoardSnapshotEvent {
        pub fn new(step_index: usize, phase: ReplayBoardSnapshotPhase, occupied: u64) -> Self {
            Self {
                step_index,
                phase,
                occupied,
            }
        }
    }
    impl ReplayBoardSnapshotEvent {
        pub fn step_index(self) -> usize {
            self.step_index
        }
    }
    impl ReplayBoardSnapshotEvent {
        pub fn phase(self) -> ReplayBoardSnapshotPhase {
            self.phase
        }
    }
    impl ReplayBoardSnapshotEvent {
        pub fn occupied(self) -> u64 {
            self.occupied
        }
    }
}
mod cell_owner {
    use clearra_core_domain::operation::operation::OperationId;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CellOwner {
        InitialGray,
        Piece(OperationId),
    }
}
mod drop_event {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayDropEvent {
        step_index: usize,
        from_y: u16,
        to_y: u16,
        distance: u16,
    }

    impl ReplayDropEvent {
        pub fn new(step_index: usize, from_y: u16, to_y: u16) -> Self {
            Self {
                step_index,
                from_y,
                to_y,
                distance: from_y.saturating_sub(to_y),
            }
        }
    }
    impl ReplayDropEvent {
        pub fn step_index(self) -> usize {
            self.step_index
        }
    }
    impl ReplayDropEvent {
        pub fn from_y(self) -> u16 {
            self.from_y
        }
    }
    impl ReplayDropEvent {
        pub fn to_y(self) -> u16 {
            self.to_y
        }
    }
    impl ReplayDropEvent {
        pub fn distance(self) -> u16 {
            self.distance
        }
    }
}
mod event {
    use crate::event::{KickEvidenceEvent, MovementEvidenceEvent, TraceCompletenessEvent};

    use super::{
        ReplayBoardSnapshotEvent, ReplayDropEvent, ReplayHoldReleaseEvent, ReplayHoldStoreEvent,
        ReplayHoldSwapEvent, ReplayLineClearEvent, ReplayLockEvent, ReplayPlacementEvent,
        ReplayScoreBasisEvent, ReplaySpinBasisEvent, ReplayTraceMarker,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum ReplayEvent {
        Lock(ReplayLockEvent),
        Placement(ReplayPlacementEvent),
        HoldStore(ReplayHoldStoreEvent),
        HoldSwap(ReplayHoldSwapEvent),
        HoldRelease(ReplayHoldReleaseEvent),
        LineClear(ReplayLineClearEvent),
        Drop(ReplayDropEvent),
        SpinBasis(ReplaySpinBasisEvent),
        ScoreBasis(ReplayScoreBasisEvent),
        BoardSnapshot(ReplayBoardSnapshotEvent),
        KickEvidence(KickEvidenceEvent),
        MovementEvidence(MovementEvidenceEvent),
        TraceCompleteness(TraceCompletenessEvent),
        TraceMarker(ReplayTraceMarker),
    }
}
mod hold_release_event {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    /// A terminal hold swap whose incoming next-bag piece is intentionally not
    /// materialized because no later operation can observe its identity.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayHoldReleaseEvent {
        step_index: usize,
        active_piece: PieceKind,
    }

    impl ReplayHoldReleaseEvent {
        pub fn new(step_index: usize, active_piece: PieceKind) -> Self {
            Self {
                step_index,
                active_piece,
            }
        }

        pub fn step_index(self) -> usize {
            self.step_index
        }

        pub fn active_piece(self) -> PieceKind {
            self.active_piece
        }
    }
}
mod event_id {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayEventId(pub u32);
}
mod hold_store_event {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayHoldStoreEvent {
        step_index: usize,
        stored_piece: PieceKind,
    }

    impl ReplayHoldStoreEvent {
        pub fn new(step_index: usize, stored_piece: PieceKind) -> Self {
            Self {
                step_index,
                stored_piece,
            }
        }
    }
    impl ReplayHoldStoreEvent {
        pub fn step_index(self) -> usize {
            self.step_index
        }
    }
    impl ReplayHoldStoreEvent {
        pub fn stored_piece(self) -> PieceKind {
            self.stored_piece
        }
    }
}
mod hold_swap_event {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayHoldSwapEvent {
        step_index: usize,
        held_piece: PieceKind,
        active_piece: PieceKind,
    }

    impl ReplayHoldSwapEvent {
        pub fn new(step_index: usize, held_piece: PieceKind, active_piece: PieceKind) -> Self {
            Self {
                step_index,
                held_piece,
                active_piece,
            }
        }
    }
    impl ReplayHoldSwapEvent {
        pub fn step_index(self) -> usize {
            self.step_index
        }
    }
    impl ReplayHoldSwapEvent {
        pub fn held_piece(self) -> PieceKind {
            self.held_piece
        }
    }
    impl ReplayHoldSwapEvent {
        pub fn active_piece(self) -> PieceKind {
            self.active_piece
        }
    }
}
mod line_clear_event {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayLineClearEvent {
        step_index: usize,
        cleared_lines: u8,
    }

    impl ReplayLineClearEvent {
        pub fn new(step_index: usize, cleared_lines: u8) -> Self {
            Self {
                step_index,
                cleared_lines,
            }
        }
    }
    impl ReplayLineClearEvent {
        pub fn step_index(self) -> usize {
            self.step_index
        }
    }
    impl ReplayLineClearEvent {
        pub fn cleared_lines(self) -> u8 {
            self.cleared_lines
        }
    }
}
mod lock_event {
    use clearra_core_domain::{
        field::occupancy_field::OccupancyField,
        operation::operation::OperationId,
        piece::{piece_kind::PieceKind, rotation::RotationState},
    };

    use super::{CellOwner, ReplayEventId, RowMask};

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ReplayLockEvent {
        event_id: ReplayEventId,
        operation_id: OperationId,
        piece: PieceKind,
        rotation: RotationState,
        lock_x: i16,
        lock_y: i16,
        board_before: OccupancyField,
        board_after_place: OccupancyField,
        cleared_lines: RowMask,
        cleared_cell_owners: Vec<CellOwner>,
        board_after_clear: OccupancyField,
    }

    impl ReplayLockEvent {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            event_id: ReplayEventId,
            operation_id: OperationId,
            piece: PieceKind,
            rotation: RotationState,
            lock_x: i16,
            lock_y: i16,
            board_before: OccupancyField,
            board_after_place: OccupancyField,
            cleared_lines: RowMask,
            cleared_cell_owners: Vec<CellOwner>,
            board_after_clear: OccupancyField,
        ) -> Self {
            Self {
                event_id,
                operation_id,
                piece,
                rotation,
                lock_x,
                lock_y,
                board_before,
                board_after_place,
                cleared_lines,
                cleared_cell_owners,
                board_after_clear,
            }
        }
    }
    impl ReplayLockEvent {
        pub fn event_id(&self) -> ReplayEventId {
            self.event_id
        }
    }
    impl ReplayLockEvent {
        pub fn operation_id(&self) -> OperationId {
            self.operation_id
        }
    }
    impl ReplayLockEvent {
        pub fn piece(&self) -> PieceKind {
            self.piece
        }
    }
    impl ReplayLockEvent {
        pub fn rotation(&self) -> RotationState {
            self.rotation
        }
    }
    impl ReplayLockEvent {
        pub fn lock_x(&self) -> i16 {
            self.lock_x
        }
    }
    impl ReplayLockEvent {
        pub fn lock_y(&self) -> i16 {
            self.lock_y
        }
    }
    impl ReplayLockEvent {
        pub fn board_before(&self) -> OccupancyField {
            self.board_before
        }
    }
    impl ReplayLockEvent {
        pub fn board_after_place(&self) -> OccupancyField {
            self.board_after_place
        }
    }
    impl ReplayLockEvent {
        pub fn cleared_lines(&self) -> RowMask {
            self.cleared_lines
        }
    }
    impl ReplayLockEvent {
        pub fn cleared_cell_owners(&self) -> &[CellOwner] {
            &self.cleared_cell_owners
        }
    }
    impl ReplayLockEvent {
        pub fn board_after_clear(&self) -> OccupancyField {
            self.board_after_clear
        }
    }
}
mod placement_event {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayPlacementEvent {
        step_index: usize,
        piece: PieceKind,
        rotation: RotationState,
        x: u16,
        y: u16,
        placed_mask: u64,
    }

    impl ReplayPlacementEvent {
        pub fn new(
            step_index: usize,
            piece: PieceKind,
            rotation: RotationState,
            x: u16,
            y: u16,
            placed_mask: u64,
        ) -> Self {
            Self {
                step_index,
                piece,
                rotation,
                x,
                y,
                placed_mask,
            }
        }
    }
    impl ReplayPlacementEvent {
        pub fn step_index(self) -> usize {
            self.step_index
        }
    }
    impl ReplayPlacementEvent {
        pub fn piece(self) -> PieceKind {
            self.piece
        }
    }
    impl ReplayPlacementEvent {
        pub fn rotation(self) -> RotationState {
            self.rotation
        }
    }
    impl ReplayPlacementEvent {
        pub fn x(self) -> u16 {
            self.x
        }
    }
    impl ReplayPlacementEvent {
        pub fn y(self) -> u16 {
            self.y
        }
    }
    impl ReplayPlacementEvent {
        pub fn placed_mask(self) -> u64 {
            self.placed_mask
        }
    }
}
mod row_mask {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct RowMask(pub u64);
}
mod score_basis_event {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayScoreBasisEvent {
        step_index: usize,
        piece: PieceKind,
        cleared_lines: u8,
        board_before: u64,
        board_after_line_clear: u64,
    }

    impl ReplayScoreBasisEvent {
        pub fn new(
            step_index: usize,
            piece: PieceKind,
            cleared_lines: u8,
            board_before: u64,
            board_after_line_clear: u64,
        ) -> Self {
            Self {
                step_index,
                piece,
                cleared_lines,
                board_before,
                board_after_line_clear,
            }
        }
    }
    impl ReplayScoreBasisEvent {
        pub fn step_index(self) -> usize {
            self.step_index
        }
    }
    impl ReplayScoreBasisEvent {
        pub fn piece(self) -> PieceKind {
            self.piece
        }
    }
    impl ReplayScoreBasisEvent {
        pub fn cleared_lines(self) -> u8 {
            self.cleared_lines
        }
    }
    impl ReplayScoreBasisEvent {
        pub fn board_before(self) -> u64 {
            self.board_before
        }
    }
    impl ReplayScoreBasisEvent {
        pub fn board_after_line_clear(self) -> u64 {
            self.board_after_line_clear
        }
    }
}
mod spin_basis_event {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplaySpinBasisEvent {
        step_index: usize,
        piece: PieceKind,
        rotation: RotationState,
        x: u16,
        y: u16,
        board_before: u64,
        board_after_placement: u64,
        cleared_lines: u8,
    }

    impl ReplaySpinBasisEvent {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            step_index: usize,
            piece: PieceKind,
            rotation: RotationState,
            x: u16,
            y: u16,
            board_before: u64,
            board_after_placement: u64,
            cleared_lines: u8,
        ) -> Self {
            Self {
                step_index,
                piece,
                rotation,
                x,
                y,
                board_before,
                board_after_placement,
                cleared_lines,
            }
        }
    }
    impl ReplaySpinBasisEvent {
        pub fn step_index(self) -> usize {
            self.step_index
        }
    }
    impl ReplaySpinBasisEvent {
        pub fn piece(self) -> PieceKind {
            self.piece
        }
    }
    impl ReplaySpinBasisEvent {
        pub fn rotation(self) -> RotationState {
            self.rotation
        }
    }
    impl ReplaySpinBasisEvent {
        pub fn x(self) -> u16 {
            self.x
        }
    }
    impl ReplaySpinBasisEvent {
        pub fn y(self) -> u16 {
            self.y
        }
    }
    impl ReplaySpinBasisEvent {
        pub fn board_before(self) -> u64 {
            self.board_before
        }
    }
    impl ReplaySpinBasisEvent {
        pub fn board_after_placement(self) -> u64 {
            self.board_after_placement
        }
    }
    impl ReplaySpinBasisEvent {
        pub fn cleared_lines(self) -> u8 {
            self.cleared_lines
        }
    }
}
mod trace_marker {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReplayTraceMarker {
        representative: bool,
        sample: bool,
    }

    impl ReplayTraceMarker {
        pub fn new(representative: bool, sample: bool) -> Self {
            Self {
                representative,
                sample,
            }
        }
    }
    impl ReplayTraceMarker {
        pub fn representative(self) -> bool {
            self.representative
        }
    }
    impl ReplayTraceMarker {
        pub fn sample(self) -> bool {
            self.sample
        }
    }
}

pub use board_snapshot_event::{ReplayBoardSnapshotEvent, ReplayBoardSnapshotPhase};
pub use cell_owner::CellOwner;
pub use drop_event::ReplayDropEvent;
pub use event::ReplayEvent;
pub use event_id::ReplayEventId;
pub use hold_release_event::ReplayHoldReleaseEvent;
pub use hold_store_event::ReplayHoldStoreEvent;
pub use hold_swap_event::ReplayHoldSwapEvent;
pub use line_clear_event::ReplayLineClearEvent;
pub use lock_event::ReplayLockEvent;
pub use placement_event::ReplayPlacementEvent;
pub use row_mask::RowMask;
pub use score_basis_event::ReplayScoreBasisEvent;
pub use spin_basis_event::ReplaySpinBasisEvent;
pub use trace_marker::ReplayTraceMarker;
