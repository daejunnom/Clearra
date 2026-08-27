use std::{
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
};

use super::{mix_digest, piece_order_language::PieceOrderLanguageCache, WasmExactSearchError};
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

type ExactU64Map = HashMap<u64, u32, BuildHasherDefault<ExactU64Hasher>>;

/// Query-local hasher for already packed exact integer keys. Hash collisions
/// remain harmless because `HashMap` confirms the complete `u64` key.
#[derive(Default)]
struct ExactU64Hasher(u64);

impl Hasher for ExactU64Hasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut value = 0xcbf2_9ce4_8422_2325_u64;
        for byte in bytes {
            value ^= u64::from(*byte);
            value = value.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.0 = splitmix64(value);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = splitmix64(value);
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
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

#[derive(Clone, Copy, Debug)]
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

pub(super) struct StandardBagCoverageResult {
    pub root: u32,
    pub covers_any_pattern: bool,
    pub witness_pattern_id: Option<u32>,
    pub covered_pattern_count: usize,
    pub product_states: usize,
    pub edge_checks: usize,
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
    supply_automaton: SupplyExecutionAutomaton,
    supply_identity: SupplyExecutionState,
    suffix_counts: Vec<u128>,
    nodes: Vec<DecisionNode>,
    node_summaries: Vec<DecisionSummary>,
    node_levels: Vec<u8>,
    node_valid_masks: Vec<u8>,
    bucket_heads: Vec<u32>,
    product_memo: ExactU64Map,
    union_memo: ExactU64Map,
    epoch_global_root: u32,
    global_words: Vec<u64>,
    global_covered_pattern_count: usize,
    interning_disabled: bool,
    cancellation_poll_counter: u32,
    product_state_count: usize,
    edge_check_count: usize,
    root_cache_hits: usize,
    root_cache_misses: usize,
}

impl StandardBagCoverage {
    pub fn supports(
        universe: &MaterializedPatternUniverse,
        initial_hold: HoldAutomatonState,
    ) -> bool {
        matches!(
            universe.structure(),
            MaterializedPatternUniverseStructure::Standard7BagLexicographic { .. }
        ) && initial_hold.cursor() == 0
            && initial_hold.bag_epoch() == 0
            && initial_hold.bag_remainder_key()
                == BagState::fresh_standard_7_bag().packed_remainder_key()
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
        let MaterializedPatternUniverseStructure::Standard7BagLexicographic { sequence_len } =
            universe.structure()
        else {
            return Ok(None);
        };
        let sequence_len = u8::try_from(sequence_len).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_sequence_too_long")
        })?;
        if !Self::supports(universe, initial_hold) {
            return Ok(None);
        }
        let initial_hold_code = match (initial_hold.hold_empty(), initial_hold.hold_piece()) {
            (true, None) => 0,
            (false, Some(piece)) => piece_code(piece),
            _ => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_initial_hold_state_invalid",
                ));
            }
        };
        let suffix_counts = compile_suffix_counts(sequence_len)?;
        let supply_automaton = SupplyExecutionAutomaton::for_bag(&PieceKind::STANDARD_TETROMINOES)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_standard_bag_supply_automaton_invalid")
            })?;
        let mut supply_identity = initial_hold;
        supply_identity.source_kind = PieceSourceKind::BagUniverse;
        supply_identity.hold_policy = if hold_enabled {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        };
        Ok(Some(Self {
            sequence_len,
            materialized_pattern_count: universe.pattern_count(),
            hold_enabled,
            projects_unplaced_lookahead,
            initial_hold_code,
            supply_automaton,
            supply_identity,
            suffix_counts,
            nodes: Vec::new(),
            node_summaries: Vec::new(),
            node_levels: Vec::new(),
            node_valid_masks: Vec::new(),
            bucket_heads: Vec::new(),
            product_memo: ExactU64Map::default(),
            union_memo: ExactU64Map::default(),
            epoch_global_root: REJECT,
            global_words: Vec::new(),
            global_covered_pattern_count: 0,
            interning_disabled: false,
            cancellation_poll_counter: 0,
            product_state_count: 0,
            edge_check_count: 0,
            root_cache_hits: 0,
            root_cache_misses: 0,
        }))
    }

    pub fn cover_language(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_root: u32,
        control: &ExecutionControl,
    ) -> Result<StandardBagCoverageResult, WasmExactSearchError> {
        let state = SourceState {
            depth: 0,
            bag_remainder: FULL_STANDARD_BAG,
        };
        let key = product_key(
            language_root,
            SourceState {
                depth: 0,
                bag_remainder: FULL_STANDARD_BAG,
            },
            self.initial_hold_code,
        );
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
        let root_source = SourceState {
            depth: 0,
            bag_remainder: FULL_STANDARD_BAG,
        };
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
            covers_any_pattern: root != REJECT,
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

    pub fn materialize_root(&self, root: u32) -> Result<PatternBitSet, WasmExactSearchError> {
        let word_count = self.materialized_pattern_count.div_ceil(u64::BITS as usize);
        let mut words = Vec::new();
        words.try_reserve_exact(word_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_coverage_storage_unavailable")
        })?;
        words.resize(word_count, 0);
        self.accumulate_into(
            root,
            SourceState {
                depth: 0,
                bag_remainder: FULL_STANDARD_BAG,
            },
            0,
            self.materialized_pattern_count as u128,
            &mut words,
        )?;
        PatternBitSet::from_words(self.materialized_pattern_count, words).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_coverage_shape_mismatch")
        })
    }

    pub fn retained_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<DecisionNode>()
            + self.suffix_counts.capacity() * core::mem::size_of::<u128>()
            + self.node_summaries.capacity() * core::mem::size_of::<DecisionSummary>()
            + self.node_levels.capacity() * core::mem::size_of::<u8>()
            + self.node_valid_masks.capacity() * core::mem::size_of::<u8>()
            + self.bucket_heads.capacity() * core::mem::size_of::<u32>()
            + self.product_memo.capacity()
                * (core::mem::size_of::<u64>() + core::mem::size_of::<u32>())
            + self.union_memo.capacity()
                * (core::mem::size_of::<u64>() + core::mem::size_of::<u32>())
            + self.global_words.capacity() * core::mem::size_of::<u64>()
    }

    pub const fn root_cache_hits(&self) -> usize {
        self.root_cache_hits
    }

    pub const fn root_cache_misses(&self) -> usize {
        self.root_cache_misses
    }

    pub fn local_live_bytes(&self) -> usize {
        self.nodes.len() * core::mem::size_of::<DecisionNode>()
            + self.node_summaries.len() * core::mem::size_of::<DecisionSummary>()
            + self.node_levels.len() * core::mem::size_of::<u8>()
            + self.node_valid_masks.len() * core::mem::size_of::<u8>()
            + self.product_memo.len() * (core::mem::size_of::<u64>() + core::mem::size_of::<u32>())
            + self.union_memo.len() * (core::mem::size_of::<u64>() + core::mem::size_of::<u32>())
    }

    pub fn flush_and_recycle_local_cache(&mut self) -> Result<(), WasmExactSearchError> {
        self.flush_epoch_global()?;
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

    fn supply_state(
        &self,
        source: SourceState,
        hold_code: u8,
    ) -> Result<SupplyExecutionState, WasmExactSearchError> {
        let hold_piece = match hold_code {
            0 => None,
            1..=7 => Some(PieceKind::STANDARD_TETROMINOES[usize::from(hold_code - 1)]),
            _ => {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_standard_bag_hold_state_invalid",
                ))
            }
        };
        let source = source.normalized();
        Ok(SupplyExecutionState {
            cursor: u16::from(source.depth),
            hold_piece,
            hold_empty: hold_piece.is_none(),
            hold_policy: if self.hold_enabled {
                HoldPolicy::Allowed
            } else {
                HoldPolicy::Forbidden
            },
            bag_epoch: if source.depth == 0 {
                0
            } else {
                u16::from((source.depth - 1) / 7)
            },
            bag_remainder_key: if source.depth != 0
                && source.depth % PieceKind::STANDARD_TETROMINOES.len() as u8 == 0
                && source.bag_remainder == FULL_STANDARD_BAG
            {
                0
            } else {
                standard_bag_mask_key(source.bag_remainder)
            },
            ..self.supply_identity
        })
    }

    fn source_state(
        &self,
        state: SupplyExecutionState,
    ) -> Result<SourceState, WasmExactSearchError> {
        Ok(SourceState {
            depth: u8::try_from(state.cursor).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_standard_bag_cursor_overflow")
            })?,
            bag_remainder: standard_bag_key_mask(state.bag_remainder_key)?,
        })
    }

    fn advance_source(
        &self,
        source: SourceState,
        piece_index: usize,
    ) -> Result<SourceState, WasmExactSearchError> {
        let state = self.supply_state(source, 0)?;
        let next = self
            .supply_automaton
            .advance_bag_cursor(state, PieceKind::STANDARD_TETROMINOES[piece_index])
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_standard_bag_cursor_transition_invalid")
            })?
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_piece_missing",
            ))?;
        self.source_state(next)
    }

    fn matching_supply_step(
        &self,
        source: SourceState,
        hold_code: u8,
        desired_piece: PieceKind,
        branch_kind: SupplyBranchKind,
        current_piece: PieceKind,
    ) -> Result<Option<clearra_supply::SupplyExecutionStep>, WasmExactSearchError> {
        let state = self.supply_state(source, hold_code)?;
        let mut matched = None;
        self.supply_automaton
            .for_each_matching_bag_step(state, desired_piece, |step| {
                let drawn_current = match step.evidence.branch_kind {
                    SupplyBranchKind::Current => step.used_piece,
                    SupplyBranchKind::SwapHeld | SupplyBranchKind::StoreCurrent => {
                        step.next_state.hold_piece.unwrap_or(current_piece)
                    }
                };
                if step.evidence.branch_kind == branch_kind && drawn_current == current_piece {
                    matched = Some(step);
                }
            })
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_standard_bag_supply_transition_invalid")
            })?;
        Ok(matched)
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

            if source.bag_remainder.count_ones() == 1 {
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
                let current_source = self.source_state(current_step.next_state)?;
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
                        current_step.next_state.hold_piece.map_or(0, piece_code),
                        control,
                    )?;
                    result = self.union(result, next)?;
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
                let swap_source = self.source_state(swap_step.next_state)?;
                for edge in held_edges.iter().copied() {
                    self.edge_check_count = self.edge_check_count.saturating_add(1);
                    let next = self.solve(
                        language,
                        edge.child(),
                        swap_source,
                        swap_step.next_state.hold_piece.map_or(0, piece_code),
                        control,
                    )?;
                    result = self.union(result, next)?;
                }
            } else {
                // A fresh next bag has no concrete projected identity. It may
                // only trigger the final swap that releases the held source
                // piece; the unknown incoming piece can never be placed or
                // become part of another search state.
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
        for current_index in 0..7 {
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
            let next_source = self.source_state(current_step.next_state)?;
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
                    current_step.next_state.hold_piece.map_or(0, piece_code),
                    control,
                )?;
                branch = self.union(branch, next)?;
            }
            if self.hold_enabled && hold_code != 0 {
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
                let swap_source = self.source_state(swap_step.next_state)?;
                let held_edges = language.edges_for_piece(language_node, hold_code).ok_or(
                    WasmExactSearchError::InvalidProblem("wasm_piece_language_node_out_of_range"),
                )?;
                for edge in held_edges.iter().copied() {
                    self.edge_check_count = self.edge_check_count.saturating_add(1);
                    let next = self.solve(
                        language,
                        edge.child(),
                        swap_source,
                        swap_step.next_state.hold_piece.map_or(0, piece_code),
                        control,
                    )?;
                    branch = self.union(branch, next)?;
                }
            }

            if self.hold_enabled && hold_code == 0 && next_source.depth < self.sequence_len {
                let next_source = next_source.normalized();
                let mut stored_children = [REJECT; 7];
                for desired_index in 0..7 {
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
                        let after_next = self.source_state(store_step.next_state)?;
                        let next = self.solve(
                            language,
                            edge.child(),
                            after_next,
                            store_step.next_state.hold_piece.map_or(0, piece_code),
                            control,
                        )?;
                        stored_children[desired_index] =
                            self.union(stored_children[desired_index], next)?;
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
                let after_lookahead = self.source_state(store_step.next_state)?;
                for edge in lookahead_edges.iter().copied() {
                    self.edge_check_count = self.edge_check_count.saturating_add(1);
                    let next = self.solve(
                        language,
                        edge.child(),
                        after_lookahead,
                        store_step.next_state.hold_piece.map_or(0, piece_code),
                        control,
                    )?;
                    branch = self.union(branch, next)?;
                }
            }
            children[current_index] = branch;
        }

        let root = self.intern_node(source.depth, source.bag_remainder, children)?;
        if self.product_memo.try_reserve(1).is_ok() {
            self.product_memo.insert(key, root);
        }
        Ok(root)
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
        self.nodes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_node_storage_unavailable")
        })?;
        self.node_summaries.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_node_storage_unavailable")
        })?;
        self.node_levels.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_node_storage_unavailable")
        })?;
        self.node_valid_masks.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_standard_bag_node_storage_unavailable")
        })?;
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
        let root = self.intern_node(level, valid_mask, children)?;
        if self.union_memo.try_reserve(1).is_ok() {
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
        for piece_index in 0..7 {
            if source.bag_remainder & (1_u8 << piece_index) == 0 {
                continue;
            }
            let next = self.advance_source(source, piece_index)?;
            let child_summary = self.summary(children[piece_index], next)?;
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
        for piece_index in 0..7 {
            if source.bag_remainder & (1_u8 << piece_index) == 0 {
                continue;
            }
            let next = self.advance_source(source, piece_index)?;
            newly_covered = newly_covered.saturating_add(self.accumulate_global(
                children[piece_index],
                next,
                child_base,
                limit,
            )?);
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
        for piece_index in 0..7 {
            if source.bag_remainder & (1_u8 << piece_index) == 0 {
                continue;
            }
            let next = self.advance_source(source, piece_index)?;
            self.accumulate_into(children[piece_index], next, child_base, limit, words)?;
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
            SourceState {
                depth: 0,
                bag_remainder: FULL_STANDARD_BAG,
            },
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
        for piece_index in 0..7 {
            if source.bag_remainder & (1_u8 << piece_index) == 0 {
                continue;
            }
            let next = self.advance_source(source, piece_index)?;
            count = count.saturating_add(self.count_patterns(
                children[piece_index],
                next,
                child_base,
                limit,
            )?);
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
        self.suffix_counts[depth * 128 + remainder]
    }

    fn ensure_buckets(&mut self) {
        if !self.bucket_heads.is_empty() || self.interning_disabled {
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

const fn product_key(language_node: u32, source: SourceState, hold_code: u8) -> u64 {
    language_node as u64
        | (source.depth as u64) << 32
        | (source.bag_remainder as u64) << 40
        | (hold_code as u64) << 48
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
    use clearra_supply::{
        hold_automaton::SupplyProvenanceId, piece_source::PieceSourceId,
        PatternUniverseMaterializer,
    };

    use super::*;

    fn coverage(hold_enabled: bool) -> StandardBagCoverage {
        let universe =
            PatternUniverseMaterializer::standard_7_bag(8, 1, 0x77).expect("standard bag universe");
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
        assert_eq!(swapped.used_piece, PieceKind::T);
        assert_eq!(swapped.next_state.cursor, 1);
        assert_eq!(swapped.next_state.hold_piece, Some(PieceKind::I));

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
        assert_eq!(stored.used_piece, PieceKind::O);
        assert_eq!(stored.next_state.cursor, 2);
        assert_eq!(stored.next_state.hold_piece, Some(PieceKind::I));
        assert_eq!(stored.evidence.queue_advances, 2);
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
}
// SRP rationale: this module has one behavior-level change reason: exact standard-bag language materialization and coverage projection.
