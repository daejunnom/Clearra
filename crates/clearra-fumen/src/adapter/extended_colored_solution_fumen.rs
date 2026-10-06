use clearra_core_domain::{board::standard_pc_board::Board256Mask, piece::piece_kind::PieceKind};
use fumen::CellColor;

use super::colored_solution_fumen::{
    encode_field_pages, piece_color, ColoredFumenField, ColoredSolutionFumenError,
    ColoredSolutionFumenExporter, FUMEN_HEIGHT, FUMEN_WIDTH,
};

/// A static colored-field component, not an ordered operation or replay.
/// Multiple equal-piece placements may already have been unioned by a
/// colored-field producer; publication does not invent their boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtendedColoredSolutionPlacement {
    piece: PieceKind,
    cells: Board256Mask,
}

impl ExtendedColoredSolutionPlacement {
    pub const fn new(piece: PieceKind, cells: Board256Mask) -> Self {
        Self { piece, cells }
    }

    pub const fn piece(self) -> PieceKind {
        self.piece
    }

    pub const fn cells(self) -> Board256Mask {
        self.cells
    }
}

/// Cold output ownership for all four words. The existing compact page and
/// compact search identities are deliberately unchanged. Fumen has 23 rows;
/// a 24-row request is rejected, even if its highest row happens to be empty.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtendedColoredSolutionPage {
    width: u8,
    height: u8,
    initial_board: Board256Mask,
    placements: Vec<ExtendedColoredSolutionPlacement>,
    comment: Option<String>,
}

impl ExtendedColoredSolutionPage {
    pub fn new(
        width: u8,
        height: u8,
        initial_board: Board256Mask,
        placements: Vec<ExtendedColoredSolutionPlacement>,
    ) -> Result<Self, ColoredSolutionFumenError> {
        validate_page(width, height, initial_board, &placements)?;
        Ok(Self {
            width,
            height,
            initial_board,
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

    pub const fn initial_board(&self) -> Board256Mask {
        self.initial_board
    }

    pub fn placements(&self) -> &[ExtendedColoredSolutionPlacement] {
        &self.placements
    }

    fn field(&self) -> Result<ColoredFumenField, ColoredSolutionFumenError> {
        validate_page(
            self.width,
            self.height,
            self.initial_board,
            &self.placements,
        )?;
        let mut field = [[CellColor::Empty; FUMEN_WIDTH as usize]; FUMEN_HEIGHT as usize];
        paint_mask(&mut field, self.initial_board, CellColor::Grey);
        for placement in &self.placements {
            paint_mask(&mut field, placement.cells, piece_color(placement.piece));
        }
        Ok(field)
    }
}

impl ColoredSolutionFumenExporter {
    pub fn encode_extended(
        pages: &[ExtendedColoredSolutionPage],
    ) -> Result<String, ColoredSolutionFumenError> {
        encode_field_pages(
            pages
                .iter()
                .map(|page| Ok((page.field()?, page.comment.as_deref()))),
        )
    }
}

fn validate_page(
    width: u8,
    height: u8,
    initial_board: Board256Mask,
    placements: &[ExtendedColoredSolutionPlacement],
) -> Result<(), ColoredSolutionFumenError> {
    if width != FUMEN_WIDTH {
        return Err(ColoredSolutionFumenError::UnsupportedWidth { width });
    }
    if height == 0 || height > FUMEN_HEIGHT {
        return Err(ColoredSolutionFumenError::UnsupportedHeight { height });
    }
    let active = Board256Mask::all_cells(u16::from(width) * u16::from(height))
        .expect("validated Fumen dimensions fit four words");
    if !initial_board.without(active).is_empty() {
        return Err(ColoredSolutionFumenError::InitialBoardOutsideField);
    }
    let mut occupied = initial_board;
    for (index, placement) in placements.iter().enumerate() {
        if placement.cells.is_empty() {
            return Err(ColoredSolutionFumenError::EmptyPlacement { index });
        }
        if !placement.cells.without(active).is_empty() {
            return Err(ColoredSolutionFumenError::PlacementOutsideField { index });
        }
        if occupied.intersects(placement.cells) {
            return Err(ColoredSolutionFumenError::PlacementOverlap { index });
        }
        occupied = occupied.union(placement.cells);
    }
    Ok(())
}

fn paint_mask(field: &mut ColoredFumenField, mask: Board256Mask, color: CellColor) {
    for (word_index, mut word) in mask.words().into_iter().enumerate() {
        while word != 0 {
            let bit = word_index * 64 + word.trailing_zeros() as usize;
            word &= word - 1;
            field[bit / usize::from(FUMEN_WIDTH)][bit % usize::from(FUMEN_WIDTH)] = color;
        }
    }
}

#[cfg(test)]
mod tests {
    use fumen::Fumen;

    use super::*;
    use crate::{ColoredSolutionPage, ColoredSolutionPlacement};

    fn mask(bits: &[usize]) -> Board256Mask {
        let mut words = [0_u64; 4];
        for bit in bits {
            words[bit / 64] |= 1_u64 << (bit % 64);
        }
        Board256Mask::from_words(words)
    }

    #[test]
    fn all_four_words_and_the_last_fumen_row_roundtrip_without_truncation() {
        let initial = mask(&[0, 64, 128, 192]);
        let piece = mask(&[226, 227, 228, 229]);
        let page = ExtendedColoredSolutionPage::new(
            10,
            23,
            initial,
            vec![ExtendedColoredSolutionPlacement::new(PieceKind::I, piece)],
        )
        .unwrap()
        .with_comment("PC 0.25");
        let encoded = ColoredSolutionFumenExporter::encode_extended(&[page]).unwrap();
        let decoded = Fumen::decode(&encoded).unwrap();
        for bit in 0..230_u16 {
            let expected = if initial.contains_index(bit) {
                CellColor::Grey
            } else if piece.contains_index(bit) {
                CellColor::I
            } else {
                CellColor::Empty
            };
            assert_eq!(
                decoded.pages[0].field[usize::from(bit) / 10][usize::from(bit) % 10],
                expected
            );
        }
        assert_eq!(decoded.pages[0].comment.as_deref(), Some("PC 0.25"));
        // Export capability is not authority to import a wide field into
        // compact minimum/setup identities or silently retain only word zero.
        assert!(matches!(
            crate::SourceFumenDiagramSet::decode(&encoded),
            Err(crate::SourceFumenDiagramError::CellOutsideBoard64 { .. })
        ));
        assert!(matches!(
            crate::SourceFumenColoredFieldSet::decode(&encoded),
            Err(crate::SourceFumenDiagramError::CellOutsideBoard64 { .. })
        ));
    }

    #[test]
    fn compact_and_four_word_publication_are_byte_identical_for_the_same_page() {
        let compact = ColoredSolutionPage::new(
            10,
            4,
            0b11,
            vec![ColoredSolutionPlacement::new(PieceKind::T, 0x0f << 10)],
        )
        .unwrap()
        .with_comment("PC 0.5");
        let extended = ExtendedColoredSolutionPage::new(
            10,
            4,
            mask(&[0, 1]),
            vec![ExtendedColoredSolutionPlacement::new(
                PieceKind::T,
                mask(&[10, 11, 12, 13]),
            )],
        )
        .unwrap()
        .with_comment("PC 0.5");
        assert_eq!(
            ColoredSolutionFumenExporter::encode(&[compact]).unwrap(),
            ColoredSolutionFumenExporter::encode_extended(&[extended]).unwrap(),
        );
    }

    #[test]
    fn four_word_publication_rejects_unsupported_frames_and_collisions() {
        for height in [0, 24] {
            assert_eq!(
                ExtendedColoredSolutionPage::new(10, height, mask(&[]), vec![]),
                Err(ColoredSolutionFumenError::UnsupportedHeight { height }),
            );
        }
        assert_eq!(
            ExtendedColoredSolutionPage::new(9, 23, mask(&[]), vec![]),
            Err(ColoredSolutionFumenError::UnsupportedWidth { width: 9 })
        );
        assert_eq!(
            ExtendedColoredSolutionPage::new(10, 23, mask(&[230]), vec![]),
            Err(ColoredSolutionFumenError::InitialBoardOutsideField)
        );
        for (bits, expected) in [
            (
                vec![],
                ColoredSolutionFumenError::EmptyPlacement { index: 0 },
            ),
            (
                vec![230],
                ColoredSolutionFumenError::PlacementOutsideField { index: 0 },
            ),
            (
                vec![229],
                ColoredSolutionFumenError::PlacementOverlap { index: 0 },
            ),
        ] {
            assert_eq!(
                ExtendedColoredSolutionPage::new(
                    10,
                    23,
                    mask(&[229]),
                    vec![ExtendedColoredSolutionPlacement::new(
                        PieceKind::L,
                        mask(&bits)
                    )]
                ),
                Err(expected)
            );
        }
        let placements = vec![
            ExtendedColoredSolutionPlacement::new(PieceKind::J, mask(&[192])),
            ExtendedColoredSolutionPlacement::new(PieceKind::L, mask(&[192])),
        ];
        assert_eq!(
            ExtendedColoredSolutionPage::new(10, 23, mask(&[]), placements),
            Err(ColoredSolutionFumenError::PlacementOverlap { index: 1 })
        );
        assert_eq!(
            ColoredSolutionFumenExporter::encode_extended(&[]),
            Err(ColoredSolutionFumenError::EmptyDocument)
        );
    }
}
