use std::{mem::size_of, sync::Arc};

use clearra_coverage::pattern::{
    pattern_bitset::{CoveredPatternIter, PatternBitSet},
    pattern_id::PatternId,
};

use super::PackingRunnerError;

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidatePatternGroup {
    pattern_count: u32,
    member_count: u32,
    patterns: Arc<PatternBitSet>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CandidatePatternIndex {
    pattern_groups: Vec<CandidatePatternGroup>,
    candidate_group_indices: Vec<u32>,
}

impl CandidatePatternIndex {
    pub(crate) fn push_shared_pattern_group(
        &mut self,
        patterns: Arc<PatternBitSet>,
    ) -> Result<u32, PackingRunnerError> {
        if let Some(index) = self
            .pattern_groups
            .iter()
            .position(|group| Arc::ptr_eq(&group.patterns, &patterns))
        {
            return u32::try_from(index)
                .map_err(|_| PackingRunnerError::PatternGroupCapacityExceeded);
        }
        let index = u32::try_from(self.pattern_groups.len())
            .map_err(|_| PackingRunnerError::PatternGroupCapacityExceeded)?;
        let pattern_count = u32::try_from(patterns.pattern_count())
            .map_err(|_| PackingRunnerError::PatternGroupCapacityExceeded)?;
        self.pattern_groups.push(CandidatePatternGroup {
            pattern_count,
            member_count: patterns.count_ones(),
            patterns,
        });
        Ok(index)
    }

    pub(crate) fn bind_candidate(&mut self, group_index: u32) {
        debug_assert!((group_index as usize) < self.pattern_groups.len());
        self.candidate_group_indices.push(group_index);
    }

    pub(crate) fn pattern_group_count(&self) -> usize {
        self.pattern_groups.len()
    }

    pub(crate) fn shared_pattern_group(&self, group_index: usize) -> Option<Arc<PatternBitSet>> {
        self.pattern_groups
            .get(group_index)
            .map(|group| Arc::clone(&group.patterns))
    }

    pub(crate) fn candidate_group_index(&self, candidate_index: usize) -> Option<usize> {
        self.candidate_group_indices
            .get(candidate_index)
            .map(|index| *index as usize)
    }

    pub(crate) fn patterns_for_candidate_before(
        &self,
        candidate_index: usize,
        end_exclusive: usize,
    ) -> CandidatePatternIter<'_> {
        CandidatePatternIter::new(self.group_for_candidate(candidate_index), end_exclusive)
    }

    pub(crate) fn pattern_count_before(
        &self,
        candidate_index: usize,
        end_exclusive: usize,
    ) -> usize {
        let Some(group) = self.group_for_candidate(candidate_index) else {
            return 0;
        };
        let limit = end_exclusive.min(group.pattern_count as usize);
        if limit == 0 {
            return 0;
        }
        if limit == group.pattern_count as usize {
            return group.member_count as usize;
        }
        group.patterns.covered_patterns_before(limit).count()
    }

    pub(crate) fn contains_pattern(&self, candidate_index: usize, pattern_id: u32) -> bool {
        let Some(group) = self.group_for_candidate(candidate_index) else {
            return false;
        };
        if pattern_id >= group.pattern_count {
            return false;
        }
        group.patterns.contains(PatternId::new(pattern_id as usize))
    }

    fn group_for_candidate(&self, candidate_index: usize) -> Option<&CandidatePatternGroup> {
        self.candidate_group_indices
            .get(candidate_index)
            .and_then(|group_index| self.pattern_groups.get(*group_index as usize))
    }

    pub(crate) fn append(&mut self, other: Self) -> Result<(), PackingRunnerError> {
        let mut remapped_groups = Vec::new();
        remapped_groups
            .try_reserve_exact(other.pattern_groups.len())
            .map_err(|_| PackingRunnerError::PatternGroupCapacityExceeded)?;
        for group in other.pattern_groups {
            remapped_groups.push(self.push_shared_pattern_group(group.patterns)?);
        }
        self.candidate_group_indices
            .try_reserve(other.candidate_group_indices.len())
            .map_err(|_| PackingRunnerError::PatternGroupCapacityExceeded)?;
        for group_index in other.candidate_group_indices {
            let remapped = remapped_groups
                .get(group_index as usize)
                .copied()
                .ok_or(PackingRunnerError::PatternGroupCapacityExceeded)?;
            self.candidate_group_indices.push(remapped);
        }
        Ok(())
    }

    pub(crate) fn truncate_candidates(&mut self, candidate_count: usize) {
        self.candidate_group_indices.truncate(candidate_count);
    }

    pub(crate) fn candidate_count(&self) -> usize {
        self.candidate_group_indices.len()
    }

    pub(crate) fn resident_bytes(&self) -> usize {
        let pattern_bytes = self
            .pattern_groups
            .iter()
            .map(|group| group.patterns.retained_bytes())
            .sum::<usize>();
        self.pattern_groups
            .capacity()
            .saturating_mul(size_of::<CandidatePatternGroup>())
            .saturating_add(pattern_bytes)
            .saturating_add(
                self.candidate_group_indices
                    .capacity()
                    .saturating_mul(size_of::<u32>()),
            )
    }

    pub(crate) fn resident_allocation_count(&self) -> usize {
        let pattern_allocations = self
            .pattern_groups
            .iter()
            .filter(|group| !group.patterns.is_empty())
            .count();
        usize::from(self.pattern_groups.capacity() != 0)
            + usize::from(self.candidate_group_indices.capacity() != 0)
            + pattern_allocations
    }
}

pub(crate) struct CandidatePatternIter<'a> {
    patterns: Option<CoveredPatternIter<'a>>,
}

impl<'a> CandidatePatternIter<'a> {
    fn new(group: Option<&'a CandidatePatternGroup>, end_exclusive: usize) -> Self {
        Self {
            patterns: group.map(|group| {
                group
                    .patterns
                    .covered_patterns_before(end_exclusive.min(group.pattern_count as usize))
            }),
        }
    }
}

impl Iterator for CandidatePatternIter<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        self.patterns
            .as_mut()?
            .next()
            .and_then(|pattern| u32::try_from(pattern.index()).ok())
    }
}
