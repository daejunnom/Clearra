//! Board-independent movement topology shared by every PC field size.
//!
//! Collision masks retain their existing storage and hot-path access. Pose
//! numbering, sky entry, translations and ordered kicks have one compiler;
//! changing the board representation cannot select different movement rules.

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::kicks::{normalized_kick_delta, KickTableProfile, KickTransition};

pub(super) const INVALID_STATE_INDEX: u16 = u16::MAX;
pub(super) const INVALID_STATE_MASK: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct State {
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
}

/// Four rotation planes share one anchor numbering. Compact results lend one
/// word per plane; full-height results lend four, without enlarging compact
/// cache entries or selecting a different membership algorithm.
#[inline]
fn lock_anchor_bit(
    word_count: usize,
    width: u8,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> Option<(usize, u64)> {
    if x < 0 || y < 0 || x >= width as i8 {
        return None;
    }
    let stride = word_count / RotationState::ALL.len();
    let anchor = y as usize * width as usize + x as usize;
    let word = anchor / u64::BITS as usize;
    if word >= stride {
        return None;
    }
    Some((
        rotation.quarter_turns() as usize * stride + word,
        1_u64 << (anchor % u64::BITS as usize),
    ))
}

#[inline]
pub(super) fn lock_anchor_insert(
    words: &mut [u64],
    width: u8,
    rotation: RotationState,
    x: i8,
    y: i8,
) {
    if let Some((word, bit)) = lock_anchor_bit(words.len(), width, rotation, x, y) {
        words[word] |= bit;
    }
}

#[inline]
pub(super) fn lock_anchor_contains(
    words: &[u64],
    width: u8,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> bool {
    lock_anchor_bit(words.len(), width, rotation, x, y)
        .is_some_and(|(word, bit)| words[word] & bit != 0)
}

/// Borrow the existing validity information without another allocation or a
/// second topology implementation. This adapter is used only when compiling
/// the immutable template, never in a movement search.
#[derive(Clone, Copy)]
pub(super) enum PoseValidity<'a> {
    CompactMasks(&'a [u64]),
    FullHeightFlags(&'a [bool]),
}

impl PoseValidity<'_> {
    fn len(self) -> usize {
        match self {
            Self::CompactMasks(masks) => masks.len(),
            Self::FullHeightFlags(flags) => flags.len(),
        }
    }

    fn contains(self, index: usize) -> bool {
        match self {
            Self::CompactMasks(masks) => masks[index] != INVALID_STATE_MASK,
            Self::FullHeightFlags(flags) => flags[index],
        }
    }
}

pub(super) fn state_index(width: u8, ceiling: i8, state: State) -> Option<usize> {
    if state.x < 0 || state.x >= width as i8 || state.y < 0 || state.y > ceiling {
        return None;
    }
    Some(
        (state.rotation.quarter_turns() as usize * (ceiling as usize + 1) + state.y as usize)
            * width as usize
            + state.x as usize,
    )
}

pub(super) fn state_from_index(width: u8, ceiling: i8, index: usize) -> State {
    let rotation_stride = (ceiling as usize + 1) * width as usize;
    let rotation = RotationState::from_quarter_turns((index / rotation_stride) as u8)
        .expect("compiled reachability rotation is valid");
    let remainder = index % rotation_stride;
    State {
        rotation,
        x: (remainder % width as usize) as i8,
        y: (remainder / width as usize) as i8,
    }
}

pub(super) fn rotation_transition_index(from: RotationState, transition_slot: usize) -> usize {
    from.quarter_turns() as usize * 3 + transition_slot
}

pub(super) fn normalized_kick_deltas(
    piece: PieceKind,
    allow_180: bool,
    profile: &KickTableProfile,
) -> [Vec<(i8, i8)>; 12] {
    let mut deltas: [Vec<(i8, i8)>; 12] = std::array::from_fn(|_| Vec::new());
    for from in RotationState::ALL {
        for (slot, to) in [
            from.clockwise(),
            from.counter_clockwise(),
            from.rotated_180(),
        ]
        .into_iter()
        .enumerate()
        {
            if slot == 2 && !allow_180 {
                continue;
            }
            if let Some(sequence) = profile.sequence_for(KickTransition::new(piece, from, to)) {
                deltas[rotation_transition_index(from, slot)].extend(
                    sequence.offsets().iter().map(|offset| {
                        normalized_kick_delta(piece, from, to, offset.dx(), offset.dy())
                    }),
                );
            }
        }
    }
    deltas
}

pub(super) fn source_ceiling(
    height: u8,
    piece: PieceKind,
    allow_180: bool,
    profile: &KickTableProfile,
) -> i8 {
    let maximum_downward = profile
        .entries()
        .iter()
        .filter(|entry| entry.transition().piece() == piece)
        .filter(|entry| allow_180 || !entry.transition().is_180())
        .flat_map(|entry| {
            entry.sequence().offsets().iter().map(move |offset| {
                normalized_kick_delta(
                    piece,
                    entry.transition().from(),
                    entry.transition().to(),
                    offset.dx(),
                    offset.dy(),
                )
                .1
            })
        })
        .filter(|dy| *dy < 0)
        .map(|dy| -dy)
        .max()
        .unwrap_or(0);
    height.saturating_add(maximum_downward as u8) as i8
}

pub(super) fn compile_translations(
    width: u8,
    height: u8,
    ceiling: i8,
    valid: PoseValidity<'_>,
) -> (Vec<[u16; 3]>, Vec<u16>) {
    let mut translations = Vec::with_capacity(valid.len());
    let mut seeds = Vec::new();
    for source in 0..valid.len() {
        let state = state_from_index(width, ceiling, source);
        let candidates = [
            State {
                y: state.y - 1,
                ..state
            },
            State {
                x: state.x - 1,
                ..state
            },
            State {
                x: state.x + 1,
                ..state
            },
        ];
        let mut targets = [INVALID_STATE_INDEX; 3];
        for (slot, candidate) in candidates.into_iter().enumerate() {
            if let Some(target) =
                state_index(width, ceiling, candidate).filter(|&index| valid.contains(index))
            {
                targets[slot] = u16::try_from(target).unwrap_or(INVALID_STATE_INDEX);
            }
        }
        translations.push(targets);
        if state.y >= height as i8 && valid.contains(source) {
            if let Ok(source) = u16::try_from(source) {
                seeds.push(source);
            }
        }
    }
    (translations, seeds)
}

pub(super) fn compile_rotations(
    width: u8,
    ceiling: i8,
    valid: PoseValidity<'_>,
    kick_deltas: &[&[(i8, i8)]; 12],
) -> (Vec<u32>, Vec<u16>, Vec<u8>) {
    let mut offsets = Vec::with_capacity(valid.len() * 3 + 1);
    let mut targets = Vec::new();
    let mut kick_indices = Vec::new();
    offsets.push(0);
    for source in 0..valid.len() {
        let state = state_from_index(width, ceiling, source);
        for (slot, to) in [
            state.rotation.clockwise(),
            state.rotation.counter_clockwise(),
            state.rotation.rotated_180(),
        ]
        .into_iter()
        .enumerate()
        {
            let transition = rotation_transition_index(state.rotation, slot);
            for (kick_index, &(dx, dy)) in kick_deltas[transition].iter().enumerate() {
                let candidate = State {
                    rotation: to,
                    x: state.x + dx,
                    y: state.y + dy,
                };
                let Some(target) =
                    state_index(width, ceiling, candidate).filter(|&index| valid.contains(index))
                else {
                    continue;
                };
                if let Ok(target) = u16::try_from(target) {
                    targets.push(target);
                    // Keep the original ordered-table index even when an
                    // earlier offset points outside the valid pose domain.
                    kick_indices.push(u8::try_from(kick_index).unwrap_or(u8::MAX));
                }
            }
            offsets.push(targets.len() as u32);
        }
    }
    (offsets, targets, kick_indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_anchor_planes_share_membership_without_widening_compact_storage() {
        use crate::backend::wasm_cpu::reachability::ReachableLocks;

        assert_eq!(core::mem::size_of::<ReachableLocks>(), 32);
        let mut compact = [0_u64; 4];
        let mut full = [0_u64; 16];
        for rotation in RotationState::ALL {
            for anchor in [0, 63, 64, 127, 128, 191, 192, 239, 255, 256] {
                let (x, y) = ((anchor % 10) as i8, (anchor / 10) as i8);
                lock_anchor_insert(&mut compact, 10, rotation, x, y);
                lock_anchor_insert(&mut full, 10, rotation, x, y);
                assert_eq!(
                    lock_anchor_contains(&compact, 10, rotation, x, y),
                    anchor < 64
                );
                assert_eq!(
                    lock_anchor_contains(&full, 10, rotation, x, y),
                    anchor < 256
                );
            }
        }
        let before = full;
        for rotation in RotationState::ALL {
            for (x, y) in [(-1, 0), (0, -1), (10, 0), (127, 0), (0, 127)] {
                lock_anchor_insert(&mut full, 10, rotation, x, y);
                assert!(!lock_anchor_contains(&full, 10, rotation, x, y));
            }
        }
        assert_eq!(full, before);
        for rotation in RotationState::ALL {
            let plane = usize::from(rotation.quarter_turns());
            assert_eq!(compact[plane], full[plane * 4]);
            assert_eq!(full[plane * 4 + 3] & (1 << 63), 1 << 63);
        }
    }
}
