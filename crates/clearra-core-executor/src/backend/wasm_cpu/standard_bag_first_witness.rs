//! Exact minimum original-source ordinals after this owner's coverage is full.
//!
//! This visitor consumes the already verified piece language. It never solves
//! a geometry family. Its memo replaces full coverage-root values in the same
//! owned map, after every old root consumer has finished at a candidate boundary.

use super::{
    product_key, reuse_supply_draws, CompactSupplyStep, ExactU64MemoMap, ExecutionControl,
    PieceKind, PieceOrderLanguageCache, ProductMemoMeaning, SourceState, StandardBagCoverage,
    SupplyBranchKind, WasmExactSearchError, FULL_STANDARD_BAG, REJECT,
};

// A supported materialized universe has at most u32::MAX entries, so its
// largest ordinal is at most u32::MAX - 1. The sentinel cannot be an answer.
const NO_WITNESS: u32 = u32::MAX;

#[cfg(test)]
#[path = "standard_bag_first_witness_tests.rs"]
pub(super) mod tests;

impl StandardBagCoverage {
    /// This is an owner-local capability. Float probabilities and another
    /// worker's unbound completion flag cannot authorize dropping these roots.
    pub const fn can_use_first_pattern_witness(&self) -> bool {
        self.global_is_complete() && self.materialized_pattern_count <= u32::MAX as usize
    }

    /// Cumulative logical work; callers preserve per-candidate accounting by
    /// taking the difference around either exact product query.
    pub const fn product_work_counts(&self) -> (usize, usize) {
        (self.product_state_count, self.edge_check_count)
    }

    /// Return the exact minimum original ordinal, or an exact empty result.
    ///
    /// The caller must have finished every per-candidate coverage/capture root
    /// consumer and keep the request's membership-only contract thereafter.
    /// Leaving full coverage is one-way; a full-root query requires a new
    /// owner. A Boolean-only query may later request an ordinal explicitly;
    /// the two membership modes never retain each other's memo allocation.
    /// Unsupported scopes are errors, never a negative membership proof.
    pub fn first_pattern_witness(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_root: u32,
        control: &ExecutionControl,
    ) -> Result<Option<u32>, WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if !self.can_use_first_pattern_witness() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_first_witness_scope_invalid",
            ));
        }
        self.enter_first_witness_mode()?;
        self.first_witness_calls = self.first_witness_calls.saturating_add(1);
        let source = SourceState {
            depth: 0,
            bag_remainder: FULL_STANDARD_BAG,
        };
        let key = product_key(language_root, source, self.initial_hold_code);
        if self.product_memo.contains_key(&key) {
            self.root_cache_hits = self.root_cache_hits.saturating_add(1);
        } else {
            self.root_cache_misses = self.root_cache_misses.saturating_add(1);
        }
        let ordinal = self.first_suffix_ordinal(
            language,
            language_root,
            source,
            self.initial_hold_code,
            control,
        )?;
        Ok((ordinal != NO_WITNESS).then_some(ordinal))
    }

    pub(super) fn require_coverage_root_mode(&self) -> Result<(), WasmExactSearchError> {
        if self.product_memo_meaning != ProductMemoMeaning::CoverageRoot {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_coverage_root_after_membership_transition",
            ));
        }
        Ok(())
    }

    fn enter_first_witness_mode(&mut self) -> Result<(), WasmExactSearchError> {
        if self.product_memo_meaning == ProductMemoMeaning::MinimumOriginalOrdinal {
            return Ok(());
        }
        self.release_coverage_product_storage()?;
        #[cfg(feature = "placement-dependency-pc")]
        self.boolean_membership.clear_and_release();
        self.product_memo_meaning = ProductMemoMeaning::MinimumOriginalOrdinal;
        self.first_witness_mode_transitions = self.first_witness_mode_transitions.saturating_add(1);
        Ok(())
    }

    pub(super) fn release_coverage_product_storage(&mut self) -> Result<(), WasmExactSearchError> {
        // Flush while root IDs still have their original meaning. Failure
        // leaves the full owner intact and never becomes an empty witness.
        self.flush_epoch_global()?;
        if self.global_covered_pattern_count != self.materialized_pattern_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_first_witness_coverage_incomplete",
            ));
        }
        self.release_coverage_product_allocations();
        Ok(())
    }

    #[cfg(feature = "placement-dependency-pc")]
    pub(super) fn release_coverage_product_storage_for_boolean(
        &mut self,
    ) -> Result<(), WasmExactSearchError> {
        // Preserve exactly this worker's partial evidence before retiring IDs.
        self.flush_epoch_global()?;
        let local_complete = self.global_covered_pattern_count == self.materialized_pattern_count;
        #[cfg(feature = "parallel")]
        let shared_complete = self.has_shared_boolean_saturation();
        #[cfg(not(feature = "parallel"))]
        let shared_complete = false;
        if !local_complete && !shared_complete {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_boolean_coverage_incomplete",
            ));
        }
        self.release_coverage_product_allocations();
        #[cfg(feature = "parallel")]
        if !local_complete {
            self.record_shared_retirement();
        }
        Ok(())
    }

    fn release_coverage_product_allocations(&mut self) {
        self.product_memo.clear_and_release();
        self.nodes = Vec::new();
        self.node_summaries = Vec::new();
        self.node_levels = Vec::new();
        self.node_valid_masks = Vec::new();
        self.bucket_heads = Vec::new();
        // Preserve the selected representation, with no live union allocation.
        self.union_memo = ExactU64MemoMap::new(self.union_memo.storage());
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            self.p7p4_capture = Default::default();
        }
        self.epoch_global_root = REJECT;
        self.interning_disabled = false;
    }

    /// Minimum relative ordinal, clipped to the original materialized prefix.
    /// Every caller adds a nonnegative original-prefix offset. A suffix at or
    /// beyond this bound cannot be a materialized original queue in any caller,
    /// so clipping needs neither a prefix-dependent memo key nor another map.
    fn first_witness_add_offset(&self, offset: u128, suffix: u32) -> u32 {
        if suffix == NO_WITNESS {
            return NO_WITNESS;
        }
        let ordinal = offset.saturating_add(u128::from(suffix));
        if ordinal < self.materialized_pattern_count as u128 {
            ordinal as u32
        } else {
            NO_WITNESS
        }
    }

    fn first_piece_suffix_ordinal(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_node: u32,
        piece_code: u8,
        next_source: SourceState,
        next_hold_code: u8,
        control: &ExecutionControl,
    ) -> Result<u32, WasmExactSearchError> {
        let edges = language.edges_for_piece(language_node, piece_code).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_language_node_out_of_range"),
        )?;
        let mut best = NO_WITNESS;
        for edge in edges.iter().copied() {
            self.edge_check_count = self.edge_check_count.saturating_add(1);
            best = best.min(self.first_suffix_ordinal(
                language,
                edge.child(),
                next_source,
                next_hold_code,
                control,
            )?);
            if best == 0 {
                break;
            }
        }
        Ok(best)
    }

    fn first_suffix_ordinal(
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
                self.first_witness_add_offset(0, 0)
            } else {
                NO_WITNESS
            });
        }
        if self.projects_unplaced_lookahead && source.depth == self.sequence_len {
            if !self.hold_enabled || hold_code == 0 {
                return Ok(NO_WITNESS);
            }
            if source.bag_remainder.count_ones() == 1 {
                // This determined final bag piece has no new original ordinal.
                let lookahead_index = source.bag_remainder.trailing_zeros() as usize;
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
                let mut best = self.first_piece_suffix_ordinal(
                    language,
                    language_node,
                    lookahead_index as u8 + 1,
                    current_step.next_source,
                    current_step.next_hold_code,
                    control,
                )?;
                if best != 0 {
                    let swap_step = if reuse_supply_draws() {
                        CompactSupplyStep {
                            next_source: current_step.next_source,
                            next_hold_code: lookahead_index as u8 + 1,
                        }
                    } else {
                        let held_piece =
                            PieceKind::STANDARD_TETROMINOES[usize::from(hold_code - 1)];
                        self.matching_supply_step(
                            source,
                            hold_code,
                            held_piece,
                            SupplyBranchKind::SwapHeld,
                            lookahead_piece,
                        )?
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_standard_bag_swap_transition_missing",
                        ))?
                    };
                    best = best.min(self.first_piece_suffix_ordinal(
                        language,
                        language_node,
                        hold_code,
                        swap_step.next_source,
                        swap_step.next_hold_code,
                        control,
                    )?);
                }
                return Ok(best);
            }
            // An unknown next-bag piece may only release the retained source
            // piece at the terminal lock; it may not become a placed identity.
            let held_edges = language.edges_for_piece(language_node, hold_code).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_piece_language_node_out_of_range"),
            )?;
            for edge in held_edges.iter().copied() {
                self.edge_check_count = self.edge_check_count.saturating_add(1);
                if language
                    .node(edge.child())
                    .is_some_and(|child| child.accepting)
                {
                    return Ok(self.first_witness_add_offset(0, 0));
                }
            }
            return Ok(NO_WITNESS);
        }
        if source.depth >= self.sequence_len
            || usize::from(node.remaining_depth)
                > source.remaining_output_capacity(
                    self.sequence_len,
                    hold_code,
                    self.projects_unplaced_lookahead,
                )
        {
            return Ok(NO_WITNESS);
        }
        let key = product_key(language_node, source, hold_code);
        if let Some(ordinal) = self.product_memo.get(&key).copied() {
            return Ok(ordinal);
        }
        self.product_state_count = self.product_state_count.saturating_add(1);
        let mut result = NO_WITNESS;
        let mut original_draw_offset = 0_u128;
        for current_index in 0..PieceKind::STANDARD_TETROMINOES.len() {
            if source.bag_remainder & (1_u8 << current_index) == 0 {
                continue;
            }
            if original_draw_offset >= self.materialized_pattern_count as u128 {
                break;
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
            let mut best = self.first_piece_suffix_ordinal(
                language,
                language_node,
                current_code,
                next_source,
                current_step.next_hold_code,
                control,
            )?;

            // Current and Swap with the same kind have the same Boolean
            // continuation. This visitor never counts execution multiplicity.
            if best != 0 && self.hold_enabled && hold_code != 0 && hold_code != current_code {
                let swap_step = if reuse_supply_draws() {
                    CompactSupplyStep {
                        next_source: current_step.next_source,
                        next_hold_code: current_code,
                    }
                } else {
                    let held_piece = PieceKind::STANDARD_TETROMINOES[usize::from(hold_code - 1)];
                    self.matching_supply_step(
                        source,
                        hold_code,
                        held_piece,
                        SupplyBranchKind::SwapHeld,
                        current_piece,
                    )?
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_standard_bag_swap_transition_missing",
                    ))?
                };
                best = best.min(self.first_piece_suffix_ordinal(
                    language,
                    language_node,
                    hold_code,
                    swap_step.next_source,
                    swap_step.next_hold_code,
                    control,
                )?);
            }

            if best != 0
                && self.hold_enabled
                && hold_code == 0
                && next_source.depth < self.sequence_len
            {
                let next_source = next_source.normalized();
                let mut second_draw_offset = 0_u128;
                for desired_index in 0..PieceKind::STANDARD_TETROMINOES.len() {
                    if next_source.bag_remainder & (1_u8 << desired_index) == 0 {
                        continue;
                    }
                    if second_draw_offset >= u128::from(best)
                        || second_draw_offset >= self.materialized_pattern_count as u128
                    {
                        break;
                    }
                    let store_step = if reuse_supply_draws() {
                        self.stored_step_after_current(next_source, current_code, desired_index)?
                    } else {
                        let desired_piece = PieceKind::STANDARD_TETROMINOES[desired_index];
                        self.matching_supply_step(
                            source,
                            hold_code,
                            desired_piece,
                            SupplyBranchKind::StoreCurrent,
                            current_piece,
                        )?
                        .ok_or(WasmExactSearchError::InvalidProblem(
                            "wasm_standard_bag_store_transition_missing",
                        ))?
                    };
                    let suffix = self.first_piece_suffix_ordinal(
                        language,
                        language_node,
                        desired_index as u8 + 1,
                        store_step.next_source,
                        store_step.next_hold_code,
                        control,
                    )?;
                    let stored = self.first_witness_add_offset(second_draw_offset, suffix);
                    if stored != NO_WITNESS {
                        // Later second draws have larger original ordinals.
                        best = best.min(stored);
                        break;
                    }
                    second_draw_offset = second_draw_offset
                        .saturating_add(self.suffix_count(store_step.next_source));
                }
            }

            if best != 0
                && self.hold_enabled
                && hold_code == 0
                && self.projects_unplaced_lookahead
                && next_source.depth == self.sequence_len
                && next_source.bag_remainder.count_ones() == 1
            {
                let lookahead_index = next_source.bag_remainder.trailing_zeros() as usize;
                let store_step = if reuse_supply_draws() {
                    self.stored_step_after_current(next_source, current_code, lookahead_index)?
                } else {
                    let lookahead_piece = PieceKind::STANDARD_TETROMINOES[lookahead_index];
                    self.matching_supply_step(
                        source,
                        hold_code,
                        lookahead_piece,
                        SupplyBranchKind::StoreCurrent,
                        current_piece,
                    )?
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "wasm_standard_bag_store_transition_missing",
                    ))?
                };
                best = best.min(self.first_piece_suffix_ordinal(
                    language,
                    language_node,
                    lookahead_index as u8 + 1,
                    store_step.next_source,
                    store_step.next_hold_code,
                    control,
                )?);
            }
            if best != NO_WITNESS {
                result = self.first_witness_add_offset(original_draw_offset, best);
                // Even if clipping rejected this result, every later original
                // draw block starts after this block's exact minimum.
                break;
            }
            original_draw_offset =
                original_draw_offset.saturating_add(self.suffix_count(next_source));
        }
        // Allocation refusal only loses reuse, never a successful witness.
        let _ = self.product_memo.try_insert(key, result);
        Ok(result)
    }
}
