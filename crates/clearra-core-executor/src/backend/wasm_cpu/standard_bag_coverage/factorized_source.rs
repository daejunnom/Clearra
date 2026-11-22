use clearra_supply::pattern_universe::{
    FactorizedPatternLayout, FactorizedPatternSegment, MaterializedPatternUniverse,
};

use super::{
    cursor_transition_index, piece_code, SourceState, StandardBagCursorTransitions,
    WasmExactSearchError, CURSOR_TRANSITION_UNAVAILABLE, FULL_STANDARD_BAG,
    STANDARD_BAG_MASK_COUNT, STANDARD_PIECE_COUNT,
};

/// Request-owned exact ordinal schedule. A segment ends after its declared
/// number of draws, including partial bags; unused choices are not carried to
/// the next segment. This is finite-source authority, never a fresh bag claim.
pub(super) struct FactorizedSourceLayout {
    full_sequence_len: usize,
    visible_sequence_len: usize,
    segments: Box<[FactorizedPatternSegment]>,
}

impl FactorizedSourceLayout {
    pub(super) fn supported_layout(
        universe: &MaterializedPatternUniverse,
    ) -> Option<FactorizedPatternLayout<'_>> {
        let layout = universe.complete_factorized_layout()?;
        (layout.full_sequence_len() == layout.visible_sequence_len()
            && layout.visible_sequence_len() < usize::from(u8::MAX)
            && layout.pattern_count() <= u32::MAX as usize)
            .then_some(layout)
    }

    pub(super) fn compile(
        universe: &MaterializedPatternUniverse,
    ) -> Result<Option<Self>, WasmExactSearchError> {
        let Some(layout) = Self::supported_layout(universe) else {
            return Ok(None);
        };
        let mut segments = Vec::new();
        segments
            .try_reserve_exact(layout.segment_count())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_factorized_source_layout_storage_unavailable",
                )
            })?;
        segments.extend(layout.segments());
        Ok(Some(Self {
            full_sequence_len: layout.full_sequence_len(),
            visible_sequence_len: layout.visible_sequence_len(),
            segments: segments.into_boxed_slice(),
        }))
    }

    pub(super) fn matches_universe(&self, universe: &MaterializedPatternUniverse) -> bool {
        Self::supported_layout(universe).is_some_and(|layout| {
            self.full_sequence_len == layout.full_sequence_len()
                && self.visible_sequence_len == layout.visible_sequence_len()
                && self.segments.iter().copied().eq(layout.segments())
        })
    }

    pub(super) fn sequence_len(&self) -> u8 {
        self.visible_sequence_len as u8
    }

    pub(super) fn initial_remainder(&self) -> u8 {
        self.segments[0].choice_mask()
    }

    pub(super) fn retained_bytes(&self) -> usize {
        core::mem::size_of_val(self.segments.as_ref())
    }

    pub(super) fn checked_retained_bytes(universe: &MaterializedPatternUniverse) -> Option<u128> {
        let Some(layout) = Self::supported_layout(universe) else {
            return Some(0);
        };
        (layout.segment_count() as u128)
            .checked_mul(core::mem::size_of::<FactorizedPatternSegment>() as u128)
    }

    fn segment_at(&self, depth: usize) -> Option<FactorizedPatternSegment> {
        self.segments.iter().copied().find(|segment| {
            depth >= segment.draw_start() && depth < segment.draw_start() + segment.draw_count()
        })
    }

    pub(super) fn source_piece_order(&self, depth: u8) -> [u8; STANDARD_PIECE_COUNT] {
        let mut order = [u8::MAX; STANDARD_PIECE_COUNT];
        if let Some(segment) = self.segment_at(usize::from(depth)) {
            for (slot, piece) in order.iter_mut().zip(segment.choices()) {
                *slot = piece_code(*piece) - 1;
            }
        }
        order
    }

    fn source_is_reachable(&self, source: SourceState) -> bool {
        let source = source.normalized();
        let depth = usize::from(source.depth);
        let (segment, used) = if depth == self.visible_sequence_len {
            let segment = *self.segments.last().expect("certified nonempty layout");
            (segment, segment.draw_count())
        } else {
            let Some(segment) = self.segment_at(depth) else {
                return false;
            };
            (segment, depth - segment.draw_start())
        };
        let remaining_count = segment.choices().len() - used;
        if remaining_count == 0 {
            return source.bag_remainder == FULL_STANDARD_BAG;
        }
        source.bag_remainder & !segment.choice_mask() == 0
            && source.bag_remainder.count_ones() as usize == remaining_count
    }

    fn advance(&self, source: SourceState, piece_index: usize) -> Option<SourceState> {
        let source = source.normalized();
        if !self.source_is_reachable(source)
            || source.depth >= self.sequence_len()
            || source.bag_remainder & (1 << piece_index) == 0
        {
            return None;
        }
        let segment = self.segment_at(usize::from(source.depth))?;
        let next_depth = source.depth + 1;
        let remaining = if usize::from(next_depth) == segment.draw_start() + segment.draw_count()
            && next_depth < self.sequence_len()
        {
            self.segment_at(usize::from(next_depth))?.choice_mask()
        } else {
            source.bag_remainder & !(1 << piece_index)
        };
        Some(
            SourceState {
                depth: next_depth,
                bag_remainder: remaining,
            }
            .normalized(),
        )
    }

    pub(super) fn compile_cursor(
        &self,
    ) -> Result<StandardBagCursorTransitions, WasmExactSearchError> {
        let max_source_depth = self.sequence_len();
        let len = StandardBagCursorTransitions::checked_storage_len(max_source_depth).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_factorized_cursor_table_size_overflow"),
        )?;
        let mut next_remainders = Vec::new();
        next_remainders.try_reserve_exact(len).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_factorized_cursor_table_storage_unavailable")
        })?;
        next_remainders.resize(len, CURSOR_TRANSITION_UNAVAILABLE);
        let row_count = usize::from(max_source_depth) + 1;
        let mut valid_remainders = Vec::new();
        valid_remainders.try_reserve_exact(row_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "wasm_factorized_cursor_validity_storage_unavailable",
            )
        })?;
        valid_remainders.resize(row_count, 0_u128);
        for depth in 0..=max_source_depth {
            for bag_remainder in 1..=FULL_STANDARD_BAG {
                let source = SourceState {
                    depth,
                    bag_remainder,
                };
                if !self.source_is_reachable(source) {
                    continue;
                }
                valid_remainders[usize::from(depth)] |= 1_u128 << bag_remainder;
                for piece_index in 0..STANDARD_PIECE_COUNT {
                    let Some(next) = self.advance(source, piece_index) else {
                        continue;
                    };
                    debug_assert!(self.source_is_reachable(next));
                    let index = cursor_transition_index(depth, bag_remainder, piece_index)
                        .expect("bounded factorized cursor tuple");
                    next_remainders[index] = next.bag_remainder;
                }
            }
        }
        Ok(StandardBagCursorTransitions {
            max_source_depth,
            next_remainders: next_remainders.into_boxed_slice(),
            factorized_valid_remainders: Some(valid_remainders.into_boxed_slice()),
        })
    }

    pub(super) fn compile_suffix_counts(
        &self,
        cursor: &StandardBagCursorTransitions,
    ) -> Result<Box<[u128]>, WasmExactSearchError> {
        let len = (self.visible_sequence_len + 1)
            .checked_mul(STANDARD_BAG_MASK_COUNT)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_factorized_suffix_table_overflow",
            ))?;
        let mut counts = Vec::new();
        counts.try_reserve_exact(len).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_factorized_suffix_table_storage_unavailable")
        })?;
        counts.resize(len, 0_u128);
        counts[self.visible_sequence_len * STANDARD_BAG_MASK_COUNT..].fill(1);
        for depth in (0..self.sequence_len()).rev() {
            for bag_remainder in 1..=FULL_STANDARD_BAG {
                let source = SourceState {
                    depth,
                    bag_remainder,
                };
                if !self.source_is_reachable(source) {
                    continue;
                }
                let mut count = 0_u128;
                for piece_index in self.source_piece_order(depth) {
                    if piece_index >= STANDARD_PIECE_COUNT as u8 {
                        continue;
                    }
                    if let Some(next) = cursor.advance(source, usize::from(piece_index))? {
                        count = count
                            .checked_add(
                                counts[usize::from(next.depth) * STANDARD_BAG_MASK_COUNT
                                    + usize::from(next.bag_remainder)],
                            )
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_factorized_suffix_count_overflow",
                            ))?;
                    }
                }
                counts[usize::from(depth) * STANDARD_BAG_MASK_COUNT + usize::from(bag_remainder)] =
                    count;
            }
        }
        Ok(counts.into_boxed_slice())
    }
}
