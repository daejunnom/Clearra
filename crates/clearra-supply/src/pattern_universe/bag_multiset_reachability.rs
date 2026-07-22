use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::hold_automaton::HoldAutomatonState;

use super::piece_multiset_group::PieceMultisetKey;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BagPlacementState {
    pub cursor: u16,
    pub hold_piece: Option<PieceKind>,
    pub bag_epoch: u16,
    pub bag_remainder: PieceMultisetKey,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BagHoldBranchKind {
    Current,
    SwapHeld,
    StoreCurrent,
}

impl BagHoldBranchKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::SwapHeld => "swap-held",
            Self::StoreCurrent => "store-current",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BagSupplyBranch {
    pub used_piece: PieceKind,
    pub next_state: BagPlacementState,
    pub kind: BagHoldBranchKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BagMultisetProjectionError {
    EmptyBagProfile,
    InvalidHoldState,
    InvalidBagRemainder,
    CursorExhausted,
    BagEpochExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BagPlacementAutomaton {
    full_bag: PieceMultisetKey,
    hold_enabled: bool,
}

impl BagPlacementAutomaton {
    pub fn from_initial_hold(
        bag_pattern: &[PieceKind],
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        placed_piece_count: usize,
    ) -> Result<(Self, BagPlacementState), BagMultisetProjectionError> {
        let full_bag = PieceMultisetKey::from_pieces(bag_pattern.iter().copied());
        if full_bag.total_count() == 0 {
            return Err(BagMultisetProjectionError::EmptyBagProfile);
        }
        if initial_hold.hold_empty() != initial_hold.hold_piece().is_none()
            || (!hold_enabled && initial_hold.hold_piece().is_some())
        {
            return Err(BagMultisetProjectionError::InvalidHoldState);
        }

        let initial_remainder = decode_remainder(
            initial_hold.bag_remainder_key(),
            full_bag,
            initial_hold.cursor() == 0,
        )?;
        let maximum_draw_count = placed_piece_count.saturating_add(usize::from(
            hold_enabled && initial_hold.hold_piece().is_none(),
        ));
        let maximum_draw_count_u16 = u16::try_from(maximum_draw_count)
            .map_err(|_| BagMultisetProjectionError::CursorExhausted)?;
        initial_hold
            .cursor()
            .checked_add(maximum_draw_count_u16)
            .ok_or(BagMultisetProjectionError::CursorExhausted)?;
        let refill_draw_count =
            maximum_draw_count.saturating_sub(usize::from(initial_remainder.total_count()));
        let full_bag_size = usize::from(full_bag.total_count());
        let maximum_refills = if refill_draw_count == 0 {
            0
        } else {
            (refill_draw_count + full_bag_size - 1) / full_bag_size
        };
        let maximum_refills = u16::try_from(maximum_refills)
            .map_err(|_| BagMultisetProjectionError::BagEpochExhausted)?;
        initial_hold
            .bag_epoch()
            .checked_add(maximum_refills)
            .ok_or(BagMultisetProjectionError::BagEpochExhausted)?;
        Ok((
            Self {
                full_bag,
                hold_enabled,
            },
            BagPlacementState {
                cursor: initial_hold.cursor(),
                hold_piece: initial_hold.hold_piece(),
                bag_epoch: initial_hold.bag_epoch(),
                bag_remainder: initial_remainder,
            },
        ))
    }

    pub fn write_matching_branches(
        &self,
        state: BagPlacementState,
        desired_piece: PieceKind,
        branches: &mut Vec<BagSupplyBranch>,
    ) -> Result<(), BagMultisetProjectionError> {
        self.validate_state(state)?;
        branches.clear();

        if let Some(next_state) = self.draw_piece(state, desired_piece)? {
            branches.push(BagSupplyBranch {
                used_piece: desired_piece,
                next_state,
                kind: BagHoldBranchKind::Current,
            });
        }
        if !self.hold_enabled {
            return Ok(());
        }

        if state.hold_piece == Some(desired_piece) {
            for current_piece in PieceKind::STANDARD_TETROMINOES {
                let Some(mut next_state) = self.draw_piece(state, current_piece)? else {
                    continue;
                };
                next_state.hold_piece = Some(current_piece);
                branches.push(BagSupplyBranch {
                    used_piece: desired_piece,
                    next_state,
                    kind: BagHoldBranchKind::SwapHeld,
                });
            }
        } else if state.hold_piece.is_none() {
            for current_piece in PieceKind::STANDARD_TETROMINOES {
                let Some(after_current) = self.draw_piece(state, current_piece)? else {
                    continue;
                };
                let Some(mut next_state) = self.draw_piece(after_current, desired_piece)? else {
                    continue;
                };
                next_state.hold_piece = Some(current_piece);
                branches.push(BagSupplyBranch {
                    used_piece: desired_piece,
                    next_state,
                    kind: BagHoldBranchKind::StoreCurrent,
                });
            }
        }
        branches.sort_unstable();
        branches.dedup();
        Ok(())
    }

    fn validate_state(&self, state: BagPlacementState) -> Result<(), BagMultisetProjectionError> {
        if !self.hold_enabled && state.hold_piece.is_some() {
            return Err(BagMultisetProjectionError::InvalidHoldState);
        }
        if PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .any(|piece| state.bag_remainder.count(piece) > self.full_bag.count(piece))
        {
            return Err(BagMultisetProjectionError::InvalidBagRemainder);
        }
        Ok(())
    }

    fn draw_piece(
        &self,
        mut state: BagPlacementState,
        piece: PieceKind,
    ) -> Result<Option<BagPlacementState>, BagMultisetProjectionError> {
        if state.bag_remainder.total_count() == 0 {
            if state.cursor != 0 {
                state.bag_epoch = state
                    .bag_epoch
                    .checked_add(1)
                    .ok_or(BagMultisetProjectionError::BagEpochExhausted)?;
            }
            state.bag_remainder = self.full_bag;
        }
        if !state.bag_remainder.remove(piece) {
            return Ok(None);
        }
        state.cursor = state
            .cursor
            .checked_add(1)
            .ok_or(BagMultisetProjectionError::CursorExhausted)?;
        Ok(Some(state))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BagMultisetFrontierState {
    supply: BagPlacementState,
    placed_multiset: PieceMultisetKey,
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
    let (automaton, initial_state) = BagPlacementAutomaton::from_initial_hold(
        bag_pattern,
        initial_hold,
        hold_enabled,
        placed_piece_count,
    )?;
    let mut current = vec![BagMultisetFrontierState {
        supply: initial_state,
        placed_multiset: PieceMultisetKey::default(),
    }];
    let mut next = Vec::new();
    let mut branches = Vec::with_capacity(16);

    for _ in 0..placed_piece_count {
        next.clear();
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

    let mut multisets = current
        .into_iter()
        .map(|state| state.placed_multiset)
        .collect::<Vec<_>>();
    multisets.sort_unstable();
    multisets.dedup();
    Ok(multisets)
}

fn decode_remainder(
    key: u64,
    full_bag: PieceMultisetKey,
    fresh_source: bool,
) -> Result<PieceMultisetKey, BagMultisetProjectionError> {
    if key == 0 {
        return Ok(if fresh_source {
            full_bag
        } else {
            PieceMultisetKey::default()
        });
    }
    let storage_mask = (1usize..=7).fold(0_u64, |mask, piece| mask | (0xf_u64 << (piece * 4)));
    if key & !storage_mask != 0 {
        return Err(BagMultisetProjectionError::InvalidBagRemainder);
    }
    let mut counts = [0_u8; 7];
    for (index, count) in counts.iter_mut().enumerate() {
        *count = ((key >> ((index + 1) * 4)) & 0xf) as u8;
        if *count > full_bag.count(PieceKind::STANDARD_TETROMINOES[index]) {
            return Err(BagMultisetProjectionError::InvalidBagRemainder);
        }
    }
    Ok(PieceMultisetKey::from_counts(counts))
}

#[cfg(test)]
mod tests {
    use crate::{hold_automaton::SupplyProvenanceId, piece_source::PieceSourceId};

    use super::*;

    #[test]
    fn empty_4l_bag_projection_includes_p7p4_hold_carry() {
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
}
