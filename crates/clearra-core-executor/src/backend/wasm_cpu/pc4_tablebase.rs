//! SRP rationale: this module has one change reason: the exact PC4 tablebase artifact contract,
//! including compilation, validation, installation, and certified lookup semantics.

use std::sync::{Arc, OnceLock, RwLock};

use clearra_problem::SearchProblem;
use sha2::{Digest, Sha256};

use super::{
    catalog::GeometryCatalog,
    geometry_domain::{DomainPropagation, DomainStatus},
    mix_digest, WasmExactSearchError,
};

const MAGIC: &[u8; 8] = b"CLR4TB12";
const SCHEMA_VERSION: u16 = 12;
const HEADER_BYTES: usize = 128;
const TIER_COMPACT_EXACT: u8 = 1;
const FLAG_EXACT_DEAD: u8 = 1 << 1;
const FLAG_PARTIAL: u8 = 1 << 2;
const FLAG_EXACT_PIECE_MASK_DEAD: u8 = 1 << 4;
const FLAG_EXACT_PIECE_COUNT_DEAD: u8 = 1 << 5;
const REQUIRED_FLAGS: u8 =
    FLAG_EXACT_DEAD | FLAG_PARTIAL | FLAG_EXACT_PIECE_MASK_DEAD | FLAG_EXACT_PIECE_COUNT_DEAD;
const ENCODING_GROUPED_EXACT_BITMAPS: u8 = 10;
const WIDTH: u8 = 10;
const HEIGHT: u8 = 4;
const CELL_COUNT: usize = 40;
const PIECE_COUNT: usize = CELL_COUNT / 4;
const FULL_FIELD: u64 = (1_u64 << CELL_COUNT) - 1;
const MAX_DELTA_VARINT_BYTES: usize = 6;
const PIECE_MASK_BITS: usize = 7;
const PIECE_MASK_BITMAP_BYTES: usize = 16;
const MAX_PIECE_MULTIPLICITY: u8 = 4;
const PIECE_COUNT_SIGNATURE_BITS: usize = 14;
const PIECE_COUNT_SIGNATURE_COUNT: usize = 13_925;
const PIECE_COUNT_BITMAP_WORDS: usize = PIECE_COUNT_SIGNATURE_COUNT.div_ceil(u64::BITS as usize);
const PIECE_COUNT_BITMAP_BYTES: usize = PIECE_COUNT_BITMAP_WORDS * core::mem::size_of::<u64>();
const COMPILER_MEMO_CAPACITY: usize = 1 << 26;
const COMPILER_MEMO_LOAD_LIMIT: usize = COMPILER_MEMO_CAPACITY * 7 / 10;
const MASKED_COMPILER_MEMO_CAPACITY: usize = 1 << 22;
const MASKED_COMPILER_MEMO_LOAD_LIMIT: usize = MASKED_COMPILER_MEMO_CAPACITY * 7 / 10;
const COUNTED_COMPILER_MEMO_CAPACITY: usize = 1 << 22;
const COUNTED_COMPILER_MEMO_LOAD_LIMIT: usize = COUNTED_COMPILER_MEMO_CAPACITY * 7 / 10;
const COMPILER_MEMO_SUCCESS: u64 = 1 << 63;
const COMPILER_MEMO_KEY_MASK: u64 = !COMPILER_MEMO_SUCCESS;
const ALL_PIECES_MASK: u8 = (1 << 7) - 1;

pub const PC4_COMPACT_TABLEBASE_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const PC4_COMPACT_TABLEBASE_MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;

static PC4_COMPACT_TABLEBASE: OnceLock<RwLock<Option<Arc<Pc4CompactTablebase>>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4TablebaseLookup {
    ExactDead,
    ExactResolved,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4TablebaseError {
    InvalidProblem(&'static str),
    InvalidArtifact(&'static str),
    SizeLimitExceeded { bytes: usize, maximum: usize },
    StateCountOverflow,
    CompilerStateCapacityExceeded,
}

impl Pc4TablebaseError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::InvalidProblem(reason) | Self::InvalidArtifact(reason) => reason,
            Self::SizeLimitExceeded { .. } => "pc4_tablebase_size_limit_exceeded",
            Self::StateCountOverflow => "pc4_tablebase_state_count_overflow",
            Self::CompilerStateCapacityExceeded => "pc4_tablebase_compiler_state_capacity_exceeded",
        }
    }
}

impl std::fmt::Display for Pc4TablebaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidProblem(reason) | Self::InvalidArtifact(reason) => {
                formatter.write_str(reason)
            }
            Self::SizeLimitExceeded { bytes, maximum } => {
                write!(
                    formatter,
                    "tablebase has {bytes} bytes; maximum is {maximum}"
                )
            }
            Self::StateCountOverflow | Self::CompilerStateCapacityExceeded => {
                formatter.write_str(self.reason())
            }
        }
    }
}

impl std::error::Error for Pc4TablebaseError {}

#[derive(Clone, Debug)]
pub struct Pc4CompactTablebaseArtifact {
    bytes: Vec<u8>,
    certified_state_count: u32,
    catalog_identity: u64,
    compiler_identity: u64,
    payload_sha256: [u8; 32],
}

impl Pc4CompactTablebaseArtifact {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub const fn certified_state_count(&self) -> u32 {
        self.certified_state_count
    }

    pub const fn catalog_identity(&self) -> u64 {
        self.catalog_identity
    }

    pub const fn compiler_identity(&self) -> u64 {
        self.compiler_identity
    }

    pub const fn payload_sha256(&self) -> [u8; 32] {
        self.payload_sha256
    }

    pub fn payload_sha256_hex(&self) -> String {
        hex_bytes(&self.payload_sha256)
    }
}

#[derive(Clone, Debug)]
pub struct Pc4CompactTablebase {
    exact_dead_fields: Box<[u64]>,
    exact_piece_mask_dead_groups: Box<[ExactPieceMaskDeadGroup]>,
    certified_target_counts: [u64; PIECE_COUNT_BITMAP_WORDS],
    exact_piece_count_dead_groups: Box<[ExactPieceCountDeadGroup]>,
    certified_state_count: u32,
    catalog_identity: u64,
    compiler_identity: u64,
    payload_sha256: [u8; 32],
    artifact_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExactPieceCountDeadGroup {
    placed_field: u64,
    count_bits: Box<[u64; PIECE_COUNT_BITMAP_WORDS]>,
}

impl ExactPieceCountDeadGroup {
    fn contains(&self, count_signature: u16) -> bool {
        let bit = usize::from(count_signature);
        self.count_bits[bit / u64::BITS as usize] & (1_u64 << (bit % u64::BITS as usize)) != 0
    }

    fn proof_count(&self) -> usize {
        self.count_bits
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExactPieceMaskDeadGroup {
    placed_field: u64,
    mask_bits: [u64; 2],
}

impl ExactPieceMaskDeadGroup {
    fn contains(self, piece_mask: u8) -> bool {
        let bit = usize::from(piece_mask);
        self.mask_bits[bit / 64] & (1_u64 << (bit % 64)) != 0
    }

    fn proof_count(self) -> usize {
        self.mask_bits
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }
}

impl Pc4CompactTablebase {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Pc4TablebaseError> {
        if bytes.len() > PC4_COMPACT_TABLEBASE_MAX_BYTES {
            return Err(Pc4TablebaseError::SizeLimitExceeded {
                bytes: bytes.len(),
                maximum: PC4_COMPACT_TABLEBASE_MAX_BYTES,
            });
        }
        if bytes.len() < HEADER_BYTES || &bytes[..MAGIC.len()] != MAGIC {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_header_invalid",
            ));
        }
        if read_u16(bytes, 8)? != SCHEMA_VERSION
            || usize::from(read_u16(bytes, 10)?) != HEADER_BYTES
            || bytes[12] != TIER_COMPACT_EXACT
            || bytes[13] != REQUIRED_FLAGS
            || bytes[14] != WIDTH
            || bytes[15] != HEIGHT
            || bytes[16] != ENCODING_GROUPED_EXACT_BITMAPS
            || usize::from(bytes[17]) != MAX_DELTA_VARINT_BYTES
            || usize::from(bytes[18]) != CELL_COUNT
            || bytes[19] != 0
            || usize::from(bytes[92]) != PIECE_MASK_BITS
            || usize::from(bytes[93]) != PIECE_MASK_BITMAP_BYTES
            || usize::from(bytes[94]) != PIECE_COUNT_SIGNATURE_BITS
            || bytes[95] != MAX_PIECE_MULTIPLICITY
            || usize::from(read_u16(bytes, 96)?) != PIECE_COUNT_BITMAP_BYTES
            || usize::from(bytes[98]) != PIECE_COUNT
            || bytes[99] != 0
            || read_u32(bytes, 116)? as usize != PIECE_COUNT_BITMAP_BYTES
            || bytes[124..128] != [0; 4]
        {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_contract_mismatch",
            ));
        }
        let certified_state_count = read_u32(bytes, 20)?;
        let payload_len = read_u32(bytes, 24)? as usize;
        let exact_dead_payload_len = read_u32(bytes, 76)? as usize;
        let exact_dead_state_count = read_u32(bytes, 80)?;
        let exact_piece_mask_dead_state_count = read_u32(bytes, 84)?;
        let exact_piece_mask_dead_group_count = read_u32(bytes, 88)?;
        let exact_piece_count_dead_state_count = read_u32(bytes, 100)?;
        let exact_piece_count_dead_group_count = read_u32(bytes, 104)?;
        let certified_target_count = read_u32(bytes, 108)?;
        let exact_piece_mask_payload_len = read_u32(bytes, 112)? as usize;
        let exact_piece_count_payload_len = read_u32(bytes, 120)? as usize;
        let section_state_count = exact_dead_state_count
            .checked_add(exact_piece_mask_dead_state_count)
            .and_then(|count| count.checked_add(exact_piece_count_dead_state_count))
            .ok_or(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_state_count_overflow",
            ))?;
        let maximum_payload_len = (exact_dead_state_count as usize)
            .checked_mul(MAX_DELTA_VARINT_BYTES)
            .and_then(|length| {
                (exact_piece_mask_dead_group_count as usize)
                    .checked_mul(MAX_DELTA_VARINT_BYTES + PIECE_MASK_BITMAP_BYTES)
                    .and_then(|grouped_length| length.checked_add(grouped_length))
            })
            .and_then(|length| length.checked_add(PIECE_COUNT_BITMAP_BYTES))
            .and_then(|length| {
                (exact_piece_count_dead_group_count as usize)
                    .checked_mul(MAX_DELTA_VARINT_BYTES + PIECE_COUNT_BITMAP_BYTES)
                    .and_then(|grouped_length| length.checked_add(grouped_length))
            })
            .ok_or(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_payload_length_overflow",
            ))?;
        let retained_bytes = (exact_dead_state_count as usize)
            .checked_mul(core::mem::size_of::<u64>())
            .and_then(|length| {
                (exact_piece_mask_dead_group_count as usize)
                    .checked_mul(core::mem::size_of::<ExactPieceMaskDeadGroup>())
                    .and_then(|grouped_length| length.checked_add(grouped_length))
            })
            .and_then(|length| length.checked_add(PIECE_COUNT_BITMAP_BYTES))
            .and_then(|length| {
                (exact_piece_count_dead_group_count as usize)
                    .checked_mul(core::mem::size_of::<ExactPieceCountDeadGroup>())
                    .and_then(|grouped_length| length.checked_add(grouped_length))
            })
            .ok_or(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_retained_length_overflow",
            ))?;
        if retained_bytes > PC4_COMPACT_TABLEBASE_MAX_RETAINED_BYTES {
            return Err(Pc4TablebaseError::SizeLimitExceeded {
                bytes: retained_bytes,
                maximum: PC4_COMPACT_TABLEBASE_MAX_RETAINED_BYTES,
            });
        }
        if certified_state_count == 0
            || exact_dead_state_count == 0
            || exact_piece_mask_dead_state_count == 0
            || exact_piece_mask_dead_group_count == 0
            || exact_piece_count_dead_state_count == 0
            || exact_piece_count_dead_group_count == 0
            || certified_target_count == 0
            || exact_piece_mask_dead_group_count > exact_piece_mask_dead_state_count
            || exact_piece_count_dead_group_count > exact_piece_count_dead_state_count
            || exact_piece_mask_dead_state_count
                > exact_piece_mask_dead_group_count.saturating_mul(u32::from(ALL_PIECES_MASK - 1))
            || exact_piece_count_dead_state_count
                > exact_piece_count_dead_group_count
                    .saturating_mul(PIECE_COUNT_SIGNATURE_COUNT as u32)
            || section_state_count != certified_state_count
            || payload_len > maximum_payload_len
            || !section_payload_length_valid(
                exact_dead_payload_len,
                exact_dead_state_count as usize,
            )
            || !grouped_payload_length_valid(
                exact_piece_mask_payload_len,
                exact_piece_mask_dead_group_count as usize,
                exact_piece_mask_dead_state_count as usize,
                usize::from(ALL_PIECES_MASK - 1),
                PIECE_MASK_BITMAP_BYTES,
            )
            || !grouped_payload_length_valid(
                exact_piece_count_payload_len,
                exact_piece_count_dead_group_count as usize,
                exact_piece_count_dead_state_count as usize,
                PIECE_COUNT_SIGNATURE_COUNT,
                PIECE_COUNT_BITMAP_BYTES,
            )
            || exact_dead_payload_len
                .checked_add(exact_piece_mask_payload_len)
                .and_then(|length| length.checked_add(PIECE_COUNT_BITMAP_BYTES))
                .and_then(|length| length.checked_add(exact_piece_count_payload_len))
                != Some(payload_len)
            || bytes.len().checked_sub(HEADER_BYTES) != Some(payload_len)
        {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_payload_length_mismatch",
            ));
        }
        let catalog_identity = read_u64(bytes, 28)?;
        let compiler_identity = read_u64(bytes, 36)?;
        let mut expected_sha256 = [0_u8; 32];
        expected_sha256.copy_from_slice(&bytes[44..76]);
        let payload = &bytes[HEADER_BYTES..];
        let actual_sha256: [u8; 32] = Sha256::digest(payload).into();
        if actual_sha256 != expected_sha256 {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_payload_sha256_mismatch",
            ));
        }
        let (exact_dead_payload, trailing_payload) = payload.split_at(exact_dead_payload_len);
        let (exact_piece_mask_dead_payload, trailing_payload) =
            trailing_payload.split_at(exact_piece_mask_payload_len);
        let (certified_target_payload, exact_piece_count_dead_payload) =
            trailing_payload.split_at(PIECE_COUNT_BITMAP_BYTES);
        let exact_dead_fields =
            decode_exact_dead_fields(exact_dead_payload, exact_dead_state_count as usize)?;
        let exact_piece_mask_dead_groups = decode_exact_piece_mask_dead_groups(
            exact_piece_mask_dead_payload,
            exact_piece_mask_dead_group_count as usize,
            exact_piece_mask_dead_state_count as usize,
        )?;
        let certified_target_counts =
            decode_piece_count_bitmap(certified_target_payload, certified_target_count as usize)?;
        let exact_piece_count_dead_groups = decode_exact_piece_count_dead_groups(
            exact_piece_count_dead_payload,
            exact_piece_count_dead_group_count as usize,
            exact_piece_count_dead_state_count as usize,
        )?;
        Ok(Self {
            exact_dead_fields,
            exact_piece_mask_dead_groups,
            certified_target_counts,
            exact_piece_count_dead_groups,
            certified_state_count,
            catalog_identity,
            compiler_identity,
            payload_sha256: expected_sha256,
            artifact_bytes: bytes.len(),
        })
    }

    pub fn lookup_placed_field(&self, placed_field: u64) -> Pc4TablebaseLookup {
        if placed_field == FULL_FIELD {
            return Pc4TablebaseLookup::ExactResolved;
        }
        if placed_field & !FULL_FIELD != 0 || !placed_field.count_ones().is_multiple_of(4) {
            return Pc4TablebaseLookup::ExactDead;
        }
        if self.exact_dead_fields.binary_search(&placed_field).is_ok() {
            Pc4TablebaseLookup::ExactDead
        } else {
            Pc4TablebaseLookup::Unknown
        }
    }

    pub(super) fn lookup_placed_field_with_piece_mask(
        &self,
        placed_field: u64,
        piece_mask: u8,
    ) -> Pc4TablebaseLookup {
        if placed_field == FULL_FIELD {
            return Pc4TablebaseLookup::ExactResolved;
        }
        if piece_mask & !ALL_PIECES_MASK != 0 {
            return Pc4TablebaseLookup::Unknown;
        }
        if piece_mask == 0
            || placed_field & !FULL_FIELD != 0
            || !placed_field.count_ones().is_multiple_of(4)
        {
            return Pc4TablebaseLookup::ExactDead;
        }
        if placed_field == 0 || piece_mask == ALL_PIECES_MASK {
            return Pc4TablebaseLookup::Unknown;
        }
        let Ok(group_index) = self
            .exact_piece_mask_dead_groups
            .binary_search_by_key(&placed_field, |group| group.placed_field)
        else {
            return Pc4TablebaseLookup::Unknown;
        };
        if self.exact_piece_mask_dead_groups[group_index].contains(piece_mask) {
            Pc4TablebaseLookup::ExactDead
        } else {
            Pc4TablebaseLookup::Unknown
        }
    }

    pub fn certifies_target_counts(&self, counts: [u8; 7]) -> bool {
        let Some(signature) = pack_piece_count_signature(counts) else {
            return false;
        };
        bitmap_contains(&self.certified_target_counts, signature)
    }

    pub(super) fn lookup_placed_field_with_remaining_counts(
        &self,
        placed_field: u64,
        remaining_counts: [u8; 7],
    ) -> Pc4TablebaseLookup {
        if placed_field == FULL_FIELD {
            return if remaining_counts.iter().all(|count| *count == 0) {
                Pc4TablebaseLookup::ExactResolved
            } else {
                Pc4TablebaseLookup::ExactDead
            };
        }
        if placed_field & !FULL_FIELD != 0 || !placed_field.count_ones().is_multiple_of(4) {
            return Pc4TablebaseLookup::ExactDead;
        }
        let Some(signature) = pack_piece_count_signature(remaining_counts) else {
            return Pc4TablebaseLookup::Unknown;
        };
        let remaining_piece_count = (CELL_COUNT as u32 - placed_field.count_ones()) / 4;
        if u32::from(piece_count_signature_total(signature)) != remaining_piece_count {
            return Pc4TablebaseLookup::ExactDead;
        }
        let Ok(group_index) = self
            .exact_piece_count_dead_groups
            .binary_search_by_key(&placed_field, |group| group.placed_field)
        else {
            return Pc4TablebaseLookup::Unknown;
        };
        if self.exact_piece_count_dead_groups[group_index].contains(signature) {
            Pc4TablebaseLookup::ExactDead
        } else {
            Pc4TablebaseLookup::Unknown
        }
    }

    pub const fn certified_state_count(&self) -> u32 {
        self.certified_state_count
    }

    pub fn certified_target_count(&self) -> usize {
        self.certified_target_counts
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    pub const fn catalog_identity(&self) -> u64 {
        self.catalog_identity
    }

    pub const fn compiler_identity(&self) -> u64 {
        self.compiler_identity
    }

    pub const fn payload_sha256(&self) -> [u8; 32] {
        self.payload_sha256
    }

    pub fn payload_sha256_hex(&self) -> String {
        hex_bytes(&self.payload_sha256)
    }

    pub const fn artifact_bytes(&self) -> usize {
        self.artifact_bytes
    }

    pub fn retained_bytes(&self) -> usize {
        core::mem::size_of_val(self.exact_dead_fields.as_ref())
            + core::mem::size_of_val(self.exact_piece_mask_dead_groups.as_ref())
            + core::mem::size_of_val(&self.certified_target_counts)
            + self.exact_piece_count_dead_groups.len()
                * (core::mem::size_of::<ExactPieceCountDeadGroup>() + PIECE_COUNT_BITMAP_BYTES)
    }
}

pub fn install_pc4_compact_tablebase(
    bytes: &[u8],
) -> Result<Arc<Pc4CompactTablebase>, Pc4TablebaseError> {
    let tablebase = Arc::new(Pc4CompactTablebase::from_bytes(bytes)?);
    let mut installed = tablebase_registry()
        .write()
        .map_err(|_| Pc4TablebaseError::InvalidArtifact("pc4_tablebase_registry_poisoned"))?;
    *installed = Some(Arc::clone(&tablebase));
    Ok(tablebase)
}

pub fn release_pc4_compact_tablebase() -> bool {
    let Ok(mut installed) = tablebase_registry().write() else {
        return false;
    };
    installed.take().is_some()
}

pub(super) fn loaded_pc4_compact_tablebase() -> Option<Arc<Pc4CompactTablebase>> {
    tablebase_registry()
        .read()
        .ok()
        .and_then(|installed| installed.as_ref().map(Arc::clone))
}

fn tablebase_registry() -> &'static RwLock<Option<Arc<Pc4CompactTablebase>>> {
    PC4_COMPACT_TABLEBASE.get_or_init(|| RwLock::new(None))
}

pub fn compile_pc4_compact_tablebase(
    problem: &SearchProblem,
) -> Result<Pc4CompactTablebaseArtifact, Pc4TablebaseError> {
    let catalog = GeometryCatalog::compile(problem).map_err(map_catalog_error)?;
    validate_catalog(&catalog)?;
    let compiler_identity = pc4_tablebase_profile_identity(problem, catalog.identity_digest());
    let mut compiler = ExactDeadCompiler::new()?;
    if !compiler.solve(&catalog, FULL_FIELD)? {
        return Err(Pc4TablebaseError::InvalidProblem(
            "pc4_tablebase_catalog_cannot_tile_target",
        ));
    }
    compiler.finish(&catalog, catalog.identity_digest(), compiler_identity)
}

pub(super) fn pc4_tablebase_profile_identity(
    problem: &SearchProblem,
    catalog_identity: u64,
) -> u64 {
    let mut digest = mix_digest(
        mix_digest(catalog_identity, u64::from(SCHEMA_VERSION)),
        0x5043_3454_4230_3401,
    );
    let kick_profile = problem.kick_profile();
    digest = mix_label(digest, kick_profile.profile_id().as_str());
    digest = mix_label(digest, kick_profile.source_rule().as_str());
    let spawn_profile = problem.spawn_profile();
    digest = mix_label(digest, spawn_profile.id().as_str());
    digest = mix_digest(digest, spawn_profile.x() as u16 as u64);
    digest = mix_digest(digest, spawn_profile.y() as u16 as u64);
    digest = mix_digest(digest, kick_profile.verified().into());
    digest = mix_digest(digest, kick_profile.supports_180().into());
    digest = mix_digest(digest, kick_profile.transition_count() as u64);
    mix_digest(digest, PIECE_COUNT as u64)
}

fn mix_label(mut digest: u64, label: &str) -> u64 {
    digest = mix_digest(digest, label.len() as u64);
    for byte in label.bytes() {
        digest = mix_digest(digest, u64::from(byte));
    }
    digest
}

fn validate_catalog(catalog: &GeometryCatalog) -> Result<(), Pc4TablebaseError> {
    if catalog.width() != WIDTH
        || catalog.height() != HEIGHT
        || catalog.initial_board() != 0
        || catalog.required_cells() != FULL_FIELD
    {
        return Err(Pc4TablebaseError::InvalidProblem(
            "pc4_tablebase_requires_empty_10_by_4_exact_target",
        ));
    }
    Ok(())
}

struct ExactStateMemo {
    slots: Vec<u64>,
    len: usize,
    load_limit: usize,
}

impl ExactStateMemo {
    fn new() -> Result<Self, Pc4TablebaseError> {
        Self::with_capacity(COMPILER_MEMO_CAPACITY, COMPILER_MEMO_LOAD_LIMIT)
    }

    fn new_masked() -> Result<Self, Pc4TablebaseError> {
        Self::with_capacity(
            MASKED_COMPILER_MEMO_CAPACITY,
            MASKED_COMPILER_MEMO_LOAD_LIMIT,
        )
    }

    fn new_counted() -> Result<Self, Pc4TablebaseError> {
        Self::with_capacity(
            COUNTED_COMPILER_MEMO_CAPACITY,
            COUNTED_COMPILER_MEMO_LOAD_LIMIT,
        )
    }

    fn with_capacity(capacity: usize, load_limit: usize) -> Result<Self, Pc4TablebaseError> {
        debug_assert!(capacity.is_power_of_two());
        debug_assert!(load_limit < capacity);
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(capacity)
            .map_err(|_| Pc4TablebaseError::CompilerStateCapacityExceeded)?;
        slots.resize(capacity, 0);
        Ok(Self {
            slots,
            len: 0,
            load_limit,
        })
    }

    fn lookup(&self, key: u64) -> Option<bool> {
        let mask = self.slots.len() - 1;
        let mut slot = mix_field(key) as usize & mask;
        loop {
            let stored = self.slots[slot];
            if stored == 0 {
                return None;
            }
            if stored & COMPILER_MEMO_KEY_MASK == key {
                return Some(stored & COMPILER_MEMO_SUCCESS != 0);
            }
            slot = (slot + 1) & mask;
        }
    }

    fn insert(&mut self, key: u64, success: bool) -> Result<(), Pc4TablebaseError> {
        if self.len >= self.load_limit {
            return Err(Pc4TablebaseError::CompilerStateCapacityExceeded);
        }
        let mask = self.slots.len() - 1;
        let mut slot = mix_field(key) as usize & mask;
        loop {
            let stored = self.slots[slot];
            if stored == 0 {
                self.slots[slot] = key | if success { COMPILER_MEMO_SUCCESS } else { 0 };
                self.len += 1;
                return Ok(());
            }
            if stored & COMPILER_MEMO_KEY_MASK == key {
                return Ok(());
            }
            slot = (slot + 1) & mask;
        }
    }
}

struct ExactDeadCompiler {
    memo: ExactStateMemo,
    exact_dead_fields_by_depth: [Vec<u64>; PIECE_COUNT + 1],
    canonical_viable_fields_by_depth: [Vec<u64>; PIECE_COUNT + 1],
}

impl ExactDeadCompiler {
    fn new() -> Result<Self, Pc4TablebaseError> {
        Ok(Self {
            memo: ExactStateMemo::new()?,
            exact_dead_fields_by_depth: std::array::from_fn(|_| Vec::new()),
            canonical_viable_fields_by_depth: std::array::from_fn(|_| Vec::new()),
        })
    }

    fn solve(
        &mut self,
        catalog: &GeometryCatalog,
        remaining: u64,
    ) -> Result<bool, Pc4TablebaseError> {
        let key = state_key(remaining);
        if let Some(success) = self.memo.lookup(key) {
            return Ok(success);
        }
        let success = if remaining == 0 {
            true
        } else {
            let (status, domain) =
                DomainPropagation::compile_minimum(catalog, remaining, ALL_PIECES_MASK);
            if status == DomainStatus::Empty {
                false
            } else {
                let mut any = false;
                for row_id in catalog.support(domain.pivot_cell).iter().copied() {
                    if !domain.row_allowed(catalog, row_id, remaining, ALL_PIECES_MASK) {
                        continue;
                    }
                    let row = catalog.skeleton(row_id);
                    any |= self.solve(catalog, remaining ^ row.cells)?;
                }
                any
            }
        };
        self.memo.insert(key, success)?;
        let placed_field = FULL_FIELD ^ remaining;
        let depth = placed_field.count_ones() as usize / 4;
        if placed_field != 0 && placed_field != FULL_FIELD {
            if success {
                self.canonical_viable_fields_by_depth[depth].push(placed_field);
            } else {
                self.exact_dead_fields_by_depth[depth].push(placed_field);
            }
        }
        Ok(success)
    }

    fn finish(
        mut self,
        catalog: &GeometryCatalog,
        catalog_identity: u64,
        compiler_identity: u64,
    ) -> Result<Pc4CompactTablebaseArtifact, Pc4TablebaseError> {
        let exact_dead_fields = collect_exact_fields(&mut self.exact_dead_fields_by_depth)?;
        let canonical_viable_fields =
            collect_exact_fields(&mut self.canonical_viable_fields_by_depth)?;
        if exact_dead_fields.is_empty() {
            return Err(Pc4TablebaseError::InvalidProblem(
                "pc4_tablebase_has_no_exact_dead_states",
            ));
        }
        if canonical_viable_fields.is_empty() {
            return Err(Pc4TablebaseError::InvalidProblem(
                "pc4_tablebase_has_no_canonical_viable_states",
            ));
        }
        drop(self.memo);
        let mut masked_memo = ExactStateMemo::new_masked()?;
        let maximum_masked_keys = canonical_viable_fields
            .len()
            .checked_mul(usize::from(ALL_PIECES_MASK - 1))
            .ok_or(Pc4TablebaseError::StateCountOverflow)?;
        let mut exact_piece_mask_dead_keys = Vec::new();
        exact_piece_mask_dead_keys
            .try_reserve_exact(maximum_masked_keys)
            .map_err(|_| Pc4TablebaseError::CompilerStateCapacityExceeded)?;
        for placed_field in canonical_viable_fields.iter().copied() {
            let remaining = FULL_FIELD ^ placed_field;
            for piece_mask in 1..ALL_PIECES_MASK {
                if !solve_with_piece_mask(catalog, remaining, piece_mask, &mut masked_memo)? {
                    exact_piece_mask_dead_keys.push(piece_mask_dead_key(placed_field, piece_mask));
                }
            }
        }
        exact_piece_mask_dead_keys.sort_unstable();
        exact_piece_mask_dead_keys.dedup();
        if exact_piece_mask_dead_keys.is_empty() {
            return Err(Pc4TablebaseError::InvalidProblem(
                "pc4_tablebase_has_no_exact_piece_mask_dead_states",
            ));
        }
        drop(masked_memo);
        let certified_target_signatures = enumerate_piece_count_signatures(PIECE_COUNT as u8);
        let certified_target_counts = bitmap_from_signatures(&certified_target_signatures);
        let signatures_by_remaining_depth: [Vec<u16>; PIECE_COUNT + 1] =
            std::array::from_fn(|remaining_depth| {
                enumerate_piece_count_signatures(remaining_depth as u8)
            });
        let mut counted_memo = ExactStateMemo::new_counted()?;
        let mut exact_piece_count_dead_groups = Vec::new();
        exact_piece_count_dead_groups
            .try_reserve_exact(canonical_viable_fields.len())
            .map_err(|_| Pc4TablebaseError::CompilerStateCapacityExceeded)?;
        for placed_field in canonical_viable_fields {
            let depth = placed_field.count_ones() as usize / 4;
            let remaining = FULL_FIELD ^ placed_field;
            let mut count_bits = Box::new([0_u64; PIECE_COUNT_BITMAP_WORDS]);
            for signature in signatures_by_remaining_depth[PIECE_COUNT - depth]
                .iter()
                .copied()
            {
                let counts = unpack_piece_count_signature(signature);
                if !solve_with_piece_counts(catalog, remaining, counts, &mut counted_memo)? {
                    bitmap_insert(count_bits.as_mut(), signature);
                }
            }
            if count_bits.iter().any(|word| *word != 0) {
                exact_piece_count_dead_groups.push(ExactPieceCountDeadGroup {
                    placed_field,
                    count_bits,
                });
            }
        }
        if exact_piece_count_dead_groups.is_empty() {
            return Err(Pc4TablebaseError::InvalidProblem(
                "pc4_tablebase_has_no_exact_piece_count_dead_states",
            ));
        }
        encode_artifact(
            exact_dead_fields,
            exact_piece_mask_dead_keys,
            certified_target_counts,
            exact_piece_count_dead_groups,
            catalog_identity,
            compiler_identity,
        )
    }
}

fn collect_exact_fields(fields_by_depth: &mut [Vec<u64>]) -> Result<Vec<u64>, Pc4TablebaseError> {
    let total = fields_by_depth.iter().try_fold(0_usize, |total, fields| {
        total
            .checked_add(fields.len())
            .ok_or(Pc4TablebaseError::StateCountOverflow)
    })?;
    let mut all_fields = Vec::new();
    all_fields
        .try_reserve_exact(total)
        .map_err(|_| Pc4TablebaseError::CompilerStateCapacityExceeded)?;
    for fields in fields_by_depth {
        fields.sort_unstable();
        fields.dedup();
        all_fields.extend_from_slice(fields);
    }
    all_fields.sort_unstable();
    all_fields.dedup();
    Ok(all_fields)
}

fn solve_with_piece_mask(
    catalog: &GeometryCatalog,
    remaining: u64,
    piece_mask: u8,
    memo: &mut ExactStateMemo,
) -> Result<bool, Pc4TablebaseError> {
    let key = masked_state_key(remaining, piece_mask);
    if let Some(success) = memo.lookup(key) {
        return Ok(success);
    }
    let success = if remaining == 0 {
        true
    } else {
        let (status, domain) = DomainPropagation::compile_minimum(catalog, remaining, piece_mask);
        if status == DomainStatus::Empty {
            false
        } else {
            let mut found = false;
            for row_id in catalog.support(domain.pivot_cell).iter().copied() {
                if !domain.row_allowed(catalog, row_id, remaining, piece_mask) {
                    continue;
                }
                let row = catalog.skeleton(row_id);
                if solve_with_piece_mask(catalog, remaining ^ row.cells, piece_mask, memo)? {
                    found = true;
                    break;
                }
            }
            found
        }
    };
    memo.insert(key, success)?;
    Ok(success)
}

fn solve_with_piece_counts(
    catalog: &GeometryCatalog,
    remaining: u64,
    remaining_counts: [u8; 7],
    memo: &mut ExactStateMemo,
) -> Result<bool, Pc4TablebaseError> {
    let signature = pack_piece_count_signature(remaining_counts).ok_or(
        Pc4TablebaseError::InvalidProblem("pc4_tablebase_piece_count_signature_invalid"),
    )?;
    let key = counted_state_key(remaining, signature);
    if let Some(success) = memo.lookup(key) {
        return Ok(success);
    }
    let required_piece_count = remaining.count_ones() as u8 / 4;
    let success = if piece_count_signature_total(signature) != required_piece_count {
        false
    } else if remaining == 0 {
        true
    } else {
        let piece_mask = piece_mask_for_counts(remaining_counts);
        let (status, domain) = DomainPropagation::compile_minimum(catalog, remaining, piece_mask);
        if status == DomainStatus::Empty {
            false
        } else {
            let mut found = false;
            for row_id in catalog.support(domain.pivot_cell).iter().copied() {
                if !domain.row_allowed(catalog, row_id, remaining, piece_mask) {
                    continue;
                }
                let row = catalog.skeleton(row_id);
                let piece = super::piece_index(row.piece);
                if remaining_counts[piece] == 0 {
                    continue;
                }
                let mut child_counts = remaining_counts;
                child_counts[piece] -= 1;
                if solve_with_piece_counts(catalog, remaining ^ row.cells, child_counts, memo)? {
                    found = true;
                    break;
                }
            }
            found
        }
    };
    memo.insert(key, success)?;
    Ok(success)
}

fn enumerate_piece_count_signatures(total: u8) -> Vec<u16> {
    fn visit(piece: usize, remaining: u8, counts: &mut [u8; 7], output: &mut Vec<u16>) {
        if piece + 1 == counts.len() {
            if remaining <= MAX_PIECE_MULTIPLICITY {
                counts[piece] = remaining;
                output.push(
                    pack_piece_count_signature(*counts)
                        .expect("bounded piece counts have a compact signature"),
                );
            }
            return;
        }
        for count in 0..=remaining.min(MAX_PIECE_MULTIPLICITY) {
            counts[piece] = count;
            visit(piece + 1, remaining - count, counts, output);
        }
    }

    let mut output = Vec::new();
    visit(0, total, &mut [0; 7], &mut output);
    output.sort_unstable();
    output
}

fn pack_piece_count_signature(counts: [u8; 7]) -> Option<u16> {
    let mut prefix = 0_usize;
    let mut rank = 0_usize;
    for (index, count) in counts.into_iter().enumerate() {
        if count > MAX_PIECE_MULTIPLICITY || prefix + usize::from(count) > PIECE_COUNT {
            return None;
        }
        let remaining_slots = 7 - index - 1;
        for candidate in 0..count {
            let budget = PIECE_COUNT - prefix - usize::from(candidate);
            rank += usize::from(PIECE_COUNT_SUFFIX_COUNTS[remaining_slots][budget]);
        }
        prefix += usize::from(count);
    }
    u16::try_from(rank)
        .ok()
        .filter(|rank| usize::from(*rank) < PIECE_COUNT_SIGNATURE_COUNT)
}

fn unpack_piece_count_signature(signature: u16) -> [u8; 7] {
    debug_assert!(usize::from(signature) < PIECE_COUNT_SIGNATURE_COUNT);
    let mut rank = usize::from(signature);
    let mut budget = PIECE_COUNT;
    let mut counts = [0_u8; 7];
    for (index, count) in counts.iter_mut().enumerate() {
        let remaining_slots = 7 - index - 1;
        for candidate in 0..=MAX_PIECE_MULTIPLICITY.min(budget as u8) {
            let block = usize::from(
                PIECE_COUNT_SUFFIX_COUNTS[remaining_slots][budget - usize::from(candidate)],
            );
            if rank < block {
                *count = candidate;
                budget -= usize::from(candidate);
                break;
            }
            rank -= block;
        }
    }
    debug_assert_eq!(rank, 0);
    counts
}

fn piece_count_signature_total(signature: u16) -> u8 {
    unpack_piece_count_signature(signature)
        .iter()
        .copied()
        .sum()
}

fn piece_mask_for_counts(counts: [u8; 7]) -> u8 {
    counts
        .iter()
        .copied()
        .enumerate()
        .fold(0_u8, |mask, (piece, count)| {
            mask | (u8::from(count != 0) << piece)
        })
}

fn bitmap_from_signatures(signatures: &[u16]) -> [u64; PIECE_COUNT_BITMAP_WORDS] {
    let mut bitmap = [0_u64; PIECE_COUNT_BITMAP_WORDS];
    for signature in signatures.iter().copied() {
        bitmap_insert(&mut bitmap, signature);
    }
    bitmap
}

fn bitmap_insert(bitmap: &mut [u64; PIECE_COUNT_BITMAP_WORDS], signature: u16) {
    let bit = usize::from(signature);
    bitmap[bit / u64::BITS as usize] |= 1_u64 << (bit % u64::BITS as usize);
}

fn bitmap_contains(bitmap: &[u64; PIECE_COUNT_BITMAP_WORDS], signature: u16) -> bool {
    let bit = usize::from(signature);
    bitmap[bit / u64::BITS as usize] & (1_u64 << (bit % u64::BITS as usize)) != 0
}

fn piece_count_bitmap_padding_clear(bitmap: &[u64; PIECE_COUNT_BITMAP_WORDS]) -> bool {
    let used_bits = PIECE_COUNT_SIGNATURE_COUNT % u64::BITS as usize;
    used_bits == 0
        || bitmap
            .last()
            .is_some_and(|word| *word & (!0_u64 << used_bits) == 0)
}

fn set_piece_count_signatures(
    bitmap: &[u64; PIECE_COUNT_BITMAP_WORDS],
) -> impl Iterator<Item = u16> + '_ {
    (0..PIECE_COUNT_SIGNATURE_COUNT).filter_map(|signature| {
        let signature = signature as u16;
        bitmap_contains(bitmap, signature).then_some(signature)
    })
}

const PIECE_COUNT_SUFFIX_COUNTS: [[u16; PIECE_COUNT + 1]; 8] = build_piece_count_suffix_counts();
const _: () =
    assert!(PIECE_COUNT_SUFFIX_COUNTS[7][PIECE_COUNT] as usize == PIECE_COUNT_SIGNATURE_COUNT);

const fn build_piece_count_suffix_counts() -> [[u16; PIECE_COUNT + 1]; 8] {
    let mut counts = [[0_u16; PIECE_COUNT + 1]; 8];
    let mut budget = 0;
    while budget <= PIECE_COUNT {
        counts[0][budget] = 1;
        budget += 1;
    }
    let mut slots = 1;
    while slots < counts.len() {
        budget = 0;
        while budget <= PIECE_COUNT {
            let mut value = 0;
            let mut total = 0_u16;
            while value <= MAX_PIECE_MULTIPLICITY as usize && value <= budget {
                total += counts[slots - 1][budget - value];
                value += 1;
            }
            counts[slots][budget] = total;
            budget += 1;
        }
        slots += 1;
    }
    counts
}

fn state_key(remaining: u64) -> u64 {
    debug_assert_eq!(remaining & !FULL_FIELD, 0);
    let key = remaining + 1;
    debug_assert_eq!(key & !COMPILER_MEMO_KEY_MASK, 0);
    key
}

fn masked_state_key(remaining: u64, piece_mask: u8) -> u64 {
    debug_assert_eq!(remaining & !FULL_FIELD, 0);
    debug_assert_ne!(piece_mask, 0);
    debug_assert_eq!(piece_mask & !ALL_PIECES_MASK, 0);
    let key = ((remaining + 1) << PIECE_MASK_BITS) | u64::from(piece_mask);
    debug_assert_eq!(key & !COMPILER_MEMO_KEY_MASK, 0);
    key
}

fn counted_state_key(remaining: u64, count_signature: u16) -> u64 {
    debug_assert_eq!(remaining & !FULL_FIELD, 0);
    let key = ((remaining + 1) << PIECE_COUNT_SIGNATURE_BITS) | u64::from(count_signature);
    debug_assert_eq!(key & !COMPILER_MEMO_KEY_MASK, 0);
    key
}

fn piece_mask_dead_key(placed_field: u64, piece_mask: u8) -> u64 {
    debug_assert_ne!(placed_field, 0);
    debug_assert_ne!(placed_field, FULL_FIELD);
    debug_assert_ne!(piece_mask, 0);
    debug_assert_ne!(piece_mask, ALL_PIECES_MASK);
    (placed_field << PIECE_MASK_BITS) | u64::from(piece_mask)
}

fn encode_artifact(
    exact_dead_fields: Vec<u64>,
    exact_piece_mask_dead_keys: Vec<u64>,
    certified_target_counts: [u64; PIECE_COUNT_BITMAP_WORDS],
    exact_piece_count_dead_groups: Vec<ExactPieceCountDeadGroup>,
    catalog_identity: u64,
    compiler_identity: u64,
) -> Result<Pc4CompactTablebaseArtifact, Pc4TablebaseError> {
    let exact_piece_count_dead_state_count = exact_piece_count_dead_groups
        .iter()
        .try_fold(0_usize, |count, group| {
            count.checked_add(group.proof_count())
        })
        .ok_or(Pc4TablebaseError::StateCountOverflow)?;
    let total_state_count = exact_dead_fields
        .len()
        .checked_add(exact_piece_mask_dead_keys.len())
        .and_then(|count| count.checked_add(exact_piece_count_dead_state_count))
        .ok_or(Pc4TablebaseError::StateCountOverflow)?;
    let certified_state_count =
        u32::try_from(total_state_count).map_err(|_| Pc4TablebaseError::StateCountOverflow)?;
    let exact_dead_state_count = u32::try_from(exact_dead_fields.len())
        .map_err(|_| Pc4TablebaseError::StateCountOverflow)?;
    let exact_piece_mask_dead_state_count = u32::try_from(exact_piece_mask_dead_keys.len())
        .map_err(|_| Pc4TablebaseError::StateCountOverflow)?;
    let exact_piece_mask_dead_groups =
        group_exact_piece_mask_dead_keys(&exact_piece_mask_dead_keys)?;
    let exact_piece_mask_dead_group_count = u32::try_from(exact_piece_mask_dead_groups.len())
        .map_err(|_| Pc4TablebaseError::StateCountOverflow)?;
    let exact_piece_count_dead_state_count = u32::try_from(exact_piece_count_dead_state_count)
        .map_err(|_| Pc4TablebaseError::StateCountOverflow)?;
    let exact_piece_count_dead_group_count = u32::try_from(exact_piece_count_dead_groups.len())
        .map_err(|_| Pc4TablebaseError::StateCountOverflow)?;
    let certified_target_count = certified_target_counts
        .iter()
        .map(|word| word.count_ones())
        .sum::<u32>();
    let exact_dead_payload = encode_exact_dead_fields(&exact_dead_fields)?;
    let exact_piece_mask_dead_payload =
        encode_exact_piece_mask_dead_groups(&exact_piece_mask_dead_groups)?;
    let certified_target_payload = encode_piece_count_bitmap(&certified_target_counts);
    let exact_piece_count_dead_payload =
        encode_exact_piece_count_dead_groups(&exact_piece_count_dead_groups)?;
    let payload_len = exact_dead_payload
        .len()
        .checked_add(exact_piece_mask_dead_payload.len())
        .and_then(|length| length.checked_add(certified_target_payload.len()))
        .and_then(|length| length.checked_add(exact_piece_count_dead_payload.len()))
        .ok_or(Pc4TablebaseError::StateCountOverflow)?;
    let total_len = HEADER_BYTES
        .checked_add(payload_len)
        .ok_or(Pc4TablebaseError::StateCountOverflow)?;
    if total_len > PC4_COMPACT_TABLEBASE_MAX_BYTES {
        return Err(Pc4TablebaseError::SizeLimitExceeded {
            bytes: total_len,
            maximum: PC4_COMPACT_TABLEBASE_MAX_BYTES,
        });
    }
    let mut bytes = vec![0_u8; total_len];
    bytes[..8].copy_from_slice(MAGIC);
    write_u16(&mut bytes, 8, SCHEMA_VERSION);
    write_u16(&mut bytes, 10, HEADER_BYTES as u16);
    bytes[12] = TIER_COMPACT_EXACT;
    bytes[13] = REQUIRED_FLAGS;
    bytes[14] = WIDTH;
    bytes[15] = HEIGHT;
    bytes[16] = ENCODING_GROUPED_EXACT_BITMAPS;
    bytes[17] = MAX_DELTA_VARINT_BYTES as u8;
    bytes[18] = CELL_COUNT as u8;
    write_u32(&mut bytes, 20, certified_state_count);
    write_u32(
        &mut bytes,
        24,
        u32::try_from(payload_len).map_err(|_| Pc4TablebaseError::StateCountOverflow)?,
    );
    write_u64(&mut bytes, 28, catalog_identity);
    write_u64(&mut bytes, 36, compiler_identity);
    write_u32(
        &mut bytes,
        76,
        u32::try_from(exact_dead_payload.len())
            .map_err(|_| Pc4TablebaseError::StateCountOverflow)?,
    );
    write_u32(&mut bytes, 80, exact_dead_state_count);
    write_u32(&mut bytes, 84, exact_piece_mask_dead_state_count);
    write_u32(&mut bytes, 88, exact_piece_mask_dead_group_count);
    bytes[92] = PIECE_MASK_BITS as u8;
    bytes[93] = PIECE_MASK_BITMAP_BYTES as u8;
    bytes[94] = PIECE_COUNT_SIGNATURE_BITS as u8;
    bytes[95] = MAX_PIECE_MULTIPLICITY;
    write_u16(&mut bytes, 96, PIECE_COUNT_BITMAP_BYTES as u16);
    bytes[98] = PIECE_COUNT as u8;
    write_u32(&mut bytes, 100, exact_piece_count_dead_state_count);
    write_u32(&mut bytes, 104, exact_piece_count_dead_group_count);
    write_u32(&mut bytes, 108, certified_target_count);
    write_u32(
        &mut bytes,
        112,
        u32::try_from(exact_piece_mask_dead_payload.len())
            .map_err(|_| Pc4TablebaseError::StateCountOverflow)?,
    );
    write_u32(&mut bytes, 116, PIECE_COUNT_BITMAP_BYTES as u32);
    write_u32(
        &mut bytes,
        120,
        u32::try_from(exact_piece_count_dead_payload.len())
            .map_err(|_| Pc4TablebaseError::StateCountOverflow)?,
    );
    let mut payload_cursor = HEADER_BYTES;
    for section in [
        exact_dead_payload.as_slice(),
        exact_piece_mask_dead_payload.as_slice(),
        certified_target_payload.as_slice(),
        exact_piece_count_dead_payload.as_slice(),
    ] {
        let end = payload_cursor + section.len();
        bytes[payload_cursor..end].copy_from_slice(section);
        payload_cursor = end;
    }
    debug_assert_eq!(payload_cursor, bytes.len());
    let payload_sha256: [u8; 32] = Sha256::digest(&bytes[HEADER_BYTES..]).into();
    bytes[44..76].copy_from_slice(&payload_sha256);
    Ok(Pc4CompactTablebaseArtifact {
        bytes,
        certified_state_count,
        catalog_identity,
        compiler_identity,
        payload_sha256,
    })
}

fn encode_exact_dead_fields(fields: &[u64]) -> Result<Vec<u8>, Pc4TablebaseError> {
    for field in fields.iter().copied() {
        if field == 0
            || field == FULL_FIELD
            || field & !FULL_FIELD != 0
            || field.count_ones() % 4 != 0
        {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_dead_field_invalid",
            ));
        }
    }
    encode_sorted_values(fields)
}

fn group_exact_piece_mask_dead_keys(
    keys: &[u64],
) -> Result<Vec<ExactPieceMaskDeadGroup>, Pc4TablebaseError> {
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(keys.len())
        .map_err(|_| Pc4TablebaseError::CompilerStateCapacityExceeded)?;
    let mut previous_key = 0_u64;
    for key in keys.iter().copied() {
        if key <= previous_key || !piece_mask_dead_key_valid(key) {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_mask_dead_key_invalid",
            ));
        }
        previous_key = key;
        let piece_mask = (key & u64::from(ALL_PIECES_MASK)) as u8;
        let placed_field = key >> PIECE_MASK_BITS;
        if groups
            .last()
            .is_none_or(|group: &ExactPieceMaskDeadGroup| group.placed_field != placed_field)
        {
            groups.push(ExactPieceMaskDeadGroup {
                placed_field,
                mask_bits: [0; 2],
            });
        }
        let group = groups
            .last_mut()
            .expect("piece-mask dead group exists after insertion");
        let bit = usize::from(piece_mask);
        group.mask_bits[bit / 64] |= 1_u64 << (bit % 64);
    }
    if groups.is_empty() {
        return Err(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_exact_piece_mask_dead_groups_empty",
        ));
    }
    Ok(groups)
}

fn encode_exact_piece_mask_dead_groups(
    groups: &[ExactPieceMaskDeadGroup],
) -> Result<Vec<u8>, Pc4TablebaseError> {
    let maximum_payload_len = groups
        .len()
        .checked_mul(MAX_DELTA_VARINT_BYTES + PIECE_MASK_BITMAP_BYTES)
        .ok_or(Pc4TablebaseError::StateCountOverflow)?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(maximum_payload_len)
        .map_err(|_| Pc4TablebaseError::CompilerStateCapacityExceeded)?;
    let mut previous_field = 0_u64;
    for group in groups.iter().copied() {
        if group.placed_field <= previous_field || !piece_mask_dead_group_valid(group) {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_mask_dead_group_invalid",
            ));
        }
        write_varint(&mut payload, group.placed_field - previous_field);
        payload.extend_from_slice(&group.mask_bits[0].to_le_bytes());
        payload.extend_from_slice(&group.mask_bits[1].to_le_bytes());
        previous_field = group.placed_field;
    }
    Ok(payload)
}

fn encode_piece_count_bitmap(bitmap: &[u64; PIECE_COUNT_BITMAP_WORDS]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(PIECE_COUNT_BITMAP_BYTES);
    for word in bitmap {
        payload.extend_from_slice(&word.to_le_bytes());
    }
    payload
}

fn encode_exact_piece_count_dead_groups(
    groups: &[ExactPieceCountDeadGroup],
) -> Result<Vec<u8>, Pc4TablebaseError> {
    let maximum_payload_len = groups
        .len()
        .checked_mul(MAX_DELTA_VARINT_BYTES + PIECE_COUNT_BITMAP_BYTES)
        .ok_or(Pc4TablebaseError::StateCountOverflow)?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(maximum_payload_len)
        .map_err(|_| Pc4TablebaseError::CompilerStateCapacityExceeded)?;
    let mut previous_field = 0_u64;
    for group in groups {
        if group.placed_field <= previous_field || !piece_count_dead_group_valid(group) {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_count_dead_group_invalid",
            ));
        }
        write_varint(&mut payload, group.placed_field - previous_field);
        for word in group.count_bits.iter() {
            payload.extend_from_slice(&word.to_le_bytes());
        }
        previous_field = group.placed_field;
    }
    Ok(payload)
}

fn encode_sorted_values(values: &[u64]) -> Result<Vec<u8>, Pc4TablebaseError> {
    let maximum_payload_len = values
        .len()
        .checked_mul(MAX_DELTA_VARINT_BYTES)
        .ok_or(Pc4TablebaseError::StateCountOverflow)?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(maximum_payload_len)
        .map_err(|_| Pc4TablebaseError::CompilerStateCapacityExceeded)?;
    let mut previous = 0_u64;
    for value in values.iter().copied() {
        if value == 0 || value <= previous {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_dead_value_order_invalid",
            ));
        }
        write_varint(&mut payload, value - previous);
        previous = value;
    }
    Ok(payload)
}

fn decode_exact_dead_fields(
    payload: &[u8],
    expected_count: usize,
) -> Result<Box<[u64]>, Pc4TablebaseError> {
    let fields = decode_sorted_values(payload, expected_count)?;
    if fields.iter().copied().any(|field| {
        field == 0 || field == FULL_FIELD || field & !FULL_FIELD != 0 || field.count_ones() % 4 != 0
    }) {
        return Err(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_exact_dead_field_invalid",
        ));
    }
    Ok(fields)
}

fn decode_exact_piece_mask_dead_groups(
    payload: &[u8],
    expected_group_count: usize,
    expected_proof_count: usize,
) -> Result<Box<[ExactPieceMaskDeadGroup]>, Pc4TablebaseError> {
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(expected_group_count)
        .map_err(|_| {
            Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_mask_dead_allocation_failed",
            )
        })?;
    let mut cursor = 0;
    let mut previous_field = 0_u64;
    let mut actual_proof_count = 0_usize;
    for _ in 0..expected_group_count {
        let delta = read_varint(payload, &mut cursor)?;
        let placed_field =
            previous_field
                .checked_add(delta)
                .ok_or(Pc4TablebaseError::InvalidArtifact(
                    "pc4_tablebase_exact_piece_mask_dead_field_overflow",
                ))?;
        let bitmap = payload
            .get(cursor..cursor + PIECE_MASK_BITMAP_BYTES)
            .ok_or(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_mask_dead_bitmap_truncated",
            ))?;
        cursor += PIECE_MASK_BITMAP_BYTES;
        let group = ExactPieceMaskDeadGroup {
            placed_field,
            mask_bits: [
                u64::from_le_bytes(bitmap[..8].try_into().expect("eight-byte low bitmap")),
                u64::from_le_bytes(bitmap[8..].try_into().expect("eight-byte high bitmap")),
            ],
        };
        if placed_field <= previous_field || !piece_mask_dead_group_valid(group) {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_mask_dead_group_invalid",
            ));
        }
        actual_proof_count = actual_proof_count.checked_add(group.proof_count()).ok_or(
            Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_mask_dead_count_overflow",
            ),
        )?;
        groups.push(group);
        previous_field = placed_field;
    }
    if cursor != payload.len() || actual_proof_count != expected_proof_count {
        return Err(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_exact_piece_mask_dead_payload_mismatch",
        ));
    }
    Ok(groups.into_boxed_slice())
}

fn decode_piece_count_bitmap(
    payload: &[u8],
    expected_count: usize,
) -> Result<[u64; PIECE_COUNT_BITMAP_WORDS], Pc4TablebaseError> {
    if payload.len() != PIECE_COUNT_BITMAP_BYTES {
        return Err(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_piece_count_bitmap_length_invalid",
        ));
    }
    let mut bitmap = [0_u64; PIECE_COUNT_BITMAP_WORDS];
    for (index, word) in bitmap.iter_mut().enumerate() {
        let start = index * core::mem::size_of::<u64>();
        *word = u64::from_le_bytes(
            payload[start..start + core::mem::size_of::<u64>()]
                .try_into()
                .expect("piece-count bitmap word"),
        );
    }
    let actual_count = bitmap
        .iter()
        .map(|word| word.count_ones() as usize)
        .sum::<usize>();
    if actual_count != expected_count
        || !piece_count_bitmap_padding_clear(&bitmap)
        || set_piece_count_signatures(&bitmap)
            .any(|signature| piece_count_signature_total(signature) != PIECE_COUNT as u8)
    {
        return Err(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_certified_target_bitmap_invalid",
        ));
    }
    Ok(bitmap)
}

fn decode_exact_piece_count_dead_groups(
    payload: &[u8],
    expected_group_count: usize,
    expected_proof_count: usize,
) -> Result<Box<[ExactPieceCountDeadGroup]>, Pc4TablebaseError> {
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(expected_group_count)
        .map_err(|_| {
            Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_count_dead_allocation_failed",
            )
        })?;
    let mut cursor = 0;
    let mut previous_field = 0_u64;
    let mut actual_proof_count = 0_usize;
    for _ in 0..expected_group_count {
        let delta = read_varint(payload, &mut cursor)?;
        let placed_field =
            previous_field
                .checked_add(delta)
                .ok_or(Pc4TablebaseError::InvalidArtifact(
                    "pc4_tablebase_exact_piece_count_dead_field_overflow",
                ))?;
        let bitmap = payload
            .get(cursor..cursor + PIECE_COUNT_BITMAP_BYTES)
            .ok_or(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_count_dead_bitmap_truncated",
            ))?;
        cursor += PIECE_COUNT_BITMAP_BYTES;
        let mut count_bits = Box::new([0_u64; PIECE_COUNT_BITMAP_WORDS]);
        for (index, word) in count_bits.iter_mut().enumerate() {
            let start = index * core::mem::size_of::<u64>();
            *word = u64::from_le_bytes(
                bitmap[start..start + core::mem::size_of::<u64>()]
                    .try_into()
                    .expect("piece-count dead bitmap word"),
            );
        }
        let group = ExactPieceCountDeadGroup {
            placed_field,
            count_bits,
        };
        if placed_field <= previous_field || !piece_count_dead_group_valid(&group) {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_count_dead_group_invalid",
            ));
        }
        actual_proof_count = actual_proof_count.checked_add(group.proof_count()).ok_or(
            Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_piece_count_dead_count_overflow",
            ),
        )?;
        groups.push(group);
        previous_field = placed_field;
    }
    if cursor != payload.len() || actual_proof_count != expected_proof_count {
        return Err(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_exact_piece_count_dead_payload_mismatch",
        ));
    }
    Ok(groups.into_boxed_slice())
}

fn decode_sorted_values(
    payload: &[u8],
    expected_count: usize,
) -> Result<Box<[u64]>, Pc4TablebaseError> {
    let mut values = Vec::new();
    values.try_reserve_exact(expected_count).map_err(|_| {
        Pc4TablebaseError::InvalidArtifact("pc4_tablebase_exact_dead_allocation_failed")
    })?;
    let mut cursor = 0;
    let mut previous = 0_u64;
    for _ in 0..expected_count {
        let delta = read_varint(payload, &mut cursor)?;
        let value = previous
            .checked_add(delta)
            .ok_or(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_dead_value_overflow",
            ))?;
        if value == 0 || value <= previous {
            return Err(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_dead_value_order_invalid",
            ));
        }
        values.push(value);
        previous = value;
    }
    if cursor != payload.len() {
        return Err(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_exact_dead_payload_trailing_bytes",
        ));
    }
    Ok(values.into_boxed_slice())
}

fn piece_mask_dead_key_valid(key: u64) -> bool {
    let piece_mask = (key & u64::from(ALL_PIECES_MASK)) as u8;
    let placed_field = key >> PIECE_MASK_BITS;
    piece_mask != 0
        && piece_mask != ALL_PIECES_MASK
        && placed_field != 0
        && placed_field != FULL_FIELD
        && placed_field & !FULL_FIELD == 0
        && placed_field.count_ones().is_multiple_of(4)
}

fn piece_mask_dead_group_valid(group: ExactPieceMaskDeadGroup) -> bool {
    group.placed_field != 0
        && group.placed_field != FULL_FIELD
        && group.placed_field & !FULL_FIELD == 0
        && group.placed_field.count_ones().is_multiple_of(4)
        && group.proof_count() != 0
        && group.mask_bits[0] & 1 == 0
        && group.mask_bits[1] & (1_u64 << 63) == 0
}

fn piece_count_dead_group_valid(group: &ExactPieceCountDeadGroup) -> bool {
    if group.placed_field == 0
        || group.placed_field == FULL_FIELD
        || group.placed_field & !FULL_FIELD != 0
        || !group.placed_field.count_ones().is_multiple_of(4)
        || group.proof_count() == 0
        || !piece_count_bitmap_padding_clear(group.count_bits.as_ref())
    {
        return false;
    }
    let remaining_piece_count = (CELL_COUNT as u32 - group.placed_field.count_ones()) as u8 / 4;
    set_piece_count_signatures(group.count_bits.as_ref())
        .all(|signature| piece_count_signature_total(signature) == remaining_piece_count)
}

fn section_payload_length_valid(payload_len: usize, state_count: usize) -> bool {
    state_count != 0
        && payload_len >= state_count
        && payload_len <= state_count.saturating_mul(MAX_DELTA_VARINT_BYTES)
}

fn grouped_payload_length_valid(
    payload_len: usize,
    group_count: usize,
    proof_count: usize,
    maximum_proofs_per_group: usize,
    bitmap_bytes: usize,
) -> bool {
    group_count != 0
        && proof_count >= group_count
        && proof_count <= group_count.saturating_mul(maximum_proofs_per_group)
        && payload_len >= group_count.saturating_mul(1_usize.saturating_add(bitmap_bytes))
        && payload_len <= group_count.saturating_mul(MAX_DELTA_VARINT_BYTES + bitmap_bytes)
}

fn write_varint(output: &mut Vec<u8>, mut value: u64) {
    debug_assert_ne!(value, 0);
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn read_varint(payload: &[u8], cursor: &mut usize) -> Result<u64, Pc4TablebaseError> {
    let start = *cursor;
    let mut value = 0_u64;
    for byte_index in 0..MAX_DELTA_VARINT_BYTES {
        let byte = *payload
            .get(*cursor)
            .ok_or(Pc4TablebaseError::InvalidArtifact(
                "pc4_tablebase_exact_dead_payload_truncated",
            ))?;
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << (byte_index * 7);
        if byte & 0x80 == 0 {
            if value == 0 || *cursor - start != varint_len(value) {
                return Err(Pc4TablebaseError::InvalidArtifact(
                    "pc4_tablebase_exact_dead_varint_noncanonical",
                ));
            }
            return Ok(value);
        }
    }
    Err(Pc4TablebaseError::InvalidArtifact(
        "pc4_tablebase_exact_dead_varint_overflow",
    ))
}

fn varint_len(mut value: u64) -> usize {
    let mut len = 1;
    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }
    len
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[inline]
fn mix_field(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn map_catalog_error(error: WasmExactSearchError) -> Pc4TablebaseError {
    match error {
        WasmExactSearchError::InvalidProblem(reason) => Pc4TablebaseError::InvalidProblem(reason),
        // Tablebase catalog compilation does not acquire the execution lease;
        // preserve fail-closed compatibility if the invariant changes.
        error @ WasmExactSearchError::ResourceAdmission(_) => {
            Pc4TablebaseError::InvalidProblem(error.reason())
        }
        WasmExactSearchError::Cancelled => {
            Pc4TablebaseError::InvalidProblem("pc4_tablebase_catalog_compile_cancelled")
        }
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, Pc4TablebaseError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_header_truncated",
        ))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Pc4TablebaseError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_header_truncated",
        ))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Pc4TablebaseError> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(Pc4TablebaseError::InvalidArtifact(
            "pc4_tablebase_header_truncated",
        ))?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        bitmap_insert, decode_sorted_values, encode_artifact, install_pc4_compact_tablebase,
        pack_piece_count_signature, piece_count_signature_total, piece_mask_dead_key,
        release_pc4_compact_tablebase, unpack_piece_count_signature, ExactPieceCountDeadGroup,
        Pc4CompactTablebase, Pc4TablebaseLookup, FULL_FIELD, HEADER_BYTES, MAX_PIECE_MULTIPLICITY,
        PIECE_COUNT_BITMAP_WORDS, PIECE_COUNT_SIGNATURE_COUNT,
    };

    const PRODUCT_ARTIFACT: &[u8] = include_bytes!(
        "../../../../../apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin"
    );

    fn counted_fixture() -> (
        [u64; PIECE_COUNT_BITMAP_WORDS],
        Vec<ExactPieceCountDeadGroup>,
    ) {
        let mut targets = [0_u64; PIECE_COUNT_BITMAP_WORDS];
        bitmap_insert(
            &mut targets,
            pack_piece_count_signature([3, 3, 3, 1, 0, 0, 0]).expect("target signature"),
        );
        let mut count_bits = Box::new([0_u64; PIECE_COUNT_BITMAP_WORDS]);
        bitmap_insert(
            count_bits.as_mut(),
            pack_piece_count_signature([3, 3, 3, 0, 0, 0, 0]).expect("remaining signature"),
        );
        (
            targets,
            vec![ExactPieceCountDeadGroup {
                placed_field: 0xf0,
                count_bits,
            }],
        )
    }

    #[test]
    fn compact_exact_dead_sections_round_trip_and_reject_corruption() {
        let (certified_targets, counted_groups) = counted_fixture();
        let artifact = encode_artifact(
            vec![0xf],
            vec![
                piece_mask_dead_key(0xf0, 1),
                piece_mask_dead_key(0xf0, 64),
                piece_mask_dead_key(0xff0, 2),
            ],
            certified_targets,
            counted_groups,
            17,
            29,
        )
        .expect("compact fixture");
        let tablebase =
            Pc4CompactTablebase::from_bytes(artifact.bytes()).expect("valid compact tablebase");
        assert_eq!(tablebase.certified_state_count(), 5);
        assert_eq!(tablebase.exact_piece_mask_dead_groups.len(), 2);
        assert_eq!(
            tablebase.lookup_placed_field(0xf),
            Pc4TablebaseLookup::ExactDead
        );
        assert_eq!(
            tablebase.lookup_placed_field(0xf0),
            Pc4TablebaseLookup::Unknown
        );
        assert_eq!(
            tablebase.lookup_placed_field(FULL_FIELD),
            Pc4TablebaseLookup::ExactResolved
        );
        assert_eq!(
            tablebase.lookup_placed_field_with_piece_mask(0xf0, 1),
            Pc4TablebaseLookup::ExactDead
        );
        assert_eq!(
            tablebase.lookup_placed_field_with_piece_mask(0xf0, 2),
            Pc4TablebaseLookup::Unknown
        );
        assert_eq!(
            tablebase.lookup_placed_field_with_piece_mask(0xf0, 64),
            Pc4TablebaseLookup::ExactDead
        );
        assert_eq!(
            tablebase.lookup_placed_field_with_piece_mask(0xff0, 2),
            Pc4TablebaseLookup::ExactDead
        );
        assert_eq!(
            tablebase.lookup_placed_field_with_piece_mask(0xf0, 0x80),
            Pc4TablebaseLookup::Unknown
        );
        assert!(tablebase.certifies_target_counts([3, 3, 3, 1, 0, 0, 0]));
        assert_eq!(
            tablebase.lookup_placed_field_with_remaining_counts(0xf0, [3, 3, 3, 0, 0, 0, 0]),
            Pc4TablebaseLookup::ExactDead
        );

        let mut corrupt = artifact.into_bytes();
        corrupt[HEADER_BYTES] ^= 1;
        assert!(Pc4CompactTablebase::from_bytes(&corrupt).is_err());
    }

    #[test]
    fn stale_schema_is_rejected_fail_closed() {
        let (certified_targets, counted_groups) = counted_fixture();
        let artifact = encode_artifact(
            vec![0xf],
            vec![piece_mask_dead_key(0xf0, 1)],
            certified_targets,
            counted_groups,
            17,
            29,
        )
        .expect("compact fixture");
        let mut stale = artifact.bytes().to_vec();
        stale[8] = 5;
        assert!(Pc4CompactTablebase::from_bytes(&stale).is_err());

        let mut unknown_flags = artifact.into_bytes();
        unknown_flags[13] |= 1 << 7;
        assert!(Pc4CompactTablebase::from_bytes(&unknown_flags).is_err());
    }

    #[test]
    fn delta_varints_require_canonical_complete_payloads() {
        assert!(decode_sorted_values(&[0x0f], 1).is_ok());
        assert!(decode_sorted_values(&[0x8f, 0x00], 1).is_err());
        assert!(decode_sorted_values(&[0x80], 1).is_err());
        assert!(decode_sorted_values(&[0x0f, 0x01], 1).is_err());
    }

    #[test]
    fn dense_piece_count_ranks_cover_every_bounded_vector_exactly_once() {
        assert_eq!(PIECE_COUNT_SIGNATURE_COUNT, 13_925);
        for signature in 0..PIECE_COUNT_SIGNATURE_COUNT {
            let signature = signature as u16;
            let counts = unpack_piece_count_signature(signature);
            assert!(counts.iter().all(|count| *count <= MAX_PIECE_MULTIPLICITY));
            assert!(piece_count_signature_total(signature) <= 10);
            assert_eq!(pack_piece_count_signature(counts), Some(signature));
        }
        assert_eq!(
            (0..PIECE_COUNT_SIGNATURE_COUNT)
                .filter(|signature| piece_count_signature_total(*signature as u16) == 10)
                .count(),
            4_795
        );
    }

    #[test]
    fn product_artifact_covers_all_cycles_and_one_duplicate_inventory() {
        let tablebase =
            Pc4CompactTablebase::from_bytes(PRODUCT_ARTIFACT).expect("product tablebase");
        let mut cycle_targets = BTreeSet::new();
        for remaining_count in [7_u32, 4, 1, 5, 2, 6, 3] {
            for mask in 0_u8..=0x7f {
                if mask.count_ones() == remaining_count {
                    let inventory = std::array::from_fn(|piece| u8::from(mask & (1 << piece) != 0));
                    collect_cycle_targets(inventory, &mut cycle_targets);
                }
                if remaining_count >= 2 && mask.count_ones() == remaining_count - 1 {
                    for duplicate in 0..7 {
                        if mask & (1 << duplicate) == 0 {
                            continue;
                        }
                        let mut inventory =
                            std::array::from_fn(|piece| u8::from(mask & (1 << piece) != 0));
                        inventory[duplicate] = 2;
                        collect_cycle_targets(inventory, &mut cycle_targets);
                    }
                }
            }
        }

        assert!(cycle_targets.iter().any(|counts| counts.contains(&4)));
        for counts in cycle_targets {
            assert!(
                tablebase.certifies_target_counts(counts),
                "cycle target is not certified: {counts:?}"
            );
        }
    }

    #[test]
    fn product_artifact_installs_and_releases_exact_registry_state() {
        let _ = release_pc4_compact_tablebase();
        let tablebase =
            install_pc4_compact_tablebase(PRODUCT_ARTIFACT).expect("product tablebase installs");
        assert_eq!(tablebase.artifact_bytes(), PRODUCT_ARTIFACT.len());
        assert_eq!(tablebase.certified_state_count(), 430_200);
        assert_eq!(tablebase.certified_target_count(), 4_795);
        assert_eq!(tablebase.compiler_identity(), 0x82b1_ed6e_a853_0575);
        assert!(tablebase.certifies_target_counts([3, 3, 1, 1, 1, 1, 0]));
        assert!(tablebase.certifies_target_counts([2, 2, 2, 1, 1, 1, 1]));
        assert!(tablebase.certifies_target_counts([4, 1, 1, 1, 1, 1, 1]));
        assert!(!tablebase.certifies_target_counts([5, 1, 1, 1, 1, 1, 0]));
        assert_eq!(
            tablebase.lookup_placed_field(0),
            Pc4TablebaseLookup::Unknown
        );
        assert_eq!(
            tablebase.lookup_placed_field(FULL_FIELD),
            Pc4TablebaseLookup::ExactResolved
        );
        assert_eq!(
            tablebase.lookup_placed_field(0x4020_1c07),
            Pc4TablebaseLookup::ExactDead
        );
        assert!(release_pc4_compact_tablebase());
        assert!(!release_pc4_compact_tablebase());
    }

    fn collect_cycle_targets(current_inventory: [u8; 7], output: &mut BTreeSet<[u8; 7]>) {
        let capacity = current_inventory.map(|count| count + 2);
        visit_bounded_target(0, 10, capacity, &mut [0; 7], output);
    }

    fn visit_bounded_target(
        piece: usize,
        remaining: u8,
        capacity: [u8; 7],
        counts: &mut [u8; 7],
        output: &mut BTreeSet<[u8; 7]>,
    ) {
        if piece + 1 == counts.len() {
            if remaining <= capacity[piece] {
                counts[piece] = remaining;
                output.insert(*counts);
            }
            return;
        }
        for count in 0..=remaining.min(capacity[piece]) {
            counts[piece] = count;
            visit_bounded_target(piece + 1, remaining - count, capacity, counts, output);
        }
    }
}
