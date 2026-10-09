//! PC drawing selection is not the compact Build target contract. A colored
//! drawing supplies no placement boundary or reachability proof: it is resolved
//! against the complete PC source later, and ambiguous matches are rejected.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    piece::piece_kind::PieceKind,
    solution::{ExtendedTilingSolutionKey, StandardBoard64ColoredTilingIdentity},
};
use clearra_ctk3::{decode_ctk3_exact, Ctk3Color, Ctk3Piece};
use clearra_fumen::{ActualFumenRenderColor, ActualFumenRenderDocument};

use crate::{FieldDocumentFormat, FIELD_DOCUMENT_MAX_INPUT_BYTES, FIELD_DOCUMENT_MAX_PAGES};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Drawing {
    Compact(StandardBoard64ColoredTilingIdentity),
    Extended {
        height: u8,
        initial: Board256Mask,
        pieces: [Board256Mask; 7],
    },
}

/// Profile-independent selected colors, never exact solution authority.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PcPinnedDrawing(Drawing);

impl From<StandardBoard64ColoredTilingIdentity> for PcPinnedDrawing {
    fn from(identity: StandardBoard64ColoredTilingIdentity) -> Self {
        Self(Drawing::Compact(identity))
    }
}

impl PcPinnedDrawing {
    pub const fn compact_identity(self) -> Option<StandardBoard64ColoredTilingIdentity> {
        match self.0 {
            Drawing::Compact(identity) => Some(identity),
            Drawing::Extended { .. } => None,
        }
    }

    pub(crate) fn matches_extended(self, key: ExtendedTilingSolutionKey<'_>) -> bool {
        let Drawing::Extended {
            height,
            initial,
            pieces,
        } = self.0
        else {
            return false;
        };
        if key.height() != height || key.initial_board() != initial {
            return false;
        }
        let mut actual = [Board256Mask::EMPTY; 7];
        for placement in key.placements() {
            let slot = &mut actual[piece_index(placement.piece())];
            *slot = slot.union(placement.cells());
        }
        actual == pieces
    }

    fn initial(self) -> Board256Mask {
        match self.0 {
            Drawing::Compact(identity) => {
                Board256Mask::from_words([identity.initial_board_mask(), 0, 0, 0])
            }
            Drawing::Extended { initial, .. } => initial,
        }
    }

    fn colored_union(self) -> Board256Mask {
        match self.0 {
            Drawing::Compact(identity) => Board256Mask::from_words([
                identity
                    .piece_masks()
                    .into_iter()
                    .fold(0, |all, mask| all | mask),
                0,
                0,
                0,
            ]),
            Drawing::Extended { pieces, .. } => pieces
                .into_iter()
                .fold(Board256Mask::EMPTY, Board256Mask::union),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcPinnedSolutionDocumentError {
    InputTooLarge,
    DecodeFailed,
    EmptyDocument,
    TooManyPages,
    WidthInvalid,
    HeightInvalid,
    PendingGarbageUnsupported,
    InitialBoardDiffers,
    ColoredTargetDiffers,
    ColoredAreaInvalid,
    CapacityExceeded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcPinnedSolutionDocument {
    drawings: Vec<PcPinnedDrawing>,
}

impl PcPinnedSolutionDocument {
    pub fn decode(
        format: FieldDocumentFormat,
        source: &str,
    ) -> Result<Self, PcPinnedSolutionDocumentError> {
        use PcPinnedSolutionDocumentError as Error;
        if source.len() > FIELD_DOCUMENT_MAX_INPUT_BYTES {
            return Err(Error::InputTooLarge);
        }
        let mut drawings = Vec::new();
        match format {
            FieldDocumentFormat::Ctk3 => {
                let document = decode_ctk3_exact(source).map_err(|_| Error::DecodeFailed)?;
                if document.width != 10 {
                    return Err(Error::WidthInvalid);
                }
                admit_pages(&mut drawings, document.pages.len())?;
                for page in document.pages {
                    if !(1..=24).contains(&page.height) || page.cells.len() != page.height * 10 {
                        return Err(Error::HeightInvalid);
                    }
                    if page.garbage.is_some() {
                        return Err(Error::PendingGarbageUnsupported);
                    }
                    let cells = page.cells.into_iter().map(|cell| match cell {
                        Ctk3Color::Empty => None,
                        Ctk3Color::Gray => Some(None),
                        Ctk3Color::Piece(piece) => Some(Some(match piece {
                            Ctk3Piece::I => PieceKind::I,
                            Ctk3Piece::O => PieceKind::O,
                            Ctk3Piece::T => PieceKind::T,
                            Ctk3Piece::S => PieceKind::S,
                            Ctk3Piece::Z => PieceKind::Z,
                            Ctk3Piece::J => PieceKind::J,
                            Ctk3Piece::L => PieceKind::L,
                        })),
                    });
                    drawings.push(decode_page(page.height as u8, cells)?);
                }
            }
            FieldDocumentFormat::Fumen => {
                let document =
                    ActualFumenRenderDocument::decode(source).map_err(|_| Error::DecodeFailed)?;
                admit_pages(&mut drawings, document.pages().len())?;
                for page in document.pages() {
                    if page.width() != 10 {
                        return Err(Error::WidthInvalid);
                    }
                    if page
                        .pending_garbage()
                        .iter()
                        .any(|cell| *cell != ActualFumenRenderColor::Empty)
                    {
                        return Err(Error::PendingGarbageUnsupported);
                    }
                    // Fumen has 23 rows including empty padding. Preserve all
                    // occupied rows, not the compact decoder's six-row mask.
                    let used = page
                        .cells_bottom_up()
                        .iter()
                        .rposition(|cell| *cell != ActualFumenRenderColor::Empty)
                        .map_or(1, |index| index / 10 + 1);
                    if !(1..=23).contains(&used) {
                        return Err(Error::HeightInvalid);
                    }
                    let cells =
                        page.cells_bottom_up()
                            .iter()
                            .take(used * 10)
                            .map(|cell| match cell {
                                ActualFumenRenderColor::Empty => None,
                                ActualFumenRenderColor::Garbage => Some(None),
                                ActualFumenRenderColor::I => Some(Some(PieceKind::I)),
                                ActualFumenRenderColor::O => Some(Some(PieceKind::O)),
                                ActualFumenRenderColor::T => Some(Some(PieceKind::T)),
                                ActualFumenRenderColor::S => Some(Some(PieceKind::S)),
                                ActualFumenRenderColor::Z => Some(Some(PieceKind::Z)),
                                ActualFumenRenderColor::J => Some(Some(PieceKind::J)),
                                ActualFumenRenderColor::L => Some(Some(PieceKind::L)),
                            });
                    drawings.push(decode_page(used as u8, cells)?);
                }
            }
        }
        let first = *drawings.first().ok_or(Error::EmptyDocument)?;
        for drawing in &drawings {
            if drawing.initial() != first.initial() {
                return Err(Error::InitialBoardDiffers);
            }
            if drawing.colored_union() != first.colored_union() {
                return Err(Error::ColoredTargetDiffers);
            }
        }
        drawings.sort_unstable();
        drawings.dedup();
        Ok(Self { drawings })
    }

    pub fn drawings(&self) -> &[PcPinnedDrawing] {
        &self.drawings
    }

    pub fn into_drawings(self) -> Vec<PcPinnedDrawing> {
        self.drawings
    }
}

fn admit_pages(
    drawings: &mut Vec<PcPinnedDrawing>,
    count: usize,
) -> Result<(), PcPinnedSolutionDocumentError> {
    if count == 0 {
        return Err(PcPinnedSolutionDocumentError::EmptyDocument);
    }
    if count > FIELD_DOCUMENT_MAX_PAGES {
        return Err(PcPinnedSolutionDocumentError::TooManyPages);
    }
    drawings
        .try_reserve_exact(count)
        .map_err(|_| PcPinnedSolutionDocumentError::CapacityExceeded)
}

fn decode_page(
    height: u8,
    cells: impl Iterator<Item = Option<Option<PieceKind>>>,
) -> Result<PcPinnedDrawing, PcPinnedSolutionDocumentError> {
    let mut initial = Board256Mask::EMPTY;
    let mut pieces = [Board256Mask::EMPTY; 7];
    for (index, cell) in cells.enumerate() {
        let bit = Board256Mask::singleton(index as u16)
            .map_err(|_| PcPinnedSolutionDocumentError::HeightInvalid)?;
        match cell {
            None => {}
            Some(None) => initial = initial.union(bit),
            Some(Some(piece)) => {
                let slot = &mut pieces[piece_index(piece)];
                *slot = slot.union(bit);
            }
        }
    }
    if pieces.iter().all(|cells| cells.is_empty())
        || pieces.iter().any(|cells| cells.count_ones() & 3 != 0)
    {
        return Err(PcPinnedSolutionDocumentError::ColoredAreaInvalid);
    }
    if height <= 6 {
        let compact = StandardBoard64ColoredTilingIdentity::from_piece_masks(
            initial.words()[0],
            pieces.map(|cells| cells.words()[0]),
        )
        .map_err(|_| PcPinnedSolutionDocumentError::ColoredAreaInvalid)?;
        Ok(compact.into())
    } else {
        Ok(PcPinnedDrawing(Drawing::Extended {
            height,
            initial,
            pieces,
        }))
    }
}

const fn piece_index(piece: PieceKind) -> usize {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_ctk3::{encode_ctk3, Ctk3Document, Ctk3Page};

    fn document(height: usize, gray_at: Option<usize>) -> String {
        let mut cells = vec![Ctk3Color::Empty; height * 10];
        cells[..4].fill(Ctk3Color::Piece(Ctk3Piece::I));
        if let Some(index) = gray_at {
            cells[index] = Ctk3Color::Gray;
        }
        encode_ctk3(&Ctk3Document::new(10, vec![Ctk3Page::new(height, cells)])).unwrap()
    }

    #[test]
    fn full_height_selection_keeps_the_top_word_and_deduplicates_pages() {
        for height in [7, 8, 12, 24] {
            let encoded = document(height, Some(height * 10 - 1));
            let mut pages = decode_ctk3_exact(&encoded).unwrap();
            pages.pages.push(pages.pages[0].clone());
            let decoded = PcPinnedSolutionDocument::decode(
                FieldDocumentFormat::Ctk3,
                &encode_ctk3(&pages).unwrap(),
            )
            .unwrap();
            assert_eq!(decoded.drawings().len(), 1);
            let drawing = decoded.drawings()[0];
            assert!(drawing.compact_identity().is_none());
            let top = Board256Mask::singleton(height as u16 * 10 - 1)
                .unwrap()
                .words();
            let key = format!(
                "ctk2|height={height}|initial={:016x}{:016x}{:016x}{:016x}|placements=I:{:064x}",
                top[3], top[2], top[1], top[0], 15
            );
            assert!(
                drawing.matches_extended(ExtendedTilingSolutionKey::parse_canonical(&key).unwrap())
            );
            let truncated = format!(
                "ctk2|height={height}|initial={:064x}|placements=I:{:064x}",
                0, 15
            );
            assert!(!drawing
                .matches_extended(ExtendedTilingSolutionKey::parse_canonical(&truncated).unwrap()));
        }
    }

    #[test]
    fn compact_pc_selection_retains_the_existing_build_decoder_identity() {
        let encoded = document(2, Some(10));
        let pc = PcPinnedSolutionDocument::decode(FieldDocumentFormat::Ctk3, &encoded).unwrap();
        let build =
            crate::BuildColoredTargetDocument::decode(FieldDocumentFormat::Ctk3, &encoded).unwrap();
        assert_eq!(
            pc.drawings()[0].compact_identity(),
            Some(build.target().identities()[0])
        );
    }

    #[test]
    fn command_clones_share_the_closed_full_height_selection_owner() {
        use std::sync::Arc;
        let decoded =
            PcPinnedSolutionDocument::decode(FieldDocumentFormat::Ctk3, &document(24, Some(239)))
                .unwrap();
        let command =
            crate::PcAppCommand::new(clearra_pc_graph::request::OpeningPcSearchQuery::new(
                clearra_core_domain::pc::pc_target::PcTarget::new(24).unwrap(),
            ))
            .with_pinned_minimum_selection(decoded.into_drawings(), None);
        let clone = command.clone();
        assert!(Arc::ptr_eq(
            &command.pinned_minimum_drawing_owner(),
            &clone.pinned_minimum_drawing_owner()
        ));
    }

    #[test]
    fn selection_rejects_cross_page_initial_or_colored_target_changes() {
        let mut pages = decode_ctk3_exact(&document(24, Some(239))).unwrap();
        let mut changed = pages.pages[0].clone();
        changed.cells[239] = Ctk3Color::Empty;
        pages.pages.push(changed);
        assert_eq!(
            PcPinnedSolutionDocument::decode(
                FieldDocumentFormat::Ctk3,
                &encode_ctk3(&pages).unwrap()
            ),
            Err(PcPinnedSolutionDocumentError::InitialBoardDiffers)
        );
        pages.pages.pop();
        let mut changed = pages.pages[0].clone();
        changed.cells[4..8].fill(Ctk3Color::Piece(Ctk3Piece::I));
        pages.pages.push(changed);
        assert_eq!(
            PcPinnedSolutionDocument::decode(
                FieldDocumentFormat::Ctk3,
                &encode_ctk3(&pages).unwrap()
            ),
            Err(PcPinnedSolutionDocumentError::ColoredTargetDiffers)
        );
    }

    #[test]
    fn fumen_selection_preserves_high_rows_and_never_claims_a_twenty_fourth_row() {
        use fumen::{CellColor, Fumen, Page};
        let mut page = Page::default();
        page.field[0][..4].fill(CellColor::I);
        page.field[22][9] = CellColor::Grey;
        let encoded = Fumen {
            pages: vec![page],
            ..Fumen::default()
        }
        .encode();
        let decoded =
            PcPinnedSolutionDocument::decode(FieldDocumentFormat::Fumen, &encoded).unwrap();
        let top = Board256Mask::singleton(229).unwrap().words();
        let key = format!(
            "ctk2|height=23|initial={:016x}{:016x}{:016x}{:016x}|placements=I:{:064x}",
            top[3], top[2], top[1], top[0], 15
        );
        assert!(decoded.drawings()[0]
            .matches_extended(ExtendedTilingSolutionKey::parse_canonical(&key).unwrap()));
        assert!(!decoded.drawings()[0].matches_extended(
            ExtendedTilingSolutionKey::parse_canonical(&key.replace("height=23", "height=24"))
                .unwrap()
        ));
    }
}
