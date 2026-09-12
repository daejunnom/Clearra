// SRP rationale: this module owns only one exact multiset-bag draw transition.
// It does not decide what a caller may observe or combine draws with hold/graph state.
use core::fmt;

use crate::Pc4GraphPiece;

/// Canonical order used for distinct bag outcomes.
pub const PC4_BAG_PIECES: [Pc4GraphPiece; 7] = [
    Pc4GraphPiece::I,
    Pc4GraphPiece::O,
    Pc4GraphPiece::T,
    Pc4GraphPiece::S,
    Pc4GraphPiece::Z,
    Pc4GraphPiece::J,
    Pc4GraphPiece::L,
];

/// Immutable multiplicity profile used whenever a bag is refilled.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4BagProfile {
    counts: [u32; 7],
    total: u32,
}

impl Pc4BagProfile {
    pub fn new(counts: [u32; 7]) -> Result<Self, Pc4BagProfileError> {
        let mut total = 0_u32;
        for count in counts {
            total = total
                .checked_add(count)
                .ok_or(Pc4BagProfileError::TotalOverflow)?;
        }
        if total == 0 {
            return Err(Pc4BagProfileError::Empty);
        }
        Ok(Self { counts, total })
    }

    pub const fn standard_seven_bag() -> Self {
        Self {
            counts: [1; 7],
            total: 7,
        }
    }

    pub const fn counts(self) -> [u32; 7] {
        self.counts
    }

    pub const fn total(self) -> u32 {
        self.total
    }

    pub const fn multiplicity(self, piece: Pc4GraphPiece) -> u32 {
        self.counts[piece_index(piece)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4BagProfileError {
    Empty,
    TotalOverflow,
}

impl Pc4BagProfileError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Empty => "pc4_bag_profile_empty",
            Self::TotalOverflow => "pc4_bag_profile_total_overflow",
        }
    }
}

impl fmt::Display for Pc4BagProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for Pc4BagProfileError {}

/// Exact bag identity at one draw boundary.
///
/// An empty remainder is valid and means that the next draw refills from the
/// immutable profile and advances `epoch`. No provenance or observation policy
/// is inferred from this state.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4BagState {
    profile: Pc4BagProfile,
    remainder: [u32; 7],
    remainder_total: u32,
    epoch: u64,
}

impl Pc4BagState {
    pub fn new(
        profile: Pc4BagProfile,
        remainder: [u32; 7],
        epoch: u64,
    ) -> Result<Self, Pc4BagStateError> {
        let profile_counts = profile.counts();
        let mut remainder_total = 0_u32;
        for (index, count) in remainder.into_iter().enumerate() {
            let capacity = profile_counts[index];
            if count > capacity {
                return Err(Pc4BagStateError::RemainderExceedsProfile {
                    piece: PC4_BAG_PIECES[index],
                    remainder: count,
                    profile: capacity,
                });
            }
            // This cannot overflow after the component-wise profile check and
            // validated profile total, but keep the state boundary fail-closed.
            remainder_total = remainder_total
                .checked_add(count)
                .ok_or(Pc4BagStateError::RemainderTotalOverflow)?;
        }
        Ok(Self {
            profile,
            remainder,
            remainder_total,
            epoch,
        })
    }

    pub const fn profile(self) -> Pc4BagProfile {
        self.profile
    }

    pub const fn remainder(self) -> [u32; 7] {
        self.remainder
    }

    pub const fn remainder_total(self) -> u32 {
        self.remainder_total
    }

    pub const fn remaining(self, piece: Pc4GraphPiece) -> u32 {
        self.remainder[piece_index(piece)]
    }

    pub const fn epoch(self) -> u64 {
        self.epoch
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4BagStateError {
    RemainderExceedsProfile {
        piece: Pc4GraphPiece,
        remainder: u32,
        profile: u32,
    },
    RemainderTotalOverflow,
}

impl Pc4BagStateError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::RemainderExceedsProfile { .. } => "pc4_bag_remainder_exceeds_profile",
            Self::RemainderTotalOverflow => "pc4_bag_remainder_total_overflow",
        }
    }
}

impl fmt::Display for Pc4BagStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for Pc4BagStateError {}

/// Exact probability weight of one distinct next-piece outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4BagDrawWeight {
    multiplicity: u32,
    denominator: u32,
}

impl Pc4BagDrawWeight {
    pub const fn multiplicity(self) -> u32 {
        self.multiplicity
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }
}

/// One canonical distinct-piece draw and its exact successor state.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4BagDrawTransition {
    piece: Pc4GraphPiece,
    weight: Pc4BagDrawWeight,
    next_state: Pc4BagState,
}

impl Pc4BagDrawTransition {
    pub const fn piece(self) -> Pc4GraphPiece {
        self.piece
    }

    pub const fn weight(self) -> Pc4BagDrawWeight {
        self.weight
    }

    pub const fn next_state(self) -> Pc4BagState {
        self.next_state
    }
}

/// Transactional result for one draw boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4BagDrawBatch {
    source_state: Pc4BagState,
    draw_epoch: u64,
    refilled: bool,
    denominator: u32,
    transitions: Vec<Pc4BagDrawTransition>,
}

impl Pc4BagDrawBatch {
    pub const fn source_state(&self) -> Pc4BagState {
        self.source_state
    }

    pub const fn draw_epoch(&self) -> u64 {
        self.draw_epoch
    }

    pub const fn refilled(&self) -> bool {
        self.refilled
    }

    pub const fn denominator(&self) -> u32 {
        self.denominator
    }

    pub fn transitions(&self) -> &[Pc4BagDrawTransition] {
        &self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4BagDrawError {
    EpochOverflow { epoch: u64 },
    AllocationFailed,
}

impl Pc4BagDrawError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::EpochOverflow { .. } => "pc4_bag_epoch_overflow",
            Self::AllocationFailed => "pc4_bag_draw_allocation_failed",
        }
    }
}

impl fmt::Display for Pc4BagDrawError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for Pc4BagDrawError {}

/// Enumerates at most seven canonical, exact multiset outcomes for one draw.
///
/// This function deliberately does not accept hold, preview visibility, graph
/// state, pattern syntax, or callbacks. Those policies belong to the future DP
/// composition layer.
pub fn draw_pc4_bag(state: Pc4BagState) -> Result<Pc4BagDrawBatch, Pc4BagDrawError> {
    let (effective_remainder, denominator, draw_epoch, refilled) = if state.remainder_total == 0 {
        let draw_epoch = state
            .epoch
            .checked_add(1)
            .ok_or(Pc4BagDrawError::EpochOverflow { epoch: state.epoch })?;
        (
            state.profile.counts(),
            state.profile.total(),
            draw_epoch,
            true,
        )
    } else {
        (state.remainder, state.remainder_total, state.epoch, false)
    };

    let distinct_count = effective_remainder
        .iter()
        .filter(|&&count| count != 0)
        .count();
    let mut transitions = Vec::new();
    transitions
        .try_reserve_exact(distinct_count)
        .map_err(|_| Pc4BagDrawError::AllocationFailed)?;

    for (index, piece) in PC4_BAG_PIECES.into_iter().enumerate() {
        let multiplicity = effective_remainder[index];
        if multiplicity == 0 {
            continue;
        }
        let mut next_remainder = effective_remainder;
        next_remainder[index] -= 1;
        transitions.push(Pc4BagDrawTransition {
            piece,
            weight: Pc4BagDrawWeight {
                multiplicity,
                denominator,
            },
            next_state: Pc4BagState {
                profile: state.profile,
                remainder: next_remainder,
                remainder_total: denominator - 1,
                epoch: draw_epoch,
            },
        });
    }

    Ok(Pc4BagDrawBatch {
        source_state: state,
        draw_epoch,
        refilled,
        denominator,
        transitions,
    })
}

const fn piece_index(piece: Pc4GraphPiece) -> usize {
    match piece {
        Pc4GraphPiece::I => 0,
        Pc4GraphPiece::O => 1,
        Pc4GraphPiece::T => 2,
        Pc4GraphPiece::S => 3,
        Pc4GraphPiece::Z => 4,
        Pc4GraphPiece::J => 5,
        Pc4GraphPiece::L => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_standard_remainder_refills_and_emits_seven_exact_outcomes() {
        let source = Pc4BagState::new(Pc4BagProfile::standard_seven_bag(), [0; 7], 4)
            .expect("valid empty boundary");
        let batch = draw_pc4_bag(source).expect("standard refill");

        assert_eq!(batch.source_state(), source);
        assert!(batch.refilled());
        assert_eq!(batch.draw_epoch(), 5);
        assert_eq!(batch.denominator(), 7);
        assert_eq!(batch.transitions().len(), 7);
        for (index, transition) in batch.transitions().iter().copied().enumerate() {
            assert_eq!(transition.piece(), PC4_BAG_PIECES[index]);
            assert_eq!(transition.weight().multiplicity(), 1);
            assert_eq!(transition.weight().denominator(), 7);
            assert_eq!(transition.next_state().epoch(), 5);
            assert_eq!(transition.next_state().remainder_total(), 6);
            assert_eq!(transition.next_state().remaining(transition.piece()), 0);
        }
    }

    #[test]
    fn repeated_piece_profile_uses_multiplicity_without_duplicate_outcomes() {
        let profile = Pc4BagProfile::new([2, 1, 0, 0, 0, 0, 0]).expect("custom profile");
        let source =
            Pc4BagState::new(profile, [2, 1, 0, 0, 0, 0, 0], 9).expect("valid custom remainder");
        let batch = draw_pc4_bag(source).expect("custom draw");

        assert!(!batch.refilled());
        assert_eq!(batch.draw_epoch(), 9);
        assert_eq!(batch.denominator(), 3);
        assert_eq!(batch.transitions().len(), 2);
        assert_eq!(batch.transitions()[0].piece(), Pc4GraphPiece::I);
        assert_eq!(batch.transitions()[0].weight().multiplicity(), 2);
        assert_eq!(batch.transitions()[1].piece(), Pc4GraphPiece::O);
        assert_eq!(batch.transitions()[1].weight().multiplicity(), 1);
    }

    #[test]
    fn invalid_profile_and_remainder_are_typed() {
        assert_eq!(Pc4BagProfile::new([0; 7]), Err(Pc4BagProfileError::Empty));
        assert_eq!(
            Pc4BagProfile::new([u32::MAX, 1, 0, 0, 0, 0, 0]),
            Err(Pc4BagProfileError::TotalOverflow)
        );

        let profile = Pc4BagProfile::standard_seven_bag();
        assert_eq!(
            Pc4BagState::new(profile, [2, 0, 0, 0, 0, 0, 0], 0),
            Err(Pc4BagStateError::RemainderExceedsProfile {
                piece: Pc4GraphPiece::I,
                remainder: 2,
                profile: 1,
            })
        );
    }

    #[test]
    fn epoch_overflow_occurs_only_when_refilling() {
        let profile = Pc4BagProfile::standard_seven_bag();
        let empty = Pc4BagState::new(profile, [0; 7], u64::MAX).expect("valid state");
        assert_eq!(
            draw_pc4_bag(empty),
            Err(Pc4BagDrawError::EpochOverflow { epoch: u64::MAX })
        );

        let nonempty =
            Pc4BagState::new(profile, [1, 0, 0, 0, 0, 0, 0], u64::MAX).expect("valid state");
        let batch = draw_pc4_bag(nonempty).expect("no refill does not advance epoch");
        assert_eq!(batch.draw_epoch(), u64::MAX);
        assert_eq!(batch.transitions()[0].next_state().epoch(), u64::MAX);
    }

    #[test]
    fn exhaustive_binary_profiles_and_remainders_preserve_probability_and_state() {
        for profile_mask in 1_u8..=0x7f {
            let mut profile_counts = [0_u32; 7];
            for (index, count) in profile_counts.iter_mut().enumerate() {
                *count = u32::from((profile_mask >> index) & 1);
            }
            let profile = Pc4BagProfile::new(profile_counts).expect("nonempty binary profile");

            for remainder_mask in 0_u8..=0x7f {
                if remainder_mask & !profile_mask != 0 {
                    continue;
                }
                let mut remainder = [0_u32; 7];
                for (index, count) in remainder.iter_mut().enumerate() {
                    *count = u32::from((remainder_mask >> index) & 1);
                }
                let source = Pc4BagState::new(profile, remainder, 12).expect("valid subset");
                let batch = draw_pc4_bag(source).expect("bounded binary draw");
                let expected_counts = if remainder_mask == 0 {
                    profile_counts
                } else {
                    remainder
                };
                let expected_denominator: u32 = expected_counts.iter().sum();
                let expected_epoch = if remainder_mask == 0 { 13 } else { 12 };

                assert_eq!(batch.denominator(), expected_denominator);
                assert_eq!(batch.draw_epoch(), expected_epoch);
                assert_eq!(batch.refilled(), remainder_mask == 0);
                assert!(batch.transitions().len() <= 7);
                assert_eq!(
                    batch
                        .transitions()
                        .iter()
                        .map(|transition| transition.weight().multiplicity())
                        .sum::<u32>(),
                    expected_denominator
                );

                let mut previous_index = None;
                for transition in batch.transitions().iter().copied() {
                    let index = piece_index(transition.piece());
                    if let Some(previous_index) = previous_index {
                        assert!(previous_index < index);
                    }
                    previous_index = Some(index);
                    assert_eq!(transition.weight().denominator(), expected_denominator);
                    assert_eq!(transition.weight().multiplicity(), expected_counts[index]);
                    assert_eq!(
                        transition.next_state().remainder_total(),
                        expected_denominator - 1
                    );
                    assert_eq!(transition.next_state().epoch(), expected_epoch);
                    for (piece_index, expected_count) in expected_counts.iter().enumerate() {
                        let expected = *expected_count - u32::from(piece_index == index);
                        assert_eq!(transition.next_state().remainder()[piece_index], expected);
                    }
                }
            }
        }
    }
}
