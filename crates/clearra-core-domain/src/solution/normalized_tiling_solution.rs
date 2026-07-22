use std::{cmp::Ordering, fmt, hash::Hasher};

use crate::piece::piece_kind::PieceKind;

pub const NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM: &str = "clearra-normalized-tiling-key-v1";
pub const NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM: &str =
    "clearra-normalized-tiling-set-fnv64-v1";
pub const STANDARD_BOARD64_TILING_MAX_PLACEMENTS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PiecePlacementMask {
    piece: PieceKind,
    cells_mask: u64,
}

impl PiecePlacementMask {
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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NormalizedTilingSolutionKey(String);

impl NormalizedTilingSolutionKey {
    pub fn from_placements(
        initial_board_mask: u64,
        placements: impl IntoIterator<Item = PiecePlacementMask>,
    ) -> Result<Self, NormalizedTilingSolutionError> {
        StandardBoard64TilingIdentity::from_placements(initial_board_mask, placements)
            .map(Self::from_standard_board64_identity)
    }

    pub fn from_standard_board64_identity(identity: StandardBoard64TilingIdentity) -> Self {
        Self(identity.canonical_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse_canonical(value: &str) -> Result<Self, NormalizedTilingSolutionError> {
        let payload = value
            .strip_prefix("ctk1|initial=")
            .ok_or(NormalizedTilingSolutionError::InvalidCanonicalKey)?;
        let (initial, placements) = payload
            .split_once("|placements=")
            .ok_or(NormalizedTilingSolutionError::InvalidCanonicalKey)?;
        if initial.len() != 16 {
            return Err(NormalizedTilingSolutionError::InvalidCanonicalKey);
        }
        let initial_board_mask = u64::from_str_radix(initial, 16)
            .map_err(|_| NormalizedTilingSolutionError::InvalidCanonicalKey)?;
        let mut parsed = Vec::new();
        if !placements.is_empty() {
            for placement in placements.split(',') {
                let (piece, mask) = placement
                    .split_once(':')
                    .ok_or(NormalizedTilingSolutionError::InvalidCanonicalKey)?;
                if piece.len() != 1 || mask.len() != 16 {
                    return Err(NormalizedTilingSolutionError::InvalidCanonicalKey);
                }
                let piece = match piece.as_bytes()[0] {
                    b'I' => PieceKind::I,
                    b'O' => PieceKind::O,
                    b'T' => PieceKind::T,
                    b'S' => PieceKind::S,
                    b'Z' => PieceKind::Z,
                    b'J' => PieceKind::J,
                    b'L' => PieceKind::L,
                    _ => return Err(NormalizedTilingSolutionError::InvalidCanonicalKey),
                };
                let mask = u64::from_str_radix(mask, 16)
                    .map_err(|_| NormalizedTilingSolutionError::InvalidCanonicalKey)?;
                parsed.push(PiecePlacementMask::new(piece, mask));
            }
        }
        let key = Self::from_placements(initial_board_mask, parsed)?;
        if key.as_str() != value {
            return Err(NormalizedTilingSolutionError::InvalidCanonicalKey);
        }
        Ok(key)
    }
}

/// Allocation-free canonical identity for the standard tetromino Board64 fast path.
///
/// Piece codes are packed separately from masks, so all 64 occupancy bits remain
/// available. The identity is suitable for exact hash-table confirmation; hashes
/// are never used as the equality authority.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StandardBoard64TilingIdentity {
    initial_board_mask: u64,
    placement_masks: [u64; STANDARD_BOARD64_TILING_MAX_PLACEMENTS],
    packed_piece_codes: u64,
    placement_count: u8,
}

impl StandardBoard64TilingIdentity {
    pub fn from_placements(
        initial_board_mask: u64,
        placements: impl IntoIterator<Item = PiecePlacementMask>,
    ) -> Result<Self, NormalizedTilingSolutionError> {
        let mut masks = [0_u64; STANDARD_BOARD64_TILING_MAX_PLACEMENTS];
        let mut pieces = [0_u8; STANDARD_BOARD64_TILING_MAX_PLACEMENTS];
        let mut placement_count = 0_usize;
        let mut occupied = initial_board_mask;

        for placement in placements {
            if placement_count == STANDARD_BOARD64_TILING_MAX_PLACEMENTS {
                return Err(NormalizedTilingSolutionError::TooManyPlacements {
                    count: placement_count + 1,
                    capacity: STANDARD_BOARD64_TILING_MAX_PLACEMENTS,
                });
            }
            let mask = placement.cells_mask();
            if mask == 0 {
                return Err(NormalizedTilingSolutionError::EmptyPlacementMask);
            }
            if mask.count_ones() != 4 {
                return Err(NormalizedTilingSolutionError::PlacementAreaNotFour {
                    piece: placement.piece(),
                    area: mask.count_ones(),
                });
            }
            if occupied & mask != 0 {
                return Err(NormalizedTilingSolutionError::OverlappingPlacement {
                    piece: placement.piece(),
                });
            }
            occupied |= mask;

            let piece = piece_sort_key(placement.piece());
            let mut insertion = placement_count;
            while insertion > 0 && (pieces[insertion - 1], masks[insertion - 1]) > (piece, mask) {
                pieces[insertion] = pieces[insertion - 1];
                masks[insertion] = masks[insertion - 1];
                insertion -= 1;
            }
            pieces[insertion] = piece;
            masks[insertion] = mask;
            placement_count += 1;
        }

        let mut packed_piece_codes = 0_u64;
        for (index, piece) in pieces[..placement_count].iter().copied().enumerate() {
            packed_piece_codes |= u64::from(piece) << (index * 3);
        }
        Ok(Self {
            initial_board_mask,
            placement_masks: masks,
            packed_piece_codes,
            placement_count: placement_count as u8,
        })
    }

    pub const fn placement_count(self) -> usize {
        self.placement_count as usize
    }

    pub const fn initial_board_mask(self) -> u64 {
        self.initial_board_mask
    }

    pub const fn packed_piece_codes(self) -> u64 {
        self.packed_piece_codes
    }

    pub fn placement_masks(&self) -> &[u64] {
        &self.placement_masks[..self.placement_count()]
    }

    pub fn placement(self, index: usize) -> Option<PiecePlacementMask> {
        (index < self.placement_count()).then(|| {
            PiecePlacementMask::new(
                piece_from_sort_key(self.piece_code(index)),
                self.placement_masks[index],
            )
        })
    }

    pub fn from_compact_parts(
        initial_board_mask: u64,
        packed_piece_codes: u64,
        placement_masks: &[u64],
    ) -> Result<Self, NormalizedTilingSolutionError> {
        if placement_masks.len() > STANDARD_BOARD64_TILING_MAX_PLACEMENTS {
            return Err(NormalizedTilingSolutionError::TooManyPlacements {
                count: placement_masks.len(),
                capacity: STANDARD_BOARD64_TILING_MAX_PLACEMENTS,
            });
        }
        let mut pieces = Vec::with_capacity(placement_masks.len());
        for index in 0..placement_masks.len() {
            let piece_code = ((packed_piece_codes >> (index * 3)) & 0x7) as u8;
            if piece_code > 6 {
                return Err(NormalizedTilingSolutionError::InvalidCanonicalKey);
            }
            pieces.push(piece_from_sort_key(piece_code));
        }
        let placements = placement_masks
            .iter()
            .copied()
            .zip(pieces)
            .map(|(mask, piece)| PiecePlacementMask::new(piece, mask));
        let identity = Self::from_placements(initial_board_mask, placements)?;
        if identity.packed_piece_codes != packed_piece_codes {
            return Err(NormalizedTilingSolutionError::InvalidCanonicalKey);
        }
        Ok(identity)
    }

    /// Hash-table accelerator for the exact identity. Equality must still compare
    /// the complete identity because collisions are never authoritative.
    pub fn bucket_hash(self) -> u64 {
        let mut hasher = StableFnv64::default();
        hasher.write(&self.initial_board_mask.to_le_bytes());
        hasher.write(&[self.placement_count]);
        hasher.write(&self.packed_piece_codes.to_le_bytes());
        for mask in &self.placement_masks[..self.placement_count()] {
            hasher.write(&mask.to_le_bytes());
        }
        hasher.finish()
    }

    fn canonical_string(self) -> String {
        let mut output = String::with_capacity(42 + self.placement_count() * 20);
        self.write_canonical(&mut output)
            .expect("writing to String cannot fail");
        output
    }

    fn write_canonical(self, output: &mut impl fmt::Write) -> fmt::Result {
        write!(
            output,
            "ctk1|initial={:016x}|placements=",
            self.initial_board_mask
        )?;
        for index in 0..self.placement_count() {
            if index != 0 {
                output.write_char(',')?;
            }
            let piece = piece_from_sort_key(self.piece_code(index));
            write!(
                output,
                "{}:{:016x}",
                piece.as_ascii(),
                self.placement_masks[index]
            )?;
        }
        Ok(())
    }

    const fn piece_code(self, index: usize) -> u8 {
        ((self.packed_piece_codes >> (index * 3)) & 0x7) as u8
    }
}

impl Ord for StandardBoard64TilingIdentity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.initial_board_mask
            .cmp(&other.initial_board_mask)
            .then_with(|| {
                let shared = self.placement_count().min(other.placement_count());
                for index in 0..shared {
                    let piece_order = piece_from_sort_key(self.piece_code(index))
                        .as_ascii()
                        .cmp(&piece_from_sort_key(other.piece_code(index)).as_ascii());
                    if piece_order != Ordering::Equal {
                        return piece_order;
                    }
                    let mask_order = self.placement_masks[index].cmp(&other.placement_masks[index]);
                    if mask_order != Ordering::Equal {
                        return mask_order;
                    }
                }
                self.placement_count.cmp(&other.placement_count)
            })
    }
}

impl PartialOrd for StandardBoard64TilingIdentity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for NormalizedTilingSolutionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedTilingSolutionSet {
    keys: Vec<NormalizedTilingSolutionKey>,
    hash: String,
}

impl NormalizedTilingSolutionSet {
    pub fn new(keys: impl IntoIterator<Item = NormalizedTilingSolutionKey>) -> Self {
        let mut keys = keys.into_iter().collect::<Vec<_>>();
        keys.sort_unstable();
        keys.dedup();
        let hash = stable_solution_set_hash(&keys);
        Self { keys, hash }
    }

    pub fn keys(&self) -> &[NormalizedTilingSolutionKey] {
        &self.keys
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedTilingSolutionError {
    InvalidCanonicalKey,
    EmptyPlacementMask,
    PlacementAreaNotFour { piece: PieceKind, area: u32 },
    OverlappingPlacement { piece: PieceKind },
    TooManyPlacements { count: usize, capacity: usize },
}

fn stable_solution_set_hash(keys: &[NormalizedTilingSolutionKey]) -> String {
    let mut hasher = StableFnv64::default();
    for key in keys {
        hasher.write(key.as_str().as_bytes());
        hasher.write(&[0]);
    }
    format!("cts1:{:016x}", hasher.finish())
}

pub fn normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
    identities: &[StandardBoard64TilingIdentity],
) -> String {
    let mut hasher = StableFnv64::default();
    for identity in identities {
        identity
            .write_canonical(&mut HasherWriter(&mut hasher))
            .expect("hash writer cannot fail");
        hasher.write(&[0]);
    }
    format!("cts1:{:016x}", hasher.finish())
}

const fn piece_sort_key(piece: PieceKind) -> u8 {
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

const fn piece_from_sort_key(key: u8) -> PieceKind {
    match key {
        0 => PieceKind::I,
        1 => PieceKind::O,
        2 => PieceKind::T,
        3 => PieceKind::S,
        4 => PieceKind::Z,
        5 => PieceKind::J,
        6 => PieceKind::L,
        _ => unreachable!(),
    }
}

#[derive(Default)]
struct StableFnv64(u64);

struct HasherWriter<'a>(&'a mut StableFnv64);

impl fmt::Write for HasherWriter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0.write(value.as_bytes());
        Ok(())
    }
}

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
