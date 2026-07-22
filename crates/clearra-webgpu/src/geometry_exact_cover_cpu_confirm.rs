use crate::geometry_exact_cover_model::{
    WebGpuCpuReferenceMismatchKind, WebGpuGeometryExactCoverBatch, STATE_WORDS, TRACE_WORDS,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReferenceEdge {
    operation_index: u32,
    child: [u32; STATE_WORDS],
}

#[derive(Debug, Default)]
pub(crate) struct CpuReferenceSampler {
    expected: Vec<ReferenceEdge>,
    actual: Vec<ReferenceEdge>,
    confirmed_dispatches: u32,
    confirmed_parents: u32,
}

impl CpuReferenceSampler {
    pub(crate) fn confirm_dispatch(
        &mut self,
        batch: &WebGpuGeometryExactCoverBatch,
        current_words: &[u32],
        parent_index_base: u32,
        generated_words: &[u32],
        generated_trace_words: &[u32],
    ) -> Result<(), CpuReferenceMismatch> {
        if !current_words.len().is_multiple_of(STATE_WORDS)
            || !generated_words.len().is_multiple_of(STATE_WORDS)
            || !generated_trace_words.len().is_multiple_of(TRACE_WORDS)
            || generated_words.len() / STATE_WORDS != generated_trace_words.len() / TRACE_WORDS
        {
            return Err(CpuReferenceMismatch {
                parent_index: parent_index_base,
                kind: WebGpuCpuReferenceMismatchKind::BufferShape,
            });
        }
        let parent_count = current_words.len() / STATE_WORDS;
        if parent_count == 0 {
            return Err(CpuReferenceMismatch {
                parent_index: parent_index_base,
                kind: WebGpuCpuReferenceMismatchKind::BufferShape,
            });
        }

        let sample_indices = [0, parent_count / 2, parent_count - 1];
        let mut previous = usize::MAX;
        for sample_index in sample_indices {
            if sample_index == previous {
                continue;
            }
            previous = sample_index;
            let parent_index =
                parent_index_base
                    .checked_add(sample_index as u32)
                    .ok_or(CpuReferenceMismatch {
                        parent_index: parent_index_base,
                        kind: WebGpuCpuReferenceMismatchKind::BufferShape,
                    })?;
            let parent: [u32; STATE_WORDS] = current_words
                [sample_index * STATE_WORDS..(sample_index + 1) * STATE_WORDS]
                .try_into()
                .map_err(|_| CpuReferenceMismatch {
                    parent_index,
                    kind: WebGpuCpuReferenceMismatchKind::BufferShape,
                })?;
            self.confirm_parent(
                batch,
                parent,
                parent_index,
                generated_words,
                generated_trace_words,
            )?;
            self.confirmed_parents = self.confirmed_parents.saturating_add(1);
        }
        self.confirmed_dispatches = self.confirmed_dispatches.saturating_add(1);
        Ok(())
    }

    fn confirm_parent(
        &mut self,
        batch: &WebGpuGeometryExactCoverBatch,
        parent: [u32; STATE_WORDS],
        parent_index: u32,
        generated_words: &[u32],
        generated_trace_words: &[u32],
    ) -> Result<(), CpuReferenceMismatch> {
        self.expected.clear();
        self.actual.clear();
        append_expected_edges(batch, parent, &mut self.expected);
        for (record_index, trace) in generated_trace_words.chunks_exact(TRACE_WORDS).enumerate() {
            if trace[0] != parent_index {
                continue;
            }
            let child: [u32; STATE_WORDS] = generated_words
                [record_index * STATE_WORDS..(record_index + 1) * STATE_WORDS]
                .try_into()
                .map_err(|_| CpuReferenceMismatch {
                    parent_index,
                    kind: WebGpuCpuReferenceMismatchKind::BufferShape,
                })?;
            self.actual.push(ReferenceEdge {
                operation_index: trace[1],
                child,
            });
        }
        self.expected.sort_unstable();
        self.actual.sort_unstable();
        if self.actual != self.expected {
            return Err(CpuReferenceMismatch {
                parent_index,
                kind: mismatch_kind(&self.expected, &self.actual),
            });
        }
        Ok(())
    }

    pub(crate) const fn is_trusted(&self) -> bool {
        self.confirmed_dispatches != 0 && self.confirmed_parents != 0
    }

    pub(crate) const fn confirmed_dispatches(&self) -> u32 {
        self.confirmed_dispatches
    }

    pub(crate) const fn confirmed_parents(&self) -> u32 {
        self.confirmed_parents
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CpuReferenceMismatch {
    pub(crate) parent_index: u32,
    pub(crate) kind: WebGpuCpuReferenceMismatchKind,
}

fn mismatch_kind(
    expected: &[ReferenceEdge],
    actual: &[ReferenceEdge],
) -> WebGpuCpuReferenceMismatchKind {
    if expected.len() != actual.len() {
        return WebGpuCpuReferenceMismatchKind::EdgeCount;
    }
    for (expected, actual) in expected.iter().zip(actual) {
        if expected.operation_index != actual.operation_index {
            return WebGpuCpuReferenceMismatchKind::OperationIndex;
        }
        if expected.child != actual.child {
            return WebGpuCpuReferenceMismatchKind::ChildState;
        }
    }
    WebGpuCpuReferenceMismatchKind::BufferShape
}

fn append_expected_edges(
    batch: &WebGpuGeometryExactCoverBatch,
    parent: [u32; STATE_WORDS],
    output: &mut Vec<ReferenceEdge>,
) {
    if parent[3] & 15 >= u32::from(batch.target_depth()) {
        return;
    }
    let Some(pivot) = select_pivot(batch, parent) else {
        return;
    };
    let begin = batch.support_offsets()[pivot.cell] as usize;
    let end = batch.support_offsets()[pivot.cell + 1] as usize;
    for operation_index in batch.support_operations()[begin..end].iter().copied() {
        let active_bumper = pivot
            .bumper_cell
            .filter(|cell| usize::from(*cell) == pivot.cell);
        if !skeleton_is_usable_for_pivot(batch, parent, operation_index as usize, active_bumper) {
            continue;
        }
        let piece = batch.skeleton_piece_kinds()[operation_index as usize];
        let mask = batch.skeleton_cell_masks()[operation_index as usize];
        let shift = (piece - 1) * 4;
        let mut child = parent;
        child[0] |= mask as u32;
        child[1] |= (mask >> 32) as u32;
        child[2] += 1_u32 << shift;
        child[3] += 1;
        if !residual_constraints_allow(batch, child) {
            continue;
        }
        output.push(ReferenceEdge {
            operation_index,
            child,
        });
    }
}

fn select_pivot(
    batch: &WebGpuGeometryExactCoverBatch,
    parent: [u32; STATE_WORDS],
) -> Option<SelectedPivot> {
    let occupied = u64::from(parent[0]) | (u64::from(parent[1]) << 32);
    let missing = batch.required_fill_mask() & !occupied;
    let mut separator = None;
    let mut bumper_cell = None;
    if constraints_enabled(batch) {
        let safe_columns = safe_separator_columns(batch);
        for column in 0..batch.width() as usize {
            if safe_columns & (1_u64 << column) == 0 {
                continue;
            }
            let column_mask = column_mask(batch, column);
            let column_missing = missing & column_mask;
            if column_missing == 0 {
                let left_mask = region_mask(batch, column, true);
                let right_mask = region_mask(batch, column, false);
                let left_count = (missing & left_mask).count_ones();
                let right_count = (missing & right_mask).count_ones();
                if left_count != 0 && right_count != 0 {
                    let owner_count = left_count.min(right_count);
                    if separator.is_none_or(|(_, _, best)| owner_count < best) {
                        separator = Some((column, left_count <= right_count, owner_count));
                    }
                }
            } else if missing.count_ones() <= 24 && column_missing.count_ones() == 1 {
                let top_cell = batch.cell_count() as usize - batch.width() as usize + column;
                if column_missing == 1_u64 << top_cell {
                    bumper_cell = u8::try_from(top_cell).ok();
                }
            }
        }
    }
    let mut selected = None;
    let mut selected_support = u32::MAX;
    for cell in 0..batch.cell_count() as usize {
        if missing & (1_u64 << cell) == 0 {
            continue;
        }
        if let Some((column, left, _)) = separator {
            let x = cell % batch.width() as usize;
            if (left && x >= column) || (!left && x <= column) {
                continue;
            }
        }
        let begin = batch.support_offsets()[cell] as usize;
        let end = batch.support_offsets()[cell + 1] as usize;
        let active_bumper = bumper_cell.filter(|bumper| usize::from(*bumper) == cell);
        let support_count = batch.support_operations()[begin..end]
            .iter()
            .filter(|row| {
                skeleton_is_usable_for_pivot(batch, parent, **row as usize, active_bumper)
            })
            .count() as u32;
        if support_count < selected_support {
            selected = Some(cell);
            selected_support = support_count;
        }
    }
    selected
        .filter(|_| selected_support != 0)
        .map(|cell| SelectedPivot { cell, bumper_cell })
}

#[derive(Clone, Copy)]
struct SelectedPivot {
    cell: usize,
    bumper_cell: Option<u8>,
}

fn skeleton_is_usable(
    batch: &WebGpuGeometryExactCoverBatch,
    parent: [u32; STATE_WORDS],
    operation_index: usize,
) -> bool {
    let Some((&piece, &mask)) = batch
        .skeleton_piece_kinds()
        .get(operation_index)
        .zip(batch.skeleton_cell_masks().get(operation_index))
    else {
        return false;
    };
    if !(1..=7).contains(&piece) {
        return false;
    }
    let shift = (piece - 1) * 4;
    let used = (parent[2] >> shift) & 15;
    let desired = (parent[3] >> (shift + 4)) & 15;
    let occupied = u64::from(parent[0]) | (u64::from(parent[1]) << 32);
    used < desired
        && occupied & mask == 0
        && batch.forbidden_mask() & mask == 0
        && mask & !batch.goal_mask() == 0
}

fn skeleton_is_usable_for_pivot(
    batch: &WebGpuGeometryExactCoverBatch,
    parent: [u32; STATE_WORDS],
    operation_index: usize,
    bumper_cell: Option<u8>,
) -> bool {
    if !skeleton_is_usable(batch, parent, operation_index) {
        return false;
    }
    let Some(bumper_cell) = bumper_cell else {
        return true;
    };
    let remaining =
        batch.required_fill_mask() & !(u64::from(parent[0]) | (u64::from(parent[1]) << 32));
    batch
        .skeleton_cell_masks()
        .get(operation_index)
        .is_some_and(|row| bumper_row_compatible(batch, remaining, bumper_cell, *row))
}

fn residual_constraints_allow(
    batch: &WebGpuGeometryExactCoverBatch,
    state: [u32; STATE_WORDS],
) -> bool {
    let constraints = batch.certified_constraint_words();
    if !constraints_enabled(batch) {
        return true;
    }
    let occupied = u64::from(state[0]) | (u64::from(state[1]) << 32);
    let remaining = batch.required_fill_mask() & !occupied;
    for column in 0..batch.width() as usize {
        let demand = (remaining & column_mask(batch, column)).count_ones();
        let mut minimum = 0_u32;
        let mut maximum = 0_u32;
        for piece in 0..7 {
            let shift = piece * 4;
            let used = (state[2] >> shift) & 15;
            let desired = (state[3] >> (shift + 4)) & 15;
            let count = desired - used;
            let bounds = constraints[4 + piece * batch.width() as usize + column];
            minimum += count * (bounds & 0xff);
            maximum += count * ((bounds >> 8) & 0xff);
        }
        if demand < minimum || demand > maximum {
            return false;
        }
    }
    if constraints[0] & 2 != 0 {
        let mut delta = 0_i32;
        let mut cells = remaining;
        while cells != 0 {
            let cell = cells.trailing_zeros() as usize;
            cells &= cells - 1;
            let x = cell % batch.width() as usize;
            let y = cell / batch.width() as usize;
            delta += if (x + y).is_multiple_of(2) { 1 } else { -1 };
        }
        let used_t = (state[2] >> 8) & 15;
        let desired_t = (state[3] >> 12) & 15;
        let remaining_t = (desired_t - used_t) as i32;
        let scaled = delta.abs() / 2;
        if delta % 2 != 0 || scaled > remaining_t || (remaining_t - scaled) % 2 != 0 {
            return false;
        }
    }
    true
}

fn constraints_enabled(batch: &WebGpuGeometryExactCoverBatch) -> bool {
    batch
        .certified_constraint_words()
        .first()
        .is_some_and(|flags| flags & 1 != 0)
}

fn safe_separator_columns(batch: &WebGpuGeometryExactCoverBatch) -> u64 {
    let words = batch.certified_constraint_words();
    u64::from(words[1]) | (u64::from(words[2]) << 32)
}

fn column_mask(batch: &WebGpuGeometryExactCoverBatch, column: usize) -> u64 {
    let mut mask = 0_u64;
    let width = batch.width() as usize;
    for cell in (column..batch.cell_count() as usize).step_by(width) {
        mask |= 1_u64 << cell;
    }
    mask
}

fn region_mask(batch: &WebGpuGeometryExactCoverBatch, separator: usize, left: bool) -> u64 {
    let mut mask = 0_u64;
    for cell in 0..batch.cell_count() as usize {
        let x = cell % batch.width() as usize;
        if (left && x < separator) || (!left && x > separator) {
            mask |= 1_u64 << cell;
        }
    }
    mask
}

fn bumper_row_compatible(
    batch: &WebGpuGeometryExactCoverBatch,
    remaining: u64,
    bumper_cell: u8,
    row: u64,
) -> bool {
    let column = bumper_cell as usize % batch.width() as usize;
    let separator = column_mask(batch, column);
    if row & separator != 1_u64 << bumper_cell {
        return false;
    }
    let left = region_mask(batch, column, true);
    let right = region_mask(batch, column, false);
    let left_demand = (remaining & left).count_ones();
    let right_demand = (remaining & right).count_ones();
    let left_supply = (row & left).count_ones();
    let right_supply = (row & right).count_ones();
    left_supply <= left_demand
        && right_supply <= right_demand
        && (left_demand - left_supply).is_multiple_of(4)
        && (right_demand - right_supply).is_multiple_of(4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WebGpuPlacementSkeleton;

    fn two_by_two_o_batch() -> WebGpuGeometryExactCoverBatch {
        WebGpuGeometryExactCoverBatch::new(
            2,
            2,
            0,
            0xf,
            0xf,
            0,
            [0, 1, 0, 0, 0, 0, 0],
            vec![WebGpuPlacementSkeleton {
                mask: 0xf,
                piece: 2,
                rotation: 0,
                x: 0,
                y: 0,
                operation_id: 1,
            }],
            16,
        )
        .expect("batch")
    }

    #[test]
    fn deterministic_cpu_sample_accepts_exact_gpu_transition() {
        let batch = two_by_two_o_batch();
        let parent = batch.initial_state_words();
        let child = [0xf, 0, 0x10, parent[3] + 1];
        let mut sampler = CpuReferenceSampler::default();
        sampler
            .confirm_dispatch(&batch, &parent, 7, &child, &[7, 0])
            .expect("exact transition");
        assert!(sampler.is_trusted());
        assert_eq!(sampler.confirmed_dispatches(), 1);
        assert_eq!(sampler.confirmed_parents(), 1);
    }

    #[test]
    fn deterministic_cpu_sample_rejects_missing_gpu_transition() {
        let batch = two_by_two_o_batch();
        let parent = batch.initial_state_words();
        let mut sampler = CpuReferenceSampler::default();
        assert_eq!(
            sampler.confirm_dispatch(&batch, &parent, 3, &[], &[]),
            Err(CpuReferenceMismatch {
                parent_index: 3,
                kind: WebGpuCpuReferenceMismatchKind::EdgeCount,
            })
        );
        assert!(!sampler.is_trusted());
    }
}
