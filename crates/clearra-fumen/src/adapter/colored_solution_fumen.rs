use clearra_core_domain::piece::piece_kind::PieceKind;
use fumen::{CellColor, Fumen, Page};

const FUMEN_WIDTH: u8 = 10;
const FUMEN_HEIGHT: u8 = 23;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColoredSolutionPlacement {
    piece: PieceKind,
    cells_mask: u64,
}

impl ColoredSolutionPlacement {
    pub const fn new(piece: PieceKind, cells_mask: u64) -> Self {
        Self { piece, cells_mask }
    }

    pub const fn piece(self) -> PieceKind {
        self.piece
    }

    pub const fn cells_mask(self) -> u64 {
        self.cells_mask
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColoredSolutionPage {
    width: u8,
    height: u8,
    initial_board_mask: u64,
    placements: Vec<ColoredSolutionPlacement>,
    comment: Option<String>,
}

impl ColoredSolutionPage {
    pub fn new(
        width: u8,
        height: u8,
        initial_board_mask: u64,
        placements: Vec<ColoredSolutionPlacement>,
    ) -> Result<Self, ColoredSolutionFumenError> {
        validate_page(width, height, initial_board_mask, &placements)?;
        Ok(Self {
            width,
            height,
            initial_board_mask,
            placements,
            comment: None,
        })
    }

    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    pub const fn width(&self) -> u8 {
        self.width
    }

    pub const fn height(&self) -> u8 {
        self.height
    }

    pub const fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask
    }

    pub fn placements(&self) -> &[ColoredSolutionPlacement] {
        &self.placements
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ColoredSolutionFumenExporter;

impl ColoredSolutionFumenExporter {
    pub fn encode(pages: &[ColoredSolutionPage]) -> Result<String, ColoredSolutionFumenError> {
        if pages.is_empty() {
            return Err(ColoredSolutionFumenError::EmptyDocument);
        }

        let expected_fields = pages
            .iter()
            .map(page_field)
            .collect::<Result<Vec<_>, _>>()?;
        let mut document = Fumen::default();
        for (source, field) in pages.iter().zip(expected_fields.iter().copied()) {
            document.pages.push(Page {
                field,
                comment: source.comment.clone(),
                ..Page::default()
            });
        }

        let encoded = document.encode();
        let decoded = Fumen::decode(&encoded)
            .map_err(|_| ColoredSolutionFumenError::RoundTripDecodeFailed)?;
        if decoded.pages.len() != expected_fields.len()
            || decoded
                .pages
                .iter()
                .zip(expected_fields)
                .any(|(page, expected)| page.field != expected)
        {
            return Err(ColoredSolutionFumenError::RoundTripFieldMismatch);
        }
        Ok(encoded)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColoredSolutionFumenError {
    EmptyDocument,
    UnsupportedWidth { width: u8 },
    UnsupportedHeight { height: u8 },
    BoardMaskCapacityExceeded { width: u8, height: u8 },
    InitialBoardOutsideField,
    EmptyPlacement { index: usize },
    PlacementOutsideField { index: usize },
    PlacementOverlap { index: usize },
    RoundTripDecodeFailed,
    RoundTripFieldMismatch,
}

fn validate_page(
    width: u8,
    height: u8,
    initial_board_mask: u64,
    placements: &[ColoredSolutionPlacement],
) -> Result<(), ColoredSolutionFumenError> {
    if width != FUMEN_WIDTH {
        return Err(ColoredSolutionFumenError::UnsupportedWidth { width });
    }
    if height == 0 || height > FUMEN_HEIGHT {
        return Err(ColoredSolutionFumenError::UnsupportedHeight { height });
    }
    let cell_count = usize::from(width) * usize::from(height);
    if cell_count > u64::BITS as usize {
        return Err(ColoredSolutionFumenError::BoardMaskCapacityExceeded { width, height });
    }
    let active_mask = if cell_count == u64::BITS as usize {
        u64::MAX
    } else {
        (1_u64 << cell_count) - 1
    };
    if initial_board_mask & !active_mask != 0 {
        return Err(ColoredSolutionFumenError::InitialBoardOutsideField);
    }

    let mut occupied = initial_board_mask;
    for (index, placement) in placements.iter().enumerate() {
        if placement.cells_mask == 0 {
            return Err(ColoredSolutionFumenError::EmptyPlacement { index });
        }
        if placement.cells_mask & !active_mask != 0 {
            return Err(ColoredSolutionFumenError::PlacementOutsideField { index });
        }
        if occupied & placement.cells_mask != 0 {
            return Err(ColoredSolutionFumenError::PlacementOverlap { index });
        }
        occupied |= placement.cells_mask;
    }
    Ok(())
}

fn page_field(
    page: &ColoredSolutionPage,
) -> Result<[[CellColor; FUMEN_WIDTH as usize]; FUMEN_HEIGHT as usize], ColoredSolutionFumenError> {
    validate_page(
        page.width,
        page.height,
        page.initial_board_mask,
        &page.placements,
    )?;
    let mut field = [[CellColor::Empty; FUMEN_WIDTH as usize]; FUMEN_HEIGHT as usize];
    paint_mask(
        &mut field,
        page.width,
        page.initial_board_mask,
        CellColor::Grey,
    );
    for placement in &page.placements {
        paint_mask(
            &mut field,
            page.width,
            placement.cells_mask,
            piece_color(placement.piece),
        );
    }
    Ok(field)
}

fn paint_mask(
    field: &mut [[CellColor; FUMEN_WIDTH as usize]; FUMEN_HEIGHT as usize],
    width: u8,
    mut mask: u64,
    color: CellColor,
) {
    while mask != 0 {
        let bit = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        field[bit / usize::from(width)][bit % usize::from(width)] = color;
    }
}

const fn piece_color(piece: PieceKind) -> CellColor {
    match piece {
        PieceKind::I => CellColor::I,
        PieceKind::O => CellColor::O,
        PieceKind::T => CellColor::T,
        PieceKind::S => CellColor::S,
        PieceKind::Z => CellColor::Z,
        PieceKind::J => CellColor::J,
        PieceKind::L => CellColor::L,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colored_solution_pages_roundtrip_as_real_fumen_fields() {
        let page = ColoredSolutionPage::new(
            10,
            4,
            0b11,
            vec![ColoredSolutionPlacement::new(PieceKind::I, 0b1111 << 10)],
        )
        .expect("valid page");

        let encoded = ColoredSolutionFumenExporter::encode(&[page]).expect("encoded fumen");
        let decoded = Fumen::decode(&encoded).expect("decoded fumen");
        assert_eq!(decoded.pages[0].field[0][0], CellColor::Grey);
        assert_eq!(decoded.pages[0].field[0][1], CellColor::Grey);
        assert_eq!(decoded.pages[0].field[1][0], CellColor::I);
        assert_eq!(decoded.pages[0].field[1][3], CellColor::I);
    }
}
