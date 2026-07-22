use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_supply::{
    hold_automaton::HoldAutomatonState, pattern_universe::PatternPiecePositionIndex,
};

use super::{buildup::BuildOrderGraph, WasmExactSearchError};

const HOLD_STATE_COUNT: usize = 8;
const EXTRA_DRAW_STATE_COUNT: usize = 2;
const PATTERNS_PER_WORD: usize = u64::BITS as usize;
const CANCELLATION_POLL_MASK: u32 = 0xff;

pub(super) struct CoverageProductResult {
    pub coverage_bits: PatternBitSet,
    pub path_count: u128,
    pub count_complete: bool,
    pub processed_words: usize,
    pub active_states: usize,
    pub edge_checks: usize,
}

/// Reusable scratch for the exact BuildOrder graph x HoldAutomaton x pattern
/// product. One machine word represents 64 concrete queue patterns. Generation
/// tags avoid clearing the full state table for every word and candidate.
#[derive(Default)]
pub(super) struct CoverageProductEvaluator {
    product_masks: Vec<u64>,
    product_stamps: Vec<u32>,
    product_generation: u32,
    depth_queues: Vec<Vec<usize>>,
    coverage_words: Vec<u64>,
    path_counts: Vec<u128>,
    cancellation_poll_counter: u32,
}

impl CoverageProductEvaluator {
    pub fn evaluate(
        &mut self,
        graph: &BuildOrderGraph,
        pattern_index: &PatternPiecePositionIndex,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        count_paths: bool,
        stop_after_first_pattern: bool,
        control: &ExecutionControl,
    ) -> Result<CoverageProductResult, WasmExactSearchError> {
        if graph.nodes.is_empty() || graph.root as usize >= graph.nodes.len() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_build_order_language_invalid",
            ));
        }
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
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
        let state_capacity = graph
            .nodes
            .len()
            .checked_mul(EXTRA_DRAW_STATE_COUNT)
            .and_then(|count| count.checked_mul(HOLD_STATE_COUNT))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_coverage_product_state_overflow",
            ))?;
        self.ensure_capacity(state_capacity, graph.max_depth(), count_paths)?;
        self.coverage_words.clear();
        self.coverage_words
            .try_reserve(pattern_index.word_count())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_coverage_word_storage_unavailable")
            })?;
        self.cancellation_poll_counter = 0;
        let mut total_path_count = 0_u128;
        let mut count_complete = true;
        let mut active_states = 0usize;
        let mut edge_checks = 0usize;

        for word_index in 0..pattern_index.word_count() {
            self.poll_cancellation(control)?;
            for queue in &mut self.depth_queues {
                queue.clear();
            }
            self.begin_generation(count_paths);
            let root_bits = pattern_index.active_word(word_index);
            if root_bits == 0 {
                self.coverage_words.push(0);
                continue;
            }
            self.activate_root(
                graph.root as usize,
                initial_hold_code,
                root_bits,
                graph,
                count_paths,
            )?;

            let mut covered_word = 0_u64;
            for depth in 0..self.depth_queues.len() {
                let mut cursor = 0;
                while cursor < self.depth_queues[depth].len() {
                    self.poll_cancellation(control)?;
                    let state_index = self.depth_queues[depth][cursor];
                    cursor += 1;
                    let active = self.product_masks[state_index];
                    active_states = active_states.saturating_add(1);
                    let (node_index, extra_draw, hold_code) = decode_state_index(state_index);
                    let node =
                        graph
                            .nodes
                            .get(node_index)
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_coverage_product_node_out_of_range",
                            ))?;
                    if node.accepting() {
                        covered_word |= active;
                        if count_paths {
                            let source_base = state_index * PATTERNS_PER_WORD;
                            for bit in set_bits(active) {
                                let next = total_path_count
                                    .checked_add(self.path_counts[source_base + bit]);
                                total_path_count = next.unwrap_or(u128::MAX);
                                count_complete &= next.is_some();
                            }
                        }
                        continue;
                    }
                    if !node.live {
                        continue;
                    }
                    let queue_position = initial_hold
                        .cursor()
                        .checked_add(u16::from(node.depth))
                        .and_then(|position| position.checked_add(extra_draw as u16))
                        .map(usize::from)
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_coverage_queue_position_overflow",
                        ))?;

                    let language_edges = if count_paths {
                        graph.edges(node_index)
                    } else {
                        graph.piece_edges(node_index)
                    };
                    for edge in language_edges
                        .iter()
                        .filter(|edge| graph.nodes[edge.to as usize].live)
                    {
                        edge_checks = edge_checks.saturating_add(1);
                        let desired_piece = piece_code(edge.piece);
                        if projects_unplaced_lookahead
                            && hold_enabled
                            && hold_code == desired_piece
                            && queue_position == pattern_index.sequence_len()
                            && graph.nodes[edge.to as usize].accepting()
                        {
                            self.activate_transition(
                                edge.to as usize,
                                extra_draw,
                                hold_code,
                                active,
                                state_index,
                                graph,
                                count_paths,
                                &mut count_complete,
                            )?;
                        }
                        let use_current = active
                            & pattern_index.piece_word_with_projected_standard_bag_lookahead(
                                queue_position,
                                desired_piece,
                                word_index,
                                projects_unplaced_lookahead,
                            );
                        self.activate_transition(
                            edge.to as usize,
                            extra_draw,
                            hold_code,
                            use_current,
                            state_index,
                            graph,
                            count_paths,
                            &mut count_complete,
                        )?;
                        if !hold_enabled {
                            continue;
                        }

                        if hold_code != 0 && hold_code == desired_piece {
                            for current_piece in 1..=7 {
                                let swap_bits = active
                                    & pattern_index
                                        .piece_word_with_projected_standard_bag_lookahead(
                                            queue_position,
                                            current_piece,
                                            word_index,
                                            projects_unplaced_lookahead,
                                        );
                                self.activate_transition(
                                    edge.to as usize,
                                    extra_draw,
                                    current_piece,
                                    swap_bits,
                                    state_index,
                                    graph,
                                    count_paths,
                                    &mut count_complete,
                                )?;
                            }
                        } else if hold_code == 0 && extra_draw == 0 {
                            for current_piece in 1..=7 {
                                let store_bits = active
                                    & pattern_index
                                        .piece_word_with_projected_standard_bag_lookahead(
                                            queue_position.saturating_add(1),
                                            desired_piece,
                                            word_index,
                                            projects_unplaced_lookahead,
                                        )
                                    & pattern_index
                                        .piece_word_with_projected_standard_bag_lookahead(
                                            queue_position,
                                            current_piece,
                                            word_index,
                                            projects_unplaced_lookahead,
                                        );
                                self.activate_transition(
                                    edge.to as usize,
                                    1,
                                    current_piece,
                                    store_bits,
                                    state_index,
                                    graph,
                                    count_paths,
                                    &mut count_complete,
                                )?;
                            }
                        }
                    }
                }
            }
            self.coverage_words.push(covered_word);
            if stop_after_first_pattern && covered_word != 0 {
                self.coverage_words.resize(pattern_index.word_count(), 0);
                break;
            }
        }

        let coverage_bits = pattern_index
            .expand_coverage_words(&self.coverage_words)
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_coverage_expansion_failed"))?;
        Ok(CoverageProductResult {
            coverage_bits,
            path_count: total_path_count,
            count_complete,
            processed_words: pattern_index.word_count(),
            active_states,
            edge_checks,
        })
    }

    pub fn retained_bytes(&self) -> usize {
        self.product_masks.capacity() * core::mem::size_of::<u64>()
            + self.product_stamps.capacity() * core::mem::size_of::<u32>()
            + self
                .depth_queues
                .iter()
                .map(|queue| queue.capacity() * core::mem::size_of::<usize>())
                .sum::<usize>()
            + self.coverage_words.capacity() * core::mem::size_of::<u64>()
            + self.path_counts.capacity() * core::mem::size_of::<u128>()
    }

    pub fn local_coverage_words(&self) -> &[u64] {
        &self.coverage_words
    }

    fn ensure_capacity(
        &mut self,
        state_capacity: usize,
        max_depth: usize,
        count_paths: bool,
    ) -> Result<(), WasmExactSearchError> {
        if self.product_masks.len() < state_capacity {
            self.product_masks
                .try_reserve_exact(state_capacity - self.product_masks.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_coverage_product_storage_unavailable",
                    )
                })?;
            self.product_masks.resize(state_capacity, 0);
        }
        if self.product_stamps.len() < state_capacity {
            self.product_stamps
                .try_reserve_exact(state_capacity - self.product_stamps.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_coverage_product_storage_unavailable",
                    )
                })?;
            self.product_stamps.resize(state_capacity, 0);
        }
        if self.depth_queues.len() <= max_depth {
            self.depth_queues.resize_with(max_depth + 1, Vec::new);
        }
        if count_paths {
            let count_capacity = state_capacity.checked_mul(PATTERNS_PER_WORD).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_path_count_storage_overflow"),
            )?;
            if self.path_counts.len() < count_capacity {
                self.path_counts
                    .try_reserve_exact(count_capacity - self.path_counts.len())
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem("wasm_path_count_storage_unavailable")
                    })?;
                self.path_counts.resize(count_capacity, 0);
            }
        }
        Ok(())
    }

    fn begin_generation(&mut self, count_paths: bool) {
        if self.product_generation == u32::MAX {
            self.product_stamps.fill(0);
            self.product_generation = 1;
        } else {
            self.product_generation += 1;
        }
        if count_paths {
            // Counts are reset lazily when a state is activated for this word.
        }
    }

    fn activate_root(
        &mut self,
        node_index: usize,
        hold_code: u8,
        bits: u64,
        graph: &BuildOrderGraph,
        count_paths: bool,
    ) -> Result<(), WasmExactSearchError> {
        let state_index = self.activate_state(node_index, 0, hold_code, bits, graph)?;
        if count_paths {
            let base = state_index * PATTERNS_PER_WORD;
            self.path_counts[base..base + PATTERNS_PER_WORD].fill(0);
            for bit in set_bits(bits) {
                self.path_counts[base + bit] = 1;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn activate_transition(
        &mut self,
        node_index: usize,
        extra_draw: usize,
        hold_code: u8,
        bits: u64,
        source_state_index: usize,
        graph: &BuildOrderGraph,
        count_paths: bool,
        count_complete: &mut bool,
    ) -> Result<(), WasmExactSearchError> {
        if bits == 0 {
            return Ok(());
        }
        let state_index = state_index(node_index, extra_draw, hold_code);
        let new_state = self.product_stamps[state_index] != self.product_generation;
        let state_index = self.activate_state(node_index, extra_draw, hold_code, bits, graph)?;
        if !count_paths {
            return Ok(());
        }
        let destination_base = state_index * PATTERNS_PER_WORD;
        if new_state {
            self.path_counts[destination_base..destination_base + PATTERNS_PER_WORD].fill(0);
        }
        let source_base = source_state_index * PATTERNS_PER_WORD;
        for bit in set_bits(bits) {
            let next = self.path_counts[destination_base + bit]
                .checked_add(self.path_counts[source_base + bit]);
            self.path_counts[destination_base + bit] = next.unwrap_or(u128::MAX);
            *count_complete &= next.is_some();
        }
        Ok(())
    }

    fn activate_state(
        &mut self,
        node_index: usize,
        extra_draw: usize,
        hold_code: u8,
        bits: u64,
        graph: &BuildOrderGraph,
    ) -> Result<usize, WasmExactSearchError> {
        if bits == 0 {
            return Ok(state_index(node_index, extra_draw, hold_code));
        }
        let node = graph
            .nodes
            .get(node_index)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_coverage_product_node_out_of_range",
            ))?;
        if extra_draw >= EXTRA_DRAW_STATE_COUNT || usize::from(hold_code) >= HOLD_STATE_COUNT {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_coverage_product_state_invalid",
            ));
        }
        let state_index = state_index(node_index, extra_draw, hold_code);
        if self.product_stamps[state_index] != self.product_generation {
            self.product_stamps[state_index] = self.product_generation;
            self.product_masks[state_index] = 0;
            self.depth_queues[usize::from(node.depth)].push(state_index);
        }
        self.product_masks[state_index] |= bits;
        Ok(state_index)
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

fn set_bits(mut bits: u64) -> impl Iterator<Item = usize> {
    core::iter::from_fn(move || {
        if bits == 0 {
            return None;
        }
        let bit = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        Some(bit)
    })
}

const fn state_index(node_index: usize, extra_draw: usize, hold_code: u8) -> usize {
    ((node_index * EXTRA_DRAW_STATE_COUNT + extra_draw) * HOLD_STATE_COUNT) + hold_code as usize
}

const fn decode_state_index(state_index: usize) -> (usize, usize, u8) {
    let hold_code = (state_index % HOLD_STATE_COUNT) as u8;
    let remainder = state_index / HOLD_STATE_COUNT;
    let extra_draw = remainder % EXTRA_DRAW_STATE_COUNT;
    let node_index = remainder / EXTRA_DRAW_STATE_COUNT;
    (node_index, extra_draw, hold_code)
}

const fn piece_code(piece: PieceKind) -> u8 {
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
