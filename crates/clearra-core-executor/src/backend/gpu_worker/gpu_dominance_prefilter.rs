use std::collections::BTreeMap;

use super::{GpuCandidateHash, GpuPackingCandidate};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuDominancePrefilterReport {
    retained_candidates: Vec<GpuPackingCandidate>,
    dropped_optional_count: usize,
    required_candidate_dropped: bool,
}

impl GpuDominancePrefilterReport {
    pub fn retained_candidates(&self) -> &[GpuPackingCandidate] {
        &self.retained_candidates
    }
}
impl GpuDominancePrefilterReport {
    pub const fn dropped_optional_count(&self) -> usize {
        self.dropped_optional_count
    }
}
impl GpuDominancePrefilterReport {
    pub const fn required_candidate_dropped(&self) -> bool {
        self.required_candidate_dropped
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuDominancePrefilter;

impl GpuDominancePrefilter {
    pub fn retain_required_and_deduplicate_optional(
        candidates: &[GpuPackingCandidate],
    ) -> GpuDominancePrefilterReport {
        let mut retained = Vec::new();
        let mut optional_by_hash = BTreeMap::<u64, Vec<GpuPackingCandidate>>::new();
        let mut dropped_optional_count = 0usize;

        for candidate in candidates {
            if candidate.required_candidate() {
                retained.push(candidate.clone());
                continue;
            }

            let hash = GpuCandidateHash::hash_candidate(candidate);
            let bucket = optional_by_hash.entry(hash).or_default();
            if bucket
                .iter()
                .any(|existing| existing.exact_identity_matches(candidate))
            {
                dropped_optional_count += 1;
            } else {
                bucket.push(candidate.clone());
            }
        }

        retained.extend(optional_by_hash.into_values().flatten());
        GpuDominancePrefilterReport {
            retained_candidates: retained,
            dropped_optional_count,
            required_candidate_dropped: false,
        }
    }
}
