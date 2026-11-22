//! Exact finite kind-word sets for an already observed supply suffix.
//!
//! This module owns no policy or observation authority. Its caller supplies
//! the actual relevant draws only after the original-source reveal proof and
//! excludes terminal targets and both projected-lookahead contracts. Every
//! supply edge below comes from the existing sequence execution automaton.

use clearra_supply::{
    execution_automaton::{
        SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState,
        SupplyObservationIdentity, SupplyProvenanceId,
    },
    hold::hold_policy::HoldPolicy,
    piece_source::{PieceSourceId, PieceSourceKind},
    SupplyExecutionError,
};

use crate::backend::wasm_cpu::{
    queue_observation_policy::{piece_code, piece_from_code},
    WasmExactSearchError,
};

pub(super) const MAX_REMAINING: u8 = 6;
pub(super) const MAX_BITMAP_WORDS: usize = 12;
pub(super) const MAX_QUERY_GROUPS: usize = 7;
pub(super) const MAX_QUERY_CACHE: usize = 65_536;
const FACTORIAL: [usize; 7] = [1, 1, 2, 6, 24, 120, 720];

/// The queue starts at the current original cursor. `len` is the number of
/// actually observed relevant draws, not the length of an inferred future.
/// The caller uses zeroes in the unused tail; the full struct is the cache key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::backend::wasm_cpu) struct KnownSupplyQuery {
    pub queue: [u8; 7],
    pub len: u8,
    pub hold_code: u8,
    pub hold_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct QueryKey {
    pub remaining: u8,
    pub supply: KnownSupplyQuery,
}

/// A target's present kinds are renamed in increasing original-kind order.
/// The at-most-one available piece outside the target is named 7. The target
/// counts remain in the key: identical renamed queues can have different
/// target multiplicities and therefore different rank spaces.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct TargetQueryKey {
    pub remaining: u8,
    pub counts: u32,
    pub supply: KnownSupplyQuery,
}

/// Pure source facts, retained separately from the target bitmap cache so a
/// repeated known queue need not recount its pieces or enumerate its targets.
#[derive(Clone, Copy)]
pub(super) struct QueryProfile {
    key: QueryKey,
    available: u32,
    targets: [u32; MAX_QUERY_GROUPS],
    target_count: u8,
}

impl QueryProfile {
    pub(super) fn new(
        key: QueryKey,
        mut charge: impl FnMut(usize) -> Result<(), WasmExactSearchError>,
    ) -> Result<Self, WasmExactSearchError> {
        if key.supply.key(key.remaining)? != Some(key) {
            return Err(invalid("wasm_observation_known_supply_invalid"));
        }
        let mut available = 0_u32;
        for &piece in &key.supply.queue[..usize::from(key.supply.len)] {
            charge(1)?;
            available += 1 << (usize::from(piece - 1) * 4);
        }
        if key.supply.hold_enabled && key.supply.hold_code != 0 {
            charge(1)?;
            available += 1 << (usize::from(key.supply.hold_code - 1) * 4);
        }
        let total = key.supply.len + u8::from(key.supply.hold_enabled && key.supply.hold_code != 0);
        if total > key.remaining + 1 {
            return Err(invalid("wasm_observation_known_supply_invalid"));
        }
        let mut targets = [0; MAX_QUERY_GROUPS];
        let mut target_count = 0;
        if total == key.remaining {
            charge(1)?;
            targets[0] = available;
            target_count = 1;
        } else if total == key.remaining + 1 {
            for kind in 0..7 {
                charge(1)?;
                if (available >> (kind * 4)) & 15 != 0 {
                    targets[usize::from(target_count)] = available - (1 << (kind * 4));
                    target_count += 1;
                }
            }
        }
        Ok(Self {
            key,
            available,
            targets,
            target_count,
        })
    }

    pub(super) fn target_count(&self) -> usize {
        usize::from(self.target_count)
    }

    pub(super) fn target(&self, index: usize) -> Option<u32> {
        (index < self.target_count()).then(|| self.targets[index])
    }

    pub(super) fn target_index(&self, counts: u32) -> Option<usize> {
        self.targets[..self.target_count()]
            .iter()
            .position(|&target| target == counts)
    }

    /// Without projected EOF release every lock consumes a current draw.
    /// For one of the profile's targets, len>=r is also sufficient: leave
    /// the sole unused piece held (Current), or store/swap it when encountered.
    pub(super) fn target_has_a_word(&self) -> bool {
        self.key.supply.len >= self.key.remaining
    }

    pub(super) fn canonical_target(
        &self,
        target: usize,
        mut charge: impl FnMut(usize) -> Result<(), WasmExactSearchError>,
    ) -> Result<TargetQueryKey, WasmExactSearchError> {
        let counts = self
            .target(target)
            .filter(|&counts| multiset_size(counts) == Some(self.key.remaining))
            .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
        let mut names = [7_u8; 8];
        names[0] = 0;
        let mut canonical_counts = 0_u32;
        let mut present = 0_usize;
        let mut outside = 0_u8;
        for kind in 0..7 {
            charge(1)?;
            let count = (counts >> (kind * 4)) & 15;
            let available = (self.available >> (kind * 4)) & 15;
            if count > available {
                return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
            }
            if count == 0 {
                outside += available as u8;
            } else {
                names[kind + 1] = present as u8 + 1;
                canonical_counts |= count << (present * 4);
                present += 1;
            }
        }
        if outside > 1 || present > usize::from(MAX_REMAINING) {
            return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
        }
        let mut supply = KnownSupplyQuery {
            queue: [0; 7],
            len: self.key.supply.len,
            hold_code: 0,
            hold_enabled: self.key.supply.hold_enabled,
        };
        for position in 0..usize::from(supply.len) {
            charge(1)?;
            supply.queue[position] = names[usize::from(self.key.supply.queue[position])];
        }
        if self.key.supply.hold_code != 0 {
            charge(1)?;
            supply.hold_code = names[usize::from(self.key.supply.hold_code)];
        }
        Ok(TargetQueryKey {
            remaining: self.key.remaining,
            counts: canonical_counts,
            supply,
        })
    }
}

impl KnownSupplyQuery {
    pub(super) fn key(self, remaining: u8) -> Result<Option<QueryKey>, WasmExactSearchError> {
        if !(1..=MAX_REMAINING).contains(&remaining) {
            return Ok(None);
        }
        if usize::from(self.len) > self.queue.len()
            || self.hold_code > 7
            || self.queue[..usize::from(self.len)]
                .iter()
                .any(|piece| !(1..=7).contains(piece))
        {
            return Err(invalid("wasm_observation_known_supply_invalid"));
        }
        // Do not create a new error for a broader caller whose held value is
        // unavailable under a forbidden-hold policy. It keeps the old path.
        if !self.hold_enabled && self.hold_code != 0 {
            return Ok(None);
        }
        let extra = u8::from(self.hold_enabled && self.hold_code == 0);
        if self.len > remaining + extra {
            return Ok(None);
        }
        Ok(Some(QueryKey {
            remaining,
            supply: self,
        }))
    }
}

/// Multiset counts use the same seven four-bit fields as the existing kind
/// certificate. Rank zero is the lexicographically first distinct word.
pub(super) fn permutation_count(counts: u32) -> Option<usize> {
    if counts >> 28 != 0 {
        return None;
    }
    let mut total = 0;
    let mut divisor = 1;
    for kind in 0..7 {
        let count = ((counts >> (kind * 4)) & 15) as usize;
        total += count;
        if total > usize::from(MAX_REMAINING) {
            return None;
        }
        divisor *= *FACTORIAL.get(count)?;
    }
    Some(*FACTORIAL.get(total)? / divisor)
}

pub(super) fn multiset_size(counts: u32) -> Option<u8> {
    permutation_count(counts)?;
    Some((0..7).map(|kind| ((counts >> (kind * 4)) & 15) as u8).sum())
}

/// The p-block contains P(M-e_p) ranks and starts after every smaller kind.
/// P(M-e_p) = P(M)*M[p]/|M|, so no permutation enumeration is needed.
fn prefix_block(counts: u32, piece: u8) -> Option<(usize, u32)> {
    if !(1..=7).contains(&piece) {
        return None;
    }
    let shift = usize::from(piece - 1) * 4;
    if (counts >> shift) & 15 == 0 {
        return None;
    }
    let total = usize::from(multiset_size(counts)?);
    let lower: usize = (0..usize::from(piece - 1))
        .map(|kind| ((counts >> (kind * 4)) & 15) as usize)
        .sum();
    Some((
        permutation_count(counts)? * lower / total,
        counts - (1 << shift),
    ))
}

#[cfg(test)]
fn word_rank(mut counts: u32, word: &[u8]) -> Option<usize> {
    if usize::from(multiset_size(counts)?) != word.len() {
        return None;
    }
    let mut rank = 0;
    for &piece in word {
        let (offset, child) = prefix_block(counts, piece)?;
        rank += offset;
        counts = child;
    }
    (counts == 0).then_some(rank)
}

#[derive(Clone, Copy)]
pub(super) struct RankedBits {
    pub counts: u32,
    bits: [u64; MAX_BITMAP_WORDS],
    word_len: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BitmapCoverage {
    Empty,
    Full,
    Partial,
}

impl RankedBits {
    #[cfg(test)]
    const EMPTY: Self = Self {
        counts: 0,
        bits: [0; MAX_BITMAP_WORDS],
        word_len: 1,
    };

    pub(super) fn new(counts: u32) -> Option<Self> {
        let bits = permutation_count(counts)?;
        let word_len = bits.div_ceil(64);
        (word_len <= MAX_BITMAP_WORDS).then_some(Self {
            counts,
            bits: [0; MAX_BITMAP_WORDS],
            word_len: word_len as u8,
        })
    }

    pub(super) fn words(&self) -> &[u64] {
        &self.bits[..usize::from(self.word_len)]
    }

    pub(super) fn accept_empty(&mut self) -> Result<(), WasmExactSearchError> {
        if self.counts != 0 {
            return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
        }
        self.bits[0] = 1;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn insert_word(&mut self, word: &[u8]) -> Result<(), WasmExactSearchError> {
        let rank = word_rank(self.counts, word)
            .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
        self.bits[rank / 64] |= 1_u64 << (rank % 64);
        Ok(())
    }

    fn insert_rank(&mut self, rank: usize) -> Result<(), WasmExactSearchError> {
        if rank >= permutation_count(self.counts).unwrap_or(0) {
            return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
        }
        self.bits[rank / 64] |= 1_u64 << (rank % 64);
        Ok(())
    }

    pub(super) fn or_same_counts(
        &mut self,
        counts: u32,
        words: &[u64],
        mut charge: impl FnMut(usize) -> Result<(), WasmExactSearchError>,
    ) -> Result<(), WasmExactSearchError> {
        charge(1)?;
        let bits = permutation_count(counts)
            .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
        if counts != self.counts
            || words.len() != usize::from(self.word_len)
            || (bits % 64 != 0 && words.last().is_some_and(|word| word >> (bits % 64) != 0))
        {
            return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
        }
        for (index, &word) in words.iter().enumerate() {
            charge(1)?;
            self.bits[index] |= word;
        }
        Ok(())
    }

    /// The scan proves an Empty/Full certificate without keeping a payload.
    /// Padding is excluded from Full; each actual word inspection is charged.
    pub(super) fn coverage(
        &self,
        mut charge: impl FnMut(usize) -> Result<(), WasmExactSearchError>,
    ) -> Result<BitmapCoverage, WasmExactSearchError> {
        let bits = permutation_count(self.counts)
            .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
        let mut empty = true;
        let mut full = true;
        for (index, &word) in self.words().iter().enumerate() {
            charge(1)?;
            let mask = if index + 1 == usize::from(self.word_len) && bits % 64 != 0 {
                (1_u64 << (bits % 64)) - 1
            } else {
                u64::MAX
            };
            if word & !mask != 0 {
                return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
            }
            empty &= word == 0;
            full &= word == mask;
        }
        Ok(if empty {
            BitmapCoverage::Empty
        } else if full {
            BitmapCoverage::Full
        } else {
            BitmapCoverage::Partial
        })
    }

    /// Exact work of `or_child`, including zero-valued destination ORs. A
    /// caller may precharge this amount before borrowing cached child words.
    pub(super) fn child_work(&self, piece: u8, child_counts: u32) -> Option<usize> {
        let (offset, expected) = prefix_block(self.counts, piece)?;
        if expected != child_counts {
            return None;
        }
        let child_words = permutation_count(child_counts)?.div_ceil(64);
        let high_words = if offset % 64 == 0 {
            0
        } else {
            child_words.min(usize::from(self.word_len).saturating_sub(offset / 64 + 1))
        };
        Some(1 + child_words + high_words)
    }

    pub(super) fn or_child(
        &mut self,
        piece: u8,
        child_counts: u32,
        child: &[u64],
        mut charge: impl FnMut(usize) -> Result<(), WasmExactSearchError>,
    ) -> Result<(), WasmExactSearchError> {
        charge(1)?;
        let (offset, expected) = prefix_block(self.counts, piece)
            .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
        let child_bits = permutation_count(child_counts)
            .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
        if expected != child_counts
            || child.len() != child_bits.div_ceil(64)
            || (child_bits % 64 != 0
                && child
                    .last()
                    .is_some_and(|word| word >> (child_bits % 64) != 0))
        {
            return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
        }
        let start = offset / 64;
        let shift = offset % 64;
        let word_len = usize::from(self.word_len);
        for (index, &word) in child.iter().enumerate() {
            // One actual source-word read and low-word shift/OR, plus a
            // second destination-word operation for every in-bounds spill.
            charge(1)?;
            let destination = start + index;
            let target = self
                .bits
                .get_mut(destination)
                .filter(|_| destination < word_len)
                .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
            *target |= word << shift;
            if shift != 0 {
                let spill = word >> (64 - shift);
                if destination + 1 < word_len {
                    charge(1)?;
                    self.bits[destination + 1] |= spill;
                } else if spill != 0 {
                    return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
                }
            }
        }
        Ok(())
    }
}

/// Compile only the target multiset. Input pieces outside it may still be
/// stored or left held; pruning happens on the automaton's USED piece, never
/// on its current/held input. The rank offset is accumulated along the path
/// instead of walking every completed word again at a leaf.
pub(super) fn compile_target_query(
    key: TargetQueryKey,
    mut charge: impl FnMut(usize) -> Result<(), WasmExactSearchError>,
) -> Result<RankedBits, WasmExactSearchError> {
    if key.supply.key(key.remaining)?.is_none() || multiset_size(key.counts) != Some(key.remaining)
    {
        return Err(invalid("wasm_observation_word_bitmap_multiset_invalid"));
    }
    let mut output = RankedBits::new(key.counts)
        .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
    visit_target_query(
        key,
        initial_query_state(key.supply),
        key.counts,
        0,
        &mut output,
        &mut charge,
    )?;
    Ok(output)
}

fn initial_query_state(supply: KnownSupplyQuery) -> SupplyExecutionState {
    SupplyExecutionState::with_contract(
        PieceSourceId::new(0),
        PieceSourceKind::ObservedWindow,
        0,
        (supply.hold_code != 0).then(|| piece_from_code(supply.hold_code)),
        if supply.hold_enabled {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        },
        0,
        0,
        SupplyObservationIdentity::full_queue_oracle(),
        SupplyProvenanceId(0),
    )
}

fn visit_target_query(
    key: TargetQueryKey,
    state: SupplyExecutionState,
    counts: u32,
    rank: usize,
    output: &mut RankedBits,
    charge: &mut impl FnMut(usize) -> Result<(), WasmExactSearchError>,
) -> Result<(), WasmExactSearchError> {
    charge(1)?;
    if counts == 0 {
        charge(1)?;
        return output.insert_rank(rank);
    }
    let cursor = usize::from(state.cursor);
    if cursor >= usize::from(key.supply.len) {
        return Ok(());
    }
    let current = piece_from_code(key.supply.queue[cursor]);
    let next = (cursor + 1 < usize::from(key.supply.len))
        .then(|| piece_from_code(key.supply.queue[cursor + 1]));
    for branch in [
        SupplyBranchKind::Current,
        SupplyBranchKind::SwapHeld,
        SupplyBranchKind::StoreCurrent,
    ] {
        charge(1)?;
        let step =
            match SupplyExecutionAutomaton::sequence().transition(state, branch, current, next) {
                Ok(step) => step,
                Err(
                    SupplyExecutionError::HoldForbidden
                    | SupplyExecutionError::MissingHeldPiece
                    | SupplyExecutionError::HoldSlotOccupied
                    | SupplyExecutionError::MissingNextPiece,
                ) => continue,
                Err(_) => return Err(invalid("wasm_observation_supply_transition_invalid")),
            };
        charge(1)?;
        let Some((offset, child_counts)) = prefix_block(counts, piece_code(step.used_piece)) else {
            continue;
        };
        visit_target_query(
            key,
            step.next_state,
            child_counts,
            rank + offset,
            output,
            charge,
        )?;
    }
    Ok(())
}

// The original full-word compiler is kept only as an independent test
// reference. Production always uses the target compiler above.
#[cfg(test)]
pub(super) struct CompiledQuery {
    masks: [RankedBits; MAX_QUERY_GROUPS],
    len: usize,
}

#[cfg(test)]
impl CompiledQuery {
    pub(super) fn masks(&self) -> &[RankedBits] {
        &self.masks[..self.len]
    }

    fn insert(&mut self, counts: u32, word: &[u8]) -> Result<(), WasmExactSearchError> {
        let index = match self.masks().iter().position(|mask| mask.counts == counts) {
            Some(index) => index,
            None => {
                if self.len == self.masks.len() {
                    return Err(invalid("wasm_observation_word_bitmap_query_multiset_limit"));
                }
                self.masks[self.len] = RankedBits::new(counts)
                    .ok_or_else(|| invalid("wasm_observation_word_bitmap_multiset_invalid"))?;
                self.len += 1;
                self.len - 1
            }
        };
        self.masks[index].insert_word(word)
    }
}

/// At most r+1 pieces are available, so every length-r output multiset is
/// A itself or A minus one kind. There are at most seven exact rank spaces.
#[cfg(test)]
pub(super) fn compile_query(
    key: QueryKey,
    mut charge: impl FnMut(usize) -> Result<(), WasmExactSearchError>,
) -> Result<CompiledQuery, WasmExactSearchError> {
    if key.supply.key(key.remaining)? != Some(key) {
        return Err(invalid("wasm_observation_known_supply_invalid"));
    }
    let state = initial_query_state(key.supply);
    let mut output = CompiledQuery {
        masks: [RankedBits::EMPTY; MAX_QUERY_GROUPS],
        len: 0,
    };
    visit_query(
        key,
        state,
        &mut [0; MAX_REMAINING as usize],
        0,
        0,
        &mut output,
        &mut charge,
    )?;
    Ok(output)
}

#[cfg(test)]
fn visit_query(
    key: QueryKey,
    state: SupplyExecutionState,
    word: &mut [u8; MAX_REMAINING as usize],
    depth: u8,
    counts: u32,
    output: &mut CompiledQuery,
    charge: &mut impl FnMut(usize) -> Result<(), WasmExactSearchError>,
) -> Result<(), WasmExactSearchError> {
    charge(1)?;
    if depth == key.remaining {
        // Charge the exact rank walk and the bounded multiset-group lookup.
        charge(usize::from(depth) + 1)?;
        return output.insert(counts, &word[..usize::from(depth)]);
    }
    let cursor = usize::from(state.cursor);
    if cursor >= usize::from(key.supply.len) {
        // A held-only EOF release needs projected-lookahead permission and
        // is deliberately outside this query's already-qualified contract.
        return Ok(());
    }
    let current = piece_from_code(key.supply.queue[cursor]);
    let next = (cursor + 1 < usize::from(key.supply.len))
        .then(|| piece_from_code(key.supply.queue[cursor + 1]));
    for branch in [
        SupplyBranchKind::Current,
        SupplyBranchKind::SwapHeld,
        SupplyBranchKind::StoreCurrent,
    ] {
        charge(1)?;
        let step =
            match SupplyExecutionAutomaton::sequence().transition(state, branch, current, next) {
                Ok(step) => step,
                Err(
                    SupplyExecutionError::HoldForbidden
                    | SupplyExecutionError::MissingHeldPiece
                    | SupplyExecutionError::HoldSlotOccupied
                    | SupplyExecutionError::MissingNextPiece,
                ) => continue,
                Err(_) => return Err(invalid("wasm_observation_supply_transition_invalid")),
            };
        let piece = piece_code(step.used_piece);
        word[usize::from(depth)] = piece;
        visit_query(
            key,
            step.next_state,
            word,
            depth + 1,
            counts + (1 << (usize::from(piece - 1) * 4)),
            output,
            charge,
        )?;
    }
    Ok(())
}

pub(super) struct StoredBitmap {
    pub counts: u32,
    pub words: Vec<u64>,
}

impl StoredBitmap {
    pub(super) fn retained_bytes(&self) -> usize {
        self.words.capacity() * core::mem::size_of::<u64>()
    }
}

fn invalid(code: &'static str) -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem(code)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn words(counts: u32) -> Vec<Vec<u8>> {
        fn visit(counts: u32, word: &mut Vec<u8>, out: &mut Vec<Vec<u8>>) {
            if counts == 0 {
                out.push(word.clone());
                return;
            }
            for kind in 0..7 {
                if (counts >> (kind * 4)) & 15 != 0 {
                    word.push(kind as u8 + 1);
                    visit(counts - (1 << (kind * 4)), word, out);
                    word.pop();
                }
            }
        }
        let mut out = Vec::new();
        visit(counts, &mut Vec::new(), &mut out);
        out
    }

    fn query_words(query: &CompiledQuery) -> BTreeSet<Vec<u8>> {
        query
            .masks()
            .iter()
            .flat_map(|mask| {
                words(mask.counts)
                    .into_iter()
                    .enumerate()
                    .filter_map(|(rank, word)| {
                        (mask.words()[rank / 64] & (1 << (rank % 64)) != 0).then_some(word)
                    })
            })
            .collect()
    }

    #[test]
    fn word_bitmap_rank_blocks_match_every_distinct_small_permutation() {
        for counts in [0, 6, 0x33, 0x1122, 0x111111, 0x21111, 0x200004] {
            let enumerated = words(counts);
            assert_eq!(enumerated.len(), permutation_count(counts).unwrap());
            let mut direct = RankedBits::new(counts).unwrap();
            for (rank, word) in enumerated.iter().enumerate() {
                assert_eq!(word_rank(counts, word), Some(rank));
                direct.insert_word(word).unwrap();
            }
            let mut combined = RankedBits::new(counts).unwrap();
            if counts == 0 {
                combined.accept_empty().unwrap();
            } else {
                for piece in 1..=7 {
                    let Some((_, child_counts)) = prefix_block(counts, piece) else {
                        continue;
                    };
                    let mut child = RankedBits::new(child_counts).unwrap();
                    for word in words(child_counts) {
                        child.insert_word(&word).unwrap();
                    }
                    combined
                        .or_child(piece, child_counts, child.words(), |_| Ok(()))
                        .unwrap();
                }
            }
            assert_eq!(combined.words(), direct.words());
        }
        assert_eq!(permutation_count(0x111111), Some(720));
        assert_eq!(RankedBits::new(0x111111).unwrap().words().len(), 12);
        assert!(permutation_count(7).is_none());
        assert!(word_rank(0x11, &[1, 1]).is_none());
    }

    #[test]
    fn word_bitmap_query_uses_hold_kernel_and_never_releases_at_eof() {
        let query = KnownSupplyQuery {
            queue: [1, 2, 3, 0, 0, 0, 0],
            len: 3,
            hold_code: 0,
            hold_enabled: true,
        };
        let mut work = 0;
        let compiled = compile_query(query.key(2).unwrap().unwrap(), |steps| {
            work += steps;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            query_words(&compiled),
            [vec![1, 2], vec![1, 3], vec![2, 1], vec![2, 3]]
                .into_iter()
                .collect()
        );
        assert!(work > 4);

        let held = KnownSupplyQuery {
            queue: [1, 2, 0, 0, 0, 0, 0],
            len: 2,
            hold_code: 3,
            hold_enabled: true,
        };
        assert_eq!(
            query_words(&compile_query(held.key(2).unwrap().unwrap(), |_| Ok(())).unwrap()),
            [vec![1, 2], vec![1, 3], vec![3, 2], vec![3, 1]]
                .into_iter()
                .collect()
        );
        let eof = KnownSupplyQuery { len: 0, ..held };
        assert!(
            query_words(&compile_query(eof.key(1).unwrap().unwrap(), |_| Ok(())).unwrap())
                .is_empty()
        );
        let forbidden = KnownSupplyQuery {
            hold_enabled: false,
            ..held
        };
        assert!(forbidden.key(2).unwrap().is_none());
        assert!(query.key(0).unwrap().is_none());
        assert!(query.key(7).unwrap().is_none());
    }

    #[test]
    fn word_bitmap_precharge_matches_zero_and_nonzero_word_operations() {
        let counts = 0x111111;
        for piece in 1..=6 {
            let (_, child_counts) = prefix_block(counts, piece).unwrap();
            for populated in [false, true] {
                let mut parent = RankedBits::new(counts).unwrap();
                let mut child = RankedBits::new(child_counts).unwrap();
                if populated {
                    child
                        .insert_word(words(child_counts).last().unwrap())
                        .unwrap();
                }
                let before = parent.child_work(piece, child_counts).unwrap();
                let mut actual = 0;
                parent
                    .or_child(piece, child_counts, child.words(), |work| {
                        actual += work;
                        Ok(())
                    })
                    .unwrap();
                assert_eq!(actual, before);
                assert_eq!(
                    parent
                        .words()
                        .iter()
                        .map(|word| word.count_ones())
                        .sum::<u32>(),
                    u32::from(populated),
                );
            }
        }
    }

    fn supply(queue: &[u8], held: u8, enabled: bool) -> KnownSupplyQuery {
        let mut actual = [0; 7];
        actual[..queue.len()].copy_from_slice(queue);
        KnownSupplyQuery {
            queue: actual,
            len: queue.len() as u8,
            hold_code: held,
            hold_enabled: enabled,
        }
    }

    fn counts(word: &[u8]) -> u32 {
        word.iter().fold(0, |value, &piece| {
            value + (1 << (usize::from(piece - 1) * 4))
        })
    }

    #[test]
    fn word_bitmap_target_keys_preserve_order_counts_and_one_outsider() {
        let first = QueryProfile::new(supply(&[1, 3, 5], 0, true).key(2).unwrap().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
        let mut renamed = supply(&[2, 4, 6], 0, true);
        renamed.queue[6] = 99; // Raw key stays exact; the canonical unused tail is zero.
        let second = QueryProfile::new(renamed.key(2).unwrap().unwrap(), |_| Ok(())).unwrap();
        let a = first
            .canonical_target(first.target_index(counts(&[3, 5])).unwrap(), |_| Ok(()))
            .unwrap();
        let b = second
            .canonical_target(second.target_index(counts(&[4, 6])).unwrap(), |_| Ok(()))
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.supply.queue, [7, 1, 2, 0, 0, 0, 0]);
        let bitmap = compile_target_query(a, |_| Ok(())).unwrap();
        assert_eq!(bitmap.words(), &[1]); // Store the outside current; do not prune its input.

        let profile = QueryProfile::new(
            supply(&[1, 1, 2, 2], 0, true).key(3).unwrap().unwrap(),
            |_| Ok(()),
        )
        .unwrap();
        let first = profile
            .canonical_target(
                profile.target_index(counts(&[1, 1, 2])).unwrap(),
                |_| Ok(()),
            )
            .unwrap();
        let second = profile
            .canonical_target(
                profile.target_index(counts(&[1, 2, 2])).unwrap(),
                |_| Ok(()),
            )
            .unwrap();
        assert_eq!(first.supply, second.supply);
        assert_ne!(first.counts, second.counts);
        assert_ne!(first, second);
    }

    #[test]
    fn word_bitmap_target_compiler_and_full_certificate_match_small_hold_automata() {
        // Exhaust every short raw queue/held combination. The independent
        // full compiler enumerates output words before ranking; the target
        // compiler prunes used pieces and accumulates rank during traversal.
        for remaining in 1_u8..=3 {
            for len in 0..=usize::from(remaining + 1) {
                for encoding in 0..3_usize.pow(len as u32) {
                    let mut queue = [0; 7];
                    let mut value = encoding;
                    for piece in &mut queue[..len] {
                        *piece = (value % 3) as u8 + 1;
                        value /= 3;
                    }
                    for held in 0..=3 {
                        for enabled in [false, true] {
                            let raw = KnownSupplyQuery {
                                queue,
                                len: len as u8,
                                hold_code: held,
                                hold_enabled: enabled,
                            };
                            let Some(key) = raw.key(remaining).unwrap() else {
                                continue;
                            };
                            let reference = compile_query(key, |_| Ok(())).unwrap();
                            let profile = QueryProfile::new(key, |_| Ok(())).unwrap();
                            for index in 0..profile.target_count() {
                                let actual_counts = profile.target(index).unwrap();
                                let canonical =
                                    profile.canonical_target(index, |_| Ok(())).unwrap();
                                let compiled = compile_target_query(canonical, |_| Ok(())).unwrap();
                                let empty = RankedBits::new(actual_counts).unwrap();
                                let expected = reference
                                    .masks()
                                    .iter()
                                    .find(|mask| mask.counts == actual_counts)
                                    .unwrap_or(&empty);
                                assert_eq!(
                                    compiled.words(),
                                    expected.words(),
                                    "r={remaining} raw={raw:?} counts={actual_counts:x}"
                                );
                                assert_eq!(
                                    expected.words().iter().any(|&word| word != 0),
                                    profile.target_has_a_word(),
                                    "r={remaining} raw={raw:?} counts={actual_counts:x}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn word_bitmap_target_pruning_avoids_other_multisets_and_repeated_leaf_ranks() {
        let key = supply(&[1, 2, 3, 4, 5, 6, 7], 0, true)
            .key(6)
            .unwrap()
            .unwrap();
        let profile = QueryProfile::new(key, |_| Ok(())).unwrap();
        let actual = counts(&[2, 3, 4, 5, 6, 7]);
        let canonical = profile
            .canonical_target(profile.target_index(actual).unwrap(), |_| Ok(()))
            .unwrap();
        let mut full_work = 0;
        let full = compile_query(key, |work| {
            full_work += work;
            Ok(())
        })
        .unwrap();
        let mut target_work = 0;
        let target = compile_target_query(canonical, |work| {
            target_work += work;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            target.words(),
            full.masks()
                .iter()
                .find(|mask| mask.counts == actual)
                .unwrap()
                .words()
        );
        assert!(target_work < full_work);
        assert_eq!(
            target
                .words()
                .iter()
                .map(|word| word.count_ones())
                .sum::<u32>(),
            1
        );
    }

    #[test]
    fn word_bitmap_empty_full_certificates_exclude_padding_and_charge_each_word() {
        let mut bitmap = RankedBits::new(0x111111).unwrap();
        let mut work = 0;
        assert_eq!(
            bitmap
                .coverage(|steps| {
                    work += steps;
                    Ok(())
                })
                .unwrap(),
            BitmapCoverage::Empty
        );
        assert_eq!(work, 12);
        bitmap.insert_word(&[1, 2, 3, 4, 5, 6]).unwrap();
        assert_eq!(
            bitmap.coverage(|_| Ok(())).unwrap(),
            BitmapCoverage::Partial
        );
        for word in words(bitmap.counts) {
            bitmap.insert_word(&word).unwrap();
        }
        assert_eq!(bitmap.coverage(|_| Ok(())).unwrap(), BitmapCoverage::Full);
        assert_eq!(bitmap.words()[11], (1 << 16) - 1);
        bitmap.bits[11] |= 1 << 63;
        assert!(bitmap.coverage(|_| Ok(())).is_err());
    }
}
