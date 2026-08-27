use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    execution_automaton::{
        SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionError, SupplyExecutionState,
    },
    hold_automaton::HoldAutomatonState,
};

use super::piece_multiset_group::PieceMultisetKey;

pub type BagHoldBranchKind = SupplyBranchKind;
pub type BagMultisetProjectionError = SupplyExecutionError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BagPlacementState {
    pub cursor: u16,
    pub hold_piece: Option<PieceKind>,
    pub bag_epoch: u16,
    pub bag_remainder: PieceMultisetKey,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BagSupplyBranch {
    pub used_piece: PieceKind,
    pub next_state: BagPlacementState,
    pub kind: BagHoldBranchKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BagPlacementAutomaton {
    inner: SupplyExecutionAutomaton,
    state_identity: SupplyExecutionState,
}

impl BagPlacementAutomaton {
    pub fn from_initial_hold(
        bag_pattern: &[PieceKind],
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        placed_piece_count: usize,
    ) -> Result<(Self, BagPlacementState), BagMultisetProjectionError> {
        let inner = SupplyExecutionAutomaton::for_bag(bag_pattern)?;
        let state_identity =
            inner.project_bag_initial_state(initial_hold, hold_enabled, placed_piece_count)?;
        let initial_state = compact_state(state_identity)?;
        Ok((
            Self {
                inner,
                state_identity,
            },
            initial_state,
        ))
    }

    pub fn write_matching_branches(
        &self,
        state: BagPlacementState,
        desired_piece: PieceKind,
        branches: &mut Vec<BagSupplyBranch>,
    ) -> Result<(), BagMultisetProjectionError> {
        branches.clear();
        let state = self.execution_state(state);
        let mut projection_error = None;
        self.inner
            .for_each_matching_bag_step(state, desired_piece, |step| {
                match compact_state(step.next_state) {
                    Ok(next_state) => branches.push(BagSupplyBranch {
                        used_piece: step.used_piece,
                        next_state,
                        kind: step.evidence.branch_kind,
                    }),
                    Err(error) => projection_error = Some(error),
                }
            })?;
        if let Some(error) = projection_error {
            return Err(error);
        }
        branches.sort_unstable();
        branches.dedup();
        Ok(())
    }

    fn execution_state(&self, state: BagPlacementState) -> SupplyExecutionState {
        SupplyExecutionState {
            cursor: state.cursor,
            hold_piece: state.hold_piece,
            hold_empty: state.hold_piece.is_none(),
            bag_epoch: state.bag_epoch,
            bag_remainder_key: encode_remainder(state.bag_remainder),
            ..self.state_identity
        }
    }
}

fn compact_state(
    state: SupplyExecutionState,
) -> Result<BagPlacementState, BagMultisetProjectionError> {
    Ok(BagPlacementState {
        cursor: state.cursor,
        hold_piece: state.hold_piece,
        bag_epoch: state.bag_epoch,
        bag_remainder: decode_remainder(state.bag_remainder_key)?,
    })
}

fn encode_remainder(remainder: PieceMultisetKey) -> u64 {
    remainder
        .counts()
        .into_iter()
        .enumerate()
        .fold(0_u64, |key, (index, count)| {
            key | (u64::from(count) << ((index + 1) * 4))
        })
}

fn decode_remainder(key: u64) -> Result<PieceMultisetKey, BagMultisetProjectionError> {
    let storage_mask = (1usize..=7).fold(0_u64, |mask, piece| mask | (0xf_u64 << (piece * 4)));
    if key & !storage_mask != 0 {
        return Err(BagMultisetProjectionError::InvalidBagRemainder);
    }
    let mut counts = [0_u8; 7];
    for (index, count) in counts.iter_mut().enumerate() {
        *count = ((key >> ((index + 1) * 4)) & 0xf) as u8;
    }
    Ok(PieceMultisetKey::from_counts(counts))
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BagMultisetFrontierState {
    supply: BagPlacementState,
    placed_multiset: PieceMultisetKey,
}

const MAX_MATCHING_BRANCHES_PER_DESIRED_PIECE: usize = 8;
const BAG_BRANCH_BUFFER_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BagMultisetAllocationReport {
    pub peak_bytes: u128,
    pub retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundedBagMultisetError {
    Projection(BagMultisetProjectionError),
    ProjectionOverflow,
    AllocationFailed {
        required_memory_bytes: u128,
    },
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
}

impl From<BagMultisetProjectionError> for BoundedBagMultisetError {
    fn from(error: BagMultisetProjectionError) -> Self {
        Self::Projection(error)
    }
}

/// Allocation-free upper bound for the private current/next frontier and
/// branch buffer used below. At depth `d`, the placed multiset and final hold
/// identify the drawn bag remainder; cursor can be either `d` or `d + 1`.
/// Therefore at most 16 supply identities exist per unordered multiset.
pub fn checked_reachable_bag_multiset_peak_upper_bound(placed_piece_count: usize) -> Option<u128> {
    let branch_bytes = (BAG_BRANCH_BUFFER_CAPACITY as u128)
        .checked_mul(core::mem::size_of::<BagSupplyBranch>() as u128)?;
    let frontier_item_bytes = core::mem::size_of::<BagMultisetFrontierState>() as u128;
    let mut peak = frontier_item_bytes.checked_add(branch_bytes)?;
    let mut prior_capacity = 1_u128;
    for depth in 0..placed_piece_count {
        let state_count = if depth == 0 {
            1
        } else {
            checked_unordered_multiset_count(depth)?.checked_mul(16)?
        };
        let next_capacity = state_count
            .checked_mul(PieceKind::STANDARD_TETROMINOES.len() as u128)?
            .checked_mul(MAX_MATCHING_BRANCHES_PER_DESIRED_PIECE as u128)?;
        let live = prior_capacity
            .checked_add(next_capacity)?
            .checked_mul(frontier_item_bytes)?
            .checked_add(branch_bytes)?;
        peak = peak.max(live);
        prior_capacity = next_capacity;
    }
    let final_state_count = if placed_piece_count == 0 {
        1
    } else {
        checked_unordered_multiset_count(placed_piece_count)?.checked_mul(16)?
    };
    let final_keys =
        final_state_count.checked_mul(core::mem::size_of::<PieceMultisetKey>() as u128)?;
    Some(
        peak.max(
            prior_capacity
                .checked_mul(frontier_item_bytes)?
                .checked_add(final_keys)?,
        ),
    )
}

/// Projects an exact bag/hold language to the unordered piece multisets used
/// by Packing. Queue order and hold never enter Packing state; the full supply
/// state is retained until this projection is complete and remains authoritative
/// again during pattern-specific BuildUp.
pub fn reachable_bag_multisets(
    bag_pattern: &[PieceKind],
    placed_piece_count: usize,
    initial_hold: HoldAutomatonState,
    hold_enabled: bool,
) -> Result<Vec<PieceMultisetKey>, BagMultisetProjectionError> {
    match reachable_bag_multisets_internal(
        bag_pattern,
        placed_piece_count,
        initial_hold,
        hold_enabled,
        None,
    ) {
        Ok((multisets, _)) => Ok(multisets),
        Err(BoundedBagMultisetError::Projection(error)) => Err(error),
        Err(
            BoundedBagMultisetError::ProjectionOverflow
            | BoundedBagMultisetError::AllocationFailed { .. }
            | BoundedBagMultisetError::MemoryCapacityExceeded { .. },
        ) => Err(BagMultisetProjectionError::BagEpochExhausted),
    }
}

pub fn reachable_bag_multisets_with_memory_limit(
    bag_pattern: &[PieceKind],
    placed_piece_count: usize,
    initial_hold: HoldAutomatonState,
    hold_enabled: bool,
    already_retained_bytes: u128,
    max_memory_bytes: u128,
) -> Result<(Vec<PieceMultisetKey>, BagMultisetAllocationReport), BoundedBagMultisetError> {
    reachable_bag_multisets_internal(
        bag_pattern,
        placed_piece_count,
        initial_hold,
        hold_enabled,
        Some((already_retained_bytes, max_memory_bytes)),
    )
}

fn reachable_bag_multisets_internal(
    bag_pattern: &[PieceKind],
    placed_piece_count: usize,
    initial_hold: HoldAutomatonState,
    hold_enabled: bool,
    memory_limit: Option<(u128, u128)>,
) -> Result<(Vec<PieceMultisetKey>, BagMultisetAllocationReport), BoundedBagMultisetError> {
    let (automaton, initial_state) = BagPlacementAutomaton::from_initial_hold(
        bag_pattern,
        initial_hold,
        hold_enabled,
        placed_piece_count,
    )?;
    let mut peak_bytes = 0_u128;
    let mut current = Vec::new();
    reserve_bag_capacity(&mut current, 1, 0, memory_limit, &mut peak_bytes)?;
    current.push(BagMultisetFrontierState {
        supply: initial_state,
        placed_multiset: PieceMultisetKey::default(),
    });
    let mut next = Vec::new();
    let mut branches = Vec::new();
    reserve_bag_capacity(
        &mut branches,
        BAG_BRANCH_BUFFER_CAPACITY,
        bag_vec_bytes::<BagMultisetFrontierState>(&current)?
            .checked_add(bag_vec_bytes::<BagMultisetFrontierState>(&next)?)
            .ok_or(BoundedBagMultisetError::ProjectionOverflow)?,
        memory_limit,
        &mut peak_bytes,
    )?;

    for _ in 0..placed_piece_count {
        next.clear();
        let max_children = current
            .len()
            .checked_mul(PieceKind::STANDARD_TETROMINOES.len())
            .and_then(|count| count.checked_mul(MAX_MATCHING_BRANCHES_PER_DESIRED_PIECE))
            .ok_or(BoundedBagMultisetError::ProjectionOverflow)?;
        let other_live_bytes = bag_vec_bytes::<BagMultisetFrontierState>(&current)?
            .checked_add(bag_vec_bytes::<BagSupplyBranch>(&branches)?)
            .ok_or(BoundedBagMultisetError::ProjectionOverflow)?;
        reserve_bag_capacity(
            &mut next,
            max_children,
            other_live_bytes,
            memory_limit,
            &mut peak_bytes,
        )?;
        for state in current.iter().copied() {
            for desired_piece in PieceKind::STANDARD_TETROMINOES {
                automaton.write_matching_branches(state.supply, desired_piece, &mut branches)?;
                for branch in branches.iter().copied() {
                    let mut child = BagMultisetFrontierState {
                        supply: branch.next_state,
                        placed_multiset: state.placed_multiset,
                    };
                    child.placed_multiset.push(branch.used_piece);
                    next.push(child);
                }
            }
        }
        next.sort_unstable();
        next.dedup();
        core::mem::swap(&mut current, &mut next);
    }

    drop(next);
    drop(branches);
    let current_bytes = bag_vec_bytes::<BagMultisetFrontierState>(&current)?;
    let mut multisets = Vec::new();
    reserve_bag_capacity(
        &mut multisets,
        current.len(),
        current_bytes,
        memory_limit,
        &mut peak_bytes,
    )?;
    for state in current {
        multisets.push(state.placed_multiset);
    }
    multisets.sort_unstable();
    multisets.dedup();
    let retained_bytes = bag_vec_bytes::<PieceMultisetKey>(&multisets)?;
    peak_bytes = peak_bytes.max(retained_bytes);
    Ok((
        multisets,
        BagMultisetAllocationReport {
            peak_bytes,
            retained_bytes,
        },
    ))
}

fn reserve_bag_capacity<T>(
    values: &mut Vec<T>,
    required_capacity: usize,
    other_live_bytes: u128,
    memory_limit: Option<(u128, u128)>,
    peak_bytes: &mut u128,
) -> Result<(), BoundedBagMultisetError> {
    if values.capacity() < required_capacity {
        let projected = (required_capacity as u128)
            .checked_mul(core::mem::size_of::<T>() as u128)
            .ok_or(BoundedBagMultisetError::ProjectionOverflow)?
            .checked_add(other_live_bytes)
            .ok_or(BoundedBagMultisetError::ProjectionOverflow)?;
        ensure_bag_memory_limit(memory_limit, projected)?;
        values
            .try_reserve_exact(required_capacity.saturating_sub(values.len()))
            .map_err(|_| BoundedBagMultisetError::AllocationFailed {
                required_memory_bytes: memory_limit
                    .map_or(projected, |(already, _)| already.saturating_add(projected)),
            })?;
    }
    let live = bag_vec_bytes(values)?
        .checked_add(other_live_bytes)
        .ok_or(BoundedBagMultisetError::ProjectionOverflow)?;
    ensure_bag_memory_limit(memory_limit, live)?;
    *peak_bytes = (*peak_bytes).max(live);
    Ok(())
}

fn bag_vec_bytes<T>(values: &Vec<T>) -> Result<u128, BoundedBagMultisetError> {
    (values.capacity() as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(BoundedBagMultisetError::ProjectionOverflow)
}

fn ensure_bag_memory_limit(
    memory_limit: Option<(u128, u128)>,
    live_bytes: u128,
) -> Result<(), BoundedBagMultisetError> {
    let Some((already_retained_bytes, max_memory_bytes)) = memory_limit else {
        return Ok(());
    };
    let required_memory_bytes = already_retained_bytes
        .checked_add(live_bytes)
        .ok_or(BoundedBagMultisetError::ProjectionOverflow)?;
    if required_memory_bytes <= max_memory_bytes {
        return Ok(());
    }
    Err(BoundedBagMultisetError::MemoryCapacityExceeded {
        required_memory_bytes,
        max_memory_bytes,
    })
}

fn checked_unordered_multiset_count(total_piece_count: usize) -> Option<u128> {
    let total = total_piece_count as u128;
    (1_u128..=6).try_fold(1_u128, |count, divisor| {
        count
            .checked_mul(total.checked_add(divisor)?)?
            .checked_div(divisor)
    })
}

#[cfg(test)]
mod tests {
    use crate::{hold_automaton::SupplyProvenanceId, piece_source::PieceSourceId};

    use super::*;

    #[test]
    fn empty_4l_bag_projection_includes_p7p4_hold_carry() {
        assert!(core::mem::size_of::<BagPlacementState>() <= 16);
        let initial_hold =
            HoldAutomatonState::new(PieceSourceId::new(1), 0, None, 0, 0, SupplyProvenanceId(1));
        let with_hold =
            reachable_bag_multisets(&PieceKind::STANDARD_TETROMINOES, 10, initial_hold, true)
                .expect("standard bag projection");
        let without_hold =
            reachable_bag_multisets(&PieceKind::STANDARD_TETROMINOES, 10, initial_hold, false)
                .expect("standard bag projection without hold");

        assert_eq!(with_hold.len(), 140);
        assert_eq!(without_hold.len(), 35);
        assert!(with_hold.contains(&PieceMultisetKey::from_counts([2, 2, 2, 2, 1, 1, 0,])));
        assert!(with_hold
            .iter()
            .all(|multiset| multiset.total_count() == 10));
    }

    #[test]
    fn bounded_frontier_reports_actual_peak_below_private_layout_upper_bound() {
        let initial_hold =
            HoldAutomatonState::new(PieceSourceId::new(2), 0, None, 0, 0, SupplyProvenanceId(2));
        let upper = checked_reachable_bag_multiset_peak_upper_bound(4)
            .expect("checked private frontier upper bound");
        let (bounded, report) = reachable_bag_multisets_with_memory_limit(
            &PieceKind::STANDARD_TETROMINOES,
            4,
            initial_hold,
            true,
            0,
            upper,
        )
        .expect("upper bound admits exact frontier");
        let unbounded =
            reachable_bag_multisets(&PieceKind::STANDARD_TETROMINOES, 4, initial_hold, true)
                .expect("unbounded reference");

        assert_eq!(bounded, unbounded);
        assert!(report.peak_bytes <= upper);
        assert_eq!(
            report.retained_bytes,
            (bounded.capacity() * core::mem::size_of::<PieceMultisetKey>()) as u128
        );
        assert!(report.peak_bytes >= report.retained_bytes);
    }

    #[test]
    fn bounded_frontier_never_crosses_a_one_byte_short_cap() {
        let initial_hold =
            HoldAutomatonState::new(PieceSourceId::new(3), 0, None, 0, 0, SupplyProvenanceId(3));
        let (_, report) = reachable_bag_multisets_with_memory_limit(
            &PieceKind::STANDARD_TETROMINOES,
            3,
            initial_hold,
            true,
            11,
            u128::MAX,
        )
        .expect("measure exact actual capacity peak");
        let error = reachable_bag_multisets_with_memory_limit(
            &PieceKind::STANDARD_TETROMINOES,
            3,
            initial_hold,
            true,
            11,
            report.peak_bytes + 10,
        )
        .expect_err("one-byte-short cap must fail closed");
        assert!(matches!(
            error,
            BoundedBagMultisetError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            } if required_memory_bytes > max_memory_bytes
                && max_memory_bytes == report.peak_bytes + 10
        ));
    }

    #[test]
    fn zero_depth_projection_still_accounts_branch_storage_before_reserve() {
        let initial_hold =
            HoldAutomatonState::new(PieceSourceId::new(4), 0, None, 0, 0, SupplyProvenanceId(4));
        let error = reachable_bag_multisets_with_memory_limit(
            &PieceKind::STANDARD_TETROMINOES,
            0,
            initial_hold,
            false,
            0,
            core::mem::size_of::<BagMultisetFrontierState>() as u128,
        )
        .expect_err("branch buffer does not fit");
        assert!(matches!(
            error,
            BoundedBagMultisetError::MemoryCapacityExceeded { .. }
        ));
    }
}
