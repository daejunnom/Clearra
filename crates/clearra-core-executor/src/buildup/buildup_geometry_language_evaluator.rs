use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken, piece::piece_kind::PieceKind,
};
use clearra_core_ffi::BuildUpGeometryLanguage;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_supply::{hold_automaton::HoldAutomatonState, PatternPiecePositionIndex};

const HOLD_STATE_COUNT: usize = 8;
const EXTRA_DRAW_STATE_COUNT: usize = 2;
const CANCELLATION_POLL_MASK: u32 = 0xff;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GeometryLanguageEvaluationError {
    Cancelled,
    InvalidLanguage,
    StorageUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GeometryLanguageCoverageResult {
    coverage_bits: PatternBitSet,
}

impl GeometryLanguageCoverageResult {
    pub(crate) fn coverage_bits(&self) -> &PatternBitSet {
        &self.coverage_bits
    }

    pub(crate) fn into_coverage_bits(self) -> PatternBitSet {
        self.coverage_bits
    }

    pub(crate) fn covered_pattern_count(&self) -> usize {
        self.coverage_bits.count_ones() as usize
    }

    pub(crate) fn covered(&self) -> bool {
        !self.coverage_bits.is_empty()
    }
}

#[derive(Default)]
pub(crate) struct GeometryHoldLanguageEvaluator {
    product_masks: Vec<u64>,
    product_stamps: Vec<u32>,
    product_generation: u32,
    depth_queues: Vec<Vec<usize>>,
    coverage_words: Vec<u64>,
    cancellation_poll_counter: u32,
}

impl GeometryHoldLanguageEvaluator {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.product_masks
            .capacity()
            .saturating_mul(std::mem::size_of::<u64>())
            .saturating_add(
                self.product_stamps
                    .capacity()
                    .saturating_mul(std::mem::size_of::<u32>()),
            )
            .saturating_add(
                self.depth_queues
                    .iter()
                    .map(|queue| {
                        queue
                            .capacity()
                            .saturating_mul(std::mem::size_of::<usize>())
                    })
                    .sum::<usize>(),
            )
            .saturating_add(
                self.coverage_words
                    .capacity()
                    .saturating_mul(std::mem::size_of::<u64>()),
            )
    }

    pub(crate) fn evaluate_pattern_words(
        &mut self,
        language: &BuildUpGeometryLanguage,
        pattern_index: &PatternPiecePositionIndex,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<GeometryLanguageCoverageResult, GeometryLanguageEvaluationError> {
        if !language.complete()
            || language.nodes().is_empty()
            || language.root_node_index() >= language.nodes().len()
        {
            return Err(GeometryLanguageEvaluationError::InvalidLanguage);
        }
        if cancellation.is_cancelled() {
            return Err(GeometryLanguageEvaluationError::Cancelled);
        }
        let initial_hold_code = match (initial_hold.hold_empty(), initial_hold.hold_piece()) {
            (true, None) => 0,
            (false, Some(piece)) => piece_code(piece),
            _ => return Err(GeometryLanguageEvaluationError::InvalidLanguage),
        };
        let state_capacity = language
            .nodes()
            .len()
            .checked_mul(EXTRA_DRAW_STATE_COUNT)
            .and_then(|count| count.checked_mul(HOLD_STATE_COUNT))
            .ok_or(GeometryLanguageEvaluationError::StorageUnavailable)?;
        self.ensure_product_capacity(state_capacity, language)?;
        self.cancellation_poll_counter = 0;

        self.coverage_words.clear();
        if self.coverage_words.capacity() < pattern_index.word_count() {
            self.coverage_words
                .try_reserve_exact(pattern_index.word_count())
                .map_err(|_| GeometryLanguageEvaluationError::StorageUnavailable)?;
        }
        for word_index in 0..pattern_index.word_count() {
            self.cancellation_poll_counter = self.cancellation_poll_counter.wrapping_add(1);
            if self.cancellation_poll_counter & CANCELLATION_POLL_MASK == 0
                && cancellation.is_cancelled()
            {
                return Err(GeometryLanguageEvaluationError::Cancelled);
            }
            for queue in &mut self.depth_queues {
                queue.clear();
            }
            self.begin_product_generation();
            let root_bits = pattern_index.active_word(word_index);
            if root_bits == 0 {
                self.coverage_words.push(0);
                continue;
            }
            self.activate_product_state(
                language.root_node_index(),
                0,
                initial_hold_code,
                root_bits,
                language,
            )?;

            let mut covered_word = 0_u64;
            for depth in 0..self.depth_queues.len() {
                let mut cursor = 0usize;
                while cursor < self.depth_queues[depth].len() {
                    let state_index = self.depth_queues[depth][cursor];
                    cursor += 1;
                    let active = self.product_masks[state_index];
                    let (node_index, extra_draw, hold_code) = decode_state_index(state_index);
                    let node = *language
                        .nodes()
                        .get(node_index)
                        .ok_or(GeometryLanguageEvaluationError::InvalidLanguage)?;
                    if node.accepting() {
                        covered_word |= active;
                        continue;
                    }
                    let queue_position = initial_hold
                        .cursor()
                        .checked_add(node.depth() as u16)
                        .and_then(|position| position.checked_add(extra_draw as u16))
                        .map(usize::from)
                        .ok_or(GeometryLanguageEvaluationError::InvalidLanguage)?;
                    let edge_begin = node.first_edge();
                    let edge_end = edge_begin
                        .checked_add(node.edge_count())
                        .filter(|end| *end <= language.edges().len())
                        .ok_or(GeometryLanguageEvaluationError::InvalidLanguage)?;

                    for edge in &language.edges()[edge_begin..edge_end] {
                        let desired_piece = edge.piece();
                        if !(1..=7).contains(&desired_piece) {
                            return Err(GeometryLanguageEvaluationError::InvalidLanguage);
                        }
                        let desired_current = active
                            & pattern_index.piece_word(queue_position, desired_piece, word_index);
                        self.activate_product_state(
                            edge.child_node_index(),
                            extra_draw,
                            hold_code,
                            desired_current,
                            language,
                        )?;
                        if !hold_enabled {
                            continue;
                        }

                        if hold_code != 0 && desired_piece == hold_code {
                            for current_piece in 1..=7 {
                                let swap_bits = active
                                    & pattern_index.piece_word(
                                        queue_position,
                                        current_piece,
                                        word_index,
                                    );
                                self.activate_product_state(
                                    edge.child_node_index(),
                                    extra_draw,
                                    current_piece,
                                    swap_bits,
                                    language,
                                )?;
                            }
                        } else if hold_code == 0 && extra_draw == 0 {
                            let desired_next = pattern_index.piece_word(
                                queue_position.saturating_add(1),
                                desired_piece,
                                word_index,
                            );
                            if desired_next == 0 {
                                continue;
                            }
                            for current_piece in 1..=7 {
                                let store_bits = active
                                    & desired_next
                                    & pattern_index.piece_word(
                                        queue_position,
                                        current_piece,
                                        word_index,
                                    );
                                self.activate_product_state(
                                    edge.child_node_index(),
                                    1,
                                    current_piece,
                                    store_bits,
                                    language,
                                )?;
                            }
                        }
                    }
                }
            }
            self.coverage_words.push(covered_word);
        }
        let coverage_bits = pattern_index
            .expand_coverage_words(&self.coverage_words)
            .map_err(|_| GeometryLanguageEvaluationError::StorageUnavailable)?;
        Ok(GeometryLanguageCoverageResult { coverage_bits })
    }

    fn ensure_product_capacity(
        &mut self,
        state_capacity: usize,
        language: &BuildUpGeometryLanguage,
    ) -> Result<(), GeometryLanguageEvaluationError> {
        if self.product_masks.len() < state_capacity {
            let additional = state_capacity - self.product_masks.len();
            self.product_masks
                .try_reserve_exact(additional)
                .map_err(|_| GeometryLanguageEvaluationError::StorageUnavailable)?;
            self.product_stamps
                .try_reserve_exact(additional)
                .map_err(|_| GeometryLanguageEvaluationError::StorageUnavailable)?;
            self.product_masks.resize(state_capacity, 0);
            self.product_stamps.resize(state_capacity, 0);
        }
        let depth_count = language
            .nodes()
            .iter()
            .map(|node| node.depth())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(GeometryLanguageEvaluationError::StorageUnavailable)?;
        if self.depth_queues.len() < depth_count {
            self.depth_queues
                .try_reserve_exact(depth_count - self.depth_queues.len())
                .map_err(|_| GeometryLanguageEvaluationError::StorageUnavailable)?;
            self.depth_queues.resize_with(depth_count, Vec::new);
        }
        Ok(())
    }

    fn begin_product_generation(&mut self) {
        if self.product_generation == u32::MAX {
            self.product_stamps.fill(0);
            self.product_generation = 1;
        } else {
            self.product_generation += 1;
        }
    }

    fn activate_product_state(
        &mut self,
        node_index: usize,
        extra_draw: usize,
        hold_code: u8,
        bits: u64,
        language: &BuildUpGeometryLanguage,
    ) -> Result<(), GeometryLanguageEvaluationError> {
        if bits == 0 {
            return Ok(());
        }
        let node = language
            .nodes()
            .get(node_index)
            .ok_or(GeometryLanguageEvaluationError::InvalidLanguage)?;
        if extra_draw >= EXTRA_DRAW_STATE_COUNT || usize::from(hold_code) >= HOLD_STATE_COUNT {
            return Err(GeometryLanguageEvaluationError::InvalidLanguage);
        }
        let state_index = state_index(node_index, extra_draw, hold_code);
        if self.product_stamps[state_index] != self.product_generation {
            self.product_stamps[state_index] = self.product_generation;
            self.product_masks[state_index] = bits;
            self.depth_queues[node.depth()].push(state_index);
        } else {
            self.product_masks[state_index] |= bits;
        }
        Ok(())
    }
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
