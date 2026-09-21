//! SRP rationale: this module's single change reason is enumeration of the
//! complete canonical graph x compact-supply union without
//! reveal histories. I/O, observation probabilities and product reduction stay
//! outside. Missing records suspend individual work items, never mean no edge.
use core::{convert::Infallible, num::NonZeroUsize};
use std::collections::{HashMap, HashSet, VecDeque};

#[path = "pc4_frontier_retention.rs"]
mod retention;
use retention::{capacity_bytes, FrontierRetention, FrontierRetentionError};

use clearra_core_domain::{
    board::standard_pc_board::StandardPcBoard,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_pc4_tablebase::{
    clearra_board64_mask_to_hydra_field_hash_v1, materialize_qualified_graph_edge,
    read_qualified_pc4_adjacency, ClearraPlacementIdentity, FixedQueueTraversalPageError,
    Pc4RowFrame, PlacementMaterializationError, QualifiedPc4GraphEdge, QualifiedPc4TargetIdentity,
};
use clearra_supply::pattern_universe::{
    CompactPatternUnionError, CompactPatternUnionFrontier, CompactPatternUnionLanguage,
    CompactPatternUnionLayerFrontierRef, CompactPatternUnionLayerIntoIter,
    CompactPatternUnionLayerOwner, CompactPatternUnionLayerShard, CompactPatternUnionLimits,
};

use super::{
    cooperative_canonicalizer::{
        CandidateCanonicalizationError, CooperativeCandidateCanonicalizer,
    },
    graph_candidate_adapter::Pc4GraphCandidateGuard,
    PcCandidateBoundaryError, PcCandidateCompletenessEvidence, PcCandidatePageGuard,
    PcCandidateReducerInput, PcCandidateSourceBinding, PcCandidateUniverseIdentity,
};
use crate::{
    pc4_input_disclosure_policy::{Pc4PreparedOnlineInput, Pc4PreparedQueueInput},
    pc4_lookup_graph_runtime_adapter::{
        Pc4LookupAdjacencyError, Pc4LookupGraphCache, Pc4LookupMaterializationError,
    },
};

/// Retained frontier bytes and independent work/result limits. Graph cache,
/// source owners and I/O have separate bounds; no limit authorizes truncation.
#[derive(Clone, Copy)]
pub(crate) struct CompactGraphUnionLimits {
    pub frontier_bytes: NonZeroUsize,
    /// Resident source/target dependencies, not all cold next-layer states.
    pub resident_work: NonZeroUsize,
    pub work: NonZeroUsize,
    pub candidates: NonZeroUsize,
    pub waiting_fields: NonZeroUsize,
    pub edge_placements: NonZeroUsize,
    /// Candidate buffers during collection/sorting only, not whole-search RSS.
    pub canonicalization_bytes: NonZeroUsize,
    pub language: CompactPatternUnionLimits,
}

#[derive(Debug)]
pub(crate) enum CompactGraphUnionError {
    Contract(&'static str),
    Supply(CompactPatternUnionError),
    Adjacency(FixedQueueTraversalPageError<Pc4LookupAdjacencyError, Infallible>),
    Placement(PlacementMaterializationError<Pc4LookupMaterializationError>),
    Boundary(PcCandidateBoundaryError),
    Canonicalization(CandidateCanonicalizationError),
    Retention(FrontierRetentionError),
    Limit {
        kind: &'static str,
        limit: usize,
        attempted: usize,
    },
}

impl CompactGraphUnionError {
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            Self::Contract(reason) => reason,
            Self::Supply(error) => error.reason(),
            Self::Adjacency(error) => error.reason(),
            Self::Placement(error) => error.reason(),
            Self::Boundary(error) => error.reason(),
            Self::Canonicalization(error) => error.reason(),
            Self::Retention(error) => error.reason(),
            Self::Limit { kind, .. } => kind,
        }
    }
}

impl From<FrontierRetentionError> for CompactGraphUnionError {
    fn from(error: FrontierRetentionError) -> Self {
        Self::Retention(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompactGraphUnionStep {
    Progress,
    Waiting,
    Complete,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CompactGraphUnionUsage {
    pub work: usize,
    pub generated_states: usize,
    pub merged_states: usize,
    pub promoted_states: usize,
    pub materialized_edges: usize,
    pub peak_waiting_fields: usize,
    pub canonicalization_work: usize,
    pub peak_canonicalization_buffer_bytes: usize,
    pub peak_frontier_bytes: usize,
    pub terminal_arrivals: usize,
    pub peak_resident_work: usize,
    pub peak_ready_work: usize,
    pub pin_compactions: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct StateKey {
    layout: CompactPc4LayoutIdentity,
    // Qualified graph IDs currently occupy fewer than 24 bits and the exact
    // row-frame subset occupies four. Keeping both in one word avoids four
    // bytes of alignment padding in every hash key. Construction rejects a
    // future graph outside that domain rather than truncating its identity.
    field_frame: u32,
}

const PC4_CELL_COUNT: u32 = 40;
const PC4_MAX_PLACEMENTS: usize = 10;
// Terminal layouts are emitted immediately and never become frontier keys.
// A four-line partial frontier therefore contains at most nine placements.
const PC4_MAX_FRONTIER_PLACEMENTS: usize = PC4_MAX_PLACEMENTS - 1;
const PC4_PLACEMENT_RANK_BITS: u32 = 17;
const PC4_PLACEMENT_RANK_MASK: u32 = (1 << PC4_PLACEMENT_RANK_BITS) - 1;
const PC4_PACKED_PLACEMENT_BITS: usize = 20;
const PC4_PACKED_PLACEMENT_MASK: u32 = (1 << PC4_PACKED_PLACEMENT_BITS) - 1;
const PC4_PACKED_LAYOUT_WORDS: usize =
    (PC4_MAX_FRONTIER_PLACEMENTS * PC4_PACKED_PLACEMENT_BITS).div_ceil(u32::BITS as usize);
const PC4_FIELD_BITS: u32 = 24;
const PC4_FIELD_MASK: u32 = (1 << PC4_FIELD_BITS) - 1;

/// Exact PC4-only partial-layout identity.
///
/// `StandardBoard64TilingIdentity` deliberately supports sixteen placements
/// and arbitrary Board64 masks, so embedding it in every breadth-layer key
/// retains 128 unused mask bytes for a four-line search. PC4 has exactly forty
/// cells and at most ten placements. Rank each four-cell mask in the 40-choose-4
/// domain and store the piece code beside that rank. Each exact code plus one
/// occupies twenty bits; zero is the unused-slot sentinel. Terminal histories
/// bypass the frontier, so nine placements need only 180 bits. Neither the
/// occupied mask nor a count is retained in the hash key: both are reconstructed
/// exactly from the packed placements. This
/// remains collision-free and reconstructs the ordinary product identity at
/// the terminal boundary; it is not a hash, quotient, colored-field merge, or
/// change of solution meaning.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CompactPc4LayoutIdentity {
    words: [u32; PC4_PACKED_LAYOUT_WORDS],
}

const _: [(); 24] = [(); core::mem::size_of::<CompactPc4LayoutIdentity>()];
const _: [(); 28] = [(); core::mem::size_of::<StateKey>()];
const _: [(); 1] = [(); (core::mem::size_of::<CompactPatternUnionFrontier>() <= 32) as usize];
#[cfg(target_pointer_width = "64")]
const _: [(); 56] = [(); core::mem::size_of::<(StateKey, CompactPatternUnionFrontier)>()];
#[cfg(target_pointer_width = "32")]
const _: [(); 40] = [(); core::mem::size_of::<(StateKey, CompactPatternUnionFrontier)>()];
#[cfg(target_pointer_width = "64")]
const _: [(); 48] = [(); CompactPatternUnionLayerShard::<StateKey>::entry_size()];
#[cfg(target_pointer_width = "32")]
const _: [(); 36] = [(); CompactPatternUnionLayerShard::<StateKey>::entry_size()];

impl StateKey {
    fn new(
        layout: CompactPc4LayoutIdentity,
        field: u32,
        frame: Pc4RowFrame,
    ) -> Result<Self, CompactGraphUnionError> {
        if field > PC4_FIELD_MASK {
            return Err(contract(
                "pc4_compact_union_field_id_outside_compact_domain",
            ));
        }
        let frame = u32::from(frame.surviving_original_rows_mask());
        debug_assert!(frame < 16);
        Ok(Self {
            layout,
            field_frame: field | (frame << PC4_FIELD_BITS),
        })
    }

    const fn field(self) -> u32 {
        self.field_frame & PC4_FIELD_MASK
    }

    fn frame(self) -> Result<Pc4RowFrame, CompactGraphUnionError> {
        let mask = (self.field_frame >> PC4_FIELD_BITS) as u8;
        Pc4RowFrame::from_surviving_original_rows_mask(mask)
            .ok_or_else(|| contract("pc4_compact_union_row_frame_invalid"))
    }
}

impl CompactPc4LayoutIdentity {
    fn initial(initial_board_mask: u64) -> Result<Self, CompactGraphUnionError> {
        if initial_board_mask >> PC4_CELL_COUNT != 0 {
            return Err(contract("pc4_compact_union_initial_layout_invalid"));
        }
        Ok(Self {
            words: [0; PC4_PACKED_LAYOUT_WORDS],
        })
    }

    fn placement_count(self) -> usize {
        (0..PC4_MAX_FRONTIER_PLACEMENTS)
            .take_while(|&index| self.slot(index) != 0)
            .count()
    }

    fn occupied(self, initial_board_mask: u64) -> Result<u64, CompactGraphUnionError> {
        let mut occupied = initial_board_mask;
        for index in 0..self.placement_count() {
            let encoded = self
                .slot(index)
                .checked_sub(1)
                .ok_or_else(|| contract("pc4_compact_union_layout_invalid"))?;
            occupied |= decode_pc4_placement(encoded)?.cells_mask();
        }
        Ok(occupied)
    }

    fn with_placement(
        self,
        initial_board_mask: u64,
        piece: PieceKind,
        cells: u64,
    ) -> Result<Self, CompactGraphUnionError> {
        let count = self.placement_count();
        if count == PC4_MAX_FRONTIER_PLACEMENTS
            || cells.count_ones() != 4
            || cells >> PC4_CELL_COUNT != 0
            || self.occupied(initial_board_mask)? & cells != 0
        {
            return Err(contract("pc4_compact_union_layout_invalid"));
        }
        let encoded = encode_pc4_placement(piece, cells)?
            .checked_add(1)
            .ok_or_else(|| contract("pc4_compact_union_layout_invalid"))?;
        if encoded > PC4_PACKED_PLACEMENT_MASK {
            return Err(contract("pc4_compact_union_layout_invalid"));
        }
        let mut next = self;
        let mut insertion = count;
        while insertion > 0 && next.slot(insertion - 1) > encoded {
            let previous = next.slot(insertion - 1);
            next.set_slot(insertion, previous)?;
            insertion -= 1;
        }
        next.set_slot(insertion, encoded)?;
        Ok(next)
    }

    fn into_standard_with_placement(
        self,
        initial_board_mask: u64,
        piece: PieceKind,
        cells: u64,
    ) -> Result<StandardBoard64TilingIdentity, CompactGraphUnionError> {
        let count = self.placement_count();
        if count >= PC4_MAX_PLACEMENTS
            || cells.count_ones() != 4
            || cells >> PC4_CELL_COUNT != 0
            || self.occupied(initial_board_mask)? & cells != 0
        {
            return Err(contract("pc4_compact_union_layout_invalid"));
        }
        let encoded = encode_pc4_placement(piece, cells)?;
        let mut placements = [0_u32; PC4_MAX_PLACEMENTS];
        for (index, slot) in placements[..count].iter_mut().enumerate() {
            *slot = self
                .slot(index)
                .checked_sub(1)
                .ok_or_else(|| contract("pc4_compact_union_layout_invalid"))?;
        }
        let mut insertion = count;
        while insertion > 0 && placements[insertion - 1] > encoded {
            placements[insertion] = placements[insertion - 1];
            insertion -= 1;
        }
        placements[insertion] = encoded;
        standard_from_pc4_codes(initial_board_mask, &placements[..count + 1])
    }

    fn into_standard(
        self,
        initial_board_mask: u64,
    ) -> Result<StandardBoard64TilingIdentity, CompactGraphUnionError> {
        let mut placements = [0_u32; PC4_MAX_FRONTIER_PLACEMENTS];
        let count = self.placement_count();
        for (index, slot) in placements[..count].iter_mut().enumerate() {
            *slot = self
                .slot(index)
                .checked_sub(1)
                .ok_or_else(|| contract("pc4_compact_union_layout_invalid"))?;
        }
        standard_from_pc4_codes(initial_board_mask, &placements[..count])
    }

    fn slot(self, index: usize) -> u32 {
        debug_assert!(index < PC4_MAX_FRONTIER_PLACEMENTS);
        let bit = index * PC4_PACKED_PLACEMENT_BITS;
        let word = bit / u32::BITS as usize;
        let shift = bit % u32::BITS as usize;
        let mut value = u64::from(self.words[word]) >> shift;
        if shift + PC4_PACKED_PLACEMENT_BITS > u32::BITS as usize {
            value |= u64::from(self.words[word + 1]) << (u32::BITS as usize - shift);
        }
        (value & u64::from(PC4_PACKED_PLACEMENT_MASK)) as u32
    }

    fn set_slot(&mut self, index: usize, value: u32) -> Result<(), CompactGraphUnionError> {
        if index >= PC4_MAX_FRONTIER_PLACEMENTS || value > PC4_PACKED_PLACEMENT_MASK {
            return Err(contract("pc4_compact_union_layout_invalid"));
        }
        let bit = index * PC4_PACKED_PLACEMENT_BITS;
        let word = bit / u32::BITS as usize;
        let shift = bit % u32::BITS as usize;
        let low_mask = u64::from(PC4_PACKED_PLACEMENT_MASK) << shift;
        let low_word_mask = low_mask as u32;
        self.words[word] &= !low_word_mask;
        self.words[word] |= ((u64::from(value) << shift) & low_mask) as u32;
        if shift + PC4_PACKED_PLACEMENT_BITS > u32::BITS as usize {
            let high_bits = shift + PC4_PACKED_PLACEMENT_BITS - u32::BITS as usize;
            let high_mask = (1_u32 << high_bits) - 1;
            self.words[word + 1] &= !high_mask;
            self.words[word + 1] |= value >> (u32::BITS as usize - shift);
        }
        Ok(())
    }
}

fn standard_from_pc4_codes(
    initial_board_mask: u64,
    placements: &[u32],
) -> Result<StandardBoard64TilingIdentity, CompactGraphUnionError> {
    let mut masks = [0_u64; PC4_MAX_PLACEMENTS];
    let mut packed_piece_codes = 0_u64;
    for (index, &encoded) in placements.iter().enumerate() {
        let placement = decode_pc4_placement(encoded)?;
        masks[index] = placement.cells_mask();
        packed_piece_codes |= u64::from(pc4_piece_code(placement.piece())) << (index * 3);
    }
    StandardBoard64TilingIdentity::from_compact_parts(
        initial_board_mask,
        packed_piece_codes,
        &masks[..placements.len()],
    )
    .map_err(|_| contract("pc4_compact_union_layout_invalid"))
}

fn encode_pc4_placement(piece: PieceKind, cells: u64) -> Result<u32, CompactGraphUnionError> {
    let rank =
        rank_four_cell_mask(cells).ok_or_else(|| contract("pc4_compact_union_layout_invalid"))?;
    if rank > PC4_PLACEMENT_RANK_MASK {
        return Err(contract("pc4_compact_union_layout_invalid"));
    }
    Ok((u32::from(pc4_piece_code(piece)) << PC4_PLACEMENT_RANK_BITS) | rank)
}

fn decode_pc4_placement(encoded: u32) -> Result<PiecePlacementMask, CompactGraphUnionError> {
    let piece = pc4_piece_from_code((encoded >> PC4_PLACEMENT_RANK_BITS) as u8)
        .ok_or_else(|| contract("pc4_compact_union_layout_invalid"))?;
    let cells = unrank_four_cell_mask(encoded & PC4_PLACEMENT_RANK_MASK)
        .ok_or_else(|| contract("pc4_compact_union_layout_invalid"))?;
    Ok(PiecePlacementMask::new(piece, cells))
}

fn rank_four_cell_mask(cells: u64) -> Option<u32> {
    if cells.count_ones() != 4 || cells >> PC4_CELL_COUNT != 0 {
        return None;
    }
    let mut rank = 0_u32;
    let mut ordinal = 1_u32;
    for cell in 0..PC4_CELL_COUNT {
        if cells & (1_u64 << cell) != 0 {
            rank = rank.checked_add(binomial(cell, ordinal))?;
            ordinal += 1;
        }
    }
    (ordinal == 5).then_some(rank)
}

fn unrank_four_cell_mask(mut rank: u32) -> Option<u64> {
    if rank >= binomial(PC4_CELL_COUNT, 4) {
        return None;
    }
    let mut cells = 0_u64;
    let mut upper = PC4_CELL_COUNT;
    for ordinal in (1..=4).rev() {
        let mut cell = upper.checked_sub(1)?;
        while binomial(cell, ordinal) > rank {
            cell = cell.checked_sub(1)?;
        }
        cells |= 1_u64 << cell;
        rank -= binomial(cell, ordinal);
        upper = cell;
    }
    Some(cells)
}

const fn binomial(n: u32, k: u32) -> u32 {
    if k > n {
        return 0;
    }
    let k = if k < n - k { k } else { n - k };
    let mut value = 1_u32;
    let mut i = 0_u32;
    while i < k {
        value = value * (n - i) / (i + 1);
        i += 1;
    }
    value
}

const fn pc4_piece_code(piece: PieceKind) -> u8 {
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

const fn pc4_piece_from_code(code: u8) -> Option<PieceKind> {
    match code {
        0 => Some(PieceKind::I),
        1 => Some(PieceKind::O),
        2 => Some(PieceKind::T),
        3 => Some(PieceKind::S),
        4 => Some(PieceKind::Z),
        5 => Some(PieceKind::J),
        6 => Some(PieceKind::L),
        _ => None,
    }
}

// Keeping a breadth layer in one HashMap makes `into_iter` retain the entire
// bucket allocation until its final entry is promoted. At the widest PC4
// layer that overlaps a full old table with a growing successor table and can
// exceed the bounded frontier even though live state count stays bounded. A
// fixed shard count preserves one exact merge owner per StateKey while letting
// completed old shards return their bucket allocation immediately.
// Full Jstris qualification showed why powers of two alone do not remove the
// last breadth-layer cliff: around 1.845M live successor keys, both 64 and 128
// shards place the average shard just beyond a hash-table growth boundary and
// retain 3,239,936 aggregate slots. A 192-shard follow-up crossed a later
// frontier by only 64,978 bytes while retaining 2,365,440 successor slots for
// 1,384,912 live keys. That measurement predated the layer-owner split: every
// entry was then 56 bytes because it repeated the same Arc owner. The exact
// layer shard now retains that owner once and its entry is 48 bytes. A later
// 160-way proof reached a different overlap with 143 still-live 28,672-slot
// promotion shards plus 69 successor shards and missed 256 MiB by 92,616
// bytes. Restoring 192 owners moves both measured overlaps back below the
// 14,336-slot per-shard boundary; the old 192-way cliff is more than covered
// by the eight bytes removed from each retained entry. This changes storage
// granularity only: every StateKey still has one deterministic merge owner and
// no search state or solution identity is altered.
const PC4_LAYER_SHARDS: usize = 192;

struct LayerFrontier {
    // Build the directory as a dynamic boxed slice. `Box<[T; N]>` still
    // constructs its fixed array value on the caller stack before moving it,
    // which overflowed the deliberately small product/test worker stack.
    // Entry and nested payload capacities remain charged independently below.
    shards: Box<[CompactPatternUnionLayerShard<StateKey>]>,
    owner: Option<CompactPatternUnionLayerOwner>,
    len: usize,
}

impl LayerFrontier {
    fn new() -> Self {
        let mut shards = Vec::with_capacity(PC4_LAYER_SHARDS);
        shards.resize_with(PC4_LAYER_SHARDS, CompactPatternUnionLayerShard::new);
        Self {
            shards: shards.into_boxed_slice(),
            owner: None,
            len: 0,
        }
    }

    fn shard_index(key: &StateKey) -> usize {
        // Only selects an ownership shard; HashMap still performs the exact
        // key comparison. Mix every compact layout word with the qualified
        // field/frame word so both field-heavy and layout-heavy layers spread.
        let mut mixed = key.field_frame.wrapping_mul(0x9e37_79b9);
        for word in key.layout.words {
            mixed ^= word.wrapping_add(0x9e37_79b9).rotate_left(13);
            mixed = mixed.wrapping_mul(0x85eb_ca6b).rotate_left(7);
        }
        // The measured layout intentionally is not a power of two: a
        // mask would map only 128 owners and recreate the observed capacity
        // cliff. Modulo is paid once per outer lookup/insert and keeps every
        // shard reachable without changing HashMap's exact key comparison.
        (mixed as usize) % PC4_LAYER_SHARDS
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn len(&self) -> usize {
        self.len
    }

    fn capacity(&self) -> Result<usize, CompactGraphUnionError> {
        self.shards.iter().try_fold(0usize, |total, shard| {
            total
                .checked_add(shard.capacity())
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))
        })
    }

    fn directory_bytes() -> Result<usize, CompactGraphUnionError> {
        Ok(capacity_bytes::<CompactPatternUnionLayerShard<StateKey>>(
            PC4_LAYER_SHARDS,
        )?)
    }

    fn retained_capacity_bytes(&self) -> Result<usize, CompactGraphUnionError> {
        self.capacity()?
            .checked_mul(CompactPatternUnionLayerShard::<StateKey>::entry_size())
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))
    }

    fn get(&self, key: &StateKey) -> Option<CompactPatternUnionLayerFrontierRef<'_>> {
        self.owner
            .as_ref()
            .and_then(|owner| self.shards[Self::shard_index(key)].get(key, owner))
    }

    fn reserve_for(&mut self, key: &StateKey) -> Result<(), CompactGraphUnionError> {
        self.shards[Self::shard_index(key)]
            .try_reserve(1)
            .map_err(CompactGraphUnionError::Supply)
    }

    fn additional_capacity_for(&self, key: &StateKey) -> usize {
        let shard = &self.shards[Self::shard_index(key)];
        (shard.len() + 1).saturating_sub(shard.capacity())
    }

    fn insert(
        &mut self,
        key: StateKey,
        supply: CompactPatternUnionFrontier,
    ) -> Result<bool, CompactGraphUnionError> {
        let incoming_owner = supply.layer_owner();
        if let Some(owner) = &self.owner {
            if !owner.same_identity(&incoming_owner) {
                return Err(CompactGraphUnionError::Supply(
                    CompactPatternUnionError::ForeignFrontier,
                ));
            }
        } else {
            self.owner = Some(incoming_owner);
        }
        let owner = self.owner.as_ref().expect("layer owner installed");
        let replaced = self.shards[Self::shard_index(&key)]
            .insert(key, supply, owner)
            .map_err(CompactGraphUnionError::Supply)?;
        if !replaced {
            self.len += 1;
        }
        Ok(replaced)
    }

    fn into_promotion(self) -> Result<LayerPromotion, CompactGraphUnionError> {
        let table_capacity = self.capacity()?;
        let owner = self
            .owner
            .ok_or_else(|| contract("pc4_compact_union_nonempty_layer_owner_missing"))?;
        let shards = self
            .shards
            .into_vec()
            .into_iter()
            .map(|table| {
                let table_capacity = table.capacity();
                Some(PromotionShard {
                    entries: table.into_iter(owner.clone()),
                    table_capacity,
                })
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(LayerPromotion {
            shards,
            shard_index: 0,
            table_capacity,
        })
    }
}

impl Default for LayerFrontier {
    fn default() -> Self {
        Self::new()
    }
}

struct PromotionShard {
    entries: CompactPatternUnionLayerIntoIter<StateKey>,
    table_capacity: usize,
}

/// Moving a completed layer is cooperative too: a large retained frontier
/// must not become one uninterruptible O(layer size) queue conversion. Shards
/// additionally make the old allocation releasable throughout that move.
struct LayerPromotion {
    shards: Box<[Option<PromotionShard>]>,
    shard_index: usize,
    table_capacity: usize,
}

impl LayerPromotion {
    fn directory_bytes() -> Result<usize, CompactGraphUnionError> {
        Ok(capacity_bytes::<Option<PromotionShard>>(PC4_LAYER_SHARDS)?)
    }

    fn next(
        &mut self,
    ) -> Result<Option<(StateKey, CompactPatternUnionFrontier)>, CompactGraphUnionError> {
        while self.shard_index < self.shards.len() {
            let index = self.shard_index;
            let (entry, exhausted) = {
                let shard = self.shards[index]
                    .as_mut()
                    .expect("unvisited promotion shard");
                let entry = shard.entries.next();
                let exhausted = shard.entries.len() == 0;
                (entry, exhausted)
            };
            if exhausted {
                let shard = self.shards[index]
                    .take()
                    .expect("promotion shard exists until exhausted");
                self.table_capacity = self
                    .table_capacity
                    .checked_sub(shard.table_capacity)
                    .ok_or_else(|| contract("pc4_compact_union_frontier_accounting_failed"))?;
                self.shard_index += 1;
            }
            if entry.is_some() {
                return Ok(entry);
            }
        }
        Ok(None)
    }
}

struct Work {
    key: StateKey,
    supply: CompactPatternUnionFrontier,
    piece: usize,
    next_supply: Option<CompactPatternUnionFrontier>,
    targets: Option<Vec<u32>>,
    target_index: usize,
    placements: Option<Vec<ClearraPlacementIdentity>>,
    placement_index: usize,
    resident: bool,
    target_pin: Option<u32>,
}

impl Work {
    fn new(key: StateKey, supply: CompactPatternUnionFrontier) -> Self {
        Self {
            key,
            supply,
            piece: 0,
            next_supply: None,
            targets: None,
            target_index: 0,
            placements: None,
            placement_index: 0,
            resident: false,
            target_pin: None,
        }
    }

    fn supply_states(&self) -> usize {
        self.supply.state_count() + self.next_supply.as_ref().map_or(0, |s| s.state_count())
    }

    fn retained_payload_bytes(&self) -> Result<usize, CompactGraphUnionError> {
        let mut bytes = self.supply.retained_state_capacity_bytes();
        for payload in [
            self.next_supply
                .as_ref()
                .map_or(0, |s| s.retained_state_capacity_bytes()),
            capacity_bytes::<u32>(self.targets.as_ref().map_or(0, Vec::capacity))?,
            capacity_bytes::<ClearraPlacementIdentity>(
                self.placements.as_ref().map_or(0, Vec::capacity),
            )?,
        ] {
            bytes = bytes
                .checked_add(payload)
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        }
        Ok(bytes)
    }

    fn next_piece(&mut self) -> Result<(usize, usize), CompactGraphUnionError> {
        let payload = self
            .retained_payload_bytes()?
            .checked_sub(self.supply.retained_state_capacity_bytes())
            .ok_or(FrontierRetentionError::Underflow)?;
        let released = self.next_supply.take().map_or(0, |s| s.state_count());
        self.targets = None;
        self.placements = None;
        self.target_index = 0;
        self.placement_index = 0;
        self.piece += 1;
        Ok((released, payload))
    }
}

enum WorkStep {
    Continue,
    Done,
    Missing(u32),
}

/// Input is nominally source-bound. A layer is sealed before expanding its
/// successor, so all incoming supply frontiers are merged exactly once before
/// a state is expanded. Independent items within the layer may finish in any
/// I/O order. A future cross-layer scheduler must supply a delta/fixpoint proof;
/// removing this barrier by merely marking a node "visited" loses solutions.
pub(crate) struct Pc4CompactGraphUnion {
    source: PcCandidateSourceBinding,
    target: QualifiedPc4TargetIdentity,
    language: CompactPatternUnionLanguage,
    limits: CompactGraphUnionLimits,
    start_field: u32,
    start_hash: u64,
    start_verified: bool,
    placement_count: usize,
    full_mask: u64,
    ready: VecDeque<Work>,
    waiting: HashMap<u32, Vec<Work>>,
    waiting_count: usize,
    next_layer: LayerFrontier,
    promotion: Option<LayerPromotion>,
    candidates: HashSet<StandardBoard64TilingIdentity>,
    canonicalizer: Option<CooperativeCandidateCanonicalizer>,
    retained_supply_states: usize,
    retention: FrontierRetention,
    residents: usize,
    pins: HashMap<u32, usize>,
    cache_revision_seen: u64,
    completed: bool,
    terminated: bool,
    usage: CompactGraphUnionUsage,
    last_failure_diagnostic: Option<String>,
}

impl Pc4CompactGraphUnion {
    pub(crate) fn check_current<G: Pc4GraphCandidateGuard>(
        &self,
        guard: &G,
    ) -> Result<(), CompactGraphUnionError> {
        if self.terminated {
            return Err(contract("pc4_compact_union_terminated"));
        }
        check_guard(&self.source, &self.target, guard)
    }

    pub(crate) fn has_ready_work(&self) -> bool {
        !self.completed
            && !self.terminated
            && (self.canonicalizer.is_some()
                || self.promotion.is_some()
                || self.ready.front().is_some_and(|task| {
                    task.resident || self.residents < self.limits.resident_work.get()
                })
                || (self.ready.is_empty() && self.waiting.is_empty()))
    }

    pub(crate) fn protects_field(&self, field: u32) -> bool {
        self.pins.contains_key(&field) || (!self.start_verified && field == self.start_field)
    }

    /// Watermarks throttle new CPU demand before the hard retained-state
    /// limits are reached. They are counts, not whole-owner byte authority.
    #[cfg(test)]
    pub(crate) fn io_demand_is_full(&self, fields: usize, continuations: usize) -> bool {
        // Before its first verified record the root is a real pending demand,
        // but has not been parked in `waiting` yet. Match pending_fields rather
        // than silently reporting zero demand for that bootstrap phase.
        let root = usize::from(!self.start_verified && !self.completed && !self.terminated);
        self.waiting.len() + root >= fields || self.waiting_count + root >= continuations
    }

    #[cfg(test)]
    pub(crate) fn resident_capacity_blocks_cold_front(&self, cache: &Pc4LookupGraphCache) -> bool {
        self.residents == self.limits.resident_work.get()
            && self.ready.front().is_some_and(|task| !task.resident)
            && !self
                .waiting
                .keys()
                .any(|field| cache.contains_field_id(*field))
    }

    pub(crate) fn prepare<G: Pc4GraphCandidateGuard>(
        source: &PcCandidateSourceBinding,
        prepared: &Pc4PreparedOnlineInput,
        cache: &Pc4LookupGraphCache,
        start_field: u32,
        limits: CompactGraphUnionLimits,
        guard: &G,
    ) -> Result<Option<Self>, CompactGraphUnionError> {
        let Pc4PreparedQueueInput::CompiledPattern(pattern) = prepared.queue() else {
            return Ok(None);
        };
        let target = prepared.target();
        if cache.target() != target || start_field >= cache.field_count() {
            return Err(contract("pc4_compact_union_target_mismatch"));
        }
        check_guard(source, target, guard)?;
        let problem = pattern.problem();
        crate::validate_pc4_search_problem_compatibility(target.profile(), problem)
            .map_err(|e| contract(e.reason()))?;
        let initial = problem.initial_board();
        let board = StandardPcBoard::from_words(
            target.target_lines().get(),
            [initial.occupied_mask(), 0, 0, 0],
        )
        .map_err(|_| contract("pc4_compact_union_initial_board_invalid"))?;
        let hold = crate::pc_candidate_execution_bridge::fixed_queue_hold_state(
            problem.core_query().allow_hold(),
            problem.core_query().hold_state(),
        );
        crate::online_pc4_fixed_queue_candidate_session::validate_prepared_candidate_source(
            source, prepared, board, hold,
        )
        .map_err(|e| contract(e.reason()))?;
        if initial.width() != 10 || initial.visible_height() != u16::from(board.lines()) {
            return Err(contract("pc4_compact_union_initial_board_mismatch"));
        }
        let free = u32::from(board.cell_count()) - board.occupied().count_ones();
        if !free.is_multiple_of(4) {
            return Err(contract("pc4_compact_union_invalid_area"));
        }
        let Some((language, supply)) = pattern
            .compact_union_language(limits.language, &|| {
                PcCandidatePageGuard::is_cancelled(guard)
            })
            .map_err(|e| contract(e.reason()))?
        else {
            return Ok(None);
        };
        let layout = CompactPc4LayoutIdentity::initial(source.initial_board_mask())?;
        let mut prefix = 0;
        while prefix < 4 && (source.initial_board_mask() >> (10 * prefix)) & 1023 == 1023 {
            prefix += 1;
        }
        let frame = Pc4RowFrame::new(prefix)
            .map_err(|_| contract("pc4_compact_union_row_frame_invalid"))?;
        let retained_supply_states = supply.state_count();
        let mut retention = FrontierRetention::new(limits.frontier_bytes);
        let next_layer = LayerFrontier::new();
        let initial_payload = supply.retained_state_capacity_bytes();
        let initial_outer = capacity_bytes::<Work>(1)?
            .checked_add(LayerFrontier::directory_bytes()?)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        retention.authorize(initial_outer, initial_payload)?;
        let mut ready = VecDeque::new();
        ready.try_reserve(1).map_err(|_| allocation())?;
        let retained_outer = capacity_bytes::<Work>(ready.capacity())?
            .checked_add(LayerFrontier::directory_bytes()?)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        retention.retain(retained_outer, initial_payload)?;
        ready.push_back(Work::new(
            StateKey::new(layout, start_field, frame)?,
            supply,
        ));
        let start_hash = clearra_board64_mask_to_hydra_field_hash_v1(source.initial_board_mask())
            .map_err(|_| contract("pc4_compact_union_initial_hash_invalid"))?;
        let initial_ready_len = ready.len();
        Ok(Some(Self {
            source: source.clone(),
            target: target.clone(),
            language,
            limits,
            start_field,
            start_hash,
            start_verified: false,
            placement_count: (free / 4) as usize,
            full_mask: (1u64 << (10 * u32::from(board.lines()))) - 1,
            ready,
            waiting: HashMap::new(),
            waiting_count: 0,
            next_layer,
            promotion: None,
            candidates: HashSet::new(),
            canonicalizer: None,
            retained_supply_states,
            retention,
            residents: 0,
            pins: HashMap::new(),
            cache_revision_seen: 0,
            completed: false,
            terminated: false,
            usage: CompactGraphUnionUsage {
                peak_ready_work: initial_ready_len,
                ..CompactGraphUnionUsage::default()
            },
            last_failure_diagnostic: None,
        }))
    }

    /// Bounded demand list: duplicates share one cache record. Exposing demands
    /// while CPU progress remains lets the host start I/O without a global wait.
    pub(crate) fn pending_fields(
        &self,
        maximum: usize,
    ) -> Result<Vec<u32>, CompactGraphUnionError> {
        if self.terminated || self.completed || maximum == 0 {
            return Ok(Vec::new());
        }
        // The caller owns this small demand list. Never allocate every waiting
        // ID merely to return at most the caller's I/O window (normally 16).
        let mut ids = Vec::new();
        ids.try_reserve_exact(if self.start_verified {
            maximum.min(self.waiting.len())
        } else {
            1
        })
        .map_err(|_| allocation())?;
        if !self.start_verified {
            ids.push(self.start_field);
            return Ok(ids);
        }
        for &id in self.waiting.keys() {
            let index = ids.binary_search(&id).unwrap_err();
            if index < maximum {
                if ids.len() == maximum {
                    ids.pop();
                }
                ids.insert(index, id);
            }
        }
        Ok(ids)
    }

    pub(crate) const fn usage(&self) -> CompactGraphUnionUsage {
        self.usage
    }

    #[cfg(test)]
    pub(crate) fn retained_promotion_capacity_for_test(&self) -> Option<usize> {
        self.promotion.as_ref().map(|layer| layer.table_capacity)
    }

    #[cfg(test)]
    pub(crate) fn completion_state_for_test(&self) -> String {
        format!(
            "terminated={} completed={} ready={} waiting={} next={} promotion={} residents={} pins={} canonicalizer={}",
            self.terminated,
            self.completed,
            self.ready.len(),
            self.waiting.len(),
            self.next_layer.len(),
            self.promotion.is_some(),
            self.residents,
            self.pins.len(),
            self.canonicalizer.is_some(),
        )
    }

    fn render_failure_diagnostic(&self, error: &CompactGraphUnionError) -> String {
        format!(
            "error={error:?} outer_bytes={} nested_bytes={} retained_supply_states={} ready={}/{} waiting_fields={}/{} waiting_work={} next_layer={}/{} promotion_capacity={} pins={}/{} candidates={}/{} usage={:?}",
            self.frontier_outer_bytes().unwrap_or(usize::MAX),
            self.retention.nested(),
            self.retained_supply_states,
            self.ready.len(),
            self.ready.capacity(),
            self.waiting.len(),
            self.waiting.capacity(),
            self.waiting_count,
            self.next_layer.len(),
            self.next_layer.capacity().unwrap_or(usize::MAX),
            self.promotion.as_ref().map_or(0, |layer| layer.table_capacity),
            self.pins.len(),
            self.pins.capacity(),
            self.candidates.len(),
            self.candidates.capacity(),
            self.usage,
        )
    }

    pub(crate) fn last_failure_diagnostic(&self) -> Option<&str> {
        self.last_failure_diagnostic.as_deref()
    }

    pub(crate) fn advance<G: Pc4GraphCandidateGuard>(
        &mut self,
        cache: &Pc4LookupGraphCache,
        work: NonZeroUsize,
        guard: &G,
    ) -> Result<CompactGraphUnionStep, CompactGraphUnionError> {
        if self.terminated {
            return Err(contract("pc4_compact_union_terminated"));
        }
        let result = self.advance_inner(cache, work, guard);
        if let Err(error) = &result {
            self.last_failure_diagnostic = Some(self.render_failure_diagnostic(error));
            self.terminated = true;
            self.completed = false;
            self.ready = VecDeque::new();
            self.waiting = HashMap::new();
            self.next_layer = LayerFrontier::new();
            self.promotion = None;
            self.candidates = HashSet::new();
            self.canonicalizer = None;
            self.waiting_count = 0;
            self.retained_supply_states = 0;
            self.residents = 0;
            self.pins = HashMap::new();
        }
        self.usage.peak_frontier_bytes = self.retention.peak();
        result
    }

    fn advance_inner<G: Pc4GraphCandidateGuard>(
        &mut self,
        cache: &Pc4LookupGraphCache,
        work: NonZeroUsize,
        guard: &G,
    ) -> Result<CompactGraphUnionStep, CompactGraphUnionError> {
        check_guard(&self.source, &self.target, guard)?;
        if cache.target() != &self.target {
            return Err(contract("pc4_compact_union_cache_mismatch"));
        }
        if self.completed {
            return Ok(CompactGraphUnionStep::Complete);
        }
        if let Some(canonicalizer) = &mut self.canonicalizer {
            let consumed = self
                .usage
                .work
                .checked_add(self.usage.canonicalization_work)
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
            let remaining = self.limits.work.get().saturating_sub(consumed);
            let Some(remaining) = NonZeroUsize::new(remaining) else {
                return Err(CompactGraphUnionError::Limit {
                    kind: "pc4_compact_union_work_limit",
                    limit: self.limits.work.get(),
                    attempted: consumed.saturating_add(1),
                });
            };
            let complete = canonicalizer
                .advance(work.min(remaining), &|| {
                    PcCandidatePageGuard::is_cancelled(guard)
                })
                .map_err(CompactGraphUnionError::Canonicalization)?;
            self.usage.canonicalization_work = canonicalizer.work_done();
            self.usage.peak_canonicalization_buffer_bytes = canonicalizer.peak_buffer_bytes();
            check_guard(&self.source, &self.target, guard)?;
            self.completed = complete;
            return Ok(if complete {
                CompactGraphUnionStep::Complete
            } else {
                CompactGraphUnionStep::Progress
            });
        }
        if !self.start_verified {
            let Some(hash) = cache.field_hash(self.start_field) else {
                return Ok(CompactGraphUnionStep::Waiting);
            };
            if hash != self.start_hash {
                return Err(contract("pc4_compact_union_start_field_mismatch"));
            }
            self.start_verified = true;
        }
        let partially_woken_residents = self.residents == self.limits.resident_work.get()
            && self.ready.len() < self.limits.resident_work.get();
        if cache.admission_revision() != self.cache_revision_seen
            || self.ready.is_empty()
            || partially_woken_residents
        {
            // The host bounds waiting fields with its I/O watermarks. Wake
            // known records without allocating a second list of all waiters.
            // Keep the ready backing store within the resident-work window:
            // waking every task for one admitted field at once can leave a
            // 128-slot deque behind even though at most 64 tasks may run.
            // If a partial wake filled that window, later work can free a
            // slot while more waiters for the SAME cached record remain. Such
            // an idempotent record has no new admission revision, so resident
            // saturation must also reopen this bounded wake scan.
            while self.ready.len() < self.limits.resident_work.get() {
                let Some(id) = self
                    .waiting
                    .keys()
                    .copied()
                    .find(|id| cache.contains_field_id(*id))
                else {
                    break;
                };
                let available = self.limits.resident_work.get() - self.ready.len();
                let wake = self
                    .waiting
                    .get(&id)
                    .expect("collected waiting field")
                    .len()
                    .min(available);
                self.reserve_ready(wake)?;
                self.waiting_count -= wake;
                // Resumed residents precede cold work. Pop from the retained
                // waiter buffer so a partial wake needs no temporary vector.
                for _ in 0..wake {
                    let task = self
                        .waiting
                        .get_mut(&id)
                        .expect("collected waiting field")
                        .pop()
                        .expect("bounded wake");
                    self.ready.push_front(task);
                }
                if self.waiting.get(&id).is_some_and(|tasks| tasks.is_empty()) {
                    let tasks = self.waiting.remove(&id).expect("empty waiting field");
                    let old_buffer = capacity_bytes::<Work>(tasks.capacity())?;
                    drop(tasks);
                    self.retention.release(old_buffer)?;
                }
                self.usage.peak_ready_work = self.usage.peak_ready_work.max(self.ready.len());
            }
            self.cache_revision_seen = cache.admission_revision();
        }
        for _ in 0..work.get() {
            check_guard(&self.source, &self.target, guard)?;
            if self.promotion.is_none() && self.ready.is_empty() && !self.waiting.is_empty() {
                return Ok(CompactGraphUnionStep::Waiting);
            }
            if self.residents == self.limits.resident_work.get()
                && !self.ready.front().is_some_and(|task| task.resident)
            {
                // All resident continuations are waiting. Do not charge work
                // or promote another cold task while only I/O can unblock a
                // resident and return its dependency pins/credit.
                return Ok(CompactGraphUnionStep::Waiting);
            }
            self.usage.work = self
                .usage
                .work
                .checked_add(1)
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
            limit(
                "pc4_compact_union_work_limit",
                self.limits.work,
                self.usage.work,
            )?;
            if self.promotion.is_some() && self.ready.len() < self.limits.resident_work.get() {
                let entry = self.promotion.as_mut().expect("checked promotion").next()?;
                if let Some((key, supply)) = entry {
                    // Keep the old hash allocation and nested supply payloads
                    // under their existing credit while streaming only a
                    // bounded number of work owners into the ready queue.
                    // Draining an entire million-state layer before doing any
                    // work would temporarily retain both full containers.
                    self.reserve_ready(1)?;
                    self.ready.push_back(Work::new(key, supply));
                    self.usage.peak_ready_work = self.usage.peak_ready_work.max(self.ready.len());
                    self.usage.promoted_states += 1;
                } else {
                    self.promotion = None;
                }
                continue;
            }
            if self.ready.is_empty() {
                if !self.waiting.is_empty() {
                    return Ok(CompactGraphUnionStep::Waiting);
                }
                if self.next_layer.is_empty() {
                    if self.retained_supply_states != 0
                        || self.retention.nested() != 0
                        || self.residents != 0
                        || !self.pins.is_empty()
                    {
                        return Err(contract("pc4_compact_union_frontier_accounting_failed"));
                    }
                    // No histories remain. Release queue backing stores before
                    // retaining both candidate buffers during canonicalization.
                    self.ready = VecDeque::new();
                    self.waiting = HashMap::new();
                    self.next_layer = LayerFrontier::new();
                    self.pins = HashMap::new();
                    self.canonicalizer = Some(
                        CooperativeCandidateCanonicalizer::begin(
                            core::mem::take(&mut self.candidates),
                            self.limits.canonicalization_bytes,
                        )
                        .map_err(CompactGraphUnionError::Canonicalization)?,
                    );
                    return Ok(CompactGraphUnionStep::Progress);
                }
                // Account for both fixed directories during the consuming
                // handoff: replacement next-layer directory, old directory,
                // and promotion directory are briefly live together.
                let directory_handoff = LayerFrontier::directory_bytes()?
                    .checked_add(LayerPromotion::directory_bytes()?)
                    .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
                self.authorize_frontier(directory_handoff)?;
                let old_layer = core::mem::take(&mut self.next_layer);
                self.promotion = Some(old_layer.into_promotion()?);
                self.retention.observe(self.frontier_outer_bytes()?)?;
                continue;
            }
            let mut task = self.ready.pop_front().expect("ready layer");
            if !task.resident {
                self.pin_field(task.key.field())?;
                self.residents += 1;
                self.usage.peak_resident_work = self.usage.peak_resident_work.max(self.residents);
                task.resident = true;
            }
            match self.step_work(&mut task, cache, guard)? {
                WorkStep::Continue => self.ready.push_front(task),
                WorkStep::Done => {
                    self.unpin_field(task.key.field())?;
                    if let Some(field) = task.target_pin {
                        self.unpin_field(field)?;
                    }
                    self.residents -= 1;
                    let states = task.supply_states();
                    let bytes = task.retained_payload_bytes()?;
                    drop(task);
                    self.retained_supply_states -= states;
                    self.retention.release(bytes)?;
                }
                WorkStep::Missing(id) => {
                    if !self.waiting.contains_key(&id) {
                        limit(
                            "pc4_compact_union_waiting_field_limit",
                            self.limits.waiting_fields,
                            self.waiting.len() + 1,
                        )?;
                        self.reserve_waiting_field()?;
                    }
                    self.reserve_waiting_work(id)?;
                    self.waiting
                        .get_mut(&id)
                        .expect("reserved waiter")
                        .push(task);
                    self.waiting_count += 1;
                    self.usage.peak_waiting_fields =
                        self.usage.peak_waiting_fields.max(self.waiting.len());
                }
            }
        }
        check_guard(&self.source, &self.target, guard)?;
        Ok(CompactGraphUnionStep::Progress)
    }

    fn step_work<G: Pc4GraphCandidateGuard>(
        &mut self,
        task: &mut Work,
        cache: &Pc4LookupGraphCache,
        guard: &G,
    ) -> Result<WorkStep, CompactGraphUnionError> {
        if task.supply.is_empty() {
            return Ok(WorkStep::Done);
        }
        if task.key.layout.placement_count() == self.placement_count {
            self.accept_terminal(
                task.key
                    .layout
                    .into_standard(self.source.initial_board_mask())?,
                task.key.field(),
            )?;
            return Ok(WorkStep::Done);
        }
        if task.piece == PieceKind::STANDARD_TETROMINOES.len() {
            return Ok(WorkStep::Done);
        }
        let piece = PieceKind::STANDARD_TETROMINOES[task.piece];
        if task.next_supply.is_none() {
            self.authorize_frontier(
                self.language
                    .maximum_frontier_capacity_bytes()
                    .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?,
            )?;
            let next = self
                .language
                .advance(&task.supply, piece, &|| {
                    PcCandidatePageGuard::is_cancelled(guard)
                })
                .map_err(CompactGraphUnionError::Supply)?;
            if next.is_empty() {
                task.piece += 1;
                return Ok(WorkStep::Continue);
            }
            self.retain_supply(next.state_count(), next.retained_state_capacity_bytes())?;
            task.next_supply = Some(next);
            return Ok(WorkStep::Continue);
        }
        if task.targets.is_none() {
            // Qualified v1 records use seven cumulative u8 degree endpoints.
            self.authorize_frontier(capacity_bytes::<u32>(usize::from(u8::MAX))?)?;
            let edges = match read_qualified_pc4_adjacency(
                &self.target,
                task.key.field(),
                crate::pc_candidate_execution_bridge::core_piece_to_graph(piece),
                task.key.layout.placement_count(),
                &mut cache.adjacency_provider(),
                guard,
            ) {
                Ok(edges) => edges,
                Err(FixedQueueTraversalPageError::Provider(
                    Pc4LookupAdjacencyError::RecordRequired { field_id },
                )) => {
                    return Ok(WorkStep::Missing(field_id));
                }
                Err(error) => return Err(CompactGraphUnionError::Adjacency(error)),
            };
            let mut targets = Vec::new();
            targets
                .try_reserve_exact(edges.len())
                .map_err(|_| allocation())?;
            targets.extend(edges.into_iter().map(|edge| edge.target_field_id()));
            self.retain_frontier(capacity_bytes::<u32>(targets.capacity())?)?;
            task.targets = Some(targets);
            return Ok(WorkStep::Continue);
        }
        let targets = task.targets.as_ref().expect("prepared piece adjacency");
        let Some(&target_field) = targets.get(task.target_index) else {
            let (states, bytes) = task.next_piece()?;
            self.retained_supply_states -= states;
            self.retention.release(bytes)?;
            return Ok(WorkStep::Continue);
        };
        if task.placements.is_none() {
            if task.target_pin.is_none() {
                self.pin_field(target_field)?;
                task.target_pin = Some(target_field);
            }
            self.authorize_frontier(capacity_bytes::<ClearraPlacementIdentity>(
                self.limits.edge_placements.get(),
            )?)?;
            let edge = QualifiedPc4GraphEdge::from_qualified_record(
                &self.target,
                task.key.field(),
                crate::pc_candidate_execution_bridge::core_piece_to_graph(piece),
                target_field,
            );
            let placements = match materialize_qualified_graph_edge(
                &edge,
                &mut cache.placement_materializer(),
                guard,
            ) {
                Ok(placements) => placements,
                Err(PlacementMaterializationError::Materializer(
                    Pc4LookupMaterializationError::SourceRecordRequired { field_id }
                    | Pc4LookupMaterializationError::TargetRecordRequired { field_id },
                )) => {
                    return Ok(WorkStep::Missing(field_id));
                }
                Err(error) => return Err(CompactGraphUnionError::Placement(error)),
            };
            limit(
                "pc4_compact_union_edge_placement_limit",
                self.limits.edge_placements,
                placements.len(),
            )?;
            self.usage.materialized_edges += 1;
            self.unpin_field(task.target_pin.take().expect("materialization target pin"))?;
            self.retain_frontier(capacity_bytes::<ClearraPlacementIdentity>(
                placements.capacity(),
            )?)?;
            task.placements = Some(placements);
            return Ok(WorkStep::Continue);
        }
        let Some(&placement) = task
            .placements
            .as_ref()
            .expect("materialized edge")
            .get(task.placement_index)
        else {
            let old = task.placements.take().expect("exhausted placements");
            let bytes = capacity_bytes::<ClearraPlacementIdentity>(old.capacity())?;
            drop(old);
            self.retention.release(bytes)?;
            task.placement_index = 0;
            task.target_index += 1;
            return Ok(WorkStep::Continue);
        };
        let (lifted, frame) = placement
            .rebase_in_frame(task.key.frame()?)
            .map_err(|e| contract(e.reason()))?;
        let cells = lifted.occupied_cells();
        // A graph edge can have several ILC realizations. A realization that
        // overlaps this original-frame history is not this history's child.
        let occupied = task.key.layout.occupied(self.source.initial_board_mask())?;
        if cells & !self.full_mask == 0 && occupied & cells == 0 {
            let supply = task.next_supply.as_ref().expect("advanced supply");
            if task.key.layout.placement_count() + 1 == self.placement_count {
                // Existence is already witnessed by this nonempty supply
                // prefix. A terminal has no successor to union/expand, so its
                // canonical layout need not occupy the next layer or copy a
                // supply frontier. Completion still waits for every branch.
                self.accept_terminal(
                    task.key.layout.into_standard_with_placement(
                        self.source.initial_board_mask(),
                        piece,
                        cells,
                    )?,
                    target_field,
                )?;
            } else {
                let layout = task.key.layout.with_placement(
                    self.source.initial_board_mask(),
                    piece,
                    cells,
                )?;
                let key = StateKey::new(layout, target_field, frame)?;
                if let Some(previous) = self.next_layer.get(&key) {
                    let old_len = previous.state_count();
                    let old_bytes = previous.retained_state_capacity_bytes();
                    self.authorize_frontier(
                        self.language
                            .maximum_frontier_capacity_bytes()
                            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?,
                    )?;
                    let merged = self
                        .language
                        .merge_layer(previous, supply, &|| {
                            PcCandidatePageGuard::is_cancelled(guard)
                        })
                        .map_err(CompactGraphUnionError::Supply)?;
                    self.retain_supply(
                        merged.state_count(),
                        merged.retained_state_capacity_bytes(),
                    )?;
                    if !self.next_layer.insert(key, merged)? {
                        return Err(contract("pc4_compact_union_merge_owner_missing"));
                    }
                    self.retained_supply_states -= old_len;
                    self.retention.release(old_bytes)?;
                    self.usage.merged_states += 1;
                } else {
                    self.reserve_next_layer(&key)?;
                    self.authorize_frontier(supply.retained_state_capacity_bytes())?;
                    let copy = supply.try_clone().map_err(CompactGraphUnionError::Supply)?;
                    self.retain_supply(copy.state_count(), copy.retained_state_capacity_bytes())?;
                    if self.next_layer.insert(key, copy)? {
                        return Err(contract("pc4_compact_union_duplicate_insert"));
                    }
                    self.usage.generated_states += 1;
                }
            }
        }
        task.placement_index += 1;
        Ok(WorkStep::Continue)
    }

    fn retain_supply(&mut self, added: usize, bytes: usize) -> Result<(), CompactGraphUnionError> {
        let count = self
            .retained_supply_states
            .checked_add(added)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        self.retain_frontier(bytes)?;
        self.retained_supply_states = count;
        Ok(())
    }

    fn frontier_outer_bytes(&self) -> Result<usize, CompactGraphUnionError> {
        let ready = capacity_bytes::<Work>(self.ready.capacity())?;
        let waiting = capacity_bytes::<(u32, Vec<Work>)>(self.waiting.capacity())?;
        let next = self.next_layer.retained_capacity_bytes()?;
        let promotion = self.promotion.as_ref().map_or(Ok(0), |layer| {
            layer
                .table_capacity
                .checked_mul(CompactPatternUnionLayerShard::<StateKey>::entry_size())
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))
        })?;
        let directories = LayerFrontier::directory_bytes()?
            .checked_add(if self.promotion.is_some() {
                LayerPromotion::directory_bytes()?
            } else {
                0
            })
            .ok_or_else(|| FrontierRetentionError::Overflow)?;
        let pins = capacity_bytes::<(u32, usize)>(self.pins.capacity())?;
        ready
            .checked_add(waiting)
            .and_then(|n| n.checked_add(next))
            .and_then(|n| n.checked_add(promotion))
            .and_then(|n| n.checked_add(directories))
            .and_then(|n| n.checked_add(pins))
            .ok_or_else(|| FrontierRetentionError::Overflow.into())
    }

    fn authorize_frontier(&self, additional: usize) -> Result<(), CompactGraphUnionError> {
        self.retention
            .authorize(self.frontier_outer_bytes()?, additional)?;
        Ok(())
    }

    fn pin_field(&mut self, field: u32) -> Result<(), CompactGraphUnionError> {
        if let Some(count) = self.pins.get_mut(&field) {
            *count = count
                .checked_add(1)
                .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
            return Ok(());
        }
        self.authorize_frontier(capacity_bytes::<(u32, usize)>(
            (self.pins.len() + 1).saturating_sub(self.pins.capacity()),
        )?)?;
        self.pins.try_reserve(1).map_err(|_| allocation())?;
        let outer = self.frontier_outer_bytes()?;
        self.retention.observe(outer)?;
        self.pins.insert(field, 1);
        self.compact_pins_if_oversized()?;
        Ok(())
    }

    fn unpin_field(&mut self, field: u32) -> Result<(), CompactGraphUnionError> {
        let count = self
            .pins
            .get_mut(&field)
            .ok_or_else(|| contract("pc4_compact_union_pin_accounting_failed"))?;
        *count = count
            .checked_sub(1)
            .ok_or_else(|| contract("pc4_compact_union_pin_accounting_failed"))?;
        if *count == 0 {
            self.pins.remove(&field);
        }
        Ok(())
    }

    fn compact_pins_if_oversized(&mut self) -> Result<(), CompactGraphUnionError> {
        // Every resident work item pins its source and at most one target.
        // Repeated insert/remove churn can exhaust hash-table growth slots and
        // double the backing table despite this finite live-key bound.
        let maximum_live = self
            .limits
            .resident_work
            .get()
            .checked_mul(2)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        if self.pins.len() > maximum_live {
            return Err(contract("pc4_compact_union_pin_limit"));
        }
        let oversized = maximum_live
            .checked_mul(3)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        if self.pins.capacity() <= oversized {
            return Ok(());
        }
        // A fresh table for at most `maximum_live` entries has a load-factor
        // capacity below this conservative two-times bound. Charge the full
        // temporary table while the old allocation is still live; if that
        // transient allocation has no authority, retain the current table and
        // retry after later work releases payloads.
        let temporary_capacity = maximum_live
            .checked_mul(2)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        let temporary_bytes = capacity_bytes::<(u32, usize)>(temporary_capacity)?;
        let outer = self.frontier_outer_bytes()?;
        match self.retention.retain(outer, temporary_bytes) {
            Ok(()) => {}
            Err(FrontierRetentionError::Limit { .. }) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let mut compact = HashMap::new();
        if compact.try_reserve(self.pins.len()).is_err() {
            self.retention.release(temporary_bytes)?;
            return Err(allocation());
        }
        if compact.capacity() > temporary_capacity {
            self.retention.release(temporary_bytes)?;
            return Err(contract("pc4_compact_union_pin_capacity_invalid"));
        }
        for (field, count) in self.pins.drain() {
            compact.insert(field, count);
        }
        let old = core::mem::replace(&mut self.pins, compact);
        drop(old);
        self.retention.release(temporary_bytes)?;
        self.retention.observe(self.frontier_outer_bytes()?)?;
        self.usage.pin_compactions += 1;
        Ok(())
    }

    fn retain_frontier(&mut self, bytes: usize) -> Result<(), CompactGraphUnionError> {
        let outer = self.frontier_outer_bytes()?;
        self.retention.retain(outer, bytes)?;
        Ok(())
    }

    // Pre-admit requested growth, then measure actual capacity immediately.
    // Hash table control/allocator metadata follows the existing logical
    // payload model, not an invented process RSS claim.
    fn reserve_ready(&mut self, additional: usize) -> Result<(), CompactGraphUnionError> {
        let requested = self
            .ready
            .len()
            .checked_add(additional)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        self.authorize_frontier(capacity_bytes::<Work>(
            requested.saturating_sub(self.ready.capacity()),
        )?)?;
        self.ready
            .try_reserve(additional)
            .map_err(|_| allocation())?;
        let outer = self.frontier_outer_bytes()?;
        self.retention.observe(outer)?;
        Ok(())
    }

    fn reserve_waiting_field(&mut self) -> Result<(), CompactGraphUnionError> {
        self.authorize_frontier(capacity_bytes::<(u32, Vec<Work>)>(
            (self.waiting.len() + 1).saturating_sub(self.waiting.capacity()),
        )?)?;
        self.waiting.try_reserve(1).map_err(|_| allocation())?;
        let outer = self.frontier_outer_bytes()?;
        self.retention.observe(outer)?;
        Ok(())
    }

    fn reserve_waiting_work(&mut self, id: u32) -> Result<(), CompactGraphUnionError> {
        let (old_capacity, requested) = self
            .waiting
            .get(&id)
            .map_or((0, 1), |tasks| (tasks.capacity(), tasks.len() + 1));
        self.authorize_frontier(capacity_bytes::<Work>(
            requested.saturating_sub(old_capacity),
        )?)?;
        let new_capacity = {
            let tasks = self.waiting.entry(id).or_default();
            tasks.try_reserve(1).map_err(|_| allocation())?;
            tasks.capacity()
        };
        self.retain_frontier(capacity_bytes::<Work>(new_capacity - old_capacity)?)?;
        Ok(())
    }

    fn reserve_next_layer(&mut self, key: &StateKey) -> Result<(), CompactGraphUnionError> {
        let additional = self
            .next_layer
            .additional_capacity_for(key)
            .checked_mul(CompactPatternUnionLayerShard::<StateKey>::entry_size())
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        self.authorize_frontier(additional)?;
        self.next_layer.reserve_for(key)?;
        let outer = self.frontier_outer_bytes()?;
        self.retention.observe(outer)?;
        Ok(())
    }

    fn accept_terminal(
        &mut self,
        layout: StandardBoard64TilingIdentity,
        field: u32,
    ) -> Result<(), CompactGraphUnionError> {
        if field != self.target.terminal_field().field_id() {
            return Ok(());
        }
        let occupied = layout
            .placement_masks()
            .iter()
            .fold(self.source.initial_board_mask(), |all, mask| all | mask);
        if layout.placement_count() != self.placement_count || occupied != self.full_mask {
            return Err(contract("pc4_compact_union_terminal_layout_mismatch"));
        }
        self.usage.terminal_arrivals += 1;
        if self.candidates.contains(&layout) {
            return Ok(());
        }
        let count = self
            .candidates
            .len()
            .checked_add(1)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        limit(
            "pc4_compact_union_candidate_limit",
            self.limits.candidates,
            count,
        )?;
        self.check_candidate_capacity(self.candidates.capacity().max(count), count)?;
        self.candidates.try_reserve(1).map_err(|_| allocation())?;
        self.check_candidate_capacity(self.candidates.capacity(), count)?;
        self.candidates.insert(layout);
        Ok(())
    }

    fn check_candidate_capacity(
        &self,
        table: usize,
        count: usize,
    ) -> Result<(), CompactGraphUnionError> {
        // Reserve both the retained set and the eventual exact output vector;
        // completing traversal must not discover a predictable copy cliff.
        let elements = table
            .checked_add(count)
            .ok_or_else(|| contract("pc4_compact_union_counter_overflow"))?;
        limit(
            "pc4_compact_union_candidate_byte_limit",
            self.limits.canonicalization_bytes,
            capacity_bytes::<StandardBoard64TilingIdentity>(elements)?,
        )
    }

    /// Consuming handoff only after complete traversal/materialization. There
    /// is no public raw-parts constructor and no observation/replay certificate.
    pub(crate) fn into_reducer_input<G: Pc4GraphCandidateGuard>(
        self,
        guard: &G,
    ) -> Result<PcCandidateReducerInput, CompactGraphUnionError> {
        check_guard(&self.source, &self.target, guard)?;
        if self.terminated
            || !self.completed
            || !self.ready.is_empty()
            || !self.waiting.is_empty()
            || !self.next_layer.is_empty()
            || self.promotion.is_some()
            || self.residents != 0
            || !self.pins.is_empty()
        {
            return Err(contract("pc4_compact_union_incomplete"));
        }
        let (candidates, digest) = self
            .canonicalizer
            .ok_or_else(|| contract("pc4_compact_union_incomplete"))?
            .into_parts()
            .map_err(CompactGraphUnionError::Canonicalization)?;
        check_guard(&self.source, &self.target, guard)?;
        let evidence = PcCandidateCompletenessEvidence::from_verified_complete_source(
            self.source,
            Some(self.target),
            candidates.len() as u64,
            digest,
        )
        .map_err(CompactGraphUnionError::Boundary)?;
        Ok(PcCandidateReducerInput {
            universe_identity: PcCandidateUniverseIdentity::from_complete_evidence(evidence)
                .map_err(CompactGraphUnionError::Boundary)?,
            candidates,
        })
    }
}

fn contract(reason: &'static str) -> CompactGraphUnionError {
    CompactGraphUnionError::Contract(reason)
}
fn allocation() -> CompactGraphUnionError {
    contract("pc4_compact_union_allocation_failed")
}
fn limit(
    kind: &'static str,
    maximum: NonZeroUsize,
    attempted: usize,
) -> Result<(), CompactGraphUnionError> {
    if attempted > maximum.get() {
        Err(CompactGraphUnionError::Limit {
            kind,
            limit: maximum.get(),
            attempted,
        })
    } else {
        Ok(())
    }
}
fn check_guard<G: Pc4GraphCandidateGuard>(
    source: &PcCandidateSourceBinding,
    target: &QualifiedPc4TargetIdentity,
    guard: &G,
) -> Result<(), CompactGraphUnionError> {
    if PcCandidatePageGuard::is_cancelled(guard) {
        return Err(contract("pc4_compact_union_cancelled"));
    }
    if !PcCandidatePageGuard::is_current_source(guard, source) {
        return Err(contract("pc4_compact_union_stale_source"));
    }
    if !PcCandidatePageGuard::is_current_snapshot(guard, target.snapshot()) {
        return Err(contract("pc4_compact_union_stale_snapshot"));
    }
    Ok(())
}

#[cfg(test)]
mod compact_layout_tests {
    use super::*;

    #[test]
    fn pc4_four_cell_combinadic_is_exact_and_matches_mask_order() {
        let mut ranked = Vec::new();
        for a in 0..PC4_CELL_COUNT {
            for b in (a + 1)..PC4_CELL_COUNT {
                for c in (b + 1)..PC4_CELL_COUNT {
                    for d in (c + 1)..PC4_CELL_COUNT {
                        let mask = (1_u64 << a) | (1_u64 << b) | (1_u64 << c) | (1_u64 << d);
                        let rank = rank_four_cell_mask(mask).expect("valid four-cell mask");
                        assert_eq!(unrank_four_cell_mask(rank), Some(mask));
                        ranked.push((mask, rank));
                    }
                }
            }
        }
        ranked.sort_unstable_by_key(|&(mask, _)| mask);
        assert_eq!(ranked.len() as u32, binomial(PC4_CELL_COUNT, 4));
        for (expected, &(_, rank)) in ranked.iter().enumerate() {
            assert_eq!(rank as usize, expected);
        }
    }

    #[test]
    fn pc4_compact_layout_is_lossless_order_independent_and_smaller() {
        let first = PiecePlacementMask::new(PieceKind::T, 0x0000_0000_0000_000f);
        let second = PiecePlacementMask::new(PieceKind::I, 0x0000_0000_0000_00f0);
        let left = CompactPc4LayoutIdentity::initial(0)
            .unwrap()
            .with_placement(0, first.piece(), first.cells_mask())
            .unwrap()
            .with_placement(0, second.piece(), second.cells_mask())
            .unwrap();
        let right = CompactPc4LayoutIdentity::initial(0)
            .unwrap()
            .with_placement(0, second.piece(), second.cells_mask())
            .unwrap()
            .with_placement(0, first.piece(), first.cells_mask())
            .unwrap();
        assert_eq!(left, right);
        assert_eq!(left.occupied(0).unwrap(), 0xff);
        assert_eq!(
            left.into_standard(0).unwrap(),
            StandardBoard64TilingIdentity::from_placements(0, [first, second]).unwrap()
        );
        assert_eq!(core::mem::size_of::<CompactPc4LayoutIdentity>(), 24);
        assert_eq!(core::mem::size_of::<StateKey>(), 28);
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(core::mem::size_of::<CompactPatternUnionFrontier>(), 24);
            assert_eq!(
                core::mem::size_of::<(StateKey, CompactPatternUnionFrontier)>(),
                56
            );
        }
        #[cfg(target_pointer_width = "32")]
        {
            assert_eq!(core::mem::size_of::<CompactPatternUnionFrontier>(), 12);
            assert_eq!(
                core::mem::size_of::<(StateKey, CompactPatternUnionFrontier)>(),
                40
            );
        }
        assert!(
            core::mem::size_of::<StateKey>()
                < core::mem::size_of::<StandardBoard64TilingIdentity>()
        );
    }

    #[test]
    fn pc4_packed_layout_slots_round_trip_across_word_boundaries() {
        let mut layout = CompactPc4LayoutIdentity::initial(0).unwrap();
        for index in 0..PC4_MAX_FRONTIER_PLACEMENTS {
            let value = ((index + 1) * 91_391) as u32 & PC4_PACKED_PLACEMENT_MASK;
            layout.set_slot(index, value).unwrap();
            assert_eq!(layout.slot(index), value);
        }
        for index in 0..PC4_MAX_FRONTIER_PLACEMENTS {
            let value = ((index + 1) * 91_391) as u32 & PC4_PACKED_PLACEMENT_MASK;
            assert_eq!(layout.slot(index), value);
        }
    }

    #[test]
    fn pc4_state_key_packs_field_and_exact_row_frame_without_truncation() {
        let layout = CompactPc4LayoutIdentity::initial(0).unwrap();
        for mask in 0..16 {
            let frame = Pc4RowFrame::from_surviving_original_rows_mask(mask).unwrap();
            let key = StateKey::new(layout, PC4_FIELD_MASK, frame).unwrap();
            assert_eq!(key.field(), PC4_FIELD_MASK);
            assert_eq!(key.frame().unwrap(), frame);
        }
        assert_eq!(
            StateKey::new(layout, PC4_FIELD_MASK + 1, Pc4RowFrame::new(0).unwrap())
                .unwrap_err()
                .reason(),
            "pc4_compact_union_field_id_outside_compact_domain"
        );
    }
}
