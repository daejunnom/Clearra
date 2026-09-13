//! SRP: bounded collection, in-place canonical ordering and the existing set
//! digest. This helper cannot mint completeness, source or profile evidence.
use core::{mem::size_of, num::NonZeroUsize};
use std::collections::{hash_set::IntoIter, HashSet};

use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
use sha2::{Digest, Sha256};

use super::{PcCandidateBoundaryError, PcCandidateSetDigest};

#[derive(Debug)]
pub(crate) enum CandidateCanonicalizationError {
    Boundary(PcCandidateBoundaryError),
    BufferLimit { limit: usize, required: usize },
    Cancelled,
    Incomplete,
    Terminated,
}

impl CandidateCanonicalizationError {
    pub(super) const fn reason(&self) -> &'static str {
        match self {
            Self::Boundary(error) => error.reason(),
            Self::BufferLimit { .. } => "pc_candidate_canonicalization_buffer_limit",
            Self::Cancelled => "pc_candidate_canonicalization_cancelled",
            Self::Incomplete => "pc_candidate_canonicalization_incomplete",
            Self::Terminated => "pc_candidate_canonicalization_terminated",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Collect,
    BuildHeap { remaining: usize },
    Sort { end: usize },
    Hash { next: usize },
    Complete,
}

#[derive(Clone, Copy)]
struct Sift {
    root: usize,
    end: usize,
}

/// Candidate-buffer envelope only: HashSet element capacity + output Vec
/// element capacity are charged simultaneously before allocating the latter.
/// Allocator/hash control metadata and the caller's other owners are outside
/// this envelope. It is not process RSS or whole-search memory authority.
pub(super) struct CooperativeCandidateCanonicalizer {
    input: Option<IntoIter<StandardBoard64TilingIdentity>>,
    count: usize,
    values: Vec<StandardBoard64TilingIdentity>,
    phase: Phase,
    sift: Option<Sift>,
    hasher: Option<Sha256>,
    digest: Option<PcCandidateSetDigest>,
    terminated: bool,
    work_done: usize,
    peak_buffer_bytes: usize,
}

impl CooperativeCandidateCanonicalizer {
    pub(super) fn begin(
        input: HashSet<StandardBoard64TilingIdentity>,
        buffer_bytes: NonZeroUsize,
    ) -> Result<Self, CandidateCanonicalizationError> {
        let count = input.len();
        let check = |vector_capacity| {
            let required = input
                .capacity()
                .checked_add(vector_capacity)
                .and_then(|capacity| {
                    capacity.checked_mul(size_of::<StandardBoard64TilingIdentity>())
                })
                .ok_or(CandidateCanonicalizationError::Boundary(
                    PcCandidateBoundaryError::CandidateOrdinalOverflow,
                ))?;
            if required > buffer_bytes.get() {
                return Err(CandidateCanonicalizationError::BufferLimit {
                    limit: buffer_bytes.get(),
                    required,
                });
            }
            Ok(required)
        };
        check(count)?;
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(|_| {
            CandidateCanonicalizationError::Boundary(
                PcCandidateBoundaryError::CandidateAllocationFailed,
            )
        })?;
        let peak_buffer_bytes = check(values.capacity())?;
        Ok(Self {
            input: Some(input.into_iter()),
            count,
            values,
            phase: Phase::Collect,
            sift: None,
            hasher: None,
            digest: None,
            terminated: false,
            work_done: 0,
            peak_buffer_bytes,
        })
    }

    pub(super) const fn work_done(&self) -> usize {
        self.work_done
    }
    pub(super) const fn peak_buffer_bytes(&self) -> usize {
        self.peak_buffer_bytes
    }

    /// One unit copies one candidate, visits one heap level (at most two
    /// comparisons and one swap), hashes one bounded Board64 candidate, or
    /// changes phase. No whole-vector sort/hash hides behind this quantum.
    pub(super) fn advance<G: Fn() -> bool>(
        &mut self,
        work: NonZeroUsize,
        cancelled: &G,
    ) -> Result<bool, CandidateCanonicalizationError> {
        if self.terminated {
            return Err(CandidateCanonicalizationError::Terminated);
        }
        let result = self.advance_inner(work, cancelled);
        if result.is_err() {
            self.terminated = true;
            self.input = None;
            self.values = Vec::new();
            self.hasher = None;
            self.digest = None;
        }
        result
    }

    fn advance_inner<G: Fn() -> bool>(
        &mut self,
        work: NonZeroUsize,
        cancelled: &G,
    ) -> Result<bool, CandidateCanonicalizationError> {
        for _ in 0..work.get() {
            if cancelled() {
                return Err(CandidateCanonicalizationError::Cancelled);
            }
            if self.phase == Phase::Complete {
                return Ok(true);
            }
            self.work_done =
                self.work_done
                    .checked_add(1)
                    .ok_or(CandidateCanonicalizationError::Boundary(
                        PcCandidateBoundaryError::CandidateOrdinalOverflow,
                    ))?;
            if let Some(sift) = self.sift {
                // root < end and a real Vec's element count cannot make this
                // overflow; checked arithmetic also handles a future carrier.
                let child = sift.root.checked_mul(2).and_then(|n| n.checked_add(1));
                if let Some(mut child) = child.filter(|child| *child < sift.end) {
                    if child + 1 < sift.end && self.values[child] < self.values[child + 1] {
                        child += 1;
                    }
                    if self.values[sift.root] < self.values[child] {
                        self.values.swap(sift.root, child);
                        self.sift = Some(Sift {
                            root: child,
                            end: sift.end,
                        });
                    } else {
                        self.sift = None;
                    }
                } else {
                    self.sift = None;
                }
                continue;
            }
            match self.phase {
                Phase::Collect => {
                    let input = self.input.as_mut().expect("collection owns its iterator");
                    if let Some(candidate) = input.next() {
                        self.values.push(candidate);
                    } else {
                        if self.values.len() != self.count {
                            return Err(CandidateCanonicalizationError::Boundary(
                                PcCandidateBoundaryError::CompletenessCountMismatch,
                            ));
                        }
                        self.input = None;
                        self.phase = Phase::BuildHeap {
                            remaining: self.values.len() / 2,
                        };
                    }
                }
                Phase::BuildHeap { remaining } => {
                    if remaining == 0 {
                        self.phase = Phase::Sort {
                            end: self.values.len(),
                        };
                    } else {
                        self.phase = Phase::BuildHeap {
                            remaining: remaining - 1,
                        };
                        self.sift = Some(Sift {
                            root: remaining - 1,
                            end: self.values.len(),
                        });
                    }
                }
                Phase::Sort { end } => {
                    if end > 1 {
                        self.values.swap(0, end - 1);
                        self.phase = Phase::Sort { end: end - 1 };
                        self.sift = Some(Sift {
                            root: 0,
                            end: end - 1,
                        });
                    } else {
                        let count = u64::try_from(self.count).map_err(|_| {
                            CandidateCanonicalizationError::Boundary(
                                PcCandidateBoundaryError::CandidateOrdinalOverflow,
                            )
                        })?;
                        self.hasher = Some(PcCandidateSetDigest::begin_hash(count));
                        self.phase = Phase::Hash { next: 0 };
                    }
                }
                Phase::Hash { next } => {
                    if let Some(candidate) = self.values.get(next) {
                        if next != 0 && self.values[next - 1] >= *candidate {
                            return Err(CandidateCanonicalizationError::Boundary(
                                PcCandidateBoundaryError::CandidatesNotStrictlyCanonical,
                            ));
                        }
                        PcCandidateSetDigest::hash_candidate(
                            self.hasher.as_mut().expect("hash phase"),
                            candidate,
                        );
                        self.phase = Phase::Hash { next: next + 1 };
                    } else {
                        self.digest = Some(PcCandidateSetDigest(
                            self.hasher.take().expect("hash phase").finalize().into(),
                        ));
                        self.phase = Phase::Complete;
                    }
                }
                Phase::Complete => unreachable!("handled before a work unit"),
            }
        }
        if cancelled() {
            return Err(CandidateCanonicalizationError::Cancelled);
        }
        Ok(self.phase == Phase::Complete)
    }

    // These parts describe ordering/digest only, never source completeness.
    pub(super) fn into_parts(
        self,
    ) -> Result<
        (Vec<StandardBoard64TilingIdentity>, PcCandidateSetDigest),
        CandidateCanonicalizationError,
    > {
        if self.terminated || self.phase != Phase::Complete {
            return Err(CandidateCanonicalizationError::Incomplete);
        }
        Ok((self.values, self.digest.expect("completed digest")))
    }
}

#[cfg(test)]
#[path = "pc_candidate_cooperative_canonicalizer_tests.rs"]
mod tests;
