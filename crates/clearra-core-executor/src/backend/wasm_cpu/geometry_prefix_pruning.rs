use super::row_order::FamilyRowOrderTable;
use super::{GeometryCatalog, MAX_BOARD64_PIECES};
use crate::performance::{profile_sample_scale, ExecutorSearchStage, SearchStageSpan};

/// Selected operations keep their exact positive realization alternatives.
/// Every unselected operation is optimistically already available. Thus a
/// complete target row with no selected contributors is deleted initially;
/// otherwise its selected contributors form an AND condition. Each operation
/// keeps the OR over every concrete ILC realization in the immutable catalog.
///
/// Any physical completion's selected subsequence is contained in this least
/// fixed point: future contributors can only make row deletion harder, and
/// forbidden frames, collision, grounding and movement are all ignored here.
/// A stall therefore rejects every completion of this selected prefix. A pass
/// supplies no buildability or queue-coverage authority.
pub(super) fn selected_prefix_stalls(catalog: &GeometryCatalog, rows: &[u32]) -> bool {
    if rows.is_empty() || rows.len() > MAX_BOARD64_PIECES || catalog.height() > 16 {
        return false;
    }
    let mut row_contributors = [0_u16; 16];
    for (operation, row_id) in rows.iter().copied().enumerate() {
        if catalog.try_skeleton(row_id).is_none() {
            return false;
        }
        let mut occupied = catalog.skeleton_occupied_rows(row_id);
        while occupied != 0 {
            let row = occupied.trailing_zeros() as usize;
            occupied &= occupied - 1;
            row_contributors[row] |= 1_u16 << operation;
        }
    }
    // This is the complete catalog target, never the cells covered by the
    // selected prefix. In a blank 10x6 PC every one of these six rows is full.
    let target = catalog.initial_board() | catalog.required_cells();
    let row_mask = u64::MAX >> (64 - catalog.width());
    let mut completed_target_rows = 0_u16;
    for row in 0..usize::from(catalog.height()) {
        if (target >> (row * usize::from(catalog.width()))) & row_mask == row_mask {
            completed_target_rows |= 1_u16 << row;
        }
    }
    optimistic_dependency_stalls(
        rows.len(),
        &row_contributors[..usize::from(catalog.height())],
        completed_target_rows,
        |operation, deleted| catalog.realization_requirement_is_satisfied(rows[operation], deleted),
    )
}

/// Before the first clear all pending locks have the original row frame.
/// Give every unselected cell to the initial board and close selected locks
/// under their possible supporters. This contains every physical prefix.
/// If no completed target row can be filled even under that relaxation, the
/// selected prefix has no physical completion. A pass remains Unknown.
pub(super) fn selected_first_clear_stalls(
    catalog: &GeometryCatalog,
    rows: &[u32],
    table: Option<&FamilyRowOrderTable>,
) -> bool {
    let width = usize::from(catalog.width());
    let height = usize::from(catalog.height());
    if rows.is_empty()
        || rows.len() > MAX_BOARD64_PIECES
        || width == 0
        || height == 0
        || height > 16
        || width * height > 64
    {
        return false;
    }
    let full = u64::MAX >> (64 - width * height);
    let target = catalog.initial_board() | catalog.required_cells();
    if target & !full != 0 {
        return false;
    }
    let floor = u64::MAX >> (64 - width);
    let mut completed_rows = 0_u16;
    for row in 0..height {
        if (target >> (row * width)) & floor == floor {
            completed_rows |= 1 << row;
        }
    }
    if completed_rows == 0 {
        return false;
    }

    let mut cells = [0_u64; MAX_BOARD64_PIECES];
    let mut contributors = [0_u16; 16];
    let mut owners = [u8::MAX; 64];
    let mut selected = 0_u64;
    for (operation, &row_id) in rows.iter().enumerate() {
        let Some(skeleton) = catalog.try_skeleton(row_id) else {
            return false;
        };
        if skeleton.cells.count_ones() != 4
            || skeleton.cells & !catalog.required_cells() != 0
            || skeleton.cells & (selected | catalog.initial_board()) != 0
        {
            return false;
        }
        cells[operation] = skeleton.cells;
        selected |= skeleton.cells;
        let mut occupied = catalog.skeleton_occupied_rows(row_id);
        while occupied != 0 {
            let row = occupied.trailing_zeros() as usize;
            occupied &= occupied - 1;
            contributors[row] |= 1 << operation;
        }
        let mut occupied = skeleton.cells;
        while occupied != 0 {
            let cell = occupied.trailing_zeros() as usize;
            occupied &= occupied - 1;
            owners[cell] = operation as u8;
        }
    }
    let mut pending_rows = completed_rows;
    while pending_rows != 0 {
        let row = pending_rows.trailing_zeros() as usize;
        pending_rows &= pending_rows - 1;
        if contributors[row] == 0 {
            // Future-only contributors, including an initially full row,
            // are optimistic first-clear witnesses rather than negatives.
            return false;
        }
    }

    let optimistic_board = catalog.initial_board() | (target & !selected);
    let mut dependents = [0_u16; MAX_BOARD64_PIECES];
    let mut active = 0_u16;
    for (operation, &row_id) in rows.iter().enumerate() {
        let ready = table
            .and_then(|table| table.frame_zero_is_available(row_id))
            .unwrap_or_else(|| catalog.has_frame_zero_realization(row_id));
        if !ready {
            continue;
        }
        let mask = cells[operation];
        let bottom = mask.checked_shr(width as u32).unwrap_or(0) & !mask;
        if mask & floor != 0 || bottom & optimistic_board != 0 {
            active |= 1 << operation;
        }
        let mut supporters = bottom & selected;
        while supporters != 0 {
            let cell = supporters.trailing_zeros() as usize;
            supporters &= supporters - 1;
            dependents[usize::from(owners[cell])] |= 1 << operation;
        }
    }
    let mut pending = active;
    while pending != 0 {
        let supporter = pending.trailing_zeros() as usize;
        pending &= pending - 1;
        let added = dependents[supporter] & !active;
        active |= added;
        pending |= added;
    }
    while completed_rows != 0 {
        let row = completed_rows.trailing_zeros() as usize;
        completed_rows &= completed_rows - 1;
        if contributors[row] & !active == 0 {
            return false;
        }
    }
    true
}

fn optimistic_dependency_stalls(
    operation_count: usize,
    row_contributors: &[u16],
    completed_target_rows: u16,
    mut requirement_is_satisfied: impl FnMut(usize, u16) -> bool,
) -> bool {
    if operation_count == 0 || operation_count > 16 || row_contributors.len() > 16 {
        return false;
    }
    let all_placed = u16::MAX >> (16 - operation_count);
    let mut placed = 0_u16;
    while placed != all_placed {
        let mut deleted = 0_u16;
        for (row, contributors) in row_contributors.iter().copied().enumerate() {
            if completed_target_rows & (1 << row) != 0 && contributors & !placed == 0 {
                // Empty AND is true: all future contributors were relaxed.
                deleted |= 1 << row;
            }
        }
        let mut ready = 0_u16;
        let mut remaining = all_placed & !placed;
        while remaining != 0 {
            let operation = remaining.trailing_zeros() as usize;
            let bit = 1 << operation;
            remaining &= remaining - 1;
            if requirement_is_satisfied(operation, deleted) {
                ready |= bit;
            }
        }
        if ready == 0 {
            return true;
        }
        placed |= ready;
    }
    false
}

#[derive(Debug, Default)]
pub(super) struct PhysicalPrefixProfile {
    #[cfg(any(
        feature = "search-stage-profiling",
        feature = "wasm-stage-profiling",
        test
    ))]
    pub(super) checks: usize,
    #[cfg(any(
        feature = "search-stage-profiling",
        feature = "wasm-stage-profiling",
        test
    ))]
    pub(super) pruned_subtrees: u64,
    #[cfg(any(
        feature = "search-stage-profiling",
        feature = "wasm-stage-profiling",
        test
    ))]
    pub(super) row_order_pruned_subtrees: u64,
    #[cfg(any(
        feature = "search-stage-profiling",
        feature = "wasm-stage-profiling",
        test
    ))]
    pub(super) emitted_candidates: u64,
    #[cfg(any(
        feature = "search-stage-profiling",
        feature = "wasm-stage-profiling",
        test
    ))]
    pub(super) first_clear_checks: u64,
    #[cfg(any(
        feature = "search-stage-profiling",
        feature = "wasm-stage-profiling",
        test
    ))]
    pub(super) first_clear_pruned_subtrees: u64,
}

impl PhysicalPrefixProfile {
    pub(super) fn first_clear_stalls(
        &mut self,
        catalog: &GeometryCatalog,
        rows: &[u32],
        table: Option<&FamilyRowOrderTable>,
    ) -> bool {
        #[cfg(any(
            feature = "search-stage-profiling",
            feature = "wasm-stage-profiling",
            test
        ))]
        let ordinal = {
            self.first_clear_checks = self.first_clear_checks.saturating_add(1);
            self.first_clear_checks as usize
        };
        #[cfg(not(any(
            feature = "search-stage-profiling",
            feature = "wasm-stage-profiling",
            test
        )))]
        let ordinal = 0;
        let span = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::WasmGeometryFirstClearClosure,
            profile_sample_scale(ordinal),
        );
        let stalled = selected_first_clear_stalls(catalog, rows, table);
        span.finish(0);
        #[cfg(any(
            feature = "search-stage-profiling",
            feature = "wasm-stage-profiling",
            test
        ))]
        if stalled {
            self.first_clear_pruned_subtrees = self.first_clear_pruned_subtrees.saturating_add(1);
        }
        stalled
    }

    #[inline]
    pub(super) fn record_row_order_prune(&mut self) {
        #[cfg(any(
            feature = "search-stage-profiling",
            feature = "wasm-stage-profiling",
            test
        ))]
        {
            self.row_order_pruned_subtrees = self.row_order_pruned_subtrees.saturating_add(1);
        }
    }

    #[inline]
    pub(super) fn record_emitted_candidate(&mut self) {
        #[cfg(any(
            feature = "search-stage-profiling",
            feature = "wasm-stage-profiling",
            test
        ))]
        {
            self.emitted_candidates = self.emitted_candidates.saturating_add(1);
        }
    }

    pub(super) fn stalls(&mut self, catalog: &GeometryCatalog, rows: &[u32]) -> bool {
        #[cfg(any(
            feature = "search-stage-profiling",
            feature = "wasm-stage-profiling",
            test
        ))]
        let ordinal = {
            self.checks = self.checks.saturating_add(1);
            self.checks
        };
        #[cfg(not(any(
            feature = "search-stage-profiling",
            feature = "wasm-stage-profiling",
            test
        )))]
        let ordinal = 0;
        let stage = SearchStageSpan::begin_scaled(
            ExecutorSearchStage::WasmGeometryPrefixClosure,
            profile_sample_scale(ordinal),
        );
        let stalled = selected_prefix_stalls(catalog, rows);
        stage.finish(0);
        #[cfg(any(
            feature = "search-stage-profiling",
            feature = "wasm-stage-profiling",
            test
        ))]
        if stalled {
            self.pruned_subtrees = self.pruned_subtrees.saturating_add(1);
        }
        stalled
    }
}

#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
impl Drop for PhysicalPrefixProfile {
    fn drop(&mut self) {
        SearchStageSpan::record_exact_work(
            ExecutorSearchStage::WasmGeometryFirstClearClosure,
            self.first_clear_checks,
        );
        SearchStageSpan::record_exact_work(
            ExecutorSearchStage::WasmGeometryFirstClearPrunedSubtrees,
            self.first_clear_pruned_subtrees,
        );
        SearchStageSpan::record_exact_work(
            ExecutorSearchStage::WasmGeometryPrefixClosure,
            self.checks as u64,
        );
        SearchStageSpan::record_exact_work(
            ExecutorSearchStage::WasmGeometryPrefixPrunedSubtrees,
            self.pruned_subtrees,
        );
        SearchStageSpan::record_exact_work(
            ExecutorSearchStage::WasmGeometryRowOrderPrunedSubtrees,
            self.row_order_pruned_subtrees,
        );
        SearchStageSpan::record_exact_work(
            ExecutorSearchStage::WasmGeometryEmittedCandidates,
            self.emitted_candidates,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::optimistic_dependency_stalls;

    #[test]
    fn physical_prefix_empty_contributors_are_future_completed_rows() {
        // The selected operation needs row 1, whose contributors are all
        // future operations. Treating empty AND as false would reject it.
        assert!(!optimistic_dependency_stalls(
            1,
            &[1, 0],
            0b11,
            |_, deleted| deleted & 0b10 != 0
        ));
        // The same row cannot be assumed clear if the full target does not
        // complete it. This is independent of partial selected-cell coverage.
        assert!(optimistic_dependency_stalls(
            1,
            &[1, 0],
            0b01,
            |_, deleted| deleted & 0b10 != 0
        ));
    }

    #[test]
    fn physical_prefix_preserves_realization_or_and_selected_contributor_and() {
        let contributors = [0b01, 0b10, 0];
        assert!(optimistic_dependency_stalls(
            2,
            &contributors,
            0b111,
            |operation, deleted| {
                let required = [0b010, 0b001][operation];
                required & !deleted == 0
            }
        ));
        // An alternative realization of A needs only the future-completed
        // third row. It unlocks A, then B: one representative is insufficient.
        assert!(!optimistic_dependency_stalls(
            2,
            &contributors,
            0b111,
            |operation, deleted| {
                if operation == 0 {
                    [0b010, 0b100]
                        .iter()
                        .any(|required| required & !deleted == 0)
                } else {
                    0b001 & !deleted == 0
                }
            }
        ));
    }
}
