//! Boolean source-language membership after exact original-universe coverage.
//! Completion is owner-local or a separately validated same-request shared
//! proof; the latter never changes this worker's partial coverage evidence.
//!
//! This consumes the already verified placement language, never a geometry
//! family. A successful query stops at its first legal original queue and
//! returns no ordinal. Only an exhaustively rejected root is cached, against
//! this fixed request's initial source/Hold state and the exact language arena.

use std::sync::Arc;

use super::super::exact_collections::ExactHashSet;
use super::{
    ExecutionControl, PieceKind, PieceOrderLanguageCache, ProductMemoMeaning, SourceState,
    StandardBagCoverage, SupplyBranchKind, WasmExactSearchError, FULL_STANDARD_BAG,
};

#[cfg(test)]
#[path = "standard_bag_membership_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct BooleanMembershipCache {
    language_identity: Option<Arc<()>>,
    failed_roots: ExactHashSet<u32>,
    admission_disabled: bool,
    calls: usize,
    mode_transitions: usize,
    certified_calls: usize,
    negative_hits: usize,
}

impl BooleanMembershipCache {
    fn bind_language(&mut self, language: &PieceOrderLanguageCache) {
        if self
            .language_identity
            .as_ref()
            .is_some_and(|identity| Arc::ptr_eq(identity, language.arena_identity()))
        {
            return;
        }
        self.failed_roots.clear();
        self.admission_disabled = false;
        // One clone per arena binding, with no atomic operation on later hits.
        self.language_identity = Some(Arc::clone(language.arena_identity()));
    }

    fn insert_exhaustive_failure(&mut self, root: u32) {
        if self.admission_disabled {
            return;
        }
        if self.failed_roots.len() == self.failed_roots.capacity()
            && self.failed_roots.try_reserve(1).is_err()
        {
            // Allocation refusal only loses reuse. No failure to allocate can
            // authorize rejection or replace the exact result of this query.
            self.admission_disabled = true;
            return;
        }
        self.failed_roots.insert(root);
    }

    pub(super) fn clear_retain_capacity(&mut self) {
        self.failed_roots.clear();
        self.language_identity = None;
        self.admission_disabled = false;
    }

    pub(super) fn clear_and_release(&mut self) {
        self.clear_retain_capacity();
        self.failed_roots = ExactHashSet::default();
    }

    pub(super) fn live_payload_bytes(&self) -> usize {
        self.failed_roots.len() * core::mem::size_of::<u32>()
    }

    pub(super) fn retained_payload_bytes(&self) -> usize {
        // As for the surrounding exact collections, this is logical payload
        // capacity, not allocator headers, hash control bytes, or an RSS claim.
        self.failed_roots.capacity() * core::mem::size_of::<u32>()
    }

    pub(super) fn detached_identity_retained_bytes(&self) -> usize {
        // The live language arena accounts for its shared identity. During an
        // out-of-band arena replacement this owner may temporarily retain the
        // old token alone; count that detached allocation here until rebinding.
        self.language_identity
            .as_ref()
            .filter(|identity| Arc::strong_count(identity) == 1)
            .map_or(0, |_| 2 * core::mem::size_of::<usize>())
    }

    #[cfg(any(feature = "parallel", test))]
    pub(super) fn accounting(&self) -> (usize, usize, usize, usize, usize, usize, usize) {
        (
            self.calls,
            self.mode_transitions,
            self.certified_calls,
            self.negative_hits,
            self.failed_roots.len(),
            self.failed_roots.capacity(),
            self.retained_payload_bytes(),
        )
    }
}

impl StandardBagCoverage {
    /// Shared completion authorizes only Boolean consumers. The minimum-
    /// ordinal capability and local completeness remain owner-local.
    pub fn can_use_boolean_pattern_membership(&self) -> bool {
        let complete = self.can_use_first_pattern_witness();
        #[cfg(feature = "parallel")]
        let complete = complete || self.has_shared_boolean_saturation();
        complete && self.materialized_pattern_count <= u32::MAX as usize
    }

    /// The product build uses the proved simplification directly. Only local
    /// A/B and test owners store an opt-out; no per-candidate environment read.
    pub const fn boolean_membership_enabled(&self) -> bool {
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            self.boolean_membership_enabled
        }
        #[cfg(not(any(test, feature = "local-search-ab")))]
        {
            true
        }
    }

    #[cfg(test)]
    pub fn set_boolean_membership_enabled_for_test(&mut self, enabled: bool) {
        self.boolean_membership_enabled = enabled;
    }

    /// Exact existence only. This API deliberately returns no representative
    /// ordinal or coverage root. The caller must exclude every consumer that
    /// needs those values, including representative trace materialization.
    pub fn contains_pattern_language(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_root: u32,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        self.begin_boolean_membership_query(language, language_root, control)?;
        if self
            .boolean_membership
            .failed_roots
            .contains(&language_root)
        {
            self.boolean_membership.negative_hits =
                self.boolean_membership.negative_hits.saturating_add(1);
            self.root_cache_hits = self.root_cache_hits.saturating_add(1);
            return Ok(false);
        }
        self.root_cache_misses = self.root_cache_misses.saturating_add(1);
        let found = self.contains_source_suffix(
            language,
            language_root,
            SourceState {
                depth: 0,
                bag_remainder: FULL_STANDARD_BAG,
            },
            self.initial_hold_code,
            0,
            control,
        )?;
        // Cancellation can arrive after the final periodic recursive poll.
        // No interrupted traversal becomes an exhaustive negative cache entry.
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if !found {
            self.boolean_membership
                .insert_exhaustive_failure(language_root);
        }
        Ok(found)
    }

    /// Consume the caller's same-request, same-language positive source proof.
    /// No positive or partial traversal is inserted into a coverage memo. The
    /// Boolean-only contract still retires the full roots at this boundary.
    pub fn accept_certified_pattern_language(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_root: u32,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        self.begin_boolean_membership_query(language, language_root, control)?;
        self.boolean_membership.certified_calls =
            self.boolean_membership.certified_calls.saturating_add(1);
        Ok(())
    }

    fn begin_boolean_membership_query(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_root: u32,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if !self.can_use_boolean_pattern_membership() {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_standard_bag_membership_scope_invalid",
            ));
        }
        language
            .node(language_root)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_node_out_of_range",
            ))?;
        self.enter_boolean_membership_mode()?;
        self.boolean_membership.bind_language(language);
        self.boolean_membership.calls = self.boolean_membership.calls.saturating_add(1);
        Ok(())
    }

    pub(super) fn enter_boolean_membership_mode(&mut self) -> Result<(), WasmExactSearchError> {
        if self.product_memo_meaning == ProductMemoMeaning::BooleanMembership {
            return Ok(());
        }
        self.release_coverage_product_storage_for_boolean()?;
        self.boolean_membership.clear_and_release();
        self.product_memo_meaning = ProductMemoMeaning::BooleanMembership;
        self.boolean_membership.mode_transitions =
            self.boolean_membership.mode_transitions.saturating_add(1);
        Ok(())
    }

    fn contains_piece_suffix(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_node: u32,
        piece_code: u8,
        next_source: SourceState,
        next_hold_code: u8,
        original_prefix_base: u128,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        let edges = language.edges_for_piece(language_node, piece_code).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_language_node_out_of_range"),
        )?;
        for edge in edges.iter().copied() {
            self.edge_check_count = self.edge_check_count.saturating_add(1);
            if self.contains_source_suffix(
                language,
                edge.child(),
                next_source,
                next_hold_code,
                original_prefix_base,
                control,
            )? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn contains_source_suffix(
        &mut self,
        language: &PieceOrderLanguageCache,
        language_node: u32,
        source: SourceState,
        hold_code: u8,
        original_prefix_base: u128,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        self.poll_cancellation(control)?;
        if original_prefix_base >= self.materialized_pattern_count as u128 {
            return Ok(false);
        }
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
            return Ok(!self.projects_unplaced_lookahead || projected_terminal);
        }
        if self.projects_unplaced_lookahead && source.depth == self.sequence_len {
            if !self.hold_enabled || hold_code == 0 {
                return Ok(false);
            }
            if source.bag_remainder.count_ones() == 1 {
                // The determined last bag piece is outside the finite original
                // queue, so neither alternative adds an original-queue offset.
                let lookahead_index = source.bag_remainder.trailing_zeros() as usize;
                let lookahead_piece = PieceKind::STANDARD_TETROMINOES[lookahead_index];
                let step = self
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
                if self.contains_piece_suffix(
                    language,
                    language_node,
                    lookahead_index as u8 + 1,
                    step.next_source,
                    step.next_hold_code,
                    original_prefix_base,
                    control,
                )? {
                    return Ok(true);
                }
                // SwapHeld consumes exactly the same validated current draw.
                return self.contains_piece_suffix(
                    language,
                    language_node,
                    hold_code,
                    step.next_source,
                    lookahead_index as u8 + 1,
                    original_prefix_base,
                    control,
                );
            }
            // Unknown input beyond this original queue can only release its
            // retained piece at the last lock, never become a placed identity.
            let held_edges = language.edges_for_piece(language_node, hold_code).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_piece_language_node_out_of_range"),
            )?;
            for edge in held_edges.iter().copied() {
                self.edge_check_count = self.edge_check_count.saturating_add(1);
                if language
                    .node(edge.child())
                    .is_some_and(|child| child.accepting)
                {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if source.depth >= self.sequence_len
            || usize::from(node.remaining_depth)
                > source.remaining_output_capacity(
                    self.sequence_len,
                    hold_code,
                    self.projects_unplaced_lookahead,
                )
        {
            return Ok(false);
        }

        self.product_state_count = self.product_state_count.saturating_add(1);
        // Every actual source prefix is a contiguous block in the original
        // lexicographic universe. Passing its absolute base preserves a clipped
        // universe even when the first successful Hold branch is not minimal.
        let mut draw_base = original_prefix_base;
        for current_index in 0..PieceKind::STANDARD_TETROMINOES.len() {
            if source.bag_remainder & (1_u8 << current_index) == 0 {
                continue;
            }
            if draw_base >= self.materialized_pattern_count as u128 {
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
            if self.contains_piece_suffix(
                language,
                language_node,
                current_code,
                next_source,
                current_step.next_hold_code,
                draw_base,
                control,
            )? {
                return Ok(true);
            }
            // Equal-kind Current and Swap have the same Boolean continuation;
            // execution multiplicity belongs to the excluded CountAll path.
            if self.hold_enabled
                && hold_code != 0
                && hold_code != current_code
                && self.contains_piece_suffix(
                    language,
                    language_node,
                    hold_code,
                    next_source,
                    current_code,
                    draw_base,
                    control,
                )?
            {
                return Ok(true);
            }
            if self.hold_enabled && hold_code == 0 && next_source.depth < self.sequence_len {
                let normalized_next = next_source.normalized();
                let mut second_draw_base = draw_base;
                for desired_index in 0..PieceKind::STANDARD_TETROMINOES.len() {
                    if normalized_next.bag_remainder & (1_u8 << desired_index) == 0 {
                        continue;
                    }
                    if second_draw_base >= self.materialized_pattern_count as u128 {
                        break;
                    }
                    let step = self.stored_step_after_current(
                        normalized_next,
                        current_code,
                        desired_index,
                    )?;
                    if self.contains_piece_suffix(
                        language,
                        language_node,
                        desired_index as u8 + 1,
                        step.next_source,
                        step.next_hold_code,
                        second_draw_base,
                        control,
                    )? {
                        return Ok(true);
                    }
                    second_draw_base =
                        second_draw_base.saturating_add(self.suffix_count(step.next_source));
                }
            }
            if self.hold_enabled
                && hold_code == 0
                && self.projects_unplaced_lookahead
                && next_source.depth == self.sequence_len
                && next_source.bag_remainder.count_ones() == 1
            {
                let lookahead_index = next_source.bag_remainder.trailing_zeros() as usize;
                let step =
                    self.stored_step_after_current(next_source, current_code, lookahead_index)?;
                if self.contains_piece_suffix(
                    language,
                    language_node,
                    lookahead_index as u8 + 1,
                    step.next_source,
                    step.next_hold_code,
                    draw_base,
                    control,
                )? {
                    return Ok(true);
                }
            }
            draw_base = draw_base.saturating_add(self.suffix_count(next_source));
        }
        Ok(false)
    }
}
