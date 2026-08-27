use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::{
        NormalizedTilingSolutionError, NormalizedTilingSolutionKey, NormalizedTilingSolutionSet,
        PiecePlacementMask, StandardBoard64TilingIdentity,
    },
};
use fumen::CellColor;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFumenBoard {
    occupied_mask: u64,
    grey_mask: u64,
    colored_mask: u64,
    visible_height: u8,
}

impl SourceFumenBoard {
    pub fn decode(source: &str) -> Result<Self, SourceFumenDiagramError> {
        let document = decode_document(source)?;
        if document.pages.len() != 1 {
            return Err(SourceFumenDiagramError::SetupPageCount {
                actual: document.pages.len(),
            });
        }
        let page = &document.pages[0];
        if page.piece.is_some() {
            return Err(SourceFumenDiagramError::SetupContainsOperation);
        }

        let mut grey_mask = 0u64;
        let mut colored_mask = 0u64;
        let mut visible_height = 0u8;
        for (y, row) in page.field.iter().enumerate() {
            for (x, cell) in row.iter().copied().enumerate() {
                if cell == CellColor::Empty {
                    continue;
                }
                let bit_index = y * 10 + x;
                if bit_index >= u64::BITS as usize {
                    return Err(SourceFumenDiagramError::CellOutsideBoard64 {
                        page_index: 0,
                        x,
                        y,
                    });
                }
                let bit = 1u64 << bit_index;
                if cell == CellColor::Grey {
                    grey_mask |= bit;
                } else {
                    colored_mask |= bit;
                }
                visible_height = visible_height.max((y + 1) as u8);
            }
        }
        Ok(Self {
            occupied_mask: grey_mask | colored_mask,
            grey_mask,
            colored_mask,
            visible_height,
        })
    }

    pub const fn occupied_mask(self) -> u64 {
        self.occupied_mask
    }

    pub const fn grey_mask(self) -> u64 {
        self.grey_mask
    }

    pub const fn colored_mask(self) -> u64 {
        self.colored_mask
    }

    pub const fn visible_height(self) -> u8 {
        self.visible_height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFumenSetup {
    initial_board_mask: u64,
    visible_height: u8,
}

impl SourceFumenSetup {
    pub fn decode(source: &str) -> Result<Self, SourceFumenDiagramError> {
        let document = decode_document(source)?;
        if document.pages.len() != 1 {
            return Err(SourceFumenDiagramError::SetupPageCount {
                actual: document.pages.len(),
            });
        }
        let page = &document.pages[0];
        if page.piece.is_some() {
            return Err(SourceFumenDiagramError::SetupContainsOperation);
        }

        let mut initial_board_mask = 0u64;
        let mut visible_height = 0u8;
        for (y, row) in page.field.iter().enumerate() {
            for (x, cell) in row.iter().copied().enumerate() {
                if cell == CellColor::Empty {
                    continue;
                }
                if cell != CellColor::Grey {
                    return Err(SourceFumenDiagramError::SetupContainsColoredCell { x, y });
                }
                let bit_index = y * 10 + x;
                if bit_index >= u64::BITS as usize {
                    return Err(SourceFumenDiagramError::CellOutsideBoard64 {
                        page_index: 0,
                        x,
                        y,
                    });
                }
                initial_board_mask |= 1u64 << bit_index;
                visible_height = visible_height.max((y + 1) as u8);
            }
        }
        if initial_board_mask == 0 {
            return Err(SourceFumenDiagramError::EmptySetupField);
        }
        Ok(Self {
            initial_board_mask,
            visible_height,
        })
    }

    pub const fn initial_board_mask(self) -> u64 {
        self.initial_board_mask
    }

    pub const fn visible_height(self) -> u8 {
        self.visible_height
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFumenDiagramSet {
    initial_board_mask: u64,
    visible_height: u8,
    page_count: usize,
    operation_replay_available: bool,
    solution_set: NormalizedTilingSolutionSet,
}

impl SourceFumenDiagramSet {
    pub fn decode(source: &str) -> Result<Self, SourceFumenDiagramError> {
        let document = decode_document(source)?;
        if document.pages.is_empty() {
            return Err(SourceFumenDiagramError::NoPages);
        }

        let mut initial_board_mask = None;
        let mut visible_height = 0u8;
        let mut keys = Vec::with_capacity(document.pages.len());
        let mut operation_replay_available = false;
        for (page_index, page) in document.pages.iter().enumerate() {
            let decoded = decode_page(page_index, &page.field)?;
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
            let identity = StandardBoard64TilingIdentity::from_placements(
                decoded.initial_board_mask,
                decoded.placements,
            )
            .map_err(|source| SourceFumenDiagramError::InvalidTiling { page_index, source })?;
            keys.push(NormalizedTilingSolutionKey::from_standard_board64_identity(
                identity,
            ));
            operation_replay_available |= page.piece.is_some();
        }

        Ok(Self {
            initial_board_mask: initial_board_mask.expect("a non-empty document has a first page"),
            visible_height,
            page_count: document.pages.len(),
            operation_replay_available,
            solution_set: NormalizedTilingSolutionSet::new(keys),
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

    pub fn solution_set(&self) -> &NormalizedTilingSolutionSet {
        &self.solution_set
    }
}

struct DecodedPage {
    initial_board_mask: u64,
    visible_height: u8,
    placements: Vec<PiecePlacementMask>,
}

fn decode_page(
    page_index: usize,
    field: &[[CellColor; 10]; 23],
) -> Result<DecodedPage, SourceFumenDiagramError> {
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
            match cell {
                CellColor::Grey => initial_board_mask |= bit,
                CellColor::Empty => {}
                color => {
                    let piece = piece_from_color(color)
                        .expect("all non-empty, non-grey fumen colors are tetromino colors");
                    piece_masks[piece_index(piece)] |= bit;
                }
            }
        }
    }

    let placements = PieceKind::STANDARD_TETROMINOES
        .iter()
        .copied()
        .zip(piece_masks)
        .filter_map(|(piece, mask)| (mask != 0).then_some(PiecePlacementMask::new(piece, mask)))
        .collect();
    Ok(DecodedPage {
        initial_board_mask,
        visible_height,
        placements,
    })
}

fn fumen_payload(source: &str) -> Result<String, SourceFumenDiagramError> {
    let source = source.trim();
    if let Some(index) = source.find("v115@") {
        return Ok(source[index..].to_owned());
    }
    for marker in ["D115@", "d115@", "m115@"] {
        if let Some(index) = source.find(marker) {
            let mut payload = source[index..].to_owned();
            payload.replace_range(..1, "v");
            return Ok(payload);
        }
    }
    Err(SourceFumenDiagramError::MissingV115Payload)
}

pub(crate) fn decode_document(source: &str) -> Result<fumen::Fumen, SourceFumenDiagramError> {
    let payload = fumen_payload(source)?;
    fumen::Fumen::decode(&payload).map_err(|_| SourceFumenDiagramError::InvalidFumenPayload)
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceFumenDiagramError {
    MissingV115Payload,
    InvalidFumenPayload,
    NoPages,
    SetupPageCount {
        actual: usize,
    },
    SetupContainsOperation,
    SetupContainsColoredCell {
        x: usize,
        y: usize,
    },
    EmptySetupField,
    CellOutsideBoard64 {
        page_index: usize,
        x: usize,
        y: usize,
    },
    InitialBoardDiffers {
        page_index: usize,
        expected: u64,
        actual: u64,
    },
    PendingGarbageUnsupported {
        page_index: usize,
    },
    InvalidTiling {
        page_index: usize,
        source: NormalizedTilingSolutionError,
    },
    ColoredPieceAreaNotMultipleOfFour {
        page_index: usize,
        piece: PieceKind,
        area: u32,
    },
}
