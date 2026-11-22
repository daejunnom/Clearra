#[cfg(any(test, feature = "local-search-ab"))]
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

mod factorized_source;
use factorized_source::FactorizedSourceLayout;
#[cfg(test)]
mod factorized_tests;

use crate::resource::ExecutionMemoryBound;

use super::{
    compact_exact_u64_map::{ExactU64MemoMap, ExactU64MemoStorage},
    mix_digest,
    piece_order_language::PieceOrderLanguageCache,
    standard_bag_product_memo::{
        StandardBagProductMemo, StandardBagProductMemoKey, StandardBagProductMemoLayout,
    },
    WasmExactSearchError,
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_supply::{
    bag::BagState,
    execution_automaton::{SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState},
    hold::hold_policy::HoldPolicy,
    hold_automaton::HoldAutomatonState,
    pattern_universe::{MaterializedPatternUniverse, MaterializedPatternUniverseStructure},
    piece_source::PieceSourceKind,
};

const REJECT: u32 = 0;
const ACCEPT: u32 = 1;
const EMPTY_REFERENCE: u32 = u32::MAX;
const NO_PATTERN_OFFSET: u64 = u64::MAX;
const FULL_STANDARD_BAG: u8 = 0x7f;
const INITIAL_BUCKET_COUNT: usize = 1024;
const CANCELLATION_POLL_MASK: u32 = 0xff;
const STANDARD_PIECE_COUNT: usize = PieceKind::STANDARD_TETROMINOES.len();
const STANDARD_BAG_MASK_COUNT: usize = 1 << STANDARD_PIECE_COUNT;
const CURSOR_TRANSITION_UNAVAILABLE: u8 = u8::MAX;

#[cfg(any(test, feature = "local-search-ab"))]
const UNION_MEMO_LIMIT_INVALID: &str = "wasm_standard_bag_union_memo_entry_limit_invalid";

#[cfg(any(test, feature = "local-search-ab"))]
fn parse_union_memo_entry_limit(value: Option<&str>) -> Result<Option<usize>, &'static str> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(UNION_MEMO_LIMIT_INVALID);
    }
    value
        .parse()
        .map(Some)
        .map_err(|_| UNION_MEMO_LIMIT_INVALID)
}

#[cfg(any(test, feature = "local-search-ab"))]
fn parse_accept_absorption(
    value: Result<String, std::env::VarError>,
) -> Result<bool, &'static str> {
    const INVALID: &str = "wasm_standard_bag_absorption_mode_invalid";
    match value {
        Ok(label) if label == "0" => Ok(false),
        Ok(label) if label == "1" => Ok(true),
        Err(std::env::VarError::NotPresent) => Ok(false),
        _ => Err(INVALID),
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
const P7P4_PREFIXES: usize = 5_040;
#[cfg(any(test, feature = "local-search-ab"))]
const P7P4_TAIL_BITS: usize = 840;
#[cfg(any(test, feature = "local-search-ab"))]
const P7P4_TAIL_BYTES: usize = P7P4_TAIL_BITS / 8;
#[cfg(any(test, feature = "local-search-ab"))]
const P7P4_TAIL_MEMO_MAX: usize = 4_096;

/// An owned projection of a live, full P7P4 decision root. Data only: no
/// worker- or epoch-local arena reference can escape through this type.
#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct P7P4CoverageBlocks {
    tails: Box<[[u8; P7P4_TAIL_BYTES]]>,
    runs: Box<[(u16, u16)]>,
    covered_count: u32,
}

#[cfg(any(test, feature = "local-search-ab"))]
impl P7P4CoverageBlocks {
    /// Each mask is over the 840 lexicographic four-draw tails, LSB first.
    pub fn tails(&self) -> &[[u8; P7P4_TAIL_BYTES]] {
        &self.tails
    }

    /// (Tail dictionary index, consecutive first-bag prefix count).
    pub fn runs(&self) -> &[(u16, u16)] {
        &self.runs
    }

    pub const fn covered_count(&self) -> u32 {
        self.covered_count
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Default)]
struct P7P4CaptureCache {
    tail_masks: HashMap<u32, [u8; P7P4_TAIL_BYTES]>,
    last_root: Option<(u32, Arc<P7P4CoverageBlocks>)>,
}

#[cfg(any(test, feature = "local-search-ab"))]
impl P7P4CaptureCache {
    fn retained_bytes(&self) -> usize {
        let last = self.last_root.as_ref().map_or(0, |(_, blocks)| {
            core::mem::size_of::<P7P4CoverageBlocks>()
                + core::mem::size_of_val(blocks.tails())
                + core::mem::size_of_val(blocks.runs())
        });
        last + self.tail_masks.capacity()
            * (core::mem::size_of::<(u32, [u8; P7P4_TAIL_BYTES])>() + 1)
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Default)]
struct P7P4CaptureBuilder {
    tails: Vec<[u8; P7P4_TAIL_BYTES]>,
    tail_indices: HashMap<[u8; P7P4_TAIL_BYTES], u16>,
    tail_counts: Vec<u16>,
    // Local references are never serialized. This map prevents re-hashing a
    // mask when several first-bag permutations point at the same tail root.
    tail_refs: HashMap<u32, u16>,
    runs: Vec<(u16, u16)>,
    prefix_count: usize,
    covered_count: u32,
}

#[cfg(any(test, feature = "local-search-ab"))]
impl P7P4CaptureBuilder {
    fn append(
        &mut self,
        root: u32,
        repeat: usize,
        render_tail: impl FnOnce() -> Result<[u8; P7P4_TAIL_BYTES], WasmExactSearchError>,
    ) -> Result<(), WasmExactSearchError> {
        if repeat == 0 || self.prefix_count + repeat > P7P4_PREFIXES {
            return Err(p7p4_bad_frame());
        }
        let index = if let Some(&index) = self.tail_refs.get(&root) {
            index
        } else {
            let mask = render_tail()?;
            let index = if let Some(&index) = self.tail_indices.get(&mask) {
                index
            } else {
                let index = u16::try_from(self.tails.len()).map_err(|_| p7p4_bad_frame())?;
                self.tail_indices
                    .try_reserve(1)
                    .map_err(|_| p7p4_storage_error())?;
                self.tails
                    .try_reserve(1)
                    .map_err(|_| p7p4_storage_error())?;
                self.tail_counts
                    .try_reserve(1)
                    .map_err(|_| p7p4_storage_error())?;
                self.tail_indices.insert(mask, index);
                self.tails.push(mask);
                self.tail_counts
                    .push(mask.iter().map(|byte| byte.count_ones() as u16).sum());
                index
            };
            self.tail_refs
                .try_reserve(1)
                .map_err(|_| p7p4_storage_error())?;
            self.tail_refs.insert(root, index);
            index
        };
        let repeat_u16 = u16::try_from(repeat).map_err(|_| p7p4_bad_frame())?;
        match self.runs.last_mut() {
            Some((last_index, length)) if *last_index == index => *length += repeat_u16,
            _ => {
                self.runs.try_reserve(1).map_err(|_| p7p4_storage_error())?;
                self.runs.push((index, repeat_u16));
            }
        }
        self.prefix_count += repeat;
        self.covered_count +=
            u32::from(self.tail_counts[usize::from(index)]) * u32::from(repeat_u16);
        Ok(())
    }

    fn finish(
        self,
        expected_covered_count: u64,
    ) -> Result<P7P4CoverageBlocks, WasmExactSearchError> {
        if self.prefix_count != P7P4_PREFIXES
            || u64::from(self.covered_count) != expected_covered_count
        {
            return Err(p7p4_bad_frame());
        }
        Ok(P7P4CoverageBlocks {
            tails: self.tails.into_boxed_slice(),
            runs: self.runs.into_boxed_slice(),
            covered_count: self.covered_count,
        })
    }
}

#[cfg(any(test, feature = "local-search-ab"))]
fn p7p4_bad_frame() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_p7p4_capture_root_shape_mismatch")
}

#[cfg(any(test, feature = "local-search-ab"))]
fn p7p4_storage_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_p7p4_capture_storage_unavailable")
}

#[derive(Clone, Copy, Debug)]
#[repr(C, align(32))]
struct DecisionNode {
    children: [u32; 7],
    next_same_bucket: u32,
}

const _: () = assert!(core::mem::size_of::<DecisionNode>() == 32);

#[derive(Clone, Copy, Debug)]
struct DecisionSummary {
    covered_pattern_count: u64,
    first_pattern_offset: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourceState {
    depth: u8,
    bag_remainder: u8,
}

impl SourceState {
    fn normalized(self) -> Self {
        Self {
            bag_remainder: if self.bag_remainder == 0 {
                FULL_STANDARD_BAG
            } else {
                self.bag_remainder
            },
            ..self
        }
    }

    fn remaining_output_capacity(
        self,
        sequence_len: u8,
        hold_code: u8,
        projects_unplaced_lookahead: bool,
    ) -> usize {
        let remaining_draws = usize::from(sequence_len.saturating_sub(self.depth));
        remaining_draws + usize::from(projects_unplaced_lookahead && hold_code != 0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompactSupplyStep {
    next_source: SourceState,
    next_hold_code: u8,
}

/// Query-local, canonical-automaton-certified cursor transitions for the only
/// symbolic fast path supported by this module. The table owns one byte for
/// every `(depth, exact bag remainder, current piece)` tuple. Hold behavior is
/// projected separately because it does not change how the bag cursor draws.
///
/// Building this once prevents the coverage hot loop from reconstructing a
/// `SupplyExecutionState` and enumerating every matching hold branch merely to
/// select one already-known branch for every piece-language edge.
struct StandardBagCursorTransitions {
    max_source_depth: u8,
    next_remainders: Box<[u8]>,
    factorized_valid_remainders: Option<Box<[u128]>>,
}

impl StandardBagCursorTransitions {
    fn checked_storage_len(max_source_depth: u8) -> Option<usize> {
        usize::from(max_source_depth)
            .checked_add(1)?
            .checked_mul(STANDARD_BAG_MASK_COUNT)?
            .checked_mul(STANDARD_PIECE_COUNT)
    }

    fn compile(
        sequence_len: u8,
        automaton: &SupplyExecutionAutomaton,
        supply_identity: SupplyExecutionState,
        hold_enabled: bool,
    ) -> Result<Self, WasmExactSearchError> {
        // One extra source row is required to certify the second draw of an
        // empty-hold branch whose first draw occurs at the query boundary.
        let max_source_depth =
            sequence_len
                .checked_add(1)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_standard_bag_cursor_table_depth_overflow",
                ))?;
        let storage_len = Self::checked_storage_len(max_source_depth).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_cursor_table_size_overflow"),
        )?;
        let mut next_remainders = Vec::new();
        next_remainders
            .try_reserve_exact(storage_len)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_standard_bag_cursor_table_storage_unavailable",
                )
            })?;
        next_remainders.resize(storage_len, CURSOR_TRANSITION_UNAVAILABLE);

        for depth in 0..=max_source_depth {
            for bag_remainder in 1..=FULL_STANDARD_BAG {
                let source = SourceState {
                    depth,
                    bag_remainder,
                };
                if !standard_bag_source_is_reachable(source) {
                    continue;
                }
                let state = canonical_supply_state(source, 0, hold_enabled, supply_identity)?;
                for (piece_index, piece) in
                    PieceKind::STANDARD_TETROMINOES.iter().copied().enumerate()
                {
                    let Some(next) = automaton.advance_bag_cursor(state, piece).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_standard_bag_cursor_transition_invalid",
                        )
                    })?
                    else {
                        continue;
                    };
                    let next_source = canonical_source_state(next)?;
                    if next_source.depth != depth.saturating_add(1)
                        || !standard_bag_source_is_reachable(next_source)
                    {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "wasm_standard_bag_cursor_transition_unreachable",
                        ));
                    }
                    let index = cursor_transition_index(depth, bag_remainder, piece_index).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_standard_bag_cursor_table_index_overflow",
                        ),
                    )?;
                    next_remainders[index] = next_source.bag_remainder;
                }
            }
        }

        Ok(Self {
            max_source_depth,
            next_remainders: next_remainders.into_boxed_slice(),
            factorized_valid_remainders: None,
        })
    }

    fn source_is_reachable(&self, source: SourceState) -> bool {
        match &self.factorized_valid_remainders {
            Some(rows) => rows.get(usize::from(source.depth)).is_some_and(|row| {
                source.bag_remainder <= FULL_STANDARD_BAG
                    && row & (1_u128 << source.normalized().bag_remainder) != 0
            }),
            None => standard_bag_source_is_reachable(source),
        }
    }

    fn validate_source(&self, source: SourceState) -> Result<SourceState, WasmExactSearchError> {
        let source = source.normalized();
        if !self.source_is_reachable(source) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_cursor_source_unreachable",
            ));
        }
        if source.depth > self.max_source_depth {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_cursor_transition_out_of_scope",
            ));
        }
        Ok(source)
    }

    fn advance(
        &self,
        source: SourceState,
        piece_index: usize,
    ) -> Result<Option<SourceState>, WasmExactSearchError> {
        let source = self.validate_source(source)?;
        if piece_index >= STANDARD_PIECE_COUNT {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_cursor_transition_out_of_scope",
            ));
        }
        let index = cursor_transition_index(source.depth, source.bag_remainder, piece_index)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_cursor_table_index_overflow",
            ))?;
        let next_remainder =
            *self
                .next_remainders
                .get(index)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_standard_bag_cursor_table_index_out_of_range",
                ))?;
        if next_remainder == CURSOR_TRANSITION_UNAVAILABLE {
            return Ok(None);
        }
        let next_source = SourceState {
            depth: source
                .depth
                .checked_add(1)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_standard_bag_cursor_depth_overflow",
                ))?,
            bag_remainder: next_remainder,
        };
        if !self.source_is_reachable(next_source) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_cursor_result_unreachable",
            ));
        }
        Ok(Some(next_source))
    }

    fn retained_bytes(&self) -> usize {
        self.next_remainders.len() * core::mem::size_of::<u8>()
            + self
                .factorized_valid_remainders
                .as_deref()
                .map_or(0, core::mem::size_of_val)
    }
}

fn cursor_transition_index(depth: u8, bag_remainder: u8, piece_index: usize) -> Option<usize> {
    usize::from(depth)
        .checked_mul(STANDARD_BAG_MASK_COUNT)?
        .checked_add(usize::from(bag_remainder))?
        .checked_mul(STANDARD_PIECE_COUNT)?
        .checked_add(piece_index)
}

fn standard_bag_source_is_reachable(source: SourceState) -> bool {
    let source = source.normalized();
    let drawn_in_epoch = usize::from(source.depth) % STANDARD_PIECE_COUNT;
    let expected = if drawn_in_epoch == 0 {
        STANDARD_PIECE_COUNT
    } else {
        STANDARD_PIECE_COUNT - drawn_in_epoch
    };
    source.bag_remainder.count_ones() as usize == expected
}

fn canonical_supply_state(
    source: SourceState,
    hold_code: u8,
    hold_enabled: bool,
    supply_identity: SupplyExecutionState,
) -> Result<SupplyExecutionState, WasmExactSearchError> {
    let hold_piece = match hold_code {
        0 => None,
        1..=7 if hold_enabled => Some(PieceKind::STANDARD_TETROMINOES[usize::from(hold_code - 1)]),
        _ => {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_hold_state_invalid",
            ));
        }
    };
    let source = source.normalized();
    if !standard_bag_source_is_reachable(source) {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_cursor_source_unreachable",
        ));
    }
    Ok(SupplyExecutionState {
        cursor: u16::from(source.depth),
        hold_piece,
        hold_empty: hold_piece.is_none(),
        hold_policy: if hold_enabled {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        },
        bag_epoch: if source.depth == 0 {
            0
        } else {
            u16::from((source.depth - 1) / STANDARD_PIECE_COUNT as u8)
        },
        bag_remainder_key: if source.depth != 0
            && source.depth.is_multiple_of(STANDARD_PIECE_COUNT as u8)
            && source.bag_remainder == FULL_STANDARD_BAG
        {
            0
        } else {
            standard_bag_mask_key(source.bag_remainder)
        },
        ..supply_identity
    })
}

fn canonical_source_state(
    state: SupplyExecutionState,
) -> Result<SourceState, WasmExactSearchError> {
    if state.source_kind != PieceSourceKind::BagUniverse {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_source_kind_invalid",
        ));
    }
    let source = SourceState {
        depth: u8::try_from(state.cursor).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_cursor_overflow")
        })?,
        bag_remainder: standard_bag_key_mask(state.bag_remainder_key)?,
    }
    .normalized();
    if !standard_bag_source_is_reachable(source) {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_cursor_result_unreachable",
        ));
    }
    let expected_epoch = if source.depth == 0 {
        0
    } else {
        u16::from((source.depth - 1) / STANDARD_PIECE_COUNT as u8)
    };
    if state.bag_epoch != expected_epoch {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_epoch_invalid",
        ));
    }
    Ok(source)
}

pub(super) struct StandardBagCoverageResult {
    pub root: u32,
    pub covers_any_pattern: bool,
    pub witness_pattern_id: Option<u32>,
    pub covered_pattern_count: usize,
    pub product_states: usize,
    pub edge_checks: usize,
}

/// Immutable supply tables belong to one request, not to its mutable language
/// or decision-node arenas. The first symbolic candidate initializes them;
/// unused or unsupported requests never allocate the table payloads.
#[derive(Default)]
pub(super) struct SharedStandardBagRequest {
    snapshot: OnceLock<StandardBagRequestSnapshot>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct StandardBagRequestIdentity {
    structure: MaterializedPatternUniverseStructure,
    pattern_count: usize,
    initial_hold: HoldAutomatonState,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    factorized: bool,
}

struct StandardBagRequestSnapshot {
    identity: StandardBagRequestIdentity,
    tables: Result<Option<Arc<StandardBagRequestTables>>, WasmExactSearchError>,
}

struct StandardBagRequestTables {
    sequence_len: u8,
    initial_hold_code: u8,
    cursor_transitions: StandardBagCursorTransitions,
    suffix_counts: Box<[u128]>,
    factorized_layout: Option<FactorizedSourceLayout>,
    #[cfg(test)]
    supply_identity: SupplyExecutionState,
}

impl StandardBagRequestTables {
    fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            .saturating_add(self.cursor_transitions.retained_bytes())
            .saturating_add(self.suffix_counts.len() * core::mem::size_of::<u128>())
            .saturating_add(
                self.factorized_layout
                    .as_ref()
                    .map_or(0, FactorizedSourceLayout::retained_bytes),
            )
    }

    fn compile(
        universe: &MaterializedPatternUniverse,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
    ) -> Result<Option<Self>, WasmExactSearchError> {
        if !StandardBagCoverage::supports(universe, initial_hold) {
            return Ok(None);
        }
        let factorized_layout = FactorizedSourceLayout::compile(universe)?;
        let sequence_len = if let Some(layout) = &factorized_layout {
            layout.sequence_len()
        } else if let MaterializedPatternUniverseStructure::Standard7BagLexicographic {
            sequence_len,
        } = universe.structure()
        {
            u8::try_from(sequence_len).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_standard_bag_sequence_too_long")
            })?
        } else {
            return Ok(None);
        };
        let initial_hold_code = match (initial_hold.hold_empty(), initial_hold.hold_piece()) {
            (true, None) => 0,
            (false, Some(piece)) => piece_code(piece),
            _ => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_initial_hold_state_invalid",
                ));
            }
        };
        let mut supply_identity = initial_hold;
        supply_identity.hold_policy = if hold_enabled {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        };
        let (cursor_transitions, suffix_counts) = if let Some(layout) = &factorized_layout {
            let cursor = layout.compile_cursor()?;
            let suffix = layout.compile_suffix_counts(&cursor)?;
            (cursor, suffix)
        } else {
            let suffix = compile_suffix_counts(sequence_len)?.into_boxed_slice();
            let supply_automaton = SupplyExecutionAutomaton::for_bag(
                &PieceKind::STANDARD_TETROMINOES,
            )
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_standard_bag_supply_automaton_invalid")
            })?;
            supply_identity.source_kind = PieceSourceKind::BagUniverse;
            let cursor = StandardBagCursorTransitions::compile(
                sequence_len,
                &supply_automaton,
                supply_identity,
                hold_enabled,
            )?;
            (cursor, suffix)
        };
        Ok(Some(Self {
            sequence_len,
            initial_hold_code,
            cursor_transitions,
            suffix_counts,
            factorized_layout,
            #[cfg(test)]
            supply_identity,
        }))
    }
}

impl SharedStandardBagRequest {
    fn tables(
        &self,
        universe: &MaterializedPatternUniverse,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
    ) -> Result<Option<Arc<StandardBagRequestTables>>, WasmExactSearchError> {
        let identity = StandardBagRequestIdentity {
            structure: universe.structure(),
            pattern_count: universe.pattern_count(),
            initial_hold,
            hold_enabled,
            projects_unplaced_lookahead,
            factorized: FactorizedSourceLayout::supported_layout(universe).is_some(),
        };
        let snapshot = self.snapshot.get_or_init(|| StandardBagRequestSnapshot {
            identity,
            tables: StandardBagRequestTables::compile(universe, initial_hold, hold_enabled)
                .map(|tables| tables.map(Arc::new)),
        });
        // A request owner must never cross the supply/hold/projected-lookahead
        // boundary, including after an unsupported result was cached.
        let layout_matches = snapshot
            .tables
            .as_ref()
            .ok()
            .and_then(Option::as_ref)
            .is_none_or(|tables| {
                tables
                    .factorized_layout
                    .as_ref()
                    .is_none_or(|layout| layout.matches_universe(universe))
            });
        if snapshot.identity != identity || !layout_matches {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_shared_request_identity_mismatch",
            ));
        }
        snapshot.tables.clone()
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn retained_bytes(&self) -> usize {
        self.snapshot
            .get()
            .and_then(|snapshot| snapshot.tables.as_ref().ok())
            .and_then(Option::as_ref)
            .map_or(0, |tables| tables.retained_bytes())
    }
}

/// Exit-time logical payload accounting, not allocator overhead or an OS peak.
/// Entry payload and row-directory payload are disjoint subsets of StandardBag.
#[derive(Clone, Copy, Debug)]
#[cfg(any(feature = "parallel", test))]
pub(super) struct StandardBagMemoAccounting {
    pub product_policy: &'static str,
    pub product_layout: &'static str,
    pub product_storage: &'static str,
    pub union_storage: &'static str,
    pub product_entries: usize,
    pub union_entries: usize,
    pub product_capacity: usize,
    pub union_capacity: usize,
    pub product_payload_bytes: usize,
    pub union_payload_bytes: usize,
    pub product_directory_bytes: usize,
    pub product_active_rows: usize,
    pub product_allocated_rows: usize,
    pub product_row_slots: usize,
    pub product_promotion_attempts: usize,
    pub product_promotions: usize,
}

#[cfg(any(feature = "parallel", test))]
impl Default for StandardBagMemoAccounting {
    fn default() -> Self {
        Self {
            product_policy: "not-used",
            product_layout: "not-used",
            product_storage: "not-used",
            union_storage: "not-used",
            product_entries: 0,
            union_entries: 0,
            product_capacity: 0,
            union_capacity: 0,
            product_payload_bytes: 0,
            union_payload_bytes: 0,
            product_directory_bytes: 0,
            product_active_rows: 0,
            product_allocated_rows: 0,
            product_row_slots: 0,
            product_promotion_attempts: 0,
            product_promotions: 0,
        }
    }
}

#[cfg(feature = "parallel")]
impl StandardBagMemoAccounting {
    /// Private owners are summed; these values are already inside the worker's
    /// StandardBag retained total and must not be added a second time.
    pub fn merge(&mut self, other: Self) {
        fn labels(left: &'static str, right: &'static str) -> &'static str {
            match (left, right) {
                ("not-used", label) | (label, "not-used") => label,
                (left, right) if left == right => left,
                _ => "mixed",
            }
        }
        self.product_policy = labels(self.product_policy, other.product_policy);
        self.product_layout = labels(self.product_layout, other.product_layout);
        self.product_storage = labels(self.product_storage, other.product_storage);
        self.union_storage = labels(self.union_storage, other.union_storage);
        self.product_entries = self.product_entries.saturating_add(other.product_entries);
        self.union_entries = self.union_entries.saturating_add(other.union_entries);
        self.product_capacity = self.product_capacity.saturating_add(other.product_capacity);
        self.union_capacity = self.union_capacity.saturating_add(other.union_capacity);
        self.product_payload_bytes = self
            .product_payload_bytes
            .saturating_add(other.product_payload_bytes);
        self.union_payload_bytes = self
            .union_payload_bytes
            .saturating_add(other.union_payload_bytes);
        self.product_directory_bytes = self
            .product_directory_bytes
            .saturating_add(other.product_directory_bytes);
        self.product_active_rows = self
            .product_active_rows
            .saturating_add(other.product_active_rows);
        self.product_allocated_rows = self
            .product_allocated_rows
            .saturating_add(other.product_allocated_rows);
        self.product_row_slots = self
            .product_row_slots
            .saturating_add(other.product_row_slots);
        self.product_promotion_attempts = self
            .product_promotion_attempts
            .saturating_add(other.product_promotion_attempts);
        self.product_promotions = self
            .product_promotions
            .saturating_add(other.product_promotions);
    }
}

/// Exact symbolic product for a lexicographically materialized standard 7-bag.
///
/// Decision nodes denote sets of concrete queue strings. Product memoization
/// shares only the exact tuple `(piece-language node, source depth, exact bag
/// remainder, hold piece)`. The hash tables are accelerators; every key and
/// decision child tuple is compared exactly before reuse.
pub(super) struct StandardBagCoverage {
    sequence_len: u8,
    materialized_pattern_count: usize,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    initial_hold_code: u8,
    tables: Arc<StandardBagRequestTables>,
    #[cfg(test)]
    supply_identity: SupplyExecutionState,
    nodes: Vec<DecisionNode>,
    node_summaries: Vec<DecisionSummary>,
    node_levels: Vec<u8>,
    node_valid_masks: Vec<u8>,
    bucket_heads: Vec<u32>,
    product_memo: StandardBagProductMemo,
    union_memo: ExactU64MemoMap,
    // Experimental admission bound on live keys, not a byte/RSS cap. Hash
    // allocation still follows the map's growth policy and survives recycling.
    #[cfg(any(test, feature = "local-search-ab"))]
    union_memo_entry_limit: Option<usize>,
    #[cfg(any(test, feature = "local-search-ab"))]
    absorb_accepted_suffix: bool,
    epoch_global_root: u32,
    global_words: Vec<u64>,
    global_covered_pattern_count: usize,
    interning_disabled: bool,
    cancellation_poll_counter: u32,
    product_state_count: usize,
    edge_check_count: usize,
    root_cache_hits: usize,
    root_cache_misses: usize,
    // The enclosing session refreshes outside bytes before entering this
    // owner. Decision recursion mutates only this owner, so that snapshot is
    // stable until control returns to the session.
    resource_boundary: Option<StandardBagResourceBoundary>,
    #[cfg(any(test, feature = "local-search-ab"))]
    p7p4_capture: P7P4CaptureCache,
}

#[derive(Clone, Copy)]
struct StandardBagResourceBoundary {
    bound: ExecutionMemoryBound,
    outside_retained_bytes: u128,
}

impl StandardBagCoverage {
    /// Pre-admit this before constructing an unshared request. It includes
    /// both table buffers and possible Vec-to-box relocation while the old
    /// buffers coexist, plus the immutable Arc allocation. A caller using an
    /// already initialized shared request may conservatively use this bound.
    pub fn checked_creation_future_bytes(universe: &MaterializedPatternUniverse) -> Option<u128> {
        let sequence_len = match universe.structure() {
            MaterializedPatternUniverseStructure::Standard7BagLexicographic { sequence_len } => {
                u8::try_from(sequence_len).ok()?
            }
            _ => {
                let Some(layout) = FactorizedSourceLayout::supported_layout(universe) else {
                    return Some(0);
                };
                u8::try_from(layout.visible_sequence_len()).ok()?
            }
        };
        let cursor_bytes = Self::checked_cursor_transition_retained_bytes(universe)?;
        let suffix_bytes = (usize::from(sequence_len) as u128)
            .checked_add(1)?
            .checked_mul(STANDARD_BAG_MASK_COUNT as u128)?
            .checked_mul(core::mem::size_of::<u128>() as u128)?;
        cursor_bytes
            .checked_add(suffix_bytes)?
            .checked_mul(2)?
            .checked_add(core::mem::size_of::<StandardBagRequestTables>() as u128)?
            .checked_add(2 * core::mem::size_of::<usize>() as u128)
    }

    pub fn set_resource_boundary(
        &mut self,
        bound: ExecutionMemoryBound,
        coexisting_retained_bytes: u128,
    ) {
        self.resource_boundary = Some(StandardBagResourceBoundary {
            bound,
            outside_retained_bytes: coexisting_retained_bytes,
        });
    }

    #[cfg_attr(
        all(feature = "local-search-ab", not(target_family = "wasm")),
        track_caller
    )]
    fn ensure_growth(&self, future: Option<u128>) -> Result<(), WasmExactSearchError> {
        let Some(boundary) = self.resource_boundary else {
            return Ok(());
        };
        let Some(required) =
            future.and_then(|bytes| (self.retained_bytes() as u128).checked_add(bytes))
        else {
            return boundary
                .bound
                .ensure(u128::MAX, 1)
                .map_err(WasmExactSearchError::resource_admission);
        };
        boundary
            .bound
            .ensure(boundary.outside_retained_bytes, required)
            .map_err(WasmExactSearchError::resource_admission)
    }

    fn optional_growth_admitted(&self, future: Option<u128>) -> bool {
        future.is_some() && self.ensure_growth(future).is_ok()
    }

    pub fn supports(
        universe: &MaterializedPatternUniverse,
        initial_hold: HoldAutomatonState,
    ) -> bool {
        let source_supported = match universe.structure() {
            MaterializedPatternUniverseStructure::Standard7BagLexicographic { .. } => {
                initial_hold.bag_remainder_key()
                    == BagState::fresh_standard_7_bag().packed_remainder_key()
            }
            _ => {
                FactorizedSourceLayout::supported_layout(universe).is_some()
                    && initial_hold.source_kind() == PieceSourceKind::MaterializedPatternUniverse
                    && initial_hold.bag_remainder_key() == 0
                    && initial_hold.hold_policy() != HoldPolicy::Required
                    && (initial_hold.hold_policy() != HoldPolicy::Forbidden
                        || initial_hold.hold_piece().is_none())
            }
        };
        source_supported
            && initial_hold.cursor() == 0
            && initial_hold.bag_epoch() == 0
            && matches!(
                (initial_hold.hold_empty(), initial_hold.hold_piece()),
                (true, None) | (false, Some(_))
            )
    }

    pub fn for_universe(
        universe: &MaterializedPatternUniverse,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
    ) -> Result<Option<Self>, WasmExactSearchError> {
        let Some(tables) = StandardBagRequestTables::compile(universe, initial_hold, hold_enabled)?
        else {
            return Ok(None);
        };
        Self::from_request_tables(
            universe,
            hold_enabled,
            projects_unplaced_lookahead,
            Arc::new(tables),
        )
        .map(Some)
    }

    pub fn for_universe_with_shared(
        universe: &MaterializedPatternUniverse,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        shared: &SharedStandardBagRequest,
    ) -> Result<Option<Self>, WasmExactSearchError> {
        let Some(tables) = shared.tables(
            universe,
            initial_hold,
            hold_enabled,
            projects_unplaced_lookahead,
        )?
        else {
            return Ok(None);
        };
        Self::from_request_tables(universe, hold_enabled, projects_unplaced_lookahead, tables)
            .map(Some)
    }

    fn from_request_tables(
        universe: &MaterializedPatternUniverse,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        tables: Arc<StandardBagRequestTables>,
    ) -> Result<Self, WasmExactSearchError> {
        // Product builds adapt product-state layout and keep lossless compact
        // union slots. Only local A/B builds may override either selection.
        let memo_storage = ExactU64MemoStorage::from_environment()
            .map_err(WasmExactSearchError::InvalidProblem)?;
        let union_storage = ExactU64MemoStorage::for_union(memo_storage)
            .map_err(WasmExactSearchError::InvalidProblem)?;
        let product_layout = StandardBagProductMemoLayout::from_environment()
            .map_err(WasmExactSearchError::InvalidProblem)?;
        #[cfg(feature = "local-search-ab")]
        let absorb_accepted_suffix =
            parse_accept_absorption(std::env::var("CLEARRA_STANDARD_BAG_ABSORB"))
                .map_err(WasmExactSearchError::InvalidProblem)?;
        #[cfg(feature = "local-search-ab")]
        let union_memo_entry_limit =
            match std::env::var("CLEARRA_STANDARD_BAG_UNION_MEMO_MAX_ENTRIES") {
                Ok(value) => parse_union_memo_entry_limit(Some(&value)),
                Err(std::env::VarError::NotPresent) => parse_union_memo_entry_limit(None),
                Err(std::env::VarError::NotUnicode(_)) => Err(UNION_MEMO_LIMIT_INVALID),
            }
            .map_err(WasmExactSearchError::InvalidProblem)?;
        let max_source_depth = tables.cursor_transitions.max_source_depth;
        Ok(Self {
            sequence_len: tables.sequence_len,
            materialized_pattern_count: universe.pattern_count(),
            hold_enabled,
            projects_unplaced_lookahead,
            initial_hold_code: tables.initial_hold_code,
            #[cfg(test)]
            supply_identity: tables.supply_identity,
            tables,
            nodes: Vec::new(),
            node_summaries: Vec::new(),
            node_levels: Vec::new(),
            node_valid_masks: Vec::new(),
            bucket_heads: Vec::new(),
            product_memo: StandardBagProductMemo::new(
                product_layout,
                memo_storage,
                max_source_depth,
            ),
            union_memo: ExactU64MemoMap::new(union_storage),
            #[cfg(feature = "local-search-ab")]
            union_memo_entry_limit,
            #[cfg(all(test, not(feature = "local-search-ab")))]
            union_memo_entry_limit: None,
            #[cfg(feature = "local-search-ab")]
            absorb_accepted_suffix,
            #[cfg(all(test, not(feature = "local-search-ab")))]
            absorb_accepted_suffix: false,
            epoch_global_root: REJECT,
            global_words: Vec::new(),
            global_covered_pattern_count: 0,
            interning_disabled: false,
            cancellation_poll_counter: 0,
            product_state_count: 0,
            edge_check_count: 0,
            root_cache_hits: 0,
            root_cache_misses: 0,
            resource_boundary: None,
            #[cfg(any(test, feature = "local-search-ab"))]
            p7p4_capture: P7P4CaptureCache::default(),
        })
    }

    pub fn cover_language(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_root: u32,
        control: &ExecutionControl,
    ) -> Result<StandardBagCoverageResult, WasmExactSearchError> {
        let state = self.root_source();
        let key = product_key(language_root, state, self.initial_hold_code);
        let cache_hit = self.product_memo.contains_key(&key);
        if cache_hit {
            self.root_cache_hits = self.root_cache_hits.saturating_add(1);
        } else {
            self.root_cache_misses = self.root_cache_misses.saturating_add(1);
        }
        let states_before = self.product_state_count;
        let edges_before = self.edge_check_count;
        let root = self.solve(
            language,
            language_root,
            state,
            self.initial_hold_code,
            control,
        )?;
        let root_source = state;
        let summary = self.summary(root, root_source)?;
        let witness_pattern_id = (summary.first_pattern_offset
            < self.materialized_pattern_count as u64)
            .then_some(summary.first_pattern_offset)
            .and_then(|offset| u32::try_from(offset).ok());
        let full_pattern_count = self.suffix_count(root_source);
        let covered_pattern_count = if full_pattern_count == self.materialized_pattern_count as u128
        {
            usize::try_from(summary.covered_pattern_count).unwrap_or(usize::MAX)
        } else {
            usize::try_from(self.count_patterns(
                root,
                root_source,
                0,
                self.materialized_pattern_count as u128,
            )?)
            .unwrap_or(usize::MAX)
        };
        Ok(StandardBagCoverageResult {
            root,
            // The symbolic root spans the full bag language. An explicitly
            // materialized prefix may contain none of its accepted queues.
            covers_any_pattern: covered_pattern_count != 0,
            witness_pattern_id,
            covered_pattern_count,
            product_states: self.product_state_count.saturating_sub(states_before),
            edge_checks: self.edge_check_count.saturating_sub(edges_before),
        })
    }

    pub fn merge_global(&mut self, root: u32) -> Result<(), WasmExactSearchError> {
        self.epoch_global_root = self.union(self.epoch_global_root, root)?;
        Ok(())
    }

    pub const fn global_is_complete(&self) -> bool {
        self.global_covered_pattern_count == self.materialized_pattern_count
            || self.epoch_global_root == ACCEPT
    }

    pub fn materialize_global(&mut self) -> Result<PatternBitSet, WasmExactSearchError> {
        self.flush_epoch_global()?;
        let expected_word_count = self.materialized_pattern_count.div_ceil(u64::BITS as usize);
        if self.global_words.len() > expected_word_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_coverage_shape_mismatch",
            ));
        }
        self.ensure_growth(
            PatternBitSet::checked_external_words_materialize_union_future_bytes(
                self.materialized_pattern_count,
            ),
        )?;
        let mut words = Vec::new();
        words.try_reserve_exact(expected_word_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_coverage_storage_unavailable")
        })?;
        words.extend_from_slice(&self.global_words);
        words.resize(expected_word_count, 0);
        PatternBitSet::from_words(self.materialized_pattern_count, words).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_coverage_shape_mismatch")
        })
    }

    /// Native workers publish this finalized coverage before dropping their
    /// workspace. Borrowing retains only the existing epoch-merge allocation.
    /// None denotes an exact empty set, not an unexecuted or failed flush.
    #[cfg(feature = "parallel")]
    pub(super) fn flushed_global_words(
        &mut self,
    ) -> Result<(usize, Option<&[u64]>), WasmExactSearchError> {
        self.flush_epoch_global()?;
        let pattern_count = self.materialized_pattern_count;
        let words = self.global_words.as_slice();
        if words.is_empty() {
            if self.global_covered_pattern_count == 0 {
                return Ok((pattern_count, None));
            }
        } else if words.len() == pattern_count.div_ceil(u64::BITS as usize) {
            return Ok((pattern_count, Some(words)));
        }
        Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_coverage_shape_mismatch",
        ))
    }

    pub fn materialize_root(&self, root: u32) -> Result<PatternBitSet, WasmExactSearchError> {
        let word_count = self.materialized_pattern_count.div_ceil(u64::BITS as usize);
        self.ensure_growth(
            PatternBitSet::checked_external_words_materialize_union_future_bytes(
                self.materialized_pattern_count,
            ),
        )?;
        let mut words = Vec::new();
        words.try_reserve_exact(word_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_coverage_storage_unavailable")
        })?;
        words.resize(word_count, 0);
        self.accumulate_into(
            root,
            self.root_source(),
            0,
            self.materialized_pattern_count as u128,
            &mut words,
        )?;
        PatternBitSet::from_words(self.materialized_pattern_count, words).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_coverage_shape_mismatch")
        })
    }

    /// Call while the candidate's root is live, before the worker accumulates
    /// it into its global union (that step may recycle the decision arena).
    /// None is unsupported, never a proof of an empty accepted queue language.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub(super) fn capture_p7p4_root(
        &mut self,
        root: u32,
    ) -> Result<Option<Arc<P7P4CoverageBlocks>>, WasmExactSearchError> {
        if self.tables.factorized_layout.is_some()
            || self.sequence_len != 11
            || self.materialized_pattern_count != P7P4_PREFIXES * P7P4_TAIL_BITS
            || !self.hold_enabled
            || self.initial_hold_code != 0
        {
            return Ok(None);
        }
        if let Some((cached_root, blocks)) = &self.p7p4_capture.last_root {
            if *cached_root == root {
                return Ok(Some(Arc::clone(blocks)));
            }
        }
        let source = SourceState {
            depth: 0,
            bag_remainder: FULL_STANDARD_BAG,
        };
        let expected = self.summary(root, source)?.covered_pattern_count;
        let mut capture = core::mem::take(&mut self.p7p4_capture);
        let mut builder = P7P4CaptureBuilder::default();
        let blocks = self
            .append_p7p4_prefix(root, source, &mut capture.tail_masks, &mut builder)
            .and_then(|()| builder.finish(expected))
            .map(Arc::new);
        if let Ok(blocks) = &blocks {
            capture.last_root = Some((root, Arc::clone(blocks)));
        }
        self.p7p4_capture = capture;
        blocks.map(Some)
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    fn append_p7p4_prefix(
        &self,
        root: u32,
        source: SourceState,
        tail_masks: &mut HashMap<u32, [u8; P7P4_TAIL_BYTES]>,
        builder: &mut P7P4CaptureBuilder,
    ) -> Result<(), WasmExactSearchError> {
        let source = source.normalized();
        if root <= ACCEPT {
            let remaining =
                usize::try_from(self.suffix_count(source)).map_err(|_| p7p4_bad_frame())?;
            if remaining % P7P4_TAIL_BITS != 0 {
                return Err(p7p4_bad_frame());
            }
            let fill = if root == ACCEPT { u8::MAX } else { 0 };
            return builder.append(root, remaining / P7P4_TAIL_BITS, || {
                Ok([fill; P7P4_TAIL_BYTES])
            });
        }
        if source.depth == 7 {
            if source.bag_remainder != FULL_STANDARD_BAG {
                return Err(p7p4_bad_frame());
            }
            return builder.append(root, 1, || {
                if let Some(&cached) = tail_masks.get(&root) {
                    return Ok(cached);
                }
                let mut mask = [0; P7P4_TAIL_BYTES];
                self.append_p7p4_tail(root, source, 0, &mut mask)?;
                // A refused memo insertion is not a failed exact projection.
                if tail_masks.len() < P7P4_TAIL_MEMO_MAX && tail_masks.try_reserve(1).is_ok() {
                    tail_masks.insert(root, mask);
                }
                Ok(mask)
            });
        }
        if source.depth > 7 {
            return Err(p7p4_bad_frame());
        }
        for (piece_index, child) in self
            .p7p4_checked_children(root, source)?
            .into_iter()
            .enumerate()
        {
            if source.bag_remainder & (1_u8 << piece_index) != 0 {
                let next = self.advance_source(source, piece_index)?;
                self.append_p7p4_prefix(child, next, tail_masks, builder)?;
            }
        }
        Ok(())
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    fn append_p7p4_tail(
        &self,
        root: u32,
        source: SourceState,
        base: usize,
        mask: &mut [u8; P7P4_TAIL_BYTES],
    ) -> Result<(), WasmExactSearchError> {
        if root == REJECT {
            return Ok(());
        }
        let source = source.normalized();
        let extent = usize::try_from(self.suffix_count(source)).map_err(|_| p7p4_bad_frame())?;
        let end = base
            .checked_add(extent)
            .filter(|end| *end <= P7P4_TAIL_BITS)
            .ok_or_else(p7p4_bad_frame)?;
        if root == ACCEPT {
            for ordinal in base..end {
                mask[ordinal / 8] |= 1_u8 << (ordinal % 8);
            }
            return Ok(());
        }
        if source.depth >= self.sequence_len {
            return Err(p7p4_bad_frame());
        }
        let mut child_base = base;
        for (index, child) in self
            .p7p4_checked_children(root, source)?
            .into_iter()
            .enumerate()
        {
            if source.bag_remainder & (1_u8 << index) != 0 {
                let next = self.advance_source(source, index)?;
                self.append_p7p4_tail(child, next, child_base, mask)?;
                child_base +=
                    usize::try_from(self.suffix_count(next)).map_err(|_| p7p4_bad_frame())?;
            }
        }
        if child_base != end {
            return Err(p7p4_bad_frame());
        }
        Ok(())
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    fn p7p4_checked_children(
        &self,
        root: u32,
        source: SourceState,
    ) -> Result<[u32; 7], WasmExactSearchError> {
        let index = reference_index(root);
        if self.node_levels.get(index).copied() != Some(source.depth)
            || self.node_valid_masks.get(index).copied() != Some(source.bag_remainder)
        {
            return Err(p7p4_bad_frame());
        }
        self.nodes
            .get(index)
            .map(|node| node.children)
            .ok_or_else(p7p4_bad_frame)
    }

    pub fn retained_bytes(&self) -> usize {
        let retained = self.shared_request_retained_bytes()
            + self.nodes.capacity() * core::mem::size_of::<DecisionNode>()
            + self.node_summaries.capacity() * core::mem::size_of::<DecisionSummary>()
            + self.node_levels.capacity() * core::mem::size_of::<u8>()
            + self.node_valid_masks.capacity() * core::mem::size_of::<u8>()
            + self.bucket_heads.capacity() * core::mem::size_of::<u32>()
            + self.memo_retained_payload_bytes()
            + self.product_memo.directory_retained_bytes()
            + self.global_words.capacity() * core::mem::size_of::<u64>();
        #[cfg(any(test, feature = "local-search-ab"))]
        let retained = retained + self.p7p4_capture.retained_bytes();
        retained
    }

    pub fn shared_request_retained_bytes(&self) -> usize {
        self.tables.retained_bytes()
    }

    pub const fn root_cache_hits(&self) -> usize {
        self.root_cache_hits
    }

    pub const fn root_cache_misses(&self) -> usize {
        self.root_cache_misses
    }

    #[cfg(any(feature = "parallel", test))]
    pub const fn memo_storage_label(&self) -> &'static str {
        self.product_memo.storage_label()
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn memo_accounting(&self) -> StandardBagMemoAccounting {
        let (product_promotion_attempts, product_promotions) = self.product_memo.promotion_counts();
        StandardBagMemoAccounting {
            product_policy: self.product_memo.policy().label(),
            product_layout: self.product_memo.layout().label(),
            product_storage: self.product_memo.storage_label(),
            union_storage: self.union_memo.storage().label(),
            product_entries: self.product_memo.len(),
            union_entries: self.union_memo.len(),
            product_capacity: self.product_memo.capacity(),
            union_capacity: self.union_memo.capacity(),
            product_payload_bytes: self.product_memo.retained_payload_bytes(),
            union_payload_bytes: self.union_memo.retained_payload_bytes(),
            product_directory_bytes: self.product_memo.directory_retained_bytes(),
            product_active_rows: self.product_memo.active_rows(),
            product_allocated_rows: self.product_memo.allocated_rows(),
            product_row_slots: self.product_memo.row_slots(),
            product_promotion_attempts,
            product_promotions,
        }
    }

    pub fn memo_retained_payload_bytes(&self) -> usize {
        self.product_memo
            .retained_payload_bytes()
            .saturating_add(self.union_memo.retained_payload_bytes())
    }

    pub fn local_live_bytes(&self) -> usize {
        // Cache-recycle admission uses the accepted logical key/value estimate
        // in both A/B arms. It deliberately excludes reference tuple padding,
        // so improved retained accounting cannot clear useful caches earlier.
        // Actual representation payload is reported separately above.
        // Preserve the accepted per-arena recycle threshold after sharing.
        // This is a logical live estimate, not unique process allocation.
        self.tables.cursor_transitions.retained_bytes()
            + self.nodes.len() * core::mem::size_of::<DecisionNode>()
            + self.node_summaries.len() * core::mem::size_of::<DecisionSummary>()
            + self.node_levels.len() * core::mem::size_of::<u8>()
            + self.node_valid_masks.len() * core::mem::size_of::<u8>()
            + self.product_memo.len() * (core::mem::size_of::<u64>() + core::mem::size_of::<u32>())
            + self.union_memo.len() * (core::mem::size_of::<u64>() + core::mem::size_of::<u32>())
    }

    pub fn flush_and_recycle_local_cache(&mut self) -> Result<(), WasmExactSearchError> {
        self.flush_epoch_global()?;
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            self.p7p4_capture = P7P4CaptureCache::default();
        }
        self.nodes.clear();
        self.node_summaries.clear();
        self.node_levels.clear();
        self.node_valid_masks.clear();
        self.bucket_heads.fill(EMPTY_REFERENCE);
        self.product_memo.clear();
        self.union_memo.clear();
        self.interning_disabled = false;
        Ok(())
    }

    pub(super) fn checked_cursor_transition_retained_bytes(
        universe: &MaterializedPatternUniverse,
    ) -> Option<u128> {
        match universe.structure() {
            MaterializedPatternUniverseStructure::Standard7BagLexicographic { sequence_len } => {
                let sequence_len = u8::try_from(sequence_len).ok()?;
                let max_source_depth = sequence_len.checked_add(1)?;
                Some(StandardBagCursorTransitions::checked_storage_len(max_source_depth)? as u128)
            }
            _ => {
                let Some(layout) = FactorizedSourceLayout::supported_layout(universe) else {
                    return Some(0);
                };
                let depth = u8::try_from(layout.visible_sequence_len()).ok()?;
                (StandardBagCursorTransitions::checked_storage_len(depth)? as u128)
                    .checked_add(
                        (usize::from(depth) as u128 + 1)
                            .checked_mul(core::mem::size_of::<u128>() as u128)?,
                    )?
                    .checked_add(FactorizedSourceLayout::checked_retained_bytes(universe)?)
            }
        }
    }

    fn root_source(&self) -> SourceState {
        SourceState {
            depth: 0,
            bag_remainder: self
                .tables
                .factorized_layout
                .as_ref()
                .map_or(FULL_STANDARD_BAG, FactorizedSourceLayout::initial_remainder),
        }
    }

    fn source_piece_order(&self, depth: u8) -> impl Iterator<Item = usize> {
        self.tables
            .factorized_layout
            .as_ref()
            .map_or([0, 1, 2, 3, 4, 5, 6], |layout| {
                layout.source_piece_order(depth)
            })
            .into_iter()
            .filter(|index| *index < STANDARD_PIECE_COUNT as u8)
            .map(usize::from)
    }

    #[cfg(test)]
    fn supply_state(
        &self,
        source: SourceState,
        hold_code: u8,
    ) -> Result<SupplyExecutionState, WasmExactSearchError> {
        canonical_supply_state(source, hold_code, self.hold_enabled, self.supply_identity)
    }

    fn advance_source(
        &self,
        source: SourceState,
        piece_index: usize,
    ) -> Result<SourceState, WasmExactSearchError> {
        self.tables
            .cursor_transitions
            .advance(source, piece_index)?
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_piece_missing",
            ))
    }

    fn matching_supply_step(
        &self,
        source: SourceState,
        hold_code: u8,
        desired_piece: PieceKind,
        branch_kind: SupplyBranchKind,
        current_piece: PieceKind,
    ) -> Result<Option<CompactSupplyStep>, WasmExactSearchError> {
        if hold_code > STANDARD_PIECE_COUNT as u8 || (!self.hold_enabled && hold_code != 0) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_hold_state_invalid",
            ));
        }
        self.tables.cursor_transitions.validate_source(source)?;
        let current_index = usize::from(piece_code(current_piece) - 1);
        let desired_index = usize::from(piece_code(desired_piece) - 1);
        match branch_kind {
            SupplyBranchKind::Current => {
                if desired_piece != current_piece {
                    return Ok(None);
                }
                Ok(self
                    .tables
                    .cursor_transitions
                    .advance(source, current_index)?
                    .map(|next_source| CompactSupplyStep {
                        next_source,
                        next_hold_code: hold_code,
                    }))
            }
            SupplyBranchKind::SwapHeld => {
                if !self.hold_enabled || hold_code == 0 || hold_code != piece_code(desired_piece) {
                    return Ok(None);
                }
                Ok(self
                    .tables
                    .cursor_transitions
                    .advance(source, current_index)?
                    .map(|next_source| CompactSupplyStep {
                        next_source,
                        next_hold_code: piece_code(current_piece),
                    }))
            }
            SupplyBranchKind::StoreCurrent => {
                if !self.hold_enabled || hold_code != 0 {
                    return Ok(None);
                }
                let Some(after_current) = self
                    .tables
                    .cursor_transitions
                    .advance(source, current_index)?
                else {
                    return Ok(None);
                };
                Ok(self
                    .tables
                    .cursor_transitions
                    .advance(after_current, desired_index)?
                    .map(|next_source| CompactSupplyStep {
                        next_source,
                        next_hold_code: piece_code(current_piece),
                    }))
            }
        }
    }

    fn solve(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_node: u32,
        source: SourceState,
        hold_code: u8,
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        self.poll_cancellation(control)?;
        let source = source.normalized();
        let node = language
            .node(language_node)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_node_out_of_range",
            ))?;
        if node.accepting {
            let projected_terminal_depth = self.sequence_len.checked_add(1);
            let projected_terminal = (source.depth == self.sequence_len && hold_code == 0)
                || (Some(source.depth) == projected_terminal_depth && hold_code != 0);
            return Ok(if !self.projects_unplaced_lookahead || projected_terminal {
                ACCEPT
            } else {
                REJECT
            });
        }
        if self.projects_unplaced_lookahead && source.depth == self.sequence_len {
            if !self.hold_enabled || hold_code == 0 {
                return Ok(REJECT);
            }

            let held_edges = language.edges_for_piece(language_node, hold_code).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_piece_language_node_out_of_range"),
            )?;
            let mut result = REJECT;

            if self.tables.factorized_layout.is_none() && source.bag_remainder.count_ones() == 1 {
                let lookahead_index = source.bag_remainder.trailing_zeros() as usize;
                let lookahead_code = lookahead_index as u8 + 1;
                let lookahead_piece = PieceKind::STANDARD_TETROMINOES[lookahead_index];
                let current_step = self
                    .matching_supply_step(
                        source,
                        hold_code,
                        lookahead_piece,
                        SupplyBranchKind::Current,
                        lookahead_piece,
                    )?
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_standard_bag_current_transition_missing",
                    ))?;
                let current_source = current_step.next_source;
                let lookahead_edges = language
                    .edges_for_piece(language_node, lookahead_code)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_node_out_of_range",
                    ))?;
                for edge in lookahead_edges.iter().copied() {
                    self.edge_check_count = self.edge_check_count.saturating_add(1);
                    let next = self.solve(
                        language,
                        edge.child(),
                        current_source,
                        current_step.next_hold_code,
                        control,
                    )?;
                    result = self.union(result, next)?;
                    // The only lookahead is fixed and outside the original
                    // ordinals. Current and Swap still cover the same source.
                    if self.accept_is_absorbing(result) {
                        return Ok(ACCEPT);
                    }
                }
                let held_piece = PieceKind::STANDARD_TETROMINOES[usize::from(hold_code - 1)];
                let swap_step = self
                    .matching_supply_step(
                        source,
                        hold_code,
                        held_piece,
                        SupplyBranchKind::SwapHeld,
                        lookahead_piece,
                    )?
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_standard_bag_swap_transition_missing",
                    ))?;
                let swap_source = swap_step.next_source;
                for edge in held_edges.iter().copied() {
                    self.edge_check_count = self.edge_check_count.saturating_add(1);
                    let next = self.solve(
                        language,
                        edge.child(),
                        swap_source,
                        swap_step.next_hold_code,
                        control,
                    )?;
                    result = self.union(result, next)?;
                    if self.accept_is_absorbing(result) {
                        return Ok(ACCEPT);
                    }
                }
            } else {
                // Finite EOF, or a fresh next bag with no concrete projected
                // identity, can only release the held source piece once. No
                // unseen incoming piece may be placed or retained as another
                // search state.
                for edge in held_edges.iter().copied() {
                    self.edge_check_count = self.edge_check_count.saturating_add(1);
                    if language
                        .node(edge.child())
                        .is_some_and(|child| child.accepting)
                    {
                        result = ACCEPT;
                        break;
                    }
                }
            }
            return Ok(result);
        }
        if source.depth >= self.sequence_len
            || usize::from(node.remaining_depth)
                > source.remaining_output_capacity(
                    self.sequence_len,
                    hold_code,
                    self.projects_unplaced_lookahead,
                )
        {
            return Ok(REJECT);
        }
        let key = product_key(language_node, source, hold_code);
        if let Some(root) = self.product_memo.get(&key).copied() {
            return Ok(root);
        }
        self.product_state_count = self.product_state_count.saturating_add(1);

        let mut children = [REJECT; 7];
        'current_draw: for (current_index, child) in children.iter_mut().enumerate() {
            let current_bit = 1_u8 << current_index;
            if source.bag_remainder & current_bit == 0 {
                continue;
            }
            let current_code = current_index as u8 + 1;
            let current_piece = PieceKind::STANDARD_TETROMINOES[current_index];
            let current_step = self
                .matching_supply_step(
                    source,
                    hold_code,
                    current_piece,
                    SupplyBranchKind::Current,
                    current_piece,
                )?
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_standard_bag_current_transition_missing",
                ))?;
            let next_source = current_step.next_source;
            let mut branch = REJECT;
            let current_edges = language
                .edges_for_piece(language_node, current_code)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_piece_language_node_out_of_range",
                ))?;
            for edge in current_edges.iter().copied() {
                self.edge_check_count = self.edge_check_count.saturating_add(1);
                let next = self.solve(
                    language,
                    edge.child(),
                    next_source,
                    current_step.next_hold_code,
                    control,
                )?;
                branch = self.union(branch, next)?;
                // ACCEPT covers every suffix for this fixed *original* draw.
                // A different current draw has a different ordinal block.
                if self.accept_is_absorbing(branch) {
                    *child = ACCEPT;
                    continue 'current_draw;
                }
            }
            // This product unions queues, not execution histories. If current
            // equals held, Current already followed every same-kind language
            // edge with exactly the same next cursor, bag remainder and hold.
            // CountAll still traverses both actions in CoverageProductEvaluator.
            if self.hold_enabled && hold_code != 0 && hold_code != current_code {
                let held_piece = PieceKind::STANDARD_TETROMINOES[usize::from(hold_code - 1)];
                let swap_step = self
                    .matching_supply_step(
                        source,
                        hold_code,
                        held_piece,
                        SupplyBranchKind::SwapHeld,
                        current_piece,
                    )?
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_standard_bag_swap_transition_missing",
                    ))?;
                let swap_source = swap_step.next_source;
                let held_edges = language.edges_for_piece(language_node, hold_code).ok_or(
                    WasmExactSearchError::InvalidProblem("wasm_piece_language_node_out_of_range"),
                )?;
                for edge in held_edges.iter().copied() {
                    self.edge_check_count = self.edge_check_count.saturating_add(1);
                    let next = self.solve(
                        language,
                        edge.child(),
                        swap_source,
                        swap_step.next_hold_code,
                        control,
                    )?;
                    branch = self.union(branch, next)?;
                    if self.accept_is_absorbing(branch) {
                        *child = ACCEPT;
                        continue 'current_draw;
                    }
                }
            }

            if self.hold_enabled && hold_code == 0 && next_source.depth < self.sequence_len {
                let next_source = next_source.normalized();
                let mut stored_children = [REJECT; 7];
                for (desired_index, stored_child) in stored_children.iter_mut().enumerate() {
                    if next_source.bag_remainder & (1_u8 << desired_index) == 0 {
                        continue;
                    }
                    let desired_edges = language
                        .edges_for_piece(language_node, desired_index as u8 + 1)
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_piece_language_node_out_of_range",
                        ))?;
                    for edge in desired_edges.iter().copied() {
                        self.edge_check_count = self.edge_check_count.saturating_add(1);
                        let desired_piece = PieceKind::STANDARD_TETROMINOES[desired_index];
                        let store_step = self
                            .matching_supply_step(
                                source,
                                hold_code,
                                desired_piece,
                                SupplyBranchKind::StoreCurrent,
                                current_piece,
                            )?
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_standard_bag_store_transition_missing",
                            ))?;
                        let after_next = store_step.next_source;
                        let next = self.solve(
                            language,
                            edge.child(),
                            after_next,
                            store_step.next_hold_code,
                            control,
                        )?;
                        *stored_child = self.union(*stored_child, next)?;
                        // The second original draw is also fixed here. It
                        // cannot absorb a different desired-piece ordinal.
                        if self.accept_is_absorbing(*stored_child) {
                            break;
                        }
                    }
                }
                let stored = self.intern_node(
                    next_source.depth,
                    next_source.bag_remainder,
                    stored_children,
                )?;
                branch = self.union(branch, stored)?;
            }

            if self.hold_enabled
                && hold_code == 0
                && self.projects_unplaced_lookahead
                && self.tables.factorized_layout.is_none()
                && next_source.depth == self.sequence_len
                && next_source.bag_remainder.count_ones() == 1
            {
                let lookahead_index = next_source.bag_remainder.trailing_zeros() as usize;
                let lookahead_code = lookahead_index as u8 + 1;
                let lookahead_piece = PieceKind::STANDARD_TETROMINOES[lookahead_index];
                let lookahead_edges = language
                    .edges_for_piece(language_node, lookahead_code)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_node_out_of_range",
                    ))?;
                let store_step = self
                    .matching_supply_step(
                        source,
                        hold_code,
                        lookahead_piece,
                        SupplyBranchKind::StoreCurrent,
                        current_piece,
                    )?
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_standard_bag_store_transition_missing",
                    ))?;
                let after_lookahead = store_step.next_source;
                for edge in lookahead_edges.iter().copied() {
                    self.edge_check_count = self.edge_check_count.saturating_add(1);
                    let next = self.solve(
                        language,
                        edge.child(),
                        after_lookahead,
                        store_step.next_hold_code,
                        control,
                    )?;
                    branch = self.union(branch, next)?;
                    if self.accept_is_absorbing(branch) {
                        break;
                    }
                }
            }
            *child = branch;
        }

        let root = self.intern_node(source.depth, source.bag_remainder, children)?;
        // Admission failure only skips caching; the exact root remains valid.
        if let Some(boundary) = self.resource_boundary {
            let memo_retained = self.product_memo.retained_payload_bytes()
                + self.product_memo.directory_retained_bytes();
            let outside_memo = boundary
                .outside_retained_bytes
                .checked_add((self.retained_bytes() - memo_retained) as u128);
            let _ = self
                .product_memo
                .try_insert_guarded(key, root, |retained, future| {
                    outside_memo.is_some_and(|outside| {
                        (retained as u128)
                            .checked_add(future)
                            .is_some_and(|required| {
                                boundary.bound.ensure(outside, required).is_ok()
                            })
                    })
                });
        } else {
            let _ = self.product_memo.try_insert(key, root);
        }
        Ok(root)
    }

    #[inline]
    fn accept_is_absorbing(&self, root: u32) -> bool {
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            root == ACCEPT && self.absorb_accepted_suffix
        }
        #[cfg(not(any(test, feature = "local-search-ab")))]
        {
            let _ = root;
            false
        }
    }

    fn intern_node(
        &mut self,
        level: u8,
        valid_piece_mask: u8,
        children: [u32; 7],
    ) -> Result<u32, WasmExactSearchError> {
        let mut has_live_child = false;
        let mut all_valid_accept = true;
        for (index, child) in children.iter().copied().enumerate() {
            if valid_piece_mask & (1_u8 << index) == 0 {
                continue;
            }
            has_live_child |= child != REJECT;
            all_valid_accept &= child == ACCEPT;
        }
        if !has_live_child {
            return Ok(REJECT);
        }
        if all_valid_accept {
            return Ok(ACCEPT);
        }
        self.ensure_buckets();
        let hash = decision_hash(level, valid_piece_mask, &children);
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            let mut reference = self.bucket_heads[bucket];
            while reference != EMPTY_REFERENCE {
                let node = self.nodes[reference_index(reference)];
                if self.node_levels[reference_index(reference)] == level
                    && self.node_valid_masks[reference_index(reference)] == valid_piece_mask
                    && node.children == children
                {
                    return Ok(reference);
                }
                reference = node.next_same_bucket;
            }
        }
        self.reserve_node_storage()?;
        let reference = u32::try_from(self.nodes.len() + 2).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_node_index_overflow")
        })?;
        let next_same_bucket = if self.interning_disabled {
            EMPTY_REFERENCE
        } else {
            self.bucket_heads[hash as usize & (self.bucket_heads.len() - 1)]
        };
        let summary = self.summarize_children(
            SourceState {
                depth: level,
                bag_remainder: valid_piece_mask,
            },
            children,
        )?;
        self.nodes.push(DecisionNode {
            children,
            next_same_bucket,
        });
        self.node_summaries.push(summary);
        self.node_levels.push(level);
        self.node_valid_masks.push(valid_piece_mask);
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            self.bucket_heads[bucket] = reference;
            self.grow_buckets_if_needed();
        }
        Ok(reference)
    }

    fn union(&mut self, left: u32, right: u32) -> Result<u32, WasmExactSearchError> {
        if left == right || right == REJECT || left == ACCEPT {
            return Ok(left);
        }
        if left == REJECT || right == ACCEPT {
            return Ok(right);
        }
        let (left, right) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        let key = union_key(left, right);
        if let Some(root) = self.union_memo.get(&key).copied() {
            return Ok(root);
        }
        let left_index = reference_index(left);
        let right_index = reference_index(right);
        let level =
            *self
                .node_levels
                .get(left_index)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_standard_bag_union_node_out_of_range",
                ))?;
        if self.node_levels.get(right_index).copied() != Some(level) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_union_level_mismatch",
            ));
        }
        let valid_mask = self.node_valid_masks[left_index];
        if self.node_valid_masks.get(right_index).copied() != Some(valid_mask) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_union_source_mismatch",
            ));
        }
        let left_children = self.nodes[left_index].children;
        let right_children = self.nodes[right_index].children;
        let mut children = [REJECT; 7];
        for index in 0..7 {
            children[index] = self.union(left_children[index], right_children[index])?;
        }
        // Both roots already have this level and valid mask. If the union
        // preserves an operand's exact child vector, reuse that decision
        // node and its summary without hashing or interning it again.
        let root = if children == left_children {
            left
        } else if children == right_children {
            right
        } else {
            self.intern_node(level, valid_mask, children)?
        };
        // The exact root is already available. Skipping a memo insertion
        // changes neither the root nor coverage, just future recomputation.
        #[cfg(any(test, feature = "local-search-ab"))]
        if self
            .union_memo_entry_limit
            .is_some_and(|limit| self.union_memo.len() >= limit)
        {
            return Ok(root);
        }
        if self.optional_growth_admitted(self.union_memo.checked_reserve_future_bytes(1))
            && self.union_memo.try_reserve(1).is_ok()
        {
            self.union_memo.insert(key, root);
        }
        Ok(root)
    }

    fn summary(
        &self,
        root: u32,
        source: SourceState,
    ) -> Result<DecisionSummary, WasmExactSearchError> {
        if root == REJECT {
            return Ok(DecisionSummary {
                covered_pattern_count: 0,
                first_pattern_offset: NO_PATTERN_OFFSET,
            });
        }
        if root == ACCEPT {
            return Ok(DecisionSummary {
                covered_pattern_count: clamp_pattern_count(self.suffix_count(source)),
                first_pattern_offset: 0,
            });
        }
        let source = source.normalized();
        let index = reference_index(root);
        if self.node_levels.get(index).copied() != Some(source.depth) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_decision_level_mismatch",
            ));
        }
        self.node_summaries
            .get(index)
            .copied()
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_decision_node_out_of_range",
            ))
    }

    fn summarize_children(
        &self,
        source: SourceState,
        children: [u32; 7],
    ) -> Result<DecisionSummary, WasmExactSearchError> {
        let source = source.normalized();
        let mut covered_pattern_count = 0_u64;
        let mut first_pattern_offset = NO_PATTERN_OFFSET;
        let mut child_base = 0_u64;
        for piece_index in self.source_piece_order(source.depth) {
            let child = children[piece_index];
            if source.bag_remainder & (1_u8 << piece_index) == 0 {
                continue;
            }
            let next = self.advance_source(source, piece_index)?;
            let child_summary = self.summary(child, next)?;
            covered_pattern_count =
                covered_pattern_count.saturating_add(child_summary.covered_pattern_count);
            if first_pattern_offset == NO_PATTERN_OFFSET
                && child_summary.first_pattern_offset != NO_PATTERN_OFFSET
            {
                first_pattern_offset =
                    child_base.saturating_add(child_summary.first_pattern_offset);
            }
            child_base = child_base.saturating_add(clamp_pattern_count(self.suffix_count(next)));
        }
        Ok(DecisionSummary {
            covered_pattern_count,
            first_pattern_offset,
        })
    }

    fn accumulate_global(
        &mut self,
        root: u32,
        source: SourceState,
        base: u128,
        limit: u128,
    ) -> Result<usize, WasmExactSearchError> {
        if root == REJECT || base >= limit {
            return Ok(0);
        }
        if root == ACCEPT {
            let end = base.saturating_add(self.suffix_count(source)).min(limit);
            return Ok(set_range_count_new(
                &mut self.global_words,
                base as usize,
                end as usize,
            ));
        }
        if source.depth >= self.sequence_len {
            return Ok(0);
        }
        let source = source.normalized();
        let index = reference_index(root);
        if self.node_levels.get(index).copied() != Some(source.depth) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_decision_level_mismatch",
            ));
        }
        let children = self.nodes[index].children;
        let mut child_base = base;
        let mut newly_covered = 0usize;
        for piece_index in self.source_piece_order(source.depth) {
            let child = children[piece_index];
            if source.bag_remainder & (1_u8 << piece_index) == 0 {
                continue;
            }
            let next = self.advance_source(source, piece_index)?;
            newly_covered = newly_covered
                .saturating_add(self.accumulate_global(child, next, child_base, limit)?);
            child_base = child_base.saturating_add(self.suffix_count(next));
            if child_base >= limit {
                break;
            }
        }
        Ok(newly_covered)
    }

    fn accumulate_into(
        &self,
        root: u32,
        source: SourceState,
        base: u128,
        limit: u128,
        words: &mut [u64],
    ) -> Result<(), WasmExactSearchError> {
        if root == REJECT || base >= limit {
            return Ok(());
        }
        if root == ACCEPT {
            let end = base.saturating_add(self.suffix_count(source)).min(limit);
            set_range_count_new(words, base as usize, end as usize);
            return Ok(());
        }
        if source.depth >= self.sequence_len {
            return Ok(());
        }
        let source = source.normalized();
        let index = reference_index(root);
        if self.node_levels.get(index).copied() != Some(source.depth) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_decision_level_mismatch",
            ));
        }
        let children = self.nodes[index].children;
        let mut child_base = base;
        for piece_index in self.source_piece_order(source.depth) {
            let child = children[piece_index];
            if source.bag_remainder & (1_u8 << piece_index) == 0 {
                continue;
            }
            let next = self.advance_source(source, piece_index)?;
            self.accumulate_into(child, next, child_base, limit, words)?;
            child_base = child_base.saturating_add(self.suffix_count(next));
            if child_base >= limit {
                break;
            }
        }
        Ok(())
    }

    fn flush_epoch_global(&mut self) -> Result<(), WasmExactSearchError> {
        let root = self.epoch_global_root;
        if root == REJECT {
            return Ok(());
        }
        if self.global_words.is_empty() {
            let word_count = self.materialized_pattern_count.div_ceil(u64::BITS as usize);
            let future = if word_count > self.global_words.capacity() {
                (word_count as u128).checked_mul(core::mem::size_of::<u64>() as u128)
            } else {
                Some(0)
            };
            self.ensure_growth(future)?;
            self.global_words
                .try_reserve_exact(word_count)
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_standard_bag_coverage_storage_unavailable",
                    )
                })?;
            self.global_words.resize(word_count, 0);
        }
        let newly_covered = self.accumulate_global(
            root,
            self.root_source(),
            0,
            self.materialized_pattern_count as u128,
        )?;
        self.global_covered_pattern_count = self
            .global_covered_pattern_count
            .saturating_add(newly_covered)
            .min(self.materialized_pattern_count);
        self.epoch_global_root = REJECT;
        Ok(())
    }

    fn count_patterns(
        &self,
        root: u32,
        source: SourceState,
        base: u128,
        limit: u128,
    ) -> Result<u128, WasmExactSearchError> {
        if root == REJECT || base >= limit {
            return Ok(0);
        }
        if root == ACCEPT {
            return Ok(self.suffix_count(source).min(limit - base));
        }
        if source.depth >= self.sequence_len {
            return Ok(0);
        }
        let source = source.normalized();
        let index = reference_index(root);
        if self.node_levels.get(index).copied() != Some(source.depth) {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_decision_level_mismatch",
            ));
        }
        let children = self.nodes[index].children;
        let mut child_base = base;
        let mut count = 0_u128;
        for piece_index in self.source_piece_order(source.depth) {
            let child = children[piece_index];
            if source.bag_remainder & (1_u8 << piece_index) == 0 {
                continue;
            }
            let next = self.advance_source(source, piece_index)?;
            count = count.saturating_add(self.count_patterns(child, next, child_base, limit)?);
            child_base = child_base.saturating_add(self.suffix_count(next));
            if child_base >= limit {
                break;
            }
        }
        Ok(count)
    }

    fn suffix_count(&self, source: SourceState) -> u128 {
        let depth = source.depth.min(self.sequence_len) as usize;
        let remainder = if depth == self.sequence_len as usize {
            0
        } else {
            usize::from(source.normalized().bag_remainder)
        };
        self.tables.suffix_counts[depth * 128 + remainder]
    }

    fn ensure_buckets(&mut self) {
        if !self.bucket_heads.is_empty() || self.interning_disabled {
            return;
        }
        let future = if INITIAL_BUCKET_COUNT > self.bucket_heads.capacity() {
            (INITIAL_BUCKET_COUNT as u128).checked_mul(core::mem::size_of::<u32>() as u128)
        } else {
            Some(0)
        };
        if !self.optional_growth_admitted(future) {
            self.interning_disabled = true;
            return;
        }
        if self
            .bucket_heads
            .try_reserve_exact(INITIAL_BUCKET_COUNT)
            .is_err()
        {
            self.interning_disabled = true;
            return;
        }
        self.bucket_heads
            .resize(INITIAL_BUCKET_COUNT, EMPTY_REFERENCE);
    }

    fn grow_buckets_if_needed(&mut self) {
        if self.interning_disabled
            || self.nodes.len().saturating_mul(4) < self.bucket_heads.len().saturating_mul(3)
        {
            return;
        }
        let Some(new_count) = self.bucket_heads.len().checked_mul(2) else {
            self.interning_disabled = true;
            return;
        };
        if !self.optional_growth_admitted(
            (new_count as u128).checked_mul(core::mem::size_of::<u32>() as u128),
        ) {
            self.interning_disabled = true;
            return;
        }
        let mut grown = Vec::new();
        if grown.try_reserve_exact(new_count).is_err() {
            self.interning_disabled = true;
            return;
        }
        grown.resize(new_count, EMPTY_REFERENCE);
        for index in 0..self.nodes.len() {
            let hash = decision_hash(
                self.node_levels[index],
                self.node_valid_masks[index],
                &self.nodes[index].children,
            );
            let bucket = hash as usize & (new_count - 1);
            self.nodes[index].next_same_bucket = grown[bucket];
            grown[bucket] = index as u32 + 2;
        }
        self.bucket_heads = grown;
    }

    fn reserve_node_storage(&mut self) -> Result<(), WasmExactSearchError> {
        if self.resource_boundary.is_some() {
            let future = checked_node_vec_growth(&self.nodes)
                .and_then(|bytes| bytes.checked_add(checked_node_vec_growth(&self.node_summaries)?))
                .and_then(|bytes| bytes.checked_add(checked_node_vec_growth(&self.node_levels)?))
                .and_then(|bytes| {
                    bytes.checked_add(checked_node_vec_growth(&self.node_valid_masks)?)
                });
            // All four capacities are admitted before the first reserve. Each
            // old allocation is already retained; the future is the sum of
            // whole replacement buffers, not the capacity deltas.
            self.ensure_growth(future)?;
            reserve_node_vec(&mut self.nodes)?;
            reserve_node_vec(&mut self.node_summaries)?;
            reserve_node_vec(&mut self.node_levels)?;
            reserve_node_vec(&mut self.node_valid_masks)?;
        } else {
            self.nodes
                .try_reserve(1)
                .map_err(|_| node_storage_error())?;
            self.node_summaries
                .try_reserve(1)
                .map_err(|_| node_storage_error())?;
            self.node_levels
                .try_reserve(1)
                .map_err(|_| node_storage_error())?;
            self.node_valid_masks
                .try_reserve(1)
                .map_err(|_| node_storage_error())?;
        }
        Ok(())
    }

    fn poll_cancellation(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.cancellation_poll_counter = self.cancellation_poll_counter.wrapping_add(1);
        if self.cancellation_poll_counter & CANCELLATION_POLL_MASK == 0 && control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        Ok(())
    }
}

fn decision_hash(level: u8, valid_piece_mask: u8, children: &[u32; 7]) -> u64 {
    let mut hash = mix_digest(0, u64::from(level));
    hash = mix_digest(hash, u64::from(valid_piece_mask));
    for child in children {
        hash = mix_digest(hash, u64::from(*child));
    }
    hash
}

fn node_storage_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_standard_bag_node_storage_unavailable")
}

fn checked_node_vec_growth<T>(values: &Vec<T>) -> Option<u128> {
    let needed = values.len().checked_add(1)?;
    if needed <= values.capacity() {
        return Some(0);
    }
    (needed.checked_next_power_of_two()? as u128).checked_mul(core::mem::size_of::<T>() as u128)
}

fn reserve_node_vec<T>(values: &mut Vec<T>) -> Result<(), WasmExactSearchError> {
    let needed = values.len().checked_add(1).ok_or_else(node_storage_error)?;
    if needed > values.capacity() {
        let capacity = needed
            .checked_next_power_of_two()
            .ok_or_else(node_storage_error)?;
        values
            .try_reserve_exact(capacity - values.len())
            .map_err(|_| node_storage_error())?;
    }
    Ok(())
}

fn compile_suffix_counts(sequence_len: u8) -> Result<Vec<u128>, WasmExactSearchError> {
    let row_count = usize::from(sequence_len) + 1;
    let entry_count = row_count
        .checked_mul(128)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_suffix_table_overflow",
        ))?;
    let mut counts = Vec::new();
    counts.try_reserve_exact(entry_count).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_standard_bag_suffix_table_unavailable")
    })?;
    counts.resize(entry_count, 0);
    let terminal_start = usize::from(sequence_len) * 128;
    counts[terminal_start..terminal_start + 128].fill(1);
    for depth in (0..usize::from(sequence_len)).rev() {
        for encoded_remainder in 0_u8..=127 {
            let remainder = if encoded_remainder == 0 {
                FULL_STANDARD_BAG
            } else {
                encoded_remainder
            };
            let next_remainder = remainder & (remainder - 1);
            counts[depth * 128 + usize::from(encoded_remainder)] =
                u128::from(remainder.count_ones())
                    .saturating_mul(counts[(depth + 1) * 128 + usize::from(next_remainder)]);
        }
    }
    Ok(counts)
}

const fn product_key(
    language_node: u32,
    source: SourceState,
    hold_code: u8,
) -> StandardBagProductMemoKey {
    StandardBagProductMemoKey::new(language_node, source.depth, source.bag_remainder, hold_code)
}

const fn union_key(left: u32, right: u32) -> u64 {
    left as u64 | (right as u64) << 32
}

fn clamp_pattern_count(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn set_range_count_new(words: &mut [u64], start: usize, end: usize) -> usize {
    if start >= end {
        return 0;
    }
    let first_word = start / u64::BITS as usize;
    let last_word = (end - 1) / u64::BITS as usize;
    if first_word == last_word {
        let width = end - start;
        let mask = if width == u64::BITS as usize {
            u64::MAX
        } else {
            ((1_u64 << width) - 1) << (start % u64::BITS as usize)
        };
        return set_word_mask(&mut words[first_word], mask);
    }
    let mut newly_covered = set_word_mask(
        &mut words[first_word],
        u64::MAX << (start % u64::BITS as usize),
    );
    for word in &mut words[first_word + 1..last_word] {
        newly_covered = newly_covered.saturating_add(set_word_mask(word, u64::MAX));
    }
    let remainder = end % u64::BITS as usize;
    let mask = if remainder == 0 {
        u64::MAX
    } else {
        (1_u64 << remainder) - 1
    };
    newly_covered.saturating_add(set_word_mask(&mut words[last_word], mask))
}

#[inline]
fn set_word_mask(word: &mut u64, mask: u64) -> usize {
    let added = mask & !*word;
    *word |= mask;
    added.count_ones() as usize
}

const fn reference_index(reference: u32) -> usize {
    (reference - 2) as usize
}

const fn piece_code(piece: clearra_core_domain::piece::piece_kind::PieceKind) -> u8 {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    match piece {
        PieceKind::I => 1,
        PieceKind::O => 2,
        PieceKind::T => 3,
        PieceKind::S => 4,
        PieceKind::Z => 5,
        PieceKind::J => 6,
        PieceKind::L => 7,
    }
}

fn standard_bag_mask_key(mask: u8) -> u64 {
    (0..7).fold(0_u64, |key, index| {
        let count = (mask >> index) & 1;
        key | (u64::from(count) << ((index + 1) * 4))
    })
}

fn standard_bag_key_mask(key: u64) -> Result<u8, WasmExactSearchError> {
    let storage_mask = (1usize..=7).fold(0_u64, |mask, piece| mask | (0xf_u64 << (piece * 4)));
    if key & !storage_mask != 0 {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_standard_bag_remainder_invalid",
        ));
    }
    let mut mask = 0_u8;
    for index in 0..7 {
        let count = ((key >> ((index + 1) * 4)) & 0xf) as u8;
        if count > 1 {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_remainder_invalid",
            ));
        }
        mask |= count << index;
    }
    Ok(mask)
}

#[cfg(test)]
mod supply_automaton_tests {
    use std::{hint::black_box, time::Instant};

    use clearra_supply::{
        hold_automaton::SupplyProvenanceId, piece_source::PieceSourceId,
        queue::fixed_sequence::FixedSequence, PatternUniverseMaterializer,
    };

    use super::*;

    fn coverage_for_len(sequence_len: usize, hold_enabled: bool) -> StandardBagCoverage {
        let universe = PatternUniverseMaterializer::standard_7_bag(sequence_len, 1, 0x77)
            .expect("standard bag universe");
        let initial = HoldAutomatonState::new(
            PieceSourceId::new(0x77),
            0,
            None,
            0,
            BagState::fresh_standard_7_bag().packed_remainder_key(),
            SupplyProvenanceId(0x77),
        );
        StandardBagCoverage::for_universe(&universe, initial, hold_enabled, true)
            .expect("coverage construction")
            .expect("standard bag fast path")
    }

    fn coverage(hold_enabled: bool) -> StandardBagCoverage {
        coverage_for_len(8, hold_enabled)
    }

    fn canonical_automaton() -> SupplyExecutionAutomaton {
        SupplyExecutionAutomaton::for_bag(&PieceKind::STANDARD_TETROMINOES)
            .expect("standard bag automaton")
    }

    fn request_fixture(sequence_len: usize) -> (MaterializedPatternUniverse, HoldAutomatonState) {
        let universe = PatternUniverseMaterializer::standard_7_bag(sequence_len, 1, 0x77)
            .expect("compact standard bag universe");
        let initial = HoldAutomatonState::new(
            PieceSourceId::new(0x77),
            0,
            None,
            0,
            BagState::fresh_standard_7_bag().packed_remainder_key(),
            SupplyProvenanceId(0x77),
        );
        (universe, initial)
    }

    #[test]
    fn bounded_standard_bag_nodes_are_admitted_before_any_replacement_allocation() {
        let mut coverage = coverage_for_len(2, false);
        let outside = 173_u128;
        let initial = coverage.retained_bytes() as u128;
        let one_node = (core::mem::size_of::<DecisionNode>()
            + core::mem::size_of::<DecisionSummary>()
            + 2) as u128;
        let mut first = [REJECT; 7];
        first[0] = ACCEPT;
        coverage.set_resource_boundary(
            ExecutionMemoryBound::for_test_cap(initial + outside + one_node - 1),
            outside,
        );
        assert!(matches!(
            coverage.intern_node(0, FULL_STANDARD_BAG, first),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        assert_eq!(coverage.retained_bytes() as u128, initial);
        assert_eq!(coverage.nodes.capacity(), 0);
        assert_eq!(coverage.node_summaries.capacity(), 0);
        assert_eq!(coverage.node_levels.capacity(), 0);
        assert_eq!(coverage.node_valid_masks.capacity(), 0);
        assert_eq!(
            coverage.bucket_heads.capacity(),
            0,
            "optional growth was declined"
        );

        coverage.set_resource_boundary(
            ExecutionMemoryBound::for_test_cap(initial + outside + one_node),
            outside,
        );
        let first_root = coverage.intern_node(0, FULL_STANDARD_BAG, first).unwrap();
        let retained = coverage.retained_bytes() as u128;
        let mut second = [REJECT; 7];
        second[1] = ACCEPT;
        // Both old and new buffers must fit while growing one node to two.
        coverage.set_resource_boundary(
            ExecutionMemoryBound::for_test_cap(retained + outside + 2 * one_node - 1),
            outside,
        );
        assert!(matches!(
            coverage.intern_node(0, FULL_STANDARD_BAG, second),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        assert_eq!(coverage.retained_bytes() as u128, retained);
        assert_eq!(coverage.nodes.len(), 1);
        assert_eq!(coverage.nodes[reference_index(first_root)].children, first);
        coverage.set_resource_boundary(
            ExecutionMemoryBound::for_test_cap(retained + outside + 2 * one_node),
            outside,
        );
        coverage.intern_node(0, FULL_STANDARD_BAG, second).unwrap();
        assert_eq!(coverage.nodes.len(), 2);
    }

    #[test]
    fn bounded_standard_bag_declines_optional_caches_and_keeps_exact_ordinals() {
        let (_, initial) = request_fixture(2);
        let universe = PatternUniverseMaterializer::standard_7_bag(2, 0, 0x77).unwrap();
        let make = || {
            StandardBagCoverage::for_universe(&universe, initial, true, false)
                .unwrap()
                .unwrap()
        };
        let mut reference = make();
        let mut bounded = make();
        // Required payload is already admitted. No optional table gets new
        // credit, and computing exact nodes must still work without its cache.
        bounded.nodes.reserve_exact(256);
        bounded.node_summaries.reserve_exact(256);
        bounded.node_levels.reserve_exact(256);
        bounded.node_valid_masks.reserve_exact(256);
        let retained = bounded.retained_bytes() as u128;
        let outside = 91;
        bounded.set_resource_boundary(
            ExecutionMemoryBound::for_test_cap(retained + outside),
            outside,
        );
        let mut language = PieceOrderLanguageCache::default();
        let root = language
            .canonicalize(&branching_piece_language(2, 0))
            .unwrap();
        let control = ExecutionControl::default();
        let expected = reference.cover_language(&language, root, &control).unwrap();
        let expected_words = reference.materialize_root(expected.root).unwrap();
        assert!(expected.covers_any_pattern);
        for _ in 0..2 {
            let actual = bounded.cover_language(&language, root, &control).unwrap();
            assert_eq!(actual.covered_pattern_count, expected.covered_pattern_count);
            assert_eq!(actual.witness_pattern_id, expected.witness_pattern_id);
            bounded.merge_global(actual.root).unwrap();
        }
        assert_eq!(bounded.product_memo.len(), 0);
        assert_eq!(bounded.union_memo.len(), 0);
        assert_eq!(bounded.bucket_heads.capacity(), 0);
        let pending_root = bounded.epoch_global_root;
        assert!(matches!(
            bounded.flush_and_recycle_local_cache(),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        assert_eq!(bounded.global_words.capacity(), 0);
        assert_eq!(
            bounded.epoch_global_root, pending_root,
            "failed flush retained the proof"
        );

        let words = universe.pattern_count().div_ceil(64) as u128 * 8;
        let materialize = PatternBitSet::checked_external_words_materialize_union_future_bytes(
            universe.pattern_count(),
        )
        .unwrap();
        bounded.set_resource_boundary(
            ExecutionMemoryBound::for_test_cap(retained + outside + words + materialize),
            outside,
        );
        bounded.flush_and_recycle_local_cache().unwrap();
        assert_eq!(bounded.materialize_global().unwrap(), expected_words);
        assert_eq!(bounded.epoch_global_root, REJECT);
        assert!(bounded.nodes.is_empty());
    }

    #[test]
    fn accept_absorption_mode_requires_an_explicit_opt_in() {
        use std::env::VarError;

        assert_eq!(
            parse_accept_absorption(Err(VarError::NotPresent)),
            Ok(false)
        );
        assert_eq!(parse_accept_absorption(Ok("0".into())), Ok(false));
        assert_eq!(parse_accept_absorption(Ok("1".into())), Ok(true));
        for value in [
            Ok("".into()),
            Ok("true".into()),
            Ok(" 1".into()),
            Ok("2".into()),
            Err(VarError::NotUnicode(std::ffi::OsString::new())),
        ] {
            assert_eq!(
                parse_accept_absorption(value),
                Err("wasm_standard_bag_absorption_mode_invalid")
            );
        }
    }

    #[test]
    fn accept_absorption_preserves_ordinals_witness_and_recycled_global_union() {
        let universe = PatternUniverseMaterializer::standard_7_bag(2, 0, 0x77)
            .expect("full two-draw standard bag");
        assert_eq!(universe.pattern_count(), 42);
        let kinds = PieceKind::STANDARD_TETROMINOES;
        let control = ExecutionControl::default();
        let mut language = PieceOrderLanguageCache::default();
        for held in [None, Some(kinds[0])] {
            let initial = HoldAutomatonState::new(
                PieceSourceId::new(0x77),
                0,
                held,
                0,
                BagState::fresh_standard_7_bag().packed_remainder_key(),
                SupplyProvenanceId(0x77),
            );
            let make = |absorb| {
                let mut coverage =
                    StandardBagCoverage::for_universe(&universe, initial, true, false)
                        .expect("bag coverage")
                        .expect("supported universe");
                coverage.absorb_accepted_suffix = absorb;
                coverage
            };
            let (mut baseline, mut treatment) = (make(false), make(true));
            let mut skipped = 0;
            let mut expected_union = 0_u64;
            for phase in 0..2 {
                let accepted = [kinds[phase], kinds[phase + 2]];
                let word = language
                    .canonicalize(&branching_piece_language(1, phase))
                    .expect("one-piece language");
                let expected = (0..universe.pattern_count()).fold(0_u64, |bits, ordinal| {
                    let queue = universe.sequence_at(ordinal);
                    let succeeds = accepted.contains(&queue[0])
                        || held.is_some_and(|piece| accepted.contains(&piece))
                        || (held.is_none() && accepted.contains(&queue[1]));
                    bits | (u64::from(succeeds) << ordinal)
                });
                assert_ne!(expected, 0);
                if held.is_none() {
                    assert_ne!(expected.count_ones(), 42);
                }
                expected_union |= expected;
                let a = baseline
                    .cover_language(&language, word, &control)
                    .expect("baseline");
                let b = treatment
                    .cover_language(&language, word, &control)
                    .expect("treatment");
                assert!(b.edge_checks <= a.edge_checks);
                skipped += a.edge_checks - b.edge_checks;
                for (coverage, result) in [(&mut baseline, a), (&mut treatment, b)] {
                    assert_eq!(result.covered_pattern_count, expected.count_ones() as usize);
                    assert_eq!(result.witness_pattern_id, Some(expected.trailing_zeros()));
                    assert_eq!(
                        coverage
                            .materialize_root(result.root)
                            .unwrap()
                            .to_owned_words(),
                        vec![expected]
                    );
                    coverage.merge_global(result.root).expect("merge decision");
                    coverage
                        .flush_and_recycle_local_cache()
                        .expect("recycle arena");
                    assert_eq!(
                        coverage.materialize_global().unwrap().to_owned_words(),
                        vec![expected_union]
                    );
                }
            }
            assert!(skipped > 0, "fixture must visit an absorbed branch");
        }
    }

    #[test]
    fn accept_absorption_keeps_projected_terminal_lookahead_in_its_original_draw() {
        let mut language = PieceOrderLanguageCache::default();
        let word = language
            .canonicalize(&branching_piece_language(1, 0))
            .expect("one-piece language");
        let kinds = PieceKind::STANDARD_TETROMINOES;
        let (held, known) = (piece_code(kinds[0]), piece_code(kinds[2]));
        let control = ExecutionControl::default();
        let make = |sequence, absorb| {
            let mut coverage = coverage_for_len(sequence, true);
            coverage.absorb_accepted_suffix = absorb;
            coverage
        };

        // A known last mino from the current bag is outside the original six
        // draws. Both Current and Swap finish; the second proof is redundant.
        let (mut a, mut b) = (make(6, false), make(6, true));
        let terminal = SourceState {
            depth: 6,
            bag_remainder: 1_u8 << (known - 1),
        };
        assert_eq!(
            a.solve(&language, word, terminal, held, &control),
            Ok(ACCEPT)
        );
        assert_eq!(
            b.solve(&language, word, terminal, held, &control),
            Ok(ACCEPT)
        );
        assert_eq!((a.edge_check_count, b.edge_check_count), (2, 1));

        // Neither original last draw can be skipped, but either draw already
        // completes directly. StoreCurrent's extra lookahead cannot add an
        // original queue once that draw's suffix has been accepted.
        let (mut a, mut b) = (make(6, false), make(6, true));
        let last_draw = SourceState {
            depth: 5,
            bag_remainder: (1_u8 << (known - 1)) | (1_u8 << (held - 1)),
        };
        assert_eq!(a.solve(&language, word, last_draw, 0, &control), Ok(ACCEPT));
        assert_eq!(b.solve(&language, word, last_draw, 0, &control), Ok(ACCEPT));
        assert_eq!((a.edge_check_count, b.edge_check_count), (4, 2));

        // A fresh bag cannot place an unknown incoming piece. It may only
        // release a matching held piece, with no new source ordinal.
        let fresh = SourceState {
            depth: 7,
            bag_remainder: FULL_STANDARD_BAG,
        };
        let (mut a, mut b) = (make(7, false), make(7, true));
        for coverage in [&mut a, &mut b] {
            assert_eq!(
                coverage.solve(&language, word, fresh, held, &control),
                Ok(ACCEPT)
            );
            assert_eq!(
                coverage.solve(&language, word, fresh, piece_code(kinds[1]), &control),
                Ok(REJECT)
            );
        }
    }

    #[test]
    fn union_memo_entry_limit_is_explicit_and_defaults_to_unbounded() {
        assert_eq!(parse_union_memo_entry_limit(None), Ok(None));
        assert_eq!(parse_union_memo_entry_limit(Some("0")), Ok(Some(0)));
        assert_eq!(
            parse_union_memo_entry_limit(Some("3670016")),
            Ok(Some(3_670_016))
        );
        for malformed in [
            "",
            "-1",
            "+1",
            " 2",
            "full",
            "99999999999999999999999999999",
        ] {
            assert_eq!(
                parse_union_memo_entry_limit(Some(malformed)),
                Err(UNION_MEMO_LIMIT_INVALID)
            );
        }
    }

    #[test]
    fn union_only_compact_slots_preserve_exact_roots_witnesses_and_recycling() {
        use ExactU64MemoStorage::{Compact, Reference};
        use PieceKind::{I, J, O, S, T, Z};

        let (_, initial) = request_fixture(4);
        let universe = PatternUniverseMaterializer::standard_7_bag(4, 0, 0x77)
            .expect("full four-draw standard bag");
        assert_eq!(universe.pattern_count(), 840);
        let make = |union_storage| {
            let mut coverage = StandardBagCoverage::for_universe(&universe, initial, true, true)
                .expect("bag coverage")
                .expect("supported universe");
            // Set the same product backend and recycling policy explicitly.
            // No process environment is modified by the two-arm test.
            coverage.product_memo = StandardBagProductMemo::new(
                StandardBagProductMemoLayout::Flat,
                Reference,
                coverage.tables.cursor_transitions.max_source_depth,
            );
            coverage.union_memo = ExactU64MemoMap::new(union_storage);
            coverage.union_memo_entry_limit = None;
            coverage
        };
        let (mut reference, mut treatment) = (make(Reference), make(Compact));
        let control = ExecutionControl::default();
        let mut language = PieceOrderLanguageCache::default();
        let mut saw_witness = false;
        for (epoch, prefixes) in [[I, O, T], [J, S, Z]].into_iter().enumerate() {
            // Force the union table to contain nontrivial unordered pairs,
            // even if all later language branches happen to be disjoint.
            for piece in prefixes {
                let mut children = [REJECT; 7];
                children[usize::from(piece_code(piece) - 1)] = ACCEPT;
                for coverage in [&mut reference, &mut treatment] {
                    let root = coverage
                        .intern_node(0, FULL_STANDARD_BAG, children)
                        .expect("first-piece decision");
                    coverage.merge_global(root).expect("merge exact decision");
                }
            }
            for placed_len in [3, 4] {
                let language_root = language
                    .canonicalize(&branching_piece_language(placed_len, epoch))
                    .expect("canonical piece language");
                let a = reference
                    .cover_language(&language, language_root, &control)
                    .expect("reference coverage");
                let b = treatment
                    .cover_language(&language, language_root, &control)
                    .expect("compact union coverage");
                saw_witness |= a.witness_pattern_id.is_some();
                assert_eq!(a.root, b.root);
                assert_eq!(a.covers_any_pattern, b.covers_any_pattern);
                assert_eq!(a.witness_pattern_id, b.witness_pattern_id);
                assert_eq!(a.covered_pattern_count, b.covered_pattern_count);
                assert_eq!(
                    reference.materialize_root(a.root).expect("reference bits"),
                    treatment.materialize_root(b.root).expect("treatment bits"),
                );
                reference.merge_global(a.root).expect("reference union");
                treatment.merge_global(b.root).expect("treatment union");
            }
            assert_eq!(reference.local_live_bytes(), treatment.local_live_bytes());
            let a = reference.memo_accounting();
            let b = treatment.memo_accounting();
            assert_eq!(a.product_storage, "reference");
            assert_eq!(b.product_storage, a.product_storage);
            assert_eq!(b.product_layout, a.product_layout);
            assert_eq!(b.product_payload_bytes, a.product_payload_bytes);
            assert_eq!(a.union_storage, "reference");
            assert_eq!(b.union_storage, "compact");
            assert!(a.union_entries >= 2);
            assert_eq!(b.union_entries, a.union_entries);
            assert_eq!(b.union_capacity, a.union_capacity);
            assert_eq!(b.union_payload_bytes * 4, a.union_payload_bytes * 3);
            for coverage in [&mut reference, &mut treatment] {
                coverage.flush_and_recycle_local_cache().expect("recycle");
                assert_eq!(coverage.union_memo.len(), 0);
                assert_eq!(coverage.product_memo.len(), 0);
            }
            assert_eq!(
                reference
                    .materialize_global()
                    .expect("reference global bits"),
                treatment
                    .materialize_global()
                    .expect("treatment global bits"),
            );
        }
        assert!(saw_witness);
    }

    #[test]
    fn tiny_union_memo_caps_preserve_exact_coverage_witnesses_and_recycling() {
        use PieceKind::{I, J, O, S, T, Z};

        let (_, initial) = request_fixture(4);
        let universe = PatternUniverseMaterializer::standard_7_bag(4, 0, 0x77)
            .expect("full four-draw standard bag");
        assert_eq!(universe.pattern_count(), 840);
        let make = |storage, limit| {
            let mut coverage = StandardBagCoverage::for_universe(&universe, initial, true, true)
                .expect("bag coverage")
                .expect("supported universe");
            coverage.union_memo = ExactU64MemoMap::new(storage);
            coverage.union_memo_entry_limit = limit;
            coverage
        };
        for storage in [ExactU64MemoStorage::Reference, ExactU64MemoStorage::Compact] {
            for limit in [0, 1] {
                let mut reference = make(storage, None);
                let mut treatment = make(storage, Some(limit));
                let mut language = PieceOrderLanguageCache::default();
                let control = ExecutionControl::default();
                let mut saw_witness = false;
                for (epoch, prefixes) in [[I, O, T], [J, S, Z]].into_iter().enumerate() {
                    // Each distinct prefix merges into the same pending global
                    // root, forcing two nontrivial union entries per epoch.
                    for piece in prefixes {
                        let mut children = [REJECT; 7];
                        children[usize::from(piece_code(piece) - 1)] = ACCEPT;
                        for coverage in [&mut reference, &mut treatment] {
                            let root = coverage
                                .intern_node(0, FULL_STANDARD_BAG, children)
                                .expect("first-piece language");
                            coverage.merge_global(root).expect("exact union");
                        }
                    }
                    assert_eq!(reference.union_memo.len(), 2);
                    assert_eq!(treatment.union_memo.len(), limit);
                    for placed_len in [3, 4] {
                        for phase in [epoch, epoch + 1] {
                            let root = language
                                .canonicalize(&branching_piece_language(placed_len, phase))
                                .expect("canonical piece language");
                            let a = reference
                                .cover_language(&language, root, &control)
                                .expect("reference coverage");
                            let b = treatment
                                .cover_language(&language, root, &control)
                                .expect("bounded coverage");
                            saw_witness |= a.witness_pattern_id.is_some();
                            assert_eq!(a.covers_any_pattern, b.covers_any_pattern);
                            assert_eq!(a.witness_pattern_id, b.witness_pattern_id);
                            assert_eq!(a.covered_pattern_count, b.covered_pattern_count);
                            assert_eq!(
                                reference.materialize_root(a.root).expect("full coverage"),
                                treatment.materialize_root(b.root).expect("full coverage")
                            );
                            reference.merge_global(a.root).expect("reference union");
                            treatment.merge_global(b.root).expect("bounded union");
                            assert!(treatment.union_memo.len() <= limit);
                        }
                    }
                    reference
                        .flush_and_recycle_local_cache()
                        .expect("reference recycle");
                    treatment
                        .flush_and_recycle_local_cache()
                        .expect("bounded recycle");
                    assert_eq!(treatment.union_memo.len(), 0);
                    assert_eq!(
                        reference
                            .materialize_global()
                            .expect("reference committed union"),
                        treatment
                            .materialize_global()
                            .expect("bounded committed union")
                    );
                }
                assert!(saw_witness);
            }
        }
    }

    #[test]
    fn contained_language_union_reuses_an_existing_node_without_an_interner() {
        let mut coverage = coverage_for_len(2, false);
        let mut broader_children = [REJECT; 7];
        broader_children[0] = ACCEPT;
        broader_children[1] = ACCEPT;
        let broader = coverage
            .intern_node(0, FULL_STANDARD_BAG, broader_children)
            .expect("two permitted prefixes");
        let mut narrower_children = [REJECT; 7];
        narrower_children[0] = ACCEPT;
        let narrower = coverage
            .intern_node(0, FULL_STANDARD_BAG, narrower_children)
            .expect("one permitted prefix");
        let mut disjoint_children = [REJECT; 7];
        disjoint_children[2] = ACCEPT;
        let disjoint = coverage
            .intern_node(0, FULL_STANDARD_BAG, disjoint_children)
            .expect("different permitted prefix");
        assert_ne!(broader, narrower);
        let original_len = coverage.nodes.len();
        coverage.interning_disabled = true;

        assert_eq!(coverage.union(broader, narrower).unwrap(), broader);
        assert_eq!(coverage.union(narrower, broader).unwrap(), broader);
        assert_eq!(coverage.nodes.len(), original_len);

        let combined = coverage.union(broader, disjoint).unwrap();
        assert_eq!(coverage.nodes.len(), original_len + 1);
        assert_eq!(
            coverage.nodes[reference_index(combined)].children,
            [ACCEPT, ACCEPT, ACCEPT, REJECT, REJECT, REJECT, REJECT]
        );
        assert_eq!(coverage.union(narrower, combined).unwrap(), combined);
        assert_eq!(coverage.nodes.len(), original_len + 1);
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn terminal_bag_coverage_borrow_matches_materialized_union() {
        use super::super::parallel_coverage::SharedCoverage;

        let (_, initial) = request_fixture(4);
        let universe = PatternUniverseMaterializer::standard_7_bag(4, 0, 0x77)
            .expect("full four-draw standard bag");
        assert_eq!(universe.pattern_count(), 840);
        let mut coverage = StandardBagCoverage::for_universe(&universe, initial, false, false)
            .expect("bag coverage")
            .expect("supported universe");
        let shared = SharedCoverage::new(840);
        let (count, words) = coverage.flushed_global_words().expect("exact empty union");
        assert!(words.is_none());
        shared
            .union_original_words(count, words)
            .expect("empty union");
        assert!(shared.to_bitset().expect("valid shape").is_empty());

        let first_piece_root = |coverage: &mut StandardBagCoverage, piece| {
            let mut children = [REJECT; 7];
            children[usize::from(piece_code(piece) - 1)] = ACCEPT;
            coverage
                .intern_node(0, FULL_STANDARD_BAG, children)
                .expect("first-piece language")
        };
        let first = first_piece_root(&mut coverage, PieceKind::I);
        coverage.merge_global(first).expect("first symbolic epoch");
        // Old caller owns a bitset copy; the second epoch remains pending
        // until the new terminal borrow explicitly flushes it.
        let mut reference = coverage
            .materialize_global()
            .expect("old materialized copy");
        let second = first_piece_root(&mut coverage, PieceKind::T);
        reference
            .union_with(
                &coverage
                    .materialize_root(second)
                    .expect("materialized second root"),
            )
            .expect("same universe");
        coverage
            .merge_global(second)
            .expect("second symbolic epoch");
        let (count, words) = coverage.flushed_global_words().expect("terminal borrow");
        shared
            .union_original_words(count, words)
            .expect("borrowed union");
        let actual = shared.to_bitset().expect("valid shape");
        assert_eq!(actual, reference);
        for ordinal in 0..840 {
            let present = matches!(
                universe.sequence_at(ordinal)[0],
                PieceKind::I | PieceKind::T
            );
            assert_eq!(
                actual.word_at(ordinal / 64) & (1u64 << (ordinal % 64)) != 0,
                present
            );
        }

        coverage.global_words.truncate(1);
        assert!(matches!(
            coverage.flushed_global_words(),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_coverage_shape_mismatch"
            ))
        ));
    }

    #[test]
    fn occupied_same_kind_hold_visits_one_boolean_continuation_per_draw() {
        use clearra_core_domain::solution::normalized_tiling_solution::{
            NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity,
        };
        use clearra_pc_graph::request::{
            PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
            PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
        };
        use clearra_problem::ProblemCompiler;
        use PieceKind::O;

        let placements = [
            PiecePlacementMask::new(O, 0x0c03),
            PiecePlacementMask::new(O, 0x300c),
        ];
        let board = 0x0f_ffff & !(placements[0].cells_mask() | placements[1].cells_mask());
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, board),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(2),
        )
        .with_exact_pieces(Some(2))
        .with_supply_window_size(SupplyWindowSize::new(2))
        .with_allow_hold(true)
        .with_hold_piece(Some(O))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false)
                .with_workers(1)
                .with_exact_legal_board_enabled(false)
                .with_conditioned_reachability_enabled(false)
                .with_tablebase_requested(false)
                .with_max_memory_mib(Some(256)),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("two O placements");
        let universe = problem.piece_source().materialized_universe().unwrap();
        assert_eq!(universe.pattern_count(), 42);
        assert!(!problem.supply().projects_unplaced_lookahead());

        // For this fixed OO word, explicitly count Current and SwapHeld as
        // distinct even when both produce the same piece and leave O in hold.
        let mut expected = 0_u64;
        let mut histories = 0;
        for ordinal in 0..universe.pattern_count() {
            let queue = universe.sequence_at(ordinal);
            let mut ways = 0;
            for (placed, held_after) in [(queue[0], O), (O, queue[0])] {
                if placed == O {
                    ways += usize::from(queue[1] == O) + usize::from(held_after == O);
                }
            }
            if ways != 0 {
                expected |= 1_u64 << ordinal;
                histories += ways;
            }
        }
        assert_eq!((expected.count_ones(), histories), (12, 18));

        let mut languages = PieceOrderLanguageCache::default();
        let word = languages
            .compile_poset_language(&[O, O], &[0, 1], 4)
            .unwrap()
            .unwrap();
        let mut coverage = StandardBagCoverage::for_universe(
            universe,
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead(),
        )
        .unwrap()
        .unwrap();
        let control = ExecutionControl::default();
        let result = coverage
            .cover_language(&languages, word.root, &control)
            .unwrap();
        assert_eq!(result.covered_pattern_count, expected.count_ones() as usize);
        assert_eq!(result.witness_pattern_id, Some(expected.trailing_zeros()));
        let row = coverage.materialize_root(result.root).unwrap();
        assert_eq!(row.to_owned_words(), vec![expected]);
        // Seven first draws plus one distinct second edge per accepted queue.
        assert!(result.edge_checks <= STANDARD_PIECE_COUNT + result.covered_pattern_count);

        let product =
            crate::WasmCpuSearchBackend::execute_with_control(&problem, &control).unwrap();
        let identity = StandardBoard64TilingIdentity::from_placements(board, placements).unwrap();
        let entry = product
            .solution_coverages()
            .iter()
            .find(|entry| entry.identity() == identity)
            .expect("the two-O tiling has a public Include row");
        assert_eq!(entry.covered_patterns(), &row);
        let key = NormalizedTilingSolutionKey::from_standard_board64_identity(identity);
        let report = product
            .solution_probabilities()
            .iter()
            .find(|report| report.solution_key() == key.as_str())
            .expect("probability belongs to the same tiling");
        assert!(report.probability_complete());
        assert_eq!(
            (report.covered_pattern_count(), report.pattern_count()),
            (12, 42)
        );
        assert!((report.probability().parse::<f64>().unwrap() - 12.0 / 42.0).abs() < 1e-14);
    }

    fn full_p7p4_capture_fixture(
        hold_enabled: bool,
    ) -> (MaterializedPatternUniverse, StandardBagCoverage) {
        let (_, initial) = request_fixture(11);
        let universe = PatternUniverseMaterializer::standard_7_bag(11, 0, 0x77)
            .expect("full standard P7P4 universe");
        assert_eq!(universe.pattern_count(), P7P4_PREFIXES * P7P4_TAIL_BITS);
        let coverage = StandardBagCoverage::for_universe(&universe, initial, hold_enabled, false)
            .expect("supported standard bag")
            .expect("standard decision source");
        (universe, coverage)
    }

    fn p7p4_capture_path(
        coverage: &mut StandardBagCoverage,
        start_depth: u8,
        pieces: &[PieceKind],
        end: u32,
    ) -> u32 {
        let mut source = SourceState {
            depth: start_depth,
            bag_remainder: FULL_STANDARD_BAG,
        };
        let mut path = Vec::new();
        for &piece in pieces {
            let source_now = source.normalized();
            let index = usize::from(piece_code(piece) - 1);
            path.push((source_now, index));
            source = coverage
                .advance_source(source_now, index)
                .expect("valid draw");
        }
        path.into_iter().rev().fold(end, |child, (source, index)| {
            let mut children = [REJECT; 7];
            children[index] = child;
            coverage
                .intern_node(source.depth, source.bag_remainder, children)
                .expect("valid decision node")
        })
    }

    fn unpack_p7p4_capture(blocks: &P7P4CoverageBlocks) -> Vec<u64> {
        // Independent conversion through ordinal offsets, intentionally
        // (unlike the exporter) crossing the 840/64-bit row alignment.
        let mut words = vec![0; (P7P4_PREFIXES * P7P4_TAIL_BITS) / 64];
        let mut ordinal = 0;
        for &(index, repeat) in blocks.runs() {
            let mask = &blocks.tails()[usize::from(index)];
            for _ in 0..repeat {
                for suffix in 0..P7P4_TAIL_BITS {
                    if mask[suffix / 8] & (1_u8 << (suffix % 8)) != 0 {
                        let offset = ordinal + suffix;
                        words[offset / 64] |= 1_u64 << (offset % 64);
                    }
                }
                ordinal += P7P4_TAIL_BITS;
            }
        }
        assert_eq!(ordinal, P7P4_PREFIXES * P7P4_TAIL_BITS);
        words
    }

    fn permutations_in_literal_piece_order(len: usize) -> Vec<Vec<PieceKind>> {
        use PieceKind::{I, J, L, O, S, T, Z};
        fn visit(len: usize, prefix: &mut Vec<PieceKind>, out: &mut Vec<Vec<PieceKind>>) {
            if prefix.len() == len {
                out.push(prefix.clone());
                return;
            }
            for next in [I, O, T, S, Z, J, L] {
                if !prefix.contains(&next) {
                    prefix.push(next);
                    visit(len, prefix, out);
                    prefix.pop();
                }
            }
        }
        let mut out = Vec::new();
        visit(len, &mut Vec::new(), &mut out);
        out
    }

    #[test]
    fn direct_p7p4_capture_matches_dense_and_original_queue_ordinals() {
        use PieceKind::I;
        let (universe, mut coverage) = full_p7p4_capture_fixture(true);
        let prefixes = permutations_in_literal_piece_order(7);
        let tails = permutations_in_literal_piece_order(4);
        assert_eq!(prefixes.len(), P7P4_PREFIXES);
        assert_eq!(tails.len(), P7P4_TAIL_BITS);
        let selected_prefix = [719, 720, 839, 5_039];
        let selected_suffix = [0, 63, 64, 839];
        let mut tail = REJECT;
        for index in selected_suffix {
            let path = p7p4_capture_path(&mut coverage, 7, &tails[index], ACCEPT);
            tail = coverage.union(tail, path).expect("exact tail union");
        }
        let mut root = p7p4_capture_path(&mut coverage, 0, &[I], ACCEPT);
        for index in selected_prefix {
            let path = p7p4_capture_path(&mut coverage, 0, &prefixes[index], tail);
            root = coverage.union(root, path).expect("exact prefix union");
        }
        let blocks = coverage
            .capture_p7p4_root(root)
            .expect("direct projection")
            .expect("eligible full fresh standard bag");
        assert!(Arc::ptr_eq(
            &blocks,
            &coverage
                .capture_p7p4_root(root)
                .expect("same arena")
                .expect("eligible")
        ));
        let words = unpack_p7p4_capture(&blocks);
        assert_eq!(
            words,
            coverage
                .materialize_root(root)
                .expect("dense reference")
                .to_owned_words()
        );
        assert_eq!(blocks.covered_count(), 720 * 840 + 3 * 4);
        assert_eq!(coverage.p7p4_capture.tail_masks.len(), 1);
        for prefix in [0, 719, 720, 839, 840, 5039] {
            for suffix in 0..P7P4_TAIL_BITS {
                let mut queue = prefixes[prefix].clone();
                queue.extend_from_slice(&tails[suffix]);
                let ordinal = prefix * P7P4_TAIL_BITS + suffix;
                assert_eq!(universe.sequence_at(ordinal).as_ref(), queue);
                let expected = queue[0] == I
                    || (selected_prefix.contains(&prefix) && selected_suffix.contains(&suffix));
                assert_eq!(
                    words[ordinal / 64] >> (ordinal % 64) & 1 != 0,
                    expected,
                    "{ordinal}"
                );
            }
        }
    }

    #[test]
    fn direct_p7p4_capture_observes_hold_product_and_arena_epoch() {
        use clearra_core_domain::pc::pc_target::PcTarget;
        use clearra_objectives::policy::objective_policy::ObjectivePolicy;
        use clearra_pc_graph::request::{OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput};
        use clearra_problem::ProblemCompiler;
        use clearra_rules::profile::builtin_rules::jstris_180;
        use PieceKind::{I, J, L, O, S, T, Z};

        let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
            .with_rule(jstris_180())
            .with_queue(PcQueueInput::standard_7_bag())
            .with_hold_policy(PcHoldPolicy::EnabledEmpty)
            .with_objective(ObjectivePolicy::unique());
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("product P7P4");
        let universe = problem
            .piece_source()
            .materialized_universe()
            .expect("native queue universe");
        assert!(problem.supply().hold_enabled());
        // 11 actual draws are available for ten locks. Projection of an
        // extra terminal lookahead only applies if the source ends at ten.
        assert!(!problem.supply().projects_unplaced_lookahead());
        let mut coverage = StandardBagCoverage::for_universe(
            universe,
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead(),
        )
        .expect("standard decision source")
        .expect("eligible P7P4");

        // Prove ordinal zero is accepted independently of the symbolic
        // coverage solver: stash the first I, run the queue normally, then
        // release the held I on the fourth second-bag draw (S).
        let queue = universe.sequence_at(0);
        assert_eq!(queue.as_ref(), &[I, O, T, S, Z, J, L, I, O, T, S]);
        let placements = [O, T, S, Z, J, L, I, O, T, I];
        let automaton = canonical_automaton();
        let mut state = automaton
            .project_bag_initial_state(problem.initial_hold(), true, placements.len())
            .expect("empty hold on fresh canonical bag");
        for (index, &piece) in placements.iter().enumerate() {
            let branch = match index {
                0 => SupplyBranchKind::StoreCurrent,
                9 => SupplyBranchKind::SwapHeld,
                _ => SupplyBranchKind::Current,
            };
            let cursor = usize::from(state.cursor());
            let next_piece = (branch == SupplyBranchKind::StoreCurrent).then(|| queue[cursor + 1]);
            let mut witnessed = None;
            automaton
                .for_each_matching_bag_step(state, piece, |step| {
                    if step.evidence.branch_kind == branch
                        && step.evidence.queue_current_piece == queue[cursor]
                        && step.evidence.queue_next_piece == next_piece
                    {
                        witnessed = Some(step.next_state);
                    }
                })
                .expect("canonical bag branches");
            state = witnessed.expect("concrete store/current/swap witness");
        }
        assert_eq!(usize::from(state.cursor()), queue.len());
        assert_eq!(state.hold_piece(), Some(S));

        let mut languages = PieceOrderLanguageCache::default();
        let mut predecessors = [0_u16; 10];
        for (index, predecessor) in predecessors.iter_mut().enumerate().skip(1) {
            *predecessor = 1 << (index - 1);
        }
        let lang = languages
            .compile_poset_language(&placements, &predecessors, 128)
            .expect("valid matching language")
            .expect("bounded language");
        let result = coverage
            .cover_language(&languages, lang.root, &ExecutionControl::default())
            .expect("actual hold-enabled bag decision product");
        assert!(result.covers_any_pattern);
        let blocks = coverage
            .capture_p7p4_root(result.root)
            .expect("compact actual product")
            .expect("eligible");
        assert_eq!(
            blocks.covered_count() as usize,
            result.covered_pattern_count
        );
        let words = unpack_p7p4_capture(&blocks);
        assert_eq!(words[0] & 1, 1);
        assert_eq!(
            words,
            coverage
                .materialize_root(result.root)
                .expect("dense product")
                .to_owned_words()
        );

        let (_, mut coverage) = full_p7p4_capture_fixture(true);
        let old_root = p7p4_capture_path(&mut coverage, 0, &[I], ACCEPT);
        let old = coverage
            .capture_p7p4_root(old_root)
            .expect("old projection")
            .expect("eligible");
        coverage
            .flush_and_recycle_local_cache()
            .expect("clear decision arena");
        let new_root = p7p4_capture_path(&mut coverage, 0, &[O], ACCEPT);
        assert_eq!(old_root, new_root, "arena can reuse the numeric reference");
        let new = coverage
            .capture_p7p4_root(new_root)
            .expect("new projection")
            .expect("eligible");
        assert!(!Arc::ptr_eq(&old, &new));
        assert_ne!(old, new);
        assert_eq!(unpack_p7p4_capture(&old)[0], u64::MAX);
        assert_eq!(unpack_p7p4_capture(&new)[0], 0);
    }

    #[test]
    fn direct_p7p4_capture_rejects_nonmatching_scope_without_an_empty_proof() {
        let mut truncated = coverage_for_len(11, true);
        assert!(truncated
            .capture_p7p4_root(ACCEPT)
            .expect("clean fallback")
            .is_none());
        let (_, mut no_hold) = full_p7p4_capture_fixture(false);
        assert!(no_hold
            .capture_p7p4_root(ACCEPT)
            .expect("clean fallback")
            .is_none());
    }

    fn branching_piece_language(
        placed_len: usize,
        phase: usize,
    ) -> super::super::buildup::BuildOrderGraph {
        use super::super::buildup::{BuildOrderGraph, BuildOrderNodeSpec};
        use clearra_core_domain::piece::rotation::RotationState;

        let mut specs = Vec::new();
        let mut edges = Vec::new();
        for depth in 0..placed_len {
            specs.push(BuildOrderNodeSpec {
                edge_start: u32::try_from(edges.len()).expect("small edge index"),
                edge_count: 2,
                depth: u8::try_from(depth).expect("small depth"),
                accepting: false,
            });
            for offset in [0, 2] {
                edges.push(BuildOrderGraph::edge(
                    u32::try_from(depth + 1).expect("small node"),
                    u8::try_from(depth).expect("small operation"),
                    PieceKind::STANDARD_TETROMINOES
                        [(depth + phase + offset) % STANDARD_PIECE_COUNT],
                    RotationState::Zero,
                    0,
                    0,
                    0,
                ));
            }
        }
        specs.push(BuildOrderNodeSpec {
            edge_start: u32::try_from(edges.len()).expect("small terminal"),
            edge_count: 0,
            depth: u8::try_from(placed_len).expect("small terminal depth"),
            accepting: true,
        });
        BuildOrderGraph::from_topological_parts(specs, edges, 0, 0, true)
            .expect("bounded exact piece language graph")
    }

    #[test]
    fn v081_state_major_product_memo_failed_admission_recomputes_exact_coverage() {
        let (universe, initial) = request_fixture(3);
        let mut language = PieceOrderLanguageCache::default();
        let root = language
            .canonicalize(&branching_piece_language(3, 0))
            .expect("exact language");
        let control = ExecutionControl::default();
        let mut observed_nonempty = false;
        for hold_enabled in [false, true] {
            for projected in [false, true] {
                let mut reference =
                    StandardBagCoverage::for_universe(&universe, initial, hold_enabled, projected)
                        .expect("reference")
                        .expect("supported");
                let mut treatment =
                    StandardBagCoverage::for_universe(&universe, initial, hold_enabled, projected)
                        .expect("candidate")
                        .expect("supported");
                reference.product_memo = StandardBagProductMemo::new(
                    StandardBagProductMemoLayout::Flat,
                    ExactU64MemoStorage::Reference,
                    reference.tables.cursor_transitions.max_source_depth,
                );
                treatment.product_memo = StandardBagProductMemo::new(
                    StandardBagProductMemoLayout::StateMajor,
                    ExactU64MemoStorage::Reference,
                    treatment.tables.cursor_transitions.max_source_depth,
                );
                treatment.product_memo.fail_admissions_for_test(usize::MAX);
                for _ in 0..2 {
                    let a = reference
                        .cover_language(&language, root, &control)
                        .expect("reference coverage");
                    let b = treatment
                        .cover_language(&language, root, &control)
                        .expect("exact fallback coverage");
                    observed_nonempty |= a.covers_any_pattern;
                    assert_eq!(a.covers_any_pattern, b.covers_any_pattern);
                    assert_eq!(a.covered_pattern_count, b.covered_pattern_count);
                    assert_eq!(a.witness_pattern_id, b.witness_pattern_id);
                    assert_eq!(
                        reference
                            .materialize_root(a.root)
                            .expect("reference queues"),
                        treatment.materialize_root(b.root).expect("fallback queues")
                    );
                    reference.merge_global(a.root).expect("reference merge");
                    treatment.merge_global(b.root).expect("fallback merge");
                    assert_eq!(treatment.memo_accounting().product_entries, 0);
                }
                assert_eq!(
                    reference.materialize_global().expect("reference global"),
                    treatment.materialize_global().expect("fallback global")
                );
                assert_eq!(treatment.memo_accounting().product_directory_bytes, 0);
            }
        }
        assert!(
            observed_nonempty,
            "fallback fixture also checks nonempty exact results"
        );
    }

    #[test]
    fn v081_state_major_product_memo_matches_symbolic_counts_witness_materialization_and_recycle() {
        for sequence_len in [1_usize, 3, 7, 8] {
            let (universe, _) = request_fixture(sequence_len);
            for initial_held_piece in [None, Some(PieceKind::J)] {
                let initial = HoldAutomatonState::new(
                    PieceSourceId::new(0x77),
                    0,
                    initial_held_piece,
                    0,
                    BagState::fresh_standard_7_bag().packed_remainder_key(),
                    SupplyProvenanceId(0x77),
                );
                for hold_enabled in [false, true] {
                    if !hold_enabled && initial_held_piece.is_some() {
                        continue;
                    }
                    for projected in [false, true] {
                        let shared = SharedStandardBagRequest::default();
                        let mut reference = StandardBagCoverage::for_universe_with_shared(
                            &universe,
                            initial,
                            hold_enabled,
                            projected,
                            &shared,
                        )
                        .expect("reference construction")
                        .expect("supported request");
                        let mut treatment = StandardBagCoverage::for_universe_with_shared(
                            &universe,
                            initial,
                            hold_enabled,
                            projected,
                            &shared,
                        )
                        .expect("candidate construction")
                        .expect("supported request");
                        let max_source_depth = reference.tables.cursor_transitions.max_source_depth;
                        reference.product_memo = StandardBagProductMemo::new(
                            StandardBagProductMemoLayout::Flat,
                            ExactU64MemoStorage::Reference,
                            max_source_depth,
                        );
                        treatment.product_memo = StandardBagProductMemo::new(
                            StandardBagProductMemoLayout::StateMajor,
                            ExactU64MemoStorage::Reference,
                            max_source_depth,
                        );
                        reference.union_memo = ExactU64MemoMap::new(ExactU64MemoStorage::Reference);
                        treatment.union_memo = ExactU64MemoMap::new(ExactU64MemoStorage::Reference);
                        let mut language = PieceOrderLanguageCache::default();
                        let control = ExecutionControl::default();
                        for placed_len in [sequence_len.saturating_sub(1).max(1), sequence_len] {
                            for phase in [0, 1] {
                                let root = language
                                    .canonicalize(&branching_piece_language(placed_len, phase))
                                    .expect("canonical piece language");
                                for _ in 0..2 {
                                    let a = reference
                                        .cover_language(&language, root, &control)
                                        .expect("reference coverage");
                                    let b = treatment
                                        .cover_language(&language, root, &control)
                                        .expect("candidate coverage");
                                    assert_eq!(a.covers_any_pattern, b.covers_any_pattern);
                                    assert_eq!(a.witness_pattern_id, b.witness_pattern_id);
                                    assert_eq!(a.covered_pattern_count, b.covered_pattern_count);
                                    assert_eq!(a.product_states, b.product_states);
                                    assert_eq!(a.edge_checks, b.edge_checks);
                                    assert_eq!(
                                        reference
                                            .materialize_root(a.root)
                                            .expect("reference queue identities"),
                                        treatment
                                            .materialize_root(b.root)
                                            .expect("candidate queue identities")
                                    );
                                    reference.merge_global(a.root).expect("reference union");
                                    treatment.merge_global(b.root).expect("candidate union");
                                    // Layout factoring must not change the accepted epoch/recycle trigger.
                                    assert_eq!(
                                        reference.local_live_bytes(),
                                        treatment.local_live_bytes()
                                    );
                                }
                            }
                        }
                        assert_eq!(reference.root_cache_hits(), treatment.root_cache_hits());
                        assert_eq!(reference.root_cache_misses(), treatment.root_cache_misses());
                        assert_eq!(
                            reference.materialize_global().expect("reference global"),
                            treatment.materialize_global().expect("candidate global")
                        );
                        let accounting = treatment.memo_accounting();
                        assert_eq!(accounting.product_layout, "state-major");
                        assert_eq!(accounting.product_storage, "state-major");
                        assert_eq!(accounting.union_storage, "reference");
                        assert!(accounting.product_capacity >= accounting.product_entries);
                        assert!(accounting.union_capacity >= accounting.union_entries);
                        assert!(
                            accounting.product_allocated_rows >= accounting.product_active_rows
                        );
                        assert!(accounting.product_row_slots >= accounting.product_allocated_rows);
                        assert_eq!(
                            accounting.product_payload_bytes + accounting.union_payload_bytes,
                            treatment.memo_retained_payload_bytes()
                        );
                        assert!(
                            accounting
                                .product_payload_bytes
                                .saturating_add(accounting.union_payload_bytes)
                                .saturating_add(accounting.product_directory_bytes)
                                <= treatment.retained_bytes()
                        );
                        let directory_bytes = accounting.product_directory_bytes;
                        reference
                            .flush_and_recycle_local_cache()
                            .expect("reference recycle");
                        treatment
                            .flush_and_recycle_local_cache()
                            .expect("candidate recycle");
                        assert_eq!(treatment.memo_accounting().product_entries, 0);
                        assert_eq!(treatment.memo_accounting().product_active_rows, 0);
                        assert_eq!(
                            treatment.memo_accounting().product_directory_bytes,
                            directory_bytes
                        );
                        let root = language
                            .canonicalize(&branching_piece_language(sequence_len, 0))
                            .expect("live language after BDD recycle");
                        let a = reference
                            .cover_language(&language, root, &control)
                            .expect("reference new epoch");
                        let b = treatment
                            .cover_language(&language, root, &control)
                            .expect("candidate new epoch");
                        assert_eq!(
                            reference
                                .materialize_root(a.root)
                                .expect("reference new identities"),
                            treatment
                                .materialize_root(b.root)
                                .expect("candidate new identities")
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn v081_shared_standard_bag_tables_match_reference_transitions_counts_and_materialization() {
        for sequence_len in [1, 3, 7, 8, 10] {
            let (universe, initial) = request_fixture(sequence_len);
            for hold_enabled in [false, true] {
                for projected in [false, true] {
                    let shared = SharedStandardBagRequest::default();
                    assert_eq!(shared.retained_bytes(), 0);
                    let mut reference = StandardBagCoverage::for_universe(
                        &universe,
                        initial,
                        hold_enabled,
                        projected,
                    )
                    .expect("reference")
                    .expect("supported");
                    let mut treatment = StandardBagCoverage::for_universe_with_shared(
                        &universe,
                        initial,
                        hold_enabled,
                        projected,
                        &shared,
                    )
                    .expect("shared")
                    .expect("supported");
                    assert_eq!(
                        reference.tables.suffix_counts,
                        treatment.tables.suffix_counts
                    );
                    assert_eq!(reference.retained_bytes(), treatment.retained_bytes());
                    assert_eq!(
                        shared.retained_bytes(),
                        treatment.shared_request_retained_bytes()
                    );
                    for depth in 0..=u8::try_from(sequence_len + 1).expect("bounded depth") {
                        for bag_remainder in 1..=FULL_STANDARD_BAG {
                            let source = SourceState {
                                depth,
                                bag_remainder,
                            };
                            for piece in 0..STANDARD_PIECE_COUNT {
                                assert_eq!(
                                    reference.tables.cursor_transitions.advance(source, piece),
                                    treatment.tables.cursor_transitions.advance(source, piece),
                                );
                            }
                        }
                    }
                    let children = [ACCEPT, REJECT, ACCEPT, REJECT, REJECT, ACCEPT, REJECT];
                    let reference_root = reference
                        .intern_node(0, FULL_STANDARD_BAG, children)
                        .expect("reference exact root");
                    let treatment_root = treatment
                        .intern_node(0, FULL_STANDARD_BAG, children)
                        .expect("shared exact root");
                    assert_eq!(
                        reference
                            .materialize_root(reference_root)
                            .expect("reference rows"),
                        treatment
                            .materialize_root(treatment_root)
                            .expect("shared rows"),
                    );
                }
            }
        }
    }

    #[test]
    fn v081_shared_standard_bag_eleven_workers_share_tables_but_keep_mutable_memo_private() {
        let (universe, initial) = request_fixture(3);
        let universe = Arc::new(universe);
        let shared = Arc::new(SharedStandardBagRequest::default());
        let workers = (0..11_u32)
            .map(|rank| {
                let universe = Arc::clone(&universe);
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || {
                    let mut coverage = StandardBagCoverage::for_universe_with_shared(
                        &universe, initial, true, true, &shared,
                    )
                    .expect("request table")
                    .expect("standard bag");
                    coverage
                        .product_memo
                        .try_insert(
                            StandardBagProductMemoKey::new(37, 0, FULL_STANDARD_BAG, 0),
                            rank + 2,
                        )
                        .expect("private memo");
                    coverage
                })
            })
            .collect::<Vec<_>>();
        let mut results = workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .collect::<Vec<_>>();
        for (rank, coverage) in results.iter().enumerate() {
            assert!(Arc::ptr_eq(&results[0].tables, &coverage.tables));
            assert_eq!(
                coverage.product_memo.get(&StandardBagProductMemoKey::new(
                    37,
                    0,
                    FULL_STANDARD_BAG,
                    0
                )),
                Some(&(rank as u32 + 2))
            );
            assert_eq!(
                coverage.memo_storage_label(),
                results[0].memo_storage_label()
            );
        }
        let tables = Arc::downgrade(&results[0].tables);
        results[0].product_memo.clear();
        assert_eq!(
            results[1].product_memo.get(&StandardBagProductMemoKey::new(
                37,
                0,
                FULL_STANDARD_BAG,
                0
            )),
            Some(&3)
        );
        drop(shared);
        assert!(tables.upgrade().is_some(), "workers retain immutable lease");
        drop(results);
        assert!(
            tables.upgrade().is_none(),
            "request tables release with final lease"
        );
    }

    #[test]
    fn v081_shared_standard_bag_request_rejects_changed_supply_identity_and_keeps_unsupported_lazy()
    {
        let (universe, initial) = request_fixture(3);
        let shared = SharedStandardBagRequest::default();
        StandardBagCoverage::for_universe_with_shared(&universe, initial, true, true, &shared)
            .expect("first request")
            .expect("supported");
        for (hold, projected) in [(false, true), (true, false)] {
            let result = StandardBagCoverage::for_universe_with_shared(
                &universe, initial, hold, projected, &shared,
            );
            assert!(matches!(
                result,
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_standard_bag_shared_request_identity_mismatch"
                ))
            ));
        }
        let (other_universe, other_initial) = request_fixture(2);
        assert!(StandardBagCoverage::for_universe_with_shared(
            &other_universe,
            other_initial,
            true,
            true,
            &shared,
        )
        .is_err());
        let mut other_initial = initial;
        other_initial.provenance = SupplyProvenanceId(0x88);
        assert!(StandardBagCoverage::for_universe_with_shared(
            &universe,
            other_initial,
            true,
            true,
            &shared,
        )
        .is_err());
        let unsupported = SharedStandardBagRequest::default();
        let fixed = FixedSequence::new(vec![PieceKind::O]);
        let fixed = PatternUniverseMaterializer::fixed_sequence(&fixed, 0x88);
        assert!(StandardBagCoverage::for_universe_with_shared(
            &fixed,
            initial,
            true,
            true,
            &unsupported,
        )
        .expect("unsupported is normal fallback")
        .is_none());
        assert_eq!(unsupported.retained_bytes(), 0);
        assert!(
            StandardBagCoverage::for_universe_with_shared(
                &universe,
                initial,
                true,
                true,
                &unsupported,
            )
            .is_err(),
            "an unsupported snapshot must not silently cross requests"
        );
    }

    #[test]
    fn compact_standard_bag_boundary_round_trips_through_canonical_epoch_transition() {
        let coverage = coverage(true);
        let boundary = SourceState {
            depth: 7,
            bag_remainder: FULL_STANDARD_BAG,
        };
        let before = coverage.supply_state(boundary, 0).expect("boundary state");
        assert_eq!(before.bag_epoch, 0);
        assert_eq!(before.bag_remainder_key, 0);

        let next = coverage
            .advance_source(boundary, 0)
            .expect("first draw in next bag");
        assert_eq!(next.depth, 8);
        assert_eq!(next.bag_remainder, FULL_STANDARD_BAG & !1);
        let after = coverage.supply_state(next, 0).expect("next epoch state");
        assert_eq!(after.bag_epoch, 1);
    }

    #[test]
    fn compact_standard_bag_hold_branches_match_canonical_fieldwise() {
        let coverage = coverage(true);
        let source = SourceState {
            depth: 0,
            bag_remainder: FULL_STANDARD_BAG,
        };
        let swapped = coverage
            .matching_supply_step(
                source,
                piece_code(PieceKind::T),
                PieceKind::T,
                SupplyBranchKind::SwapHeld,
                PieceKind::I,
            )
            .expect("swap evaluation")
            .expect("swap branch");
        assert_eq!(swapped.next_source.depth, 1);
        assert_eq!(swapped.next_hold_code, piece_code(PieceKind::I));

        let stored = coverage
            .matching_supply_step(
                source,
                0,
                PieceKind::O,
                SupplyBranchKind::StoreCurrent,
                PieceKind::I,
            )
            .expect("store evaluation")
            .expect("store branch");
        assert_eq!(stored.next_source.depth, 2);
        assert_eq!(stored.next_hold_code, piece_code(PieceKind::I));
    }

    #[test]
    fn compact_standard_bag_disabled_hold_fails_closed() {
        let coverage = coverage(false);
        let source = SourceState {
            depth: 0,
            bag_remainder: FULL_STANDARD_BAG,
        };
        assert!(coverage
            .matching_supply_step(
                source,
                piece_code(PieceKind::T),
                PieceKind::T,
                SupplyBranchKind::SwapHeld,
                PieceKind::I,
            )
            .is_err());
    }

    #[test]
    fn compact_cursor_and_hold_projection_exhaustively_match_canonical_automaton() {
        let canonical = canonical_automaton();

        for hold_enabled in [false, true] {
            // Ten queue pieces are the representative 4L PC universe. The
            // extra compiled row exists solely for StoreCurrent's second draw
            // when its input cursor is exactly this boundary.
            let coverage = coverage_for_len(10, hold_enabled);
            for depth in 0..=coverage.sequence_len {
                for raw_bag_remainder in 0..=FULL_STANDARD_BAG {
                    let source = SourceState {
                        depth,
                        bag_remainder: raw_bag_remainder,
                    };
                    let source_is_reachable = standard_bag_source_is_reachable(source);

                    for piece_index in 0..STANDARD_PIECE_COUNT {
                        let fast = coverage
                            .tables
                            .cursor_transitions
                            .advance(source, piece_index);
                        if !source_is_reachable {
                            assert!(fast.is_err(), "depth={depth} mask={raw_bag_remainder:#x}");
                            continue;
                        }
                        let state = coverage.supply_state(source, 0).expect("reachable state");
                        let expected = canonical
                            .advance_bag_cursor(state, PieceKind::STANDARD_TETROMINOES[piece_index])
                            .expect("canonical cursor transition")
                            .map(canonical_source_state)
                            .transpose()
                            .expect("canonical compact projection");
                        assert_eq!(
                            fast.expect("fast cursor transition"),
                            expected,
                            "depth={depth} mask={raw_bag_remainder:#x} piece={piece_index}"
                        );
                    }

                    for hold_code in 0..=STANDARD_PIECE_COUNT as u8 {
                        let state = coverage.supply_state(source, hold_code);
                        for desired_piece in PieceKind::STANDARD_TETROMINOES {
                            let mut canonical_steps = Vec::new();
                            let canonical_result = state.clone().and_then(|state| {
                                canonical
                                    .for_each_matching_bag_step(state, desired_piece, |step| {
                                        canonical_steps.push(step);
                                    })
                                    .map_err(|_| {
                                        WasmExactSearchError::InvalidProblem(
                                            "test_canonical_supply_transition_invalid",
                                        )
                                    })
                            });

                            for branch_kind in [
                                SupplyBranchKind::Current,
                                SupplyBranchKind::SwapHeld,
                                SupplyBranchKind::StoreCurrent,
                            ] {
                                for current_piece in PieceKind::STANDARD_TETROMINOES {
                                    let fast = coverage.matching_supply_step(
                                        source,
                                        hold_code,
                                        desired_piece,
                                        branch_kind,
                                        current_piece,
                                    );
                                    if canonical_result.is_err() {
                                        assert!(
                                            fast.is_err(),
                                            "invalid state was accepted: hold={hold_enabled} depth={depth} mask={raw_bag_remainder:#x} hold_code={hold_code} desired={desired_piece:?} branch={branch_kind:?} current={current_piece:?}"
                                        );
                                        continue;
                                    }
                                    let mut matching =
                                        canonical_steps.iter().copied().filter(|step| {
                                            step.evidence.branch_kind == branch_kind
                                                && step.evidence.queue_current_piece
                                                    == current_piece
                                        });
                                    let expected = matching.next();
                                    assert!(
                                        matching.next().is_none(),
                                        "canonical transition key was not unique"
                                    );
                                    let fast = fast.expect("valid fast projection");
                                    assert_eq!(
                                        fast.is_some(),
                                        expected.is_some(),
                                        "hold={hold_enabled} depth={depth} mask={raw_bag_remainder:#x} hold_code={hold_code} desired={desired_piece:?} branch={branch_kind:?} current={current_piece:?}"
                                    );
                                    if let (Some(fast), Some(expected)) = (fast, expected) {
                                        assert_eq!(expected.used_piece, desired_piece);
                                        assert_eq!(
                                            coverage
                                                .supply_state(
                                                    fast.next_source,
                                                    fast.next_hold_code,
                                                )
                                                .expect("fast full-state reconstruction"),
                                            expected.next_state
                                        );
                                        assert_eq!(
                                            canonical_source_state(expected.next_state)
                                                .expect("canonical next compact projection"),
                                            fast.next_source
                                        );
                                        assert_eq!(
                                            expected.next_state.hold_piece.map_or(0, piece_code),
                                            fast.next_hold_code
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn compact_cursor_table_is_accounted_and_exclusive_to_standard_bag_universes() {
        let universe = PatternUniverseMaterializer::standard_7_bag(10, 1, 0x77)
            .expect("4L standard bag universe");
        let coverage = coverage_for_len(10, true);
        let expected_bytes =
            StandardBagCursorTransitions::checked_storage_len(11).expect("4L cursor table size");

        assert_eq!(expected_bytes, 10_752);
        assert_eq!(
            coverage.tables.cursor_transitions.retained_bytes(),
            expected_bytes
        );
        assert_eq!(
            StandardBagCoverage::checked_cursor_transition_retained_bytes(&universe),
            Some(expected_bytes as u128)
        );
        assert!(coverage.retained_bytes() >= expected_bytes);
        assert!(coverage.local_live_bytes() >= expected_bytes);

        let fixed = FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ]);
        let fixed_universe = PatternUniverseMaterializer::fixed_sequence(&fixed, 0x88);
        assert_eq!(
            StandardBagCoverage::checked_cursor_transition_retained_bytes(&fixed_universe),
            Some(0)
        );
    }

    #[test]
    fn compact_cursor_rejects_reachable_shape_beyond_compiled_query_scope() {
        let coverage = coverage_for_len(10, true);
        let out_of_scope = SourceState {
            depth: 12,
            bag_remainder: 0b000_0011,
        };
        assert!(standard_bag_source_is_reachable(out_of_scope));
        assert!(coverage
            .tables
            .cursor_transitions
            .advance(out_of_scope, 0)
            .is_err());
        assert!(coverage
            .matching_supply_step(
                out_of_scope,
                0,
                PieceKind::I,
                SupplyBranchKind::Current,
                PieceKind::I,
            )
            .is_err());
    }

    /// Diagnostic only: this compares the former per-edge canonical branch
    /// scan against the certified table projection over every reachable 4L
    /// state/hold branch that actually exists. Correctness is covered by the
    /// exhaustive non-ignored test above; wall-clock timing is never a gate.
    #[test]
    #[ignore]
    fn benchmark_four_line_matching_transition_throughput() {
        let coverage = coverage_for_len(10, true);
        let canonical = canonical_automaton();
        let mut queries = Vec::new();
        for depth in 0..=coverage.sequence_len {
            for bag_remainder in 1..=FULL_STANDARD_BAG {
                let source = SourceState {
                    depth,
                    bag_remainder,
                };
                if !standard_bag_source_is_reachable(source) {
                    continue;
                }
                for hold_code in 0..=STANDARD_PIECE_COUNT as u8 {
                    let state = coverage
                        .supply_state(source, hold_code)
                        .expect("valid state");
                    for desired_piece in PieceKind::STANDARD_TETROMINOES {
                        canonical
                            .for_each_matching_bag_step(state, desired_piece, |step| {
                                queries.push((
                                    source,
                                    hold_code,
                                    desired_piece,
                                    step.evidence.branch_kind,
                                    step.evidence.queue_current_piece,
                                ));
                            })
                            .expect("canonical query enumeration");
                    }
                }
            }
        }

        let repetitions = 40_u64;
        let check_count = queries.len() as u64 * repetitions;
        let canonical_started = Instant::now();
        for _ in 0..repetitions {
            for &(source, hold_code, desired_piece, branch_kind, current_piece) in &queries {
                let state = coverage
                    .supply_state(source, hold_code)
                    .expect("valid state");
                let mut matched = None;
                canonical
                    .for_each_matching_bag_step(state, desired_piece, |step| {
                        if step.evidence.branch_kind == branch_kind
                            && step.evidence.queue_current_piece == current_piece
                        {
                            matched = Some(step);
                        }
                    })
                    .expect("canonical branch scan");
                black_box(matched.expect("canonical matching branch"));
            }
        }
        let canonical_elapsed = canonical_started.elapsed();

        let compact_started = Instant::now();
        for _ in 0..repetitions {
            for &(source, hold_code, desired_piece, branch_kind, current_piece) in &queries {
                black_box(
                    coverage
                        .matching_supply_step(
                            source,
                            hold_code,
                            desired_piece,
                            branch_kind,
                            current_piece,
                        )
                        .expect("compact branch projection")
                        .expect("compact matching branch"),
                );
            }
        }
        let compact_elapsed = compact_started.elapsed();

        eprintln!(
            "4L Standard7Bag matching transitions: checks={check_count}, canonical={canonical_elapsed:?} ({:.0}/s), compact={compact_elapsed:?} ({:.0}/s), speedup={:.2}x",
            check_count as f64 / canonical_elapsed.as_secs_f64(),
            check_count as f64 / compact_elapsed.as_secs_f64(),
            canonical_elapsed.as_secs_f64() / compact_elapsed.as_secs_f64(),
        );
    }
}
// SRP rationale: this module has one behavior-level change reason: exact standard-bag language materialization and coverage projection.
