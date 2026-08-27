use std::{collections::BTreeSet, hash::Hasher};

use clearra_core_domain::{
    piece::piece_kind::PieceKind, solution::StandardBoard64ColoredTilingIdentity,
};
use fumen::CellColor;

use super::source_fumen_diagram::{decode_document, SourceFumenDiagramError};

pub const COLORED_FIELD_SOLUTION_KEY_ALGORITHM: &str = "clearra-colored-field-key-v1";
pub const COLORED_FIELD_SOLUTION_SET_HASH_ALGORITHM: &str = "clearra-colored-field-set-fnv64-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFumenColoredFieldSet {
    initial_board_mask: u64,
    visible_height: u8,
    page_count: usize,
    operation_replay_available: bool,
    keys: BTreeSet<String>,
    identities: Vec<StandardBoard64ColoredTilingIdentity>,
    hash: String,
}

impl SourceFumenColoredFieldSet {
    pub fn decode(source: &str) -> Result<Self, SourceFumenDiagramError> {
        let document = decode_document(source)?;
        if document.pages.is_empty() {
            return Err(SourceFumenDiagramError::NoPages);
        }

        let mut initial_board_mask = None;
        let mut visible_height = 0u8;
        let mut keys = BTreeSet::new();
        let mut identities = Vec::with_capacity(document.pages.len());
        let mut operation_replay_available = false;
        for (page_index, page) in document.pages.iter().enumerate() {
            if page
                .garbage_row
                .iter()
                .any(|cell| *cell != CellColor::Empty)
            {
                return Err(SourceFumenDiagramError::PendingGarbageUnsupported { page_index });
            }
            let decoded = decode_colored_page(page_index, &page.field)?;
            match initial_board_mask {
                Some(expected) if expected != decoded.initial_board_mask => {
                    return Err(SourceFumenDiagramError::InitialBoardDiffers {
                        page_index,
                        expected,
                        actual: decoded.initial_board_mask,
                    });
                }
                None => initial_board_mask = Some(decoded.initial_board_mask),
                Some(_) => {}
            }
            visible_height = visible_height.max(decoded.visible_height);
            keys.insert(colored_field_key(
                decoded.initial_board_mask,
                &decoded.piece_masks,
            ));
            identities.push(
                StandardBoard64ColoredTilingIdentity::from_piece_masks(
                    decoded.initial_board_mask,
                    decoded.piece_masks,
                )
                .map_err(|source| SourceFumenDiagramError::InvalidTiling { page_index, source })?,
            );
            operation_replay_available |= page.piece.is_some();
        }
        identities.sort_unstable();
        identities.dedup();
        let hash = stable_colored_field_set_hash(&keys);

        Ok(Self {
            initial_board_mask: initial_board_mask.expect("a non-empty document has a first page"),
            visible_height,
            page_count: document.pages.len(),
            operation_replay_available,
            keys,
            identities,
            hash,
        })
    }

    pub const fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask
    }

    pub const fn visible_height(&self) -> u8 {
        self.visible_height
    }

    pub const fn page_count(&self) -> usize {
        self.page_count
    }

    pub const fn operation_replay_available(&self) -> bool {
        self.operation_replay_available
    }

    pub fn keys(&self) -> &BTreeSet<String> {
        &self.keys
    }

    pub fn identities(&self) -> &[StandardBoard64ColoredTilingIdentity] {
        &self.identities
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }
}

struct DecodedColoredPage {
    initial_board_mask: u64,
    visible_height: u8,
    piece_masks: [u64; 7],
}

fn decode_colored_page(
    page_index: usize,
    field: &[[CellColor; 10]; 23],
) -> Result<DecodedColoredPage, SourceFumenDiagramError> {
    let mut initial_board_mask = 0u64;
    let mut piece_masks = [0u64; 7];
    let mut visible_height = 0u8;
    for (y, row) in field.iter().enumerate() {
        for (x, cell) in row.iter().copied().enumerate() {
            if cell == CellColor::Empty {
                continue;
            }
            let bit_index = y * 10 + x;
            if bit_index >= u64::BITS as usize {
                return Err(SourceFumenDiagramError::CellOutsideBoard64 { page_index, x, y });
            }
            visible_height = visible_height.max((y + 1) as u8);
            let bit = 1u64 << bit_index;
            match piece_from_color(cell) {
                Some(piece) => piece_masks[piece_index(piece)] |= bit,
                None => initial_board_mask |= bit,
            }
        }
    }

    for (piece, mask) in PieceKind::STANDARD_TETROMINOES
        .iter()
        .copied()
        .zip(piece_masks)
    {
        let area = mask.count_ones();
        if area != 0 && area % 4 != 0 {
            return Err(SourceFumenDiagramError::ColoredPieceAreaNotMultipleOfFour {
                page_index,
                piece,
                area,
            });
        }
    }

    Ok(DecodedColoredPage {
        initial_board_mask,
        visible_height,
        piece_masks,
    })
}

fn colored_field_key(initial_board_mask: u64, piece_masks: &[u64; 7]) -> String {
    let colors = PieceKind::STANDARD_TETROMINOES
        .iter()
        .copied()
        .zip(piece_masks.iter().copied())
        .filter(|(_, mask)| *mask != 0)
        .map(|(piece, mask)| format!("{}:{mask:016x}", piece.as_ascii()))
        .collect::<Vec<_>>()
        .join(",");
    format!("cfk1|initial={initial_board_mask:016x}|colors={colors}")
}

fn stable_colored_field_set_hash(keys: &BTreeSet<String>) -> String {
    let mut hasher = StableFnv64::default();
    for key in keys {
        hasher.write(key.as_bytes());
        hasher.write(&[0]);
    }
    format!("cfs1:{:016x}", hasher.finish())
}

const fn piece_from_color(color: CellColor) -> Option<PieceKind> {
    match color {
        CellColor::I => Some(PieceKind::I),
        CellColor::O => Some(PieceKind::O),
        CellColor::T => Some(PieceKind::T),
        CellColor::S => Some(PieceKind::S),
        CellColor::Z => Some(PieceKind::Z),
        CellColor::J => Some(PieceKind::J),
        CellColor::L => Some(PieceKind::L),
        CellColor::Empty | CellColor::Grey => None,
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

#[derive(Default)]
struct StableFnv64(u64);

impl Hasher for StableFnv64 {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        if self.0 == 0 {
            self.0 = 0xcbf2_9ce4_8422_2325;
        }
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use crate::{ColoredSolutionFumenExporter, ColoredSolutionPage, ColoredSolutionPlacement};

    use super::SourceFumenColoredFieldSet;

    #[test]
    fn repeated_same_piece_color_decodes_without_inventing_placement_boundaries() {
        let page = ColoredSolutionPage::new(
            10,
            1,
            0b11u64 << 8,
            vec![
                ColoredSolutionPlacement::new(PieceKind::I, 0b1111),
                ColoredSolutionPlacement::new(PieceKind::I, 0b1111 << 4),
            ],
        )
        .expect("valid repeated-I page");
        let fumen = ColoredSolutionFumenExporter::encode(&[page]).expect("encoded fumen");

        let decoded = SourceFumenColoredFieldSet::decode(&fumen).expect("colored field set");

        assert_eq!(decoded.identities().len(), 1);
        assert_eq!(decoded.identities()[0].placement_count(), 2);
        assert_eq!(decoded.identities()[0].piece_masks()[0], 0xff);
    }
}
