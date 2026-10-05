//! Derived, exact occupancy dispatch for one immutable relation context.
//!
//! The serialized records remain the authority. A table only selects the
//! first matching record, never invents an occupancy or a negative answer.
// SRP rationale: factor and index a validated context's dependency predicates.

use crate::conditioned_local_relation::ExactConditionedLocalRelation;

const MAX_VARIABLE_BITS: u32 = 12;
const ABSENT: u32 = u32::MAX;

pub(super) struct ContextDispatch {
    pub start: usize,
    pub end: usize,
    common_mask: u64,
    common_occupancy: u64,
    variable_mask: u64,
    contiguous_shift: Option<u32>,
    selections: Vec<u32>,
}

impl ContextDispatch {
    pub fn compile(
        records: &[ExactConditionedLocalRelation],
        start: usize,
        end: usize,
        budget: &mut usize,
    ) -> Self {
        let first = &records[start];
        let mut common_mask = first.dependency_mask;
        let mut dependency_union = common_mask;
        for record in &records[start + 1..end] {
            common_mask &= record.dependency_mask
                & !(first.dependency_occupancy ^ record.dependency_occupancy);
            dependency_union |= record.dependency_mask;
        }
        let common_occupancy = first.dependency_occupancy & common_mask;
        let variable_mask = dependency_union & !common_mask;
        let variable_bits = variable_mask.count_ones();
        let mut selections = Vec::new();
        if variable_bits <= MAX_VARIABLE_BITS {
            let count = 1_usize << variable_bits;
            let bytes = count * core::mem::size_of::<u32>();
            if bytes <= *budget
                && selections.try_reserve_exact(count).is_ok()
                && selections.capacity() * core::mem::size_of::<u32>() <= *budget
            {
                for compressed in 0..count {
                    let occupancy =
                        common_occupancy | expand_bits(compressed as u64, variable_mask);
                    let selected = records[start..end]
                        .iter()
                        .position(|record| {
                            occupancy & record.dependency_mask == record.dependency_occupancy
                        })
                        .map_or(ABSENT, |offset| offset as u32);
                    selections.push(selected);
                }
                *budget = budget.saturating_sub(selections.capacity() * 4);
            } else {
                selections = Vec::new();
            }
        }
        let shift = variable_mask.trailing_zeros();
        let contiguous_shift = (variable_mask == 0
            || variable_mask >> shift == (1_u64 << variable_bits) - 1)
            .then_some(if variable_mask == 0 { 0 } else { shift });
        Self {
            start,
            end,
            common_mask,
            common_occupancy,
            variable_mask,
            contiguous_shift,
            selections,
        }
    }

    #[inline]
    pub fn selected_record(
        &self,
        records: &[ExactConditionedLocalRelation],
        board: u64,
    ) -> Option<usize> {
        if board & self.common_mask != self.common_occupancy {
            return None;
        }
        if self.selections.is_empty() {
            return records[self.start..self.end]
                .iter()
                .position(|record| board & record.dependency_mask == record.dependency_occupancy)
                .map(|offset| self.start + offset);
        }
        let compressed = match self.contiguous_shift {
            Some(shift) => (board & self.variable_mask) >> shift,
            None => compress_bits(board, self.variable_mask),
        } as usize;
        let selected = self.selections[compressed];
        if selected == ABSENT {
            None
        } else {
            Some(self.start + selected as usize)
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.selections.capacity() * core::mem::size_of::<u32>()
    }
}

fn compress_bits(board: u64, mut mask: u64) -> u64 {
    let mut compressed = 0;
    let mut destination = 1;
    while mask != 0 {
        let bit = mask & mask.wrapping_neg();
        if board & bit != 0 {
            compressed |= destination;
        }
        mask &= mask - 1;
        destination <<= 1;
    }
    compressed
}

fn expand_bits(mut compressed: u64, mut mask: u64) -> u64 {
    let mut expanded = 0;
    while mask != 0 {
        let bit = mask & mask.wrapping_neg();
        if compressed & 1 != 0 {
            expanded |= bit;
        }
        compressed >>= 1;
        mask &= mask - 1;
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::{compress_bits, expand_bits, ContextDispatch};
    use crate::conditioned_local_relation::ExactConditionedLocalRelation;

    #[test]
    fn sparse_dependency_codec_roundtrips_every_small_assignment() {
        for mask in [0_u64, 0xff, 0x1010_102, 0xf0f0] {
            for assignment in 0..1_u64 << mask.count_ones() {
                assert_eq!(
                    compress_bits(expand_bits(assignment, mask), mask),
                    assignment
                );
            }
        }
    }

    #[test]
    fn dispatch_preserves_first_matching_predicate_and_budget_fallback() {
        let make = |mask, occupancy| ExactConditionedLocalRelation {
            width: 10,
            height: 4,
            board: occupancy,
            row_frame: crate::conditioned_local_relation::LocalRelationRowFrame::new(4, 0).unwrap(),
            piece: clearra_core_domain::piece::piece_kind::PieceKind::T,
            kick_profile: clearra_rules::kicks::KickTableProfileId::SrsPlus,
            window: crate::conditioned_local_relation::ConditionedPoseWindow {
                min_x: 4,
                max_x: 4,
                min_y: 4,
                max_y: 4,
            },
            entries: Vec::new().into(),
            dependency_mask: mask,
            dependency_occupancy: occupancy,
            grounded_lock_anchors: [0; 4],
            exits: Vec::new(),
        };
        let records = vec![make(0x103, 1), make(0x105, 4), make(0x107, 2)];
        for budget in [0, 4096] {
            let mut remaining = budget;
            let dispatch = ContextDispatch::compile(&records, 0, records.len(), &mut remaining);
            for board in 0..512 {
                let reference = records.iter().position(|record| {
                    board & record.dependency_mask == record.dependency_occupancy
                });
                assert_eq!(dispatch.selected_record(&records, board), reference);
            }
            assert!(dispatch.retained_bytes() <= budget);
        }
    }
}
