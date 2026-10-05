//! Borrowed, bounded CTK2 identity. This codec validates a colored partition,
//! not reachability, CountAll multiplicity, or public PC reducer completeness.
//! Compact Board64 identities remain a separate, unchanged fast path.

use crate::{
    board::standard_pc_board::{Board256Mask, STANDARD_PC_BOARD_WIDTH, STANDARD_PC_MAX_LINES},
    piece::piece_kind::PieceKind,
};

use super::NormalizedTilingSolutionError;

pub const EXTENDED_TILING_MAX_PLACEMENTS: usize = 60;

/// Ordering deliberately matches the producer's `(piece, [low_word, ...])`,
/// not the numeric ordering of the big-endian hex presentation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExtendedPiecePlacementMask {
    piece: PieceKind,
    cells: Board256Mask,
}

impl ExtendedPiecePlacementMask {
    pub const fn piece(self) -> PieceKind {
        self.piece
    }

    pub const fn cells(self) -> Board256Mask {
        self.cells
    }
}

/// Does not allocate or copy the placement payload. Callers can keep the
/// existing key owner and iterate its immutable, already-validated contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtendedTilingSolutionKey<'a> {
    value: &'a str,
    height: u8,
    initial: Board256Mask,
    placements: &'a str,
    placement_count: u8,
}

impl<'a> ExtendedTilingSolutionKey<'a> {
    pub fn parse_canonical(value: &'a str) -> Result<Self, NormalizedTilingSolutionError> {
        let invalid = NormalizedTilingSolutionError::InvalidCanonicalKey;
        let payload = value.strip_prefix("ctk2|height=").ok_or(invalid)?;
        let (height_text, payload) = payload.split_once("|initial=").ok_or(invalid)?;
        if height_text.is_empty()
            || height_text.len() > 2
            || height_text.starts_with('0')
            || !height_text.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(invalid);
        }
        let height = height_text.parse::<u8>().map_err(|_| invalid)?;
        if !(1..=STANDARD_PC_MAX_LINES).contains(&height) {
            return Err(invalid);
        }
        let (initial, placements) = payload.split_once("|placements=").ok_or(invalid)?;
        let initial = parse_mask(initial)?;
        let cell_count = u16::from(height) * STANDARD_PC_BOARD_WIDTH;
        if !initial.fits_cell_count(cell_count).map_err(|_| invalid)? {
            return Err(invalid);
        }
        let mut occupied = initial;
        let mut previous = None;
        let mut placement_count = 0_usize;
        if !placements.is_empty() {
            for text in placements.split(',') {
                if placement_count == EXTENDED_TILING_MAX_PLACEMENTS {
                    return Err(NormalizedTilingSolutionError::TooManyPlacements {
                        count: placement_count + 1,
                        capacity: EXTENDED_TILING_MAX_PLACEMENTS,
                    });
                }
                let placement = parse_placement(text)?;
                if placement.cells.is_empty() {
                    return Err(NormalizedTilingSolutionError::EmptyPlacementMask);
                }
                let area = placement.cells.count_ones();
                if area != 4 {
                    return Err(NormalizedTilingSolutionError::PlacementAreaNotFour {
                        piece: placement.piece,
                        area,
                    });
                }
                if !placement
                    .cells
                    .fits_cell_count(cell_count)
                    .map_err(|_| invalid)?
                    || previous.is_some_and(|prior| prior >= placement)
                {
                    return Err(invalid);
                }
                if occupied.intersects(placement.cells) {
                    return Err(NormalizedTilingSolutionError::OverlappingPlacement {
                        piece: placement.piece,
                    });
                }
                occupied = occupied.union(placement.cells);
                previous = Some(placement);
                placement_count += 1;
            }
        }
        Ok(Self {
            value,
            height,
            initial,
            placements,
            placement_count: placement_count as u8,
        })
    }

    pub const fn as_str(self) -> &'a str {
        self.value
    }

    pub const fn height(self) -> u8 {
        self.height
    }

    pub const fn initial_board(self) -> Board256Mask {
        self.initial
    }

    pub const fn placement_count(self) -> usize {
        self.placement_count as usize
    }

    pub fn placements(self) -> impl Iterator<Item = ExtendedPiecePlacementMask> + 'a {
        self.placements
            .split(',')
            .filter(move |_| self.placement_count != 0)
            .map(|text| parse_placement(text).expect("immutable canonical key was validated"))
    }
}

fn parse_placement(
    text: &str,
) -> Result<ExtendedPiecePlacementMask, NormalizedTilingSolutionError> {
    let invalid = NormalizedTilingSolutionError::InvalidCanonicalKey;
    let (piece, cells) = text.split_once(':').ok_or(invalid)?;
    let piece = match piece {
        "I" => PieceKind::I,
        "O" => PieceKind::O,
        "T" => PieceKind::T,
        "S" => PieceKind::S,
        "Z" => PieceKind::Z,
        "J" => PieceKind::J,
        "L" => PieceKind::L,
        _ => return Err(invalid),
    };
    Ok(ExtendedPiecePlacementMask {
        piece,
        cells: parse_mask(cells)?,
    })
}

fn parse_mask(text: &str) -> Result<Board256Mask, NormalizedTilingSolutionError> {
    let invalid = NormalizedTilingSolutionError::InvalidCanonicalKey;
    if text.len() != 64
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid);
    }
    let mut words = [0_u64; 4];
    for (chunk, word) in text
        .as_bytes()
        .as_chunks::<16>()
        .0
        .iter()
        .zip(words.iter_mut().rev())
    {
        // ASCII validation above makes each fixed-width UTF-8 conversion safe.
        *word = u64::from_str_radix(core::str::from_utf8(chunk).map_err(|_| invalid)?, 16)
            .map_err(|_| invalid)?;
    }
    Ok(Board256Mask::from_words(words))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solution::NormalizedTilingSolutionKey;

    #[test]
    fn shared_wire_cases_preserve_all_words_and_canonical_order() {
        let cases =
            include_str!("../../../../tests/fixtures/contracts/extended_solution_keys.v1.tsv");
        for line in cases
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
        {
            let mut columns = line.split('\t');
            let name = columns.next().unwrap();
            let valid = columns.next().unwrap() == "valid";
            let value = columns.next().unwrap();
            assert!(columns.next().is_none());
            let parsed = ExtendedTilingSolutionKey::parse_canonical(value);
            assert_eq!(parsed.is_ok(), valid, "{name}");
            let owned = NormalizedTilingSolutionKey::parse_canonical(value);
            assert_eq!(owned.is_ok(), valid, "{name}");
            if let Ok(identity) = parsed {
                assert_eq!(identity.as_str(), value);
                assert_eq!(identity.placements().count(), identity.placement_count());
                let owned = owned.unwrap();
                assert_eq!(owned.extended_identity().unwrap(), identity);
                assert!(
                    owned.standard_board64_identity().is_err(),
                    "must never truncate {name}"
                );
            }
        }
    }

    #[test]
    fn compact_keys_do_not_acquire_an_extended_identity() {
        let key = NormalizedTilingSolutionKey::parse_canonical(
            "ctk1|initial=0000000000000000|placements=I:000000000000000f",
        )
        .unwrap();
        assert!(key.extended_identity().is_err());
        assert_eq!(
            key.standard_board64_identity().unwrap().placement_count(),
            1
        );
    }
}
