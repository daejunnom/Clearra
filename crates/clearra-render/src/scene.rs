use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_replay::ReplayTrace;

use crate::RenderError;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RenderTile {
    Empty,
    InitialGray,
    Piece(PieceKind),
}

impl RenderTile {
    pub const fn atlas_key(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::InitialGray => "initial_gray",
            Self::Piece(PieceKind::I) => "I",
            Self::Piece(PieceKind::O) => "O",
            Self::Piece(PieceKind::T) => "T",
            Self::Piece(PieceKind::S) => "S",
            Self::Piece(PieceKind::Z) => "Z",
            Self::Piece(PieceKind::J) => "J",
            Self::Piece(PieceKind::L) => "L",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderFramePhase {
    Initial,
    Lock,
    AfterClear,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderSceneFrame {
    width: u16,
    height: u16,
    phase: RenderFramePhase,
    step_index: Option<usize>,
    cleared_row_mask: u16,
    cells: Vec<RenderTile>,
}

impl RenderSceneFrame {
    fn new(
        width: u16,
        height: u16,
        phase: RenderFramePhase,
        step_index: Option<usize>,
        cleared_row_mask: u16,
        occupied: u64,
        owners: &[RenderTile],
    ) -> Self {
        let mut cells = vec![RenderTile::Empty; owners.len()];
        for (index, owner) in owners.iter().copied().enumerate() {
            if occupied & (1_u64 << index) != 0 {
                cells[index] = owner;
            }
        }
        Self {
            width,
            height,
            phase,
            step_index,
            cleared_row_mask,
            cells,
        }
    }

    pub const fn width(&self) -> u16 {
        self.width
    }

    pub const fn height(&self) -> u16 {
        self.height
    }

    pub const fn phase(&self) -> RenderFramePhase {
        self.phase
    }

    pub const fn step_index(&self) -> Option<usize> {
        self.step_index
    }

    pub const fn cleared_row_mask(&self) -> u16 {
        self.cleared_row_mask
    }

    pub fn tile_at_bottom_up(&self, x: u16, y: u16) -> Option<RenderTile> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells
            .get(usize::from(y) * usize::from(self.width) + usize::from(x))
            .copied()
    }

    pub fn tile_at_top_down(&self, x: u16, y: u16) -> Option<RenderTile> {
        if y >= self.height {
            return None;
        }
        self.tile_at_bottom_up(x, self.height - 1 - y)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderScene {
    frames: Vec<RenderSceneFrame>,
}

impl RenderScene {
    pub fn from_replay_trace(trace: &ReplayTrace) -> Result<Self, RenderError> {
        let steps = trace.solution_trace().steps();
        let first = steps.first().ok_or(RenderError::EmptyTimeline)?;
        let layout = first.board_before().layout();
        let width = layout.width();
        let height = layout.height();
        let cell_count = usize::from(layout.cell_count());
        let initial_occupied = first.board_before().occupied();
        let mut owners = vec![RenderTile::Empty; cell_count];

        for (index, owner) in owners.iter_mut().enumerate() {
            if initial_occupied & (1_u64 << index) != 0 {
                *owner = RenderTile::InitialGray;
            }
        }

        let mut frames = vec![RenderSceneFrame::new(
            width,
            height,
            RenderFramePhase::Initial,
            None,
            0,
            initial_occupied,
            &owners,
        )];

        for step in steps {
            if step.board_before().layout() != layout
                || step.board_after().after_placement().layout() != layout
                || step.board_after().after_line_clear().layout() != layout
            {
                return Err(RenderError::ReplayLayoutMismatch);
            }

            let piece = step.piece_decision().active_piece();
            for index in 0..cell_count {
                if step.placement().mask() & (1_u64 << index) != 0 {
                    owners[index] = RenderTile::Piece(piece);
                }
            }

            let occupied_after_place = step.board_after().after_placement().occupied();
            let cleared_row_mask = full_row_mask(width, height, occupied_after_place);
            frames.push(RenderSceneFrame::new(
                width,
                height,
                RenderFramePhase::Lock,
                Some(step.step_index()),
                cleared_row_mask,
                occupied_after_place,
                &owners,
            ));

            owners = compact_after_line_clear(width, height, &owners, cleared_row_mask);
            frames.push(RenderSceneFrame::new(
                width,
                height,
                RenderFramePhase::AfterClear,
                Some(step.step_index()),
                cleared_row_mask,
                step.board_after().after_line_clear().occupied(),
                &owners,
            ));
        }

        Ok(Self { frames })
    }

    pub fn frames(&self) -> &[RenderSceneFrame] {
        &self.frames
    }

    pub fn final_frame(&self) -> &RenderSceneFrame {
        self.frames
            .last()
            .expect("a render scene always has an initial frame")
    }
}

fn full_row_mask(width: u16, height: u16, occupied: u64) -> u16 {
    let width_usize = usize::from(width);
    let row_bits = (1_u64 << width_usize) - 1;
    let mut cleared = 0_u16;
    for y in 0..height {
        if occupied & (row_bits << (usize::from(y) * width_usize))
            == row_bits << (usize::from(y) * width_usize)
        {
            cleared |= 1_u16 << y;
        }
    }
    cleared
}

fn compact_after_line_clear(
    width: u16,
    height: u16,
    owners: &[RenderTile],
    cleared_row_mask: u16,
) -> Vec<RenderTile> {
    let width_usize = usize::from(width);
    let mut compacted = vec![RenderTile::Empty; owners.len()];
    let mut destination_y = 0_usize;
    for source_y in 0..usize::from(height) {
        if cleared_row_mask & (1_u16 << source_y) != 0 {
            continue;
        }
        let source = source_y * width_usize;
        let destination = destination_y * width_usize;
        compacted[destination..destination + width_usize]
            .copy_from_slice(&owners[source..source + width_usize]);
        destination_y += 1;
    }
    compacted
}
