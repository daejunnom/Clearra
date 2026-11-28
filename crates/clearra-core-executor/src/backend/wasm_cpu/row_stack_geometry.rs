//! Streaming geometry generation in the original, uncleared row frame.
//!
//! Join each pair of consecutive occupied rows of a placement by an arc. Two
//! arcs `a < c < b < d` cannot occur in a buildable tiling: the placement at
//! `c` must precede the placement spanning `(a, b)`, while the placement at
//! `b` must precede the placement spanning `(c, d)`. Thus a legal tiling has a
//! stack embedding of these arcs. This module constructs those embeddings
//! directly, without an exact-cover family, subset table, or residual memo.
//!
//! Noncrossing is only a necessary condition. Completed placements are looked
//! up in the caller's existing ILC catalog, and the caller must still apply its
//! exact physical and supply verifier, including every catalog realization.

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use crate::resource::SharedExecutionNodeBudget;

use super::{
    catalog::GeometryCatalog, extended_board::ExtendedBoard,
    extended_inverse_catalog::ExtendedInverseCatalog, piece_index, WasmExactSearchError,
};

const MAX_WIDTH: usize = 10;
const MAX_HEIGHT: usize = 24;
const MAX_PIECES: usize = 64;
const DISTINCT_SHAPES: usize = 19;
const ROOT_PARTITION_PIECES: usize = 2;
const NO_ROW: u32 = u32::MAX;

/// The generator borrows the same immutable catalog as the verifier. Mapping
/// complete pieces back to row IDs preserves all of its realization evidence.
pub(super) trait RowStackCatalog {
    fn width(&self) -> u8;
    fn height(&self) -> u8;
    fn required_cells(&self) -> ExtendedBoard;
    fn skeleton_id(&self, piece: PieceKind, cells: ExtendedBoard) -> Option<u32>;
}

impl RowStackCatalog for GeometryCatalog {
    fn width(&self) -> u8 {
        self.width()
    }

    fn height(&self) -> u8 {
        self.height()
    }

    fn required_cells(&self) -> ExtendedBoard {
        ExtendedBoard::from_words([self.required_cells(), 0, 0, 0])
    }

    fn skeleton_id(&self, piece: PieceKind, cells: ExtendedBoard) -> Option<u32> {
        let [cells, upper1, upper2, upper3] = cells.words();
        if upper1 | upper2 | upper3 != 0 {
            return None;
        }
        self.skeleton_id(piece, cells)
    }
}

impl RowStackCatalog for ExtendedInverseCatalog {
    fn width(&self) -> u8 {
        self.width()
    }

    fn height(&self) -> u8 {
        self.height()
    }

    fn required_cells(&self) -> ExtendedBoard {
        self.required_cells()
    }

    fn skeleton_id(&self, piece: PieceKind, cells: ExtendedBoard) -> Option<u32> {
        self.skeletons()
            .binary_search_by_key(&(piece, cells), |row| (row.piece, row.cells))
            .ok()
            .and_then(|index| u32::try_from(index).ok())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RowStackTarget {
    pub counts: [u8; 7],
    pub target_index: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RowStackCandidate {
    rows: [u32; MAX_PIECES],
    row_count: u8,
    pub target_index: u32,
}

impl RowStackCandidate {
    pub fn row_ids(&self) -> &[u32] {
        &self.rows[..usize::from(self.row_count)]
    }
}

// Candidates transfer their fixed, bounded row carrier to the caller.
#[allow(clippy::large_enum_variant)]
pub(super) enum RowStackGeometryAdvance {
    Pending,
    Candidate(RowStackCandidate),
    Complete,
    Cancelled,
    NodeBudgetExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RowShape {
    piece: PieceKind,
    rows: [u16; 4],
    columns: [u8; 4],
    width: u8,
    height: u8,
}

#[derive(Clone, Copy)]
struct PartialPiece {
    cells: ExtendedBoard,
    row_id: u32,
    occupied_rows: [u8; 4],
    shape: u8,
    x: u8,
    progress: u8,
}

#[derive(Clone, Copy)]
enum FrameStage {
    Close,
    Fill,
    Permute,
    Descend,
    AfterChild,
    Emit,
}

#[derive(Clone, Copy)]
struct RowFrame {
    row: u8,
    piece_base: u8,
    open_base: u8,
    next_close: u8,
    incoming_count: u8,
    incoming: [u8; MAX_WIDTH],
    remaining: u16,
    // One cursor per newly started piece, plus the empty fill suffix.
    next_shapes: [u8; MAX_WIDTH + 1],
    outgoing: [u8; MAX_WIDTH],
    outgoing_count: u8,
    outgoing_domains: [u32; MAX_WIDTH],
    outgoing_x: [u8; MAX_WIDTH],
    permutation: [u8; MAX_WIDTH],
    permutation_next: [u8; MAX_WIDTH + 1],
    endpoint_limits: [u8; MAX_WIDTH + 1],
    permutation_used: u16,
    permutation_depth: u8,
    root_anchor: Option<u8>,
    target_cursor: usize,
    stage: FrameStage,
}

impl RowFrame {
    fn new(row: u8, piece_base: usize, open_base: usize) -> Self {
        Self {
            row,
            piece_base: piece_base as u8,
            open_base: open_base as u8,
            next_close: 0,
            incoming_count: 0,
            incoming: [0; MAX_WIDTH],
            remaining: 0,
            next_shapes: [0; MAX_WIDTH + 1],
            outgoing: [0; MAX_WIDTH],
            outgoing_count: 0,
            outgoing_domains: [0; MAX_WIDTH],
            outgoing_x: [0; MAX_WIDTH],
            permutation: [0; MAX_WIDTH],
            permutation_next: [0; MAX_WIDTH + 1],
            endpoint_limits: [0; MAX_WIDTH + 1],
            permutation_used: 0,
            permutation_depth: 0,
            root_anchor: None,
            target_cursor: 0,
            stage: FrameStage::Close,
        }
    }

    fn waiting_count(&self) -> usize {
        usize::from(self.open_base - self.incoming_count)
    }
}

/// A reversible DFS. All heap backing is reserved at construction; advancing
/// retains only the current row path and at most one partial record per piece.
pub(super) struct RowStackGeometrySearch {
    width: u8,
    height: u8,
    target_depth: u8,
    required_rows: Vec<u16>,
    targets: Vec<RowStackTarget>,
    shapes: Vec<RowShape>,
    pieces: Vec<PartialPiece>,
    open: Vec<u8>,
    frames: Vec<RowFrame>,
    used_counts: [u8; 7],
    // Starting a fixed profile commits all of its column cells, including
    // fragments on future rows. Inverse row gaps never change their x values.
    remaining_columns: [u8; MAX_WIDTH],
    expanded_nodes: usize,
    emitted_candidates: usize,
    root_partition_index: usize,
    root_partition_count: usize,
    next_root_residue: usize,
}

impl RowStackGeometrySearch {
    /// Includes the struct and every private vector's requested backing. No
    /// catalog, target position index, or physical-verifier owner is included.
    pub fn checked_retained_upper_bound(
        target_count: usize,
        piece_count: usize,
        height: usize,
    ) -> Option<u128> {
        if piece_count > MAX_PIECES || height > MAX_HEIGHT {
            return None;
        }
        (core::mem::size_of::<Self>() as u128)
            .checked_add(
                (target_count as u128)
                    .checked_mul(core::mem::size_of::<RowStackTarget>() as u128)?,
            )?
            .checked_add((piece_count as u128).checked_mul(
                (core::mem::size_of::<PartialPiece>() + core::mem::size_of::<u8>()) as u128,
            )?)?
            .checked_add((height as u128).checked_mul(
                (core::mem::size_of::<RowFrame>() + core::mem::size_of::<u16>()) as u128,
            )?)?
            .checked_add((DISTINCT_SHAPES * core::mem::size_of::<RowShape>()) as u128)
    }

    pub fn new<C: RowStackCatalog>(
        catalog: &C,
        targets: &[RowStackTarget],
    ) -> Result<Self, WasmExactSearchError> {
        let width = catalog.width();
        let height = catalog.height();
        let required = catalog.required_cells();
        let cells = u16::from(width) * u16::from(height);
        if width == 0
            || usize::from(width) > MAX_WIDTH
            || height == 0
            || usize::from(height) > MAX_HEIGHT
            || cells > 256
            || required.cells().any(|cell| cell >= cells)
            || !required.count_ones().is_multiple_of(4)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_row_stack_target_frame_unsupported",
            ));
        }
        let target_depth = (required.count_ones() / 4) as u8;
        if targets.iter().any(|target| {
            target.counts.iter().copied().map(u16::from).sum::<u16>() != u16::from(target_depth)
        }) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_row_stack_target_depth_mismatch",
            ));
        }

        let mut required_rows = reserved_vec(usize::from(height))?;
        let mut remaining_columns = [0; MAX_WIDTH];
        for row in 0..height {
            let bits = required.row_bits(width, row);
            required_rows.push(bits);
            for (x, remaining) in remaining_columns[..usize::from(width)]
                .iter_mut()
                .enumerate()
            {
                *remaining += u8::from(bits & (1_u16 << x) != 0);
            }
        }
        let mut owned_targets = reserved_vec(targets.len())?;
        owned_targets.extend_from_slice(targets);
        let shapes = compile_shapes()?;
        let pieces = reserved_vec(usize::from(target_depth))?;
        let open = reserved_vec(usize::from(target_depth))?;
        let mut frames = reserved_vec(usize::from(height))?;
        if !targets.is_empty() {
            frames.push(RowFrame::new(0, 0, 0));
        }
        Ok(Self {
            width,
            height,
            target_depth,
            required_rows,
            targets: owned_targets,
            shapes,
            pieces,
            open,
            frames,
            used_counts: [0; 7],
            remaining_columns,
            expanded_nodes: 0,
            emitted_candidates: 0,
            root_partition_index: 0,
            root_partition_count: 1,
            next_root_residue: 0,
        })
    }

    /// Split after the first two canonical piece starts, or an earlier filled
    /// first row. A worker repeats only this small prefix, not every first-row
    /// fill and outgoing permutation. All descendants of a claimed prefix
    /// belong to that worker until backtracking removes an anchor piece.
    pub fn set_root_partition(&mut self, index: usize, count: usize) -> Result<(), &'static str> {
        if count == 0 || index >= count {
            return Err("wasm_row_stack_partition_invalid");
        }
        if self.expanded_nodes != 0 {
            return Err("wasm_row_stack_partition_already_started");
        }
        self.root_partition_index = index;
        self.root_partition_count = count;
        self.next_root_residue = 0;
        Ok(())
    }

    pub fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self.required_rows.capacity() * core::mem::size_of::<u16>()
            + self.targets.capacity() * core::mem::size_of::<RowStackTarget>()
            + self.shapes.capacity() * core::mem::size_of::<RowShape>()
            + self.pieces.capacity() * core::mem::size_of::<PartialPiece>()
            + self.open.capacity() * core::mem::size_of::<u8>()
            + self.frames.capacity() * core::mem::size_of::<RowFrame>()
    }

    pub const fn expanded_nodes(&self) -> usize {
        self.expanded_nodes
    }

    pub const fn emitted_candidates(&self) -> usize {
        self.emitted_candidates
    }

    /// Each quantum bounds resumable DFS actions, including failed choices.
    /// The optional shared budget is debited before the action it authorizes.
    /// A caller without a control object can poll cancellation between quanta.
    pub fn advance<C: RowStackCatalog>(
        &mut self,
        catalog: &C,
        work_budget: usize,
        control: Option<&ExecutionControl>,
        node_budget: Option<&SharedExecutionNodeBudget>,
    ) -> RowStackGeometryAdvance {
        for _ in 0..work_budget.max(1) {
            if control.is_some_and(ExecutionControl::is_cancelled) {
                return RowStackGeometryAdvance::Cancelled;
            }
            if self.frames.is_empty() {
                return RowStackGeometryAdvance::Complete;
            }
            if node_budget.is_some_and(|budget| !budget.try_consume()) {
                return RowStackGeometryAdvance::NodeBudgetExceeded;
            }
            self.expanded_nodes = self.expanded_nodes.saturating_add(1);
            if let Some(candidate) = self.advance_one(catalog) {
                self.emitted_candidates = self.emitted_candidates.saturating_add(1);
                return RowStackGeometryAdvance::Candidate(candidate);
            }
        }
        if self.frames.is_empty() {
            RowStackGeometryAdvance::Complete
        } else {
            RowStackGeometryAdvance::Pending
        }
    }

    fn advance_one<C: RowStackCatalog>(&mut self, catalog: &C) -> Option<RowStackCandidate> {
        // Taking the top frame keeps the reversible state in one owner and
        // avoids allocating child tasks or retaining sibling snapshots.
        let mut frame = self.frames.pop().expect("a live row frame");
        match frame.stage {
            FrameStage::Close => {
                if usize::from(frame.next_close) > usize::from(frame.open_base).min(MAX_WIDTH) {
                    return None;
                }
                let close = frame.next_close;
                frame.next_close += 1;
                if self.close_incoming(catalog, &mut frame, close) {
                    frame.stage = FrameStage::Fill;
                }
            }
            FrameStage::Fill => {
                let depth = self.pieces.len() - usize::from(frame.piece_base);
                if frame.row == 0
                    && frame.root_anchor.is_none()
                    && (depth >= ROOT_PARTITION_PIECES || frame.remaining == 0)
                {
                    // Decide ownership before completing the rest of the row
                    // or exploring its outgoing permutations. Empty and short
                    // first rows still have exactly one owner per prefix.
                    if !self.owns_next_root() {
                        self.backtrack_fill(&mut frame);
                        self.frames.push(frame);
                        return None;
                    }
                    frame.root_anchor = Some(depth as u8);
                }
                if frame.remaining == 0 {
                    if self.prepare_outgoing(&mut frame) {
                        frame.stage = FrameStage::Permute;
                    } else {
                        self.backtrack_fill(&mut frame);
                    }
                } else {
                    let next = usize::from(frame.next_shapes[depth]);
                    if next == self.shapes.len() {
                        self.backtrack_fill(&mut frame);
                    } else {
                        frame.next_shapes[depth] += 1;
                        if self.start_piece(catalog, &mut frame, next) {
                            frame.next_shapes[depth + 1] = 0;
                        }
                    }
                }
            }
            FrameStage::Permute => {
                let depth = usize::from(frame.permutation_depth);
                let count = usize::from(frame.outgoing_count);
                if depth == count {
                    frame.stage = FrameStage::Descend;
                } else if usize::from(frame.permutation_next[depth]) == count {
                    if depth == 0 {
                        self.backtrack_fill(&mut frame);
                    } else {
                        backtrack_permutation(&mut frame);
                    }
                } else {
                    let choice = frame.permutation_next[depth];
                    frame.permutation_next[depth] += 1;
                    if frame.permutation_used & (1_u16 << choice) == 0 {
                        let mut limit = frame.endpoint_limits[depth];
                        if depth != 0
                            && frame.outgoing_x[usize::from(frame.permutation[depth - 1])]
                                > frame.outgoing_x[usize::from(choice)]
                        {
                            // Equal endpoints push left to right. A descent
                            // in source x therefore needs a strictly earlier
                            // endpoint, not another parallel-arc embedding.
                            limit = limit.saturating_sub(1);
                        }
                        let domain = frame.outgoing_domains[usize::from(choice)]
                            & ((1_u32 << (u32::from(limit) + 1)) - 1);
                        if domain != 0 {
                            frame.permutation[depth] = choice;
                            frame.permutation_used |= 1_u16 << choice;
                            frame.permutation_depth += 1;
                            frame.permutation_next[depth + 1] = 0;
                            frame.endpoint_limits[depth + 1] =
                                (u32::BITS - 1 - domain.leading_zeros()) as u8;
                        }
                    }
                }
            }
            FrameStage::Descend => {
                frame.stage = FrameStage::AfterChild;
                for &index in &frame.permutation[..usize::from(frame.outgoing_count)] {
                    self.open.push(frame.outgoing[usize::from(index)]);
                }
                if frame.row + 1 < self.height {
                    let child = RowFrame::new(frame.row + 1, self.pieces.len(), self.open.len());
                    self.frames.push(frame);
                    self.frames.push(child);
                    return None;
                }
                if self.open.is_empty() && self.pieces.len() == usize::from(self.target_depth) {
                    debug_assert!(self
                        .remaining_columns
                        .iter()
                        .all(|&remaining| remaining == 0));
                    frame.stage = FrameStage::Emit;
                    frame.target_cursor = 0;
                }
            }
            FrameStage::AfterChild => {
                self.open.truncate(frame.waiting_count());
                if frame.outgoing_count == 0 {
                    self.backtrack_fill(&mut frame);
                } else {
                    backtrack_permutation(&mut frame);
                    frame.stage = FrameStage::Permute;
                }
            }
            FrameStage::Emit => {
                if frame.target_cursor == self.targets.len() {
                    frame.stage = FrameStage::AfterChild;
                } else {
                    let target = self.targets[frame.target_cursor];
                    frame.target_cursor += 1;
                    if target.counts == self.used_counts {
                        let mut candidate = RowStackCandidate {
                            rows: [0; MAX_PIECES],
                            row_count: self.pieces.len() as u8,
                            target_index: target.target_index,
                        };
                        for (row, piece) in candidate.rows.iter_mut().zip(&self.pieces) {
                            debug_assert_ne!(piece.row_id, NO_ROW);
                            *row = piece.row_id;
                        }
                        candidate.rows[..self.pieces.len()].sort_unstable();
                        self.frames.push(frame);
                        return Some(candidate);
                    }
                }
            }
        }
        self.frames.push(frame);
        None
    }

    fn close_incoming<C: RowStackCatalog>(
        &mut self,
        catalog: &C,
        frame: &mut RowFrame,
        close: u8,
    ) -> bool {
        let remaining_rows = self.height - frame.row - 1;
        let waiting = self.open.len() - usize::from(close);
        for &id in &self.open[..waiting] {
            let piece = self.pieces[usize::from(id)];
            if self.shapes[usize::from(piece.shape)].height - piece.progress > remaining_rows {
                return false;
            }
        }
        let mut occupied = 0_u16;
        let mut row_ids = [NO_ROW; MAX_WIDTH];
        let mut previous_start = None;
        for (offset, &id) in self.open[waiting..].iter().rev().enumerate() {
            let piece = self.pieces[usize::from(id)];
            let shape = self.shapes[usize::from(piece.shape)];
            let source_row = piece.occupied_rows[usize::from(piece.progress - 1)];
            let source_x =
                (shape.rows[usize::from(piece.progress - 1)] << piece.x).trailing_zeros();
            // Equal start/end arcs have one embedding. Longer arcs naturally
            // remain deeper in the stack; parallel arcs pop right to left.
            if previous_start.is_some_and(|(row, x)| row == source_row && x <= source_x) {
                return false;
            }
            previous_start = Some((source_row, source_x));
            let fragment = shape.rows[usize::from(piece.progress)] << piece.x;
            if fragment & occupied != 0
                || fragment & !self.required_rows[usize::from(frame.row)] != 0
                || shape.height - piece.progress - 1 > remaining_rows
            {
                return false;
            }
            occupied |= fragment;
            frame.incoming[offset] = id;
            if piece.progress + 1 == shape.height {
                let cells = piece
                    .cells
                    .union(row_cells(self.width, frame.row, fragment));
                let Some(row_id) = catalog.skeleton_id(shape.piece, cells) else {
                    return false;
                };
                row_ids[offset] = row_id;
            }
        }
        // Validation is complete before any reversible state is changed.
        for (offset, &id) in frame.incoming[..usize::from(close)].iter().enumerate() {
            let piece = &mut self.pieces[usize::from(id)];
            let shape = self.shapes[usize::from(piece.shape)];
            let fragment = shape.rows[usize::from(piece.progress)] << piece.x;
            piece.cells = piece
                .cells
                .union(row_cells(self.width, frame.row, fragment));
            piece.occupied_rows[usize::from(piece.progress)] = frame.row;
            piece.progress += 1;
            piece.row_id = row_ids[offset];
        }
        self.open.truncate(waiting);
        frame.incoming_count = close;
        frame.remaining = self.required_rows[usize::from(frame.row)] & !occupied;
        frame.next_shapes[0] = 0;
        true
    }

    fn start_piece<C: RowStackCatalog>(
        &mut self,
        catalog: &C,
        frame: &mut RowFrame,
        shape_id: usize,
    ) -> bool {
        if self.pieces.len() == usize::from(self.target_depth) {
            return false;
        }
        let shape = self.shapes[shape_id];
        if shape.height > self.height - frame.row {
            return false;
        }
        // The leftmost uncovered cell fixes the next piece and its x offset.
        // Starting the same row fragments in another order is not a branch.
        let Some(x) = frame
            .remaining
            .trailing_zeros()
            .checked_sub(shape.rows[0].trailing_zeros())
        else {
            return false;
        };
        let x = x as u8;
        if x + shape.width > self.width {
            return false;
        }
        let fragment = shape.rows[0] << x;
        if fragment & !frame.remaining != 0 {
            return false;
        }
        if shape.columns[..usize::from(shape.width)]
            .iter()
            .enumerate()
            .any(|(local_x, &needed)| self.remaining_columns[usize::from(x) + local_x] < needed)
        {
            return false;
        }
        let mut next_counts = self.used_counts;
        next_counts[piece_index(shape.piece)] += 1;
        if !self.targets.iter().any(|target| {
            next_counts
                .iter()
                .zip(target.counts)
                .all(|(&used, allowed)| used <= allowed)
        }) {
            return false;
        }
        let cells = row_cells(self.width, frame.row, fragment);
        let row_id = if shape.height == 1 {
            let Some(row_id) = catalog.skeleton_id(shape.piece, cells) else {
                return false;
            };
            row_id
        } else {
            NO_ROW
        };
        self.pieces.push(PartialPiece {
            cells,
            row_id,
            occupied_rows: [frame.row, 0, 0, 0],
            shape: shape_id as u8,
            x,
            progress: 1,
        });
        self.used_counts = next_counts;
        for (local_x, &needed) in shape.columns[..usize::from(shape.width)].iter().enumerate() {
            self.remaining_columns[usize::from(x) + local_x] -= needed;
        }
        frame.remaining &= !fragment;
        true
    }

    fn prepare_outgoing(&self, frame: &mut RowFrame) -> bool {
        let mut len = 0;
        for id in frame.incoming[..usize::from(frame.incoming_count)]
            .iter()
            .copied()
            .chain((usize::from(frame.piece_base)..self.pieces.len()).map(|id| id as u8))
        {
            let piece = self.pieces[usize::from(id)];
            if piece.progress < self.shapes[usize::from(piece.shape)].height {
                // Each outgoing piece owns at least one distinct current-row
                // cell, so there are at most width outgoing arcs.
                frame.outgoing[len] = id;
                frame.outgoing_domains[len] = self.next_row_domain(piece, frame.row);
                if frame.outgoing_domains[len] == 0 {
                    return false;
                }
                frame.outgoing_x[len] = (self.shapes[usize::from(piece.shape)].rows
                    [usize::from(piece.progress - 1)]
                    << piece.x)
                    .trailing_zeros() as u8;
                len += 1;
            }
        }
        frame.outgoing_count = len as u8;
        frame.permutation_used = 0;
        frame.permutation_depth = 0;
        frame.permutation_next[0] = 0;
        frame.endpoint_limits[0] = self.height - 1;
        true
    }

    /// Possible endpoints of this piece's next arc, allowing all other pieces
    /// to be absent. Placing its later fragments as high as possible leaves
    /// exactly the largest interval in which the next fragment can occur.
    /// This is a necessary local condition, not an actual row assignment.
    fn next_row_domain(&self, piece: PartialPiece, row: u8) -> u32 {
        let shape = self.shapes[usize::from(piece.shape)];
        let mut end = self.height;
        for local in (piece.progress + 1..shape.height).rev() {
            let fragment = shape.rows[usize::from(local)] << piece.x;
            let Some(found) = (row + 1..end)
                .rev()
                .find(|&next| fragment & !self.required_rows[usize::from(next)] == 0)
            else {
                return 0;
            };
            end = found;
        }
        let fragment = shape.rows[usize::from(piece.progress)] << piece.x;
        let mut domain = 0;
        for next in row + 1..end {
            if fragment & !self.required_rows[usize::from(next)] == 0 {
                domain |= 1_u32 << next;
            }
        }
        domain
    }

    fn backtrack_fill(&mut self, frame: &mut RowFrame) {
        if self.pieces.len() > usize::from(frame.piece_base) {
            let piece = self.pieces.pop().expect("a piece started on this row");
            let shape = self.shapes[usize::from(piece.shape)];
            debug_assert_eq!(piece.progress, 1);
            self.used_counts[piece_index(shape.piece)] -= 1;
            for (local_x, &needed) in shape.columns[..usize::from(shape.width)].iter().enumerate() {
                self.remaining_columns[usize::from(piece.x) + local_x] += needed;
            }
            frame.remaining |= shape.rows[0] << piece.x;
            if frame.root_anchor.is_some_and(|anchor| {
                self.pieces.len() - usize::from(frame.piece_base) < usize::from(anchor)
            }) {
                frame.root_anchor = None;
            }
            frame.stage = FrameStage::Fill;
        } else {
            for &id in frame.incoming[..usize::from(frame.incoming_count)]
                .iter()
                .rev()
            {
                let piece = &mut self.pieces[usize::from(id)];
                piece.progress -= 1;
                let fragment = self.shapes[usize::from(piece.shape)].rows
                    [usize::from(piece.progress)]
                    << piece.x;
                piece.cells = piece
                    .cells
                    .without(row_cells(self.width, frame.row, fragment));
                piece.occupied_rows[usize::from(piece.progress)] = 0;
                piece.row_id = NO_ROW;
                self.open.push(id);
            }
            frame.incoming_count = 0;
            frame.root_anchor = None;
            frame.stage = FrameStage::Close;
        }
    }

    fn owns_next_root(&mut self) -> bool {
        let owned = self.next_root_residue == self.root_partition_index;
        self.next_root_residue = if self.next_root_residue + 1 == self.root_partition_count {
            0
        } else {
            self.next_root_residue + 1
        };
        owned
    }
}

fn reserved_vec<T>(capacity: usize) -> Result<Vec<T>, WasmExactSearchError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_row_stack_storage_unavailable"))?;
    Ok(values)
}

fn compile_shapes() -> Result<Vec<RowShape>, WasmExactSearchError> {
    let mut shapes = reserved_vec(DISTINCT_SHAPES)?;
    let registry = standard_tetromino_registry();
    for piece in PieceKind::STANDARD_TETROMINOES {
        let definition = registry
            .get(piece)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "standard_piece_missing",
            ))?;
        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            let mut rows = [0_u16; 4];
            let mut columns = [0_u8; 4];
            for cell in shape.cells() {
                rows[cell.y() as usize] |= 1_u16 << cell.x();
                columns[cell.x() as usize] += 1;
            }
            let shape = RowShape {
                piece,
                rows,
                columns,
                width: shape.width(),
                height: shape.height(),
            };
            if !shapes.contains(&shape) {
                shapes.push(shape);
            }
        }
    }
    debug_assert_eq!(shapes.len(), DISTINCT_SHAPES);
    Ok(shapes)
}

fn row_cells(width: u8, row: u8, mut fragment: u16) -> ExtendedBoard {
    let mut cells = ExtendedBoard::EMPTY;
    while fragment != 0 {
        let x = fragment.trailing_zeros() as u16;
        cells.insert(u16::from(row) * u16::from(width) + x);
        fragment &= fragment - 1;
    }
    cells
}

fn backtrack_permutation(frame: &mut RowFrame) {
    frame.permutation_depth -= 1;
    frame.permutation_used &= !(1_u16 << frame.permutation[usize::from(frame.permutation_depth)]);
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    type CandidateKey = (u32, Vec<u32>);

    fn all_targets(depth: u8) -> Vec<RowStackTarget> {
        fn visit(remaining: u8, index: usize, counts: &mut [u8; 7], out: &mut Vec<RowStackTarget>) {
            if index == 6 {
                counts[index] = remaining;
                out.push(RowStackTarget {
                    counts: *counts,
                    target_index: out.len() as u32,
                });
                return;
            }
            for count in 0..=remaining {
                counts[index] = count;
                visit(remaining - count, index + 1, counts, out);
            }
        }
        let mut targets = Vec::new();
        visit(depth, 0, &mut [0; 7], &mut targets);
        targets
    }

    fn collect(
        catalog: &GeometryCatalog,
        targets: &[RowStackTarget],
        quantum: usize,
        partition: Option<(usize, usize)>,
    ) -> (BTreeSet<CandidateKey>, usize) {
        let mut search = RowStackGeometrySearch::new(catalog, targets).unwrap();
        if let Some((index, count)) = partition {
            search.set_root_partition(index, count).unwrap();
        }
        let retained = search.retained_bytes();
        let initial_columns = search.remaining_columns;
        let bound = RowStackGeometrySearch::checked_retained_upper_bound(
            targets.len(),
            (catalog.required_cells().count_ones() / 4) as usize,
            usize::from(catalog.height()),
        )
        .unwrap();
        assert!(retained as u128 <= bound);
        let mut candidates = BTreeSet::new();
        loop {
            assert!(
                search.expanded_nodes() < 2_000_000,
                "small fixture exceeded its finite bound"
            );
            match search.advance(catalog, quantum, None, None) {
                RowStackGeometryAdvance::Pending => {}
                RowStackGeometryAdvance::Candidate(candidate) => {
                    assert!(
                        candidates.insert((candidate.target_index, candidate.row_ids().to_vec())),
                        "duplicate concrete tiling"
                    );
                }
                RowStackGeometryAdvance::Complete => break,
                _ => panic!("unbounded, uncancelled fixture was interrupted"),
            }
            assert_eq!(
                search.retained_bytes(),
                retained,
                "advancing allocated backing"
            );
        }
        assert_eq!(search.emitted_candidates(), candidates.len());
        assert_eq!(
            search.remaining_columns, initial_columns,
            "column budget was not restored"
        );
        assert_eq!(search.used_counts, [0; 7]);
        assert!(search.pieces.is_empty());
        (candidates, search.expanded_nodes())
    }

    fn reference(catalog: &GeometryCatalog, targets: &[RowStackTarget]) -> BTreeSet<CandidateKey> {
        fn visit(
            catalog: &GeometryCatalog,
            targets: &[RowStackTarget],
            remaining: u64,
            chosen: &mut Vec<u32>,
            used_counts: &mut [u8; 7],
            out: &mut BTreeSet<CandidateKey>,
        ) {
            if remaining == 0 {
                if !noncrossing(catalog, chosen) {
                    return;
                }
                let mut counts = [0; 7];
                for &id in chosen.iter() {
                    counts[piece_index(catalog.skeleton(id).piece)] += 1;
                }
                let mut rows = chosen.clone();
                rows.sort_unstable();
                for target in targets.iter().filter(|target| target.counts == counts) {
                    out.insert((target.target_index, rows.clone()));
                }
                return;
            }
            for &id in catalog.support(remaining.trailing_zeros() as u8) {
                let row = catalog.skeleton(id);
                if row.cells & !remaining == 0 {
                    used_counts[piece_index(row.piece)] += 1;
                    if targets.iter().any(|target| {
                        used_counts
                            .iter()
                            .zip(target.counts)
                            .all(|(&used, allowed)| used <= allowed)
                    }) {
                        chosen.push(id);
                        visit(
                            catalog,
                            targets,
                            remaining & !row.cells,
                            chosen,
                            used_counts,
                            out,
                        );
                        chosen.pop();
                    }
                    used_counts[piece_index(row.piece)] -= 1;
                }
            }
        }
        let mut out = BTreeSet::new();
        visit(
            catalog,
            targets,
            catalog.required_cells(),
            &mut Vec::new(),
            &mut [0; 7],
            &mut out,
        );
        out
    }

    fn noncrossing(catalog: &GeometryCatalog, chosen: &[u32]) -> bool {
        let mut arcs = Vec::new();
        for &id in chosen {
            let occupied = catalog.skeleton_occupied_rows(id);
            let rows = (0..catalog.height())
                .filter(|row| occupied & (1_u16 << row) != 0)
                .collect::<Vec<_>>();
            arcs.extend(rows.windows(2).map(|rows| (rows[0], rows[1])));
        }
        arcs.iter()
            .all(|&(a, b)| arcs.iter().all(|&(c, d)| !(a < c && c < b && b < d)))
    }

    fn compact(width: u8, height: u8, required: u64) -> GeometryCatalog {
        let full = (1_u64 << (u32::from(width) * u32::from(height))) - 1;
        GeometryCatalog::compile_for_required_cells_on_dimensions(
            width,
            height,
            full & !required,
            required,
        )
        .unwrap()
    }

    #[test]
    fn row_stack_matches_independent_exact_cover_filtered_by_noncrossing() {
        // Arbitrary bases, shared endpoints, and three/four local-row pieces.
        for (width, height, required) in [(4, 2, 0xff), (4, 3, 0xfff), (2, 4, 0xff), (4, 3, 0xccf)]
        {
            let catalog = compact(width, height, required);
            let targets = all_targets((required.count_ones() / 4) as u8);
            let expected = reference(&catalog, &targets);
            assert!(!expected.is_empty());
            assert_eq!(collect(&catalog, &targets, 37, None).0, expected);
        }
    }

    #[test]
    fn row_stack_rejects_crossing_gaps_but_keeps_nested_and_shared_endpoints() {
        let catalog = compact(2, 4, 0xff);
        let targets = [RowStackTarget {
            counts: [0, 2, 0, 0, 0, 0, 0],
            target_index: 19,
        }];
        let candidates = collect(&catalog, &targets, 1, None).0;
        // Two O pieces: adjacent pairs and nested pairs remain; crossing pairs
        // (0,2)/(1,3) are impossible before physical verification begins.
        assert_eq!(candidates.len(), 2);
        for (_, rows) in candidates {
            assert!(noncrossing(&catalog, &rows));
        }

        let catalog = compact(4, 2, 0xff);
        assert_eq!(collect(&catalog, &targets, 17, None).0.len(), 1);

        // The left O closes at row 1 before the right O starts at row 1.
        let catalog = compact(4, 3, 0xcf3);
        assert_eq!(
            collect(&catalog, &targets, 17, None).0,
            reference(&catalog, &targets)
        );
        assert!(!reference(&catalog, &targets).is_empty());
    }

    #[test]
    fn row_stack_quanta_and_root_partitions_preserve_each_candidate_once() {
        let catalog = compact(4, 3, 0xfff);
        let targets = all_targets(3);
        let (whole, nodes) = collect(&catalog, &targets, 1, None);
        assert_eq!(
            collect(&catalog, &targets, 4096, None),
            (whole.clone(), nodes)
        );
        let mut merged = BTreeSet::new();
        for index in 0..3 {
            let partition = collect(&catalog, &targets, 13, Some((index, 3))).0;
            for candidate in partition {
                assert!(merged.insert(candidate), "partitions overlap");
            }
        }
        assert_eq!(merged, whole);
    }

    #[test]
    fn row_stack_partitions_cover_empty_and_single_piece_first_rows() {
        let fixtures = [
            // No target cells in row zero: its empty prefix has one owner.
            (4, 3, 0xff0, all_targets(2)),
            // A single O fragment fills row zero; an I starts on row two.
            (
                4,
                3,
                0xf33,
                vec![RowStackTarget {
                    counts: [1, 1, 0, 0, 0, 0, 0],
                    target_index: 3,
                }],
            ),
            // The first row is also the final row.
            (
                4,
                1,
                0xf,
                vec![RowStackTarget {
                    counts: [1, 0, 0, 0, 0, 0, 0],
                    target_index: 5,
                }],
            ),
        ];
        for (width, height, required, targets) in fixtures {
            let catalog = compact(width, height, required);
            let whole = collect(&catalog, &targets, 1, None).0;
            assert!(!whole.is_empty());
            assert_eq!(whole, reference(&catalog, &targets));
            let mut merged = BTreeSet::new();
            for index in 0..4 {
                let partition = collect(&catalog, &targets, 7, Some((index, 4))).0;
                if index != 0 {
                    assert!(partition.is_empty(), "a short root prefix has one owner");
                }
                for candidate in partition {
                    assert!(merged.insert(candidate), "partitions overlap");
                }
            }
            assert_eq!(merged, whole);
        }
    }

    #[test]
    fn row_stack_column_budget_rejects_excess_future_cells_and_restores_on_backtrack() {
        // A T at the left, a horizontal I at the right, and an O above it.
        // The first fragment of a vertical I fits column zero, but its four
        // committed cells exceed that column's one target cell.
        let required =
            0b111_u64 | (0b1111_u64 << 6) | (1_u64 << 11) | (0b11_u64 << 17) | (0b11_u64 << 27);
        let catalog = compact(10, 4, required);
        let targets = [RowStackTarget {
            counts: [1, 1, 1, 0, 0, 0, 0],
            target_index: 11,
        }];
        let mut search = RowStackGeometrySearch::new(&catalog, &targets).unwrap();
        let columns = search.remaining_columns;
        let vertical_i = search
            .shapes
            .iter()
            .position(|shape| shape.piece == PieceKind::I && shape.width == 1 && shape.height == 4)
            .unwrap();
        let mut frame = RowFrame::new(0, 0, 0);
        frame.remaining = search.required_rows[0];
        assert_eq!(columns[0], 1);
        assert!(frame.remaining & 1 != 0);
        assert!(!search.start_piece(&catalog, &mut frame, vertical_i));
        assert_eq!(search.remaining_columns, columns);
        assert_eq!(frame.remaining, search.required_rows[0]);
        assert!(search.pieces.is_empty());

        let expected = reference(&catalog, &targets);
        assert!(!expected.is_empty());
        assert_eq!(collect(&catalog, &targets, 1, None).0, expected);
        let mut merged = BTreeSet::new();
        for index in 0..4 {
            for candidate in collect(&catalog, &targets, 17, Some((index, 4))).0 {
                assert!(merged.insert(candidate), "partitions overlap");
            }
        }
        assert_eq!(merged, expected);
    }

    #[test]
    fn row_stack_equal_deadlines_do_not_enumerate_factorial_root_orders() {
        // Ten vertical I pieces can all start on row zero. Their next row is
        // forced to row one, leaving one canonical outgoing order rather than
        // 10! orders that fail only when the next row is entered. Horizontal
        // I pieces remain in the same real catalog and must not be lost.
        let catalog = compact(10, 4, (1_u64 << 40) - 1);
        let targets = [RowStackTarget {
            counts: [10, 0, 0, 0, 0, 0, 0],
            target_index: 0,
        }];
        let expected = reference(&catalog, &targets);
        assert_eq!(expected.len(), 14);
        let (actual, nodes) = collect(&catalog, &targets, 31, None);
        assert_eq!(actual, expected);
        assert!(
            nodes < 200_000,
            "same-deadline permutations were not pruned: {nodes}"
        );
        let mut merged = BTreeSet::new();
        let mut partition_nodes = 0;
        for index in 0..4 {
            let (candidates, visited) = collect(&catalog, &targets, 29, Some((index, 4)));
            partition_nodes += visited;
            for candidate in candidates {
                assert!(merged.insert(candidate), "partitions overlap");
            }
        }
        assert_eq!(merged, expected);
        assert!(
            partition_nodes < nodes + 10_000,
            "partitions repeated first-row permutation work: serial={nodes}, total={partition_nodes}"
        );
    }

    #[test]
    fn row_stack_cancellation_and_node_debit_preserve_resumable_state() {
        let catalog = compact(4, 2, 0xff);
        let targets = all_targets(2);
        let mut search = RowStackGeometrySearch::new(&catalog, &targets).unwrap();
        let control = ExecutionControl::default();
        control.cancellation.handle().cancel();
        assert!(matches!(
            search.advance(&catalog, 64, Some(&control), None),
            RowStackGeometryAdvance::Cancelled
        ));
        assert_eq!(search.expanded_nodes(), 0);
        let budget = SharedExecutionNodeBudget::new(1);
        assert!(matches!(
            search.advance(&catalog, 64, None, Some(&budget)),
            RowStackGeometryAdvance::NodeBudgetExceeded
        ));
        assert_eq!(search.expanded_nodes(), 1);
        assert_eq!(budget.consumed(), 1);
        assert!(search.set_root_partition(0, 2).is_err());
        let mut actual = BTreeSet::new();
        loop {
            match search.advance(&catalog, 101, None, None) {
                RowStackGeometryAdvance::Candidate(candidate) => {
                    assert!(actual.insert((candidate.target_index, candidate.row_ids().to_vec())));
                }
                RowStackGeometryAdvance::Pending => {}
                RowStackGeometryAdvance::Complete => break,
                _ => panic!("resumed fixture was interrupted"),
            }
        }
        assert_eq!(actual, reference(&catalog, &targets));
    }

    #[test]
    fn row_stack_extended_mapping_preserves_cells_across_word_boundaries() {
        use clearra_core_domain::board::standard_pc_board::Board256Mask;
        use clearra_problem::BuildProbabilityField;

        let mut required = Board256Mask::EMPTY;
        // This O occupies bits 63, 64, 73, and 74 in the original row frame.
        for cell in [63, 64, 73, 74] {
            required = required.union(Board256Mask::singleton(cell).unwrap());
        }
        let initial = Board256Mask::all_cells(80).unwrap().without(required);
        let field = BuildProbabilityField::from_words_preserving_height(
            8,
            initial.words(),
            required.words(),
        )
        .unwrap();
        let catalog = ExtendedInverseCatalog::compile(field).unwrap();
        let targets = [RowStackTarget {
            counts: [0, 1, 0, 0, 0, 0, 0],
            target_index: 7,
        }];
        let mut search = RowStackGeometrySearch::new(&catalog, &targets).unwrap();
        let mut emitted = 0;
        loop {
            match search.advance(&catalog, 7, None, None) {
                RowStackGeometryAdvance::Candidate(candidate) => {
                    emitted += 1;
                    assert_eq!(candidate.target_index, 7);
                    assert_eq!(candidate.row_ids().len(), 1);
                    let row = catalog.skeleton(candidate.row_ids()[0]);
                    assert_eq!(row.piece, PieceKind::O);
                    assert_eq!(row.cells.words(), required.words());
                    assert!(catalog.instantiations(candidate.row_ids()[0], 0).count() > 0);
                }
                RowStackGeometryAdvance::Pending => {}
                RowStackGeometryAdvance::Complete => break,
                _ => panic!("small extended fixture was interrupted"),
            }
        }
        assert_eq!(emitted, 1);
    }
}
