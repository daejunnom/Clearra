//! Multi-obligation support for one common diagram, in original source IDs.
//!
//! A family union is only an upper bound. All obligations must retain the same
//! diagram ID. An unexpanded family prevents a negative conclusion even when
//! every materialized diagram has been removed. This module is experimental;
//! it does not construct the legacy complete-source minimum authority.

use crate::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};
use std::sync::atomic::{AtomicUsize, Ordering};

// Runtime ownership only; never serialized or used for candidate ordering.
static NEXT_SLOT_OWNER: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JointDomainError {
    DimensionMismatch,
    InvalidCandidate,
    InvalidPattern,
    InvalidCheckpoint,
    CapacityExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JointDomainDecision {
    /// A materialized common diagram exists; its histories may differ by queue.
    Witness { original_row: usize },
    /// Materialized members are empty, but unopened members may still work.
    Unknown,
    /// Every member of this closed family has been eliminated.
    ProvedEmpty,
}

/// A complete, immutable support index for the *materialized* candidates.
/// Completeness of the entire source family is a separate input to each slot.
#[derive(Debug)]
pub struct JointDiagramSupportIndex {
    candidate_count: usize,
    pattern_count: usize,
    words_per_pattern: usize,
    supports: Vec<u64>,
}

impl JointDiagramSupportIndex {
    pub fn from_rows(
        rows: &[PatternBitSet],
        pattern_count: usize,
        max_bytes: usize,
    ) -> Result<Self, JointDomainError> {
        if rows.iter().any(|row| row.pattern_count() != pattern_count) {
            return Err(JointDomainError::DimensionMismatch);
        }
        let words_per_pattern = rows.len().div_ceil(64);
        let words = words_per_pattern
            .checked_mul(pattern_count)
            .ok_or(JointDomainError::CapacityExceeded)?;
        let bytes = words
            .checked_mul(8)
            .ok_or(JointDomainError::CapacityExceeded)?;
        if bytes > max_bytes {
            return Err(JointDomainError::CapacityExceeded);
        }
        let mut supports = Vec::new();
        supports
            .try_reserve_exact(words)
            .map_err(|_| JointDomainError::CapacityExceeded)?;
        supports.resize(words, 0);
        for (candidate, row) in rows.iter().enumerate() {
            for pattern in row.covered_patterns_before(pattern_count) {
                let offset = pattern.index() * words_per_pattern + candidate / 64;
                supports[offset] |= 1_u64 << (candidate % 64);
            }
        }
        Ok(Self {
            candidate_count: rows.len(),
            pattern_count,
            words_per_pattern,
            supports,
        })
    }

    pub fn retained_bytes(&self) -> usize {
        self.supports.capacity() * core::mem::size_of::<u64>()
    }

    pub fn slot(
        &self,
        original_rows: &[usize],
        has_unexpanded_members: bool,
        max_trail_entries: usize,
    ) -> Result<JointDiagramSlot<'_>, JointDomainError> {
        if original_rows.iter().any(|&row| row >= self.candidate_count) {
            return Err(JointDomainError::InvalidCandidate);
        }
        let mut live = Vec::new();
        live.try_reserve_exact(self.words_per_pattern)
            .map_err(|_| JointDomainError::CapacityExceeded)?;
        live.resize(self.words_per_pattern, 0);
        for &row in original_rows {
            live[row / 64] |= 1_u64 << (row % 64);
        }
        Ok(JointDiagramSlot {
            index: self,
            live,
            trail: Vec::new(),
            checkpoints: Vec::new(),
            next_checkpoint: 0,
            owner: NEXT_SLOT_OWNER
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |owner| {
                    owner.checked_add(1)
                })
                .map_err(|_| JointDomainError::CapacityExceeded)?,
            has_unexpanded_members,
            max_trail_entries,
        })
    }

    fn support(&self, pattern: PatternId) -> Result<&[u64], JointDomainError> {
        if pattern.index() >= self.pattern_count {
            return Err(JointDomainError::InvalidPattern);
        }
        let start = pattern.index() * self.words_per_pattern;
        Ok(&self.supports[start..start + self.words_per_pattern])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JointDomainCheckpoint {
    owner: usize,
    serial: u64,
    trail_len: usize,
}

/// Reversible intersection is over logical diagram IDs, never temporal IDs.
/// Checkpoints are LIFO, belong to this slot, and cannot be reused after rewind.
pub struct JointDiagramSlot<'a> {
    index: &'a JointDiagramSupportIndex,
    live: Vec<u64>,
    trail: Vec<(usize, u64)>,
    checkpoints: Vec<JointDomainCheckpoint>,
    next_checkpoint: u64,
    owner: usize,
    has_unexpanded_members: bool,
    max_trail_entries: usize,
}

impl JointDiagramSlot<'_> {
    pub fn checkpoint(&mut self) -> Result<JointDomainCheckpoint, JointDomainError> {
        if self.checkpoints.len() > self.index.pattern_count {
            return Err(JointDomainError::CapacityExceeded);
        }
        let serial = self
            .next_checkpoint
            .checked_add(1)
            .ok_or(JointDomainError::CapacityExceeded)?;
        self.checkpoints
            .try_reserve(1)
            .map_err(|_| JointDomainError::CapacityExceeded)?;
        let checkpoint = JointDomainCheckpoint {
            owner: self.owner,
            serial,
            trail_len: self.trail.len(),
        };
        self.checkpoints.push(checkpoint);
        self.next_checkpoint = serial;
        Ok(checkpoint)
    }

    pub fn rewind(&mut self, checkpoint: JointDomainCheckpoint) -> Result<(), JointDomainError> {
        if self.checkpoints.last() != Some(&checkpoint) {
            return Err(JointDomainError::InvalidCheckpoint);
        }
        while self.trail.len() > checkpoint.trail_len {
            let (word, value) = self
                .trail
                .pop()
                .ok_or(JointDomainError::InvalidCheckpoint)?;
            self.live[word] = value;
        }
        self.checkpoints.pop();
        Ok(())
    }

    /// Reserve the whole delta before writing it; admission failure leaves the
    /// slot unchanged and cannot masquerade as an empty-domain proof.
    pub fn require(&mut self, pattern: PatternId) -> Result<JointDomainDecision, JointDomainError> {
        let support = self.index.support(pattern)?;
        let changed = self
            .live
            .iter()
            .zip(support)
            .filter(|(live, allowed)| **live & **allowed != **live)
            .count();
        let must_trail = !self.checkpoints.is_empty();
        if must_trail {
            let entries = self
                .trail
                .len()
                .checked_add(changed)
                .ok_or(JointDomainError::CapacityExceeded)?;
            if entries > self.max_trail_entries {
                return Err(JointDomainError::CapacityExceeded);
            }
            self.trail
                .try_reserve_exact(changed)
                .map_err(|_| JointDomainError::CapacityExceeded)?;
        }
        for (word, (live, allowed)) in self.live.iter_mut().zip(support).enumerate() {
            let narrowed = *live & *allowed;
            if narrowed != *live {
                if must_trail {
                    self.trail.push((word, *live));
                }
                *live = narrowed;
            }
        }
        Ok(self.decision())
    }

    pub fn decision(&self) -> JointDomainDecision {
        for (word, &live) in self.live.iter().enumerate() {
            if live != 0 {
                return JointDomainDecision::Witness {
                    original_row: word * 64 + live.trailing_zeros() as usize,
                };
            }
        }
        if self.has_unexpanded_members {
            JointDomainDecision::Unknown
        } else {
            JointDomainDecision::ProvedEmpty
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.live.capacity() * core::mem::size_of::<u64>()
            + self.trail.capacity() * core::mem::size_of::<(usize, u64)>()
            + self.checkpoints.capacity() * core::mem::size_of::<JointDomainCheckpoint>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(masks: &[u64], patterns: usize) -> Vec<PatternBitSet> {
        masks
            .iter()
            .map(|&mask| PatternBitSet::from_words(patterns, vec![mask]).unwrap())
            .collect()
    }

    #[test]
    fn three_queues_can_conflict_without_any_pair_conflicting() {
        let index =
            JointDiagramSupportIndex::from_rows(&rows(&[0b011, 0b101, 0b110], 3), 3, 4096).unwrap();
        let mut slot = index.slot(&[0, 1, 2], false, 128).unwrap();
        for first in 0..3 {
            for second in first + 1..3 {
                let checkpoint = slot.checkpoint().unwrap();
                slot.require(PatternId::new(first)).unwrap();
                assert!(matches!(
                    slot.require(PatternId::new(second)).unwrap(),
                    JointDomainDecision::Witness { .. }
                ));
                slot.rewind(checkpoint).unwrap();
            }
        }
        slot.require(PatternId::new(0)).unwrap();
        slot.require(PatternId::new(1)).unwrap();
        assert_eq!(
            slot.require(PatternId::new(2)).unwrap(),
            JointDomainDecision::ProvedEmpty
        );
    }

    #[test]
    fn unopened_members_and_failed_reservations_are_not_negative_proofs() {
        let index = JointDiagramSupportIndex::from_rows(&rows(&[1, 2], 2), 2, 4096).unwrap();
        let mut open = index.slot(&[0, 1], true, 128).unwrap();
        open.require(PatternId::new(0)).unwrap();
        assert_eq!(
            open.require(PatternId::new(1)).unwrap(),
            JointDomainDecision::Unknown
        );
        let mut limited = index.slot(&[0, 1], false, 0).unwrap();
        let checkpoint = limited.checkpoint().unwrap();
        assert_eq!(
            limited.require(PatternId::new(1)),
            Err(JointDomainError::CapacityExceeded)
        );
        assert_eq!(
            limited.decision(),
            JointDomainDecision::Witness { original_row: 0 }
        );
        limited.rewind(checkpoint).unwrap();
        assert_eq!(
            limited.rewind(checkpoint),
            Err(JointDomainError::InvalidCheckpoint)
        );
        let foreign = open.checkpoint().unwrap();
        let own = limited.checkpoint().unwrap();
        assert_eq!(
            limited.rewind(foreign),
            Err(JointDomainError::InvalidCheckpoint)
        );
        limited.rewind(own).unwrap();
    }

    #[test]
    fn every_small_matrix_matches_a_direct_same_diagram_quantifier() {
        for encoded in 0..512_usize {
            let masks = [
                encoded as u64 & 7,
                (encoded >> 3) as u64 & 7,
                (encoded >> 6) as u64 & 7,
            ];
            let index = JointDiagramSupportIndex::from_rows(&rows(&masks, 3), 3, 4096).unwrap();
            let mut slot = index.slot(&[0, 1, 2], false, 128).unwrap();
            for requirements in 0..8_u64 {
                let checkpoint = slot.checkpoint().unwrap();
                for pattern in 0..3 {
                    if requirements & (1 << pattern) != 0 {
                        slot.require(PatternId::new(pattern)).unwrap();
                    }
                }
                let expected = masks
                    .iter()
                    .position(|row| row & requirements == requirements)
                    .map_or(JointDomainDecision::ProvedEmpty, |original_row| {
                        JointDomainDecision::Witness { original_row }
                    });
                assert_eq!(slot.decision(), expected);
                slot.rewind(checkpoint).unwrap();
            }
        }
    }
}
