use std::mem::size_of;

use crate::{
    packing_problem::{CPackingCandidate, C_PACKING_MAX_OPERATIONS},
    problem::CPackingProblem,
    PackingCandidateBatch, PackingCandidateBatchError,
};

const PACKING_INVALID_ARGUMENT: i32 = 1;
const PACKING_CAPACITY_EXCEEDED: i32 = 6;
const TRUNCATION_NONE: u16 = 0;
const TRUNCATION_CANDIDATE_BUDGET_EXCEEDED: u16 = 2;
const TRUNCATION_MEMORY_EXCEEDED: u16 = 10;
const EMPTY_BUCKET: u32 = u32::MAX;
const INITIAL_BUCKET_COUNT: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativePackingCandidateContext {
    pub accepted_candidate_count: usize,
    pub engine_resident_bytes: usize,
    pub max_candidate_rows: usize,
    pub max_total_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePackingCandidateSinkError {
    CandidateBudgetExceeded,
    MemoryExceeded,
    Invalid,
}

impl NativePackingCandidateSinkError {
    pub(crate) const fn status(self) -> i32 {
        match self {
            Self::CandidateBudgetExceeded | Self::MemoryExceeded => PACKING_CAPACITY_EXCEEDED,
            Self::Invalid => PACKING_INVALID_ARGUMENT,
        }
    }

    pub(crate) const fn truncation_reason(self) -> u16 {
        match self {
            Self::CandidateBudgetExceeded => TRUNCATION_CANDIDATE_BUDGET_EXCEEDED,
            Self::MemoryExceeded => TRUNCATION_MEMORY_EXCEEDED,
            Self::Invalid => TRUNCATION_NONE,
        }
    }
}

pub trait NativePackingCandidateConsumer {
    fn consume(
        &mut self,
        candidate: CPackingCandidate,
        context: NativePackingCandidateContext,
    ) -> Result<bool, NativePackingCandidateSinkError>;

    fn resident_bytes(&self) -> usize;
}

pub struct NativeCandidateReducer {
    candidates: PackingCandidateBatch,
    bucket_heads: Vec<u32>,
    next_indices: Vec<u32>,
}

impl NativeCandidateReducer {
    pub fn new(problem: &CPackingProblem) -> Result<Self, PackingCandidateBatchError> {
        let board_height = if problem.board.search_height == 0 {
            problem.board.visible_height
        } else {
            problem.board.search_height
        };
        Ok(Self {
            candidates: PackingCandidateBatch::new(problem.board.width, board_height)?,
            bucket_heads: Vec::new(),
            next_indices: Vec::new(),
        })
    }

    pub fn into_candidates(mut self) -> PackingCandidateBatch {
        self.candidates.canonicalize_identities();
        self.candidates
    }

    /// Transfers the compact SoA storage without sorting it. Callers that
    /// combine disjoint worker batches must canonicalize the merged batch
    /// before exposing it as a product result.
    pub fn into_uncanonicalized_candidates(self) -> PackingCandidateBatch {
        self.candidates
    }

    pub fn accepted_candidate_count(&self) -> usize {
        self.candidates.len()
    }

    pub fn resident_bytes(&self) -> usize {
        self.candidates
            .resident_bytes()
            .saturating_add(
                self.bucket_heads
                    .capacity()
                    .saturating_mul(size_of::<u32>()),
            )
            .saturating_add(
                self.next_indices
                    .capacity()
                    .saturating_mul(size_of::<u32>()),
            )
    }

    fn total_fits(
        engine_resident_bytes: usize,
        host_resident_bytes: usize,
        max_total_bytes: usize,
    ) -> bool {
        engine_resident_bytes
            .checked_add(host_resident_bytes)
            .is_some_and(|total| total <= max_total_bytes)
    }

    fn ensure_bucket_table(
        &mut self,
        max_candidate_rows: usize,
        engine_resident_bytes: usize,
        max_total_bytes: usize,
    ) -> Result<(), NativePackingCandidateSinkError> {
        if !self.bucket_heads.is_empty() {
            return Ok(());
        }
        let bucket_count = INITIAL_BUCKET_COUNT.min(max_candidate_rows.max(1));
        let projected = self
            .resident_bytes()
            .saturating_add(bucket_count.saturating_mul(size_of::<u32>()));
        if !Self::total_fits(engine_resident_bytes, projected, max_total_bytes) {
            return Err(NativePackingCandidateSinkError::MemoryExceeded);
        }
        self.bucket_heads
            .try_reserve_exact(bucket_count)
            .map_err(|_| NativePackingCandidateSinkError::MemoryExceeded)?;
        self.bucket_heads.resize(bucket_count, EMPTY_BUCKET);
        Ok(())
    }

    fn grow_buckets_if_needed(
        &mut self,
        max_candidate_rows: usize,
        engine_resident_bytes: usize,
        max_total_bytes: usize,
    ) -> Result<(), NativePackingCandidateSinkError> {
        if self.candidates.len().saturating_mul(4) < self.bucket_heads.len().saturating_mul(3)
            || self.bucket_heads.len() >= max_candidate_rows
        {
            return Ok(());
        }
        let new_count = self
            .bucket_heads
            .len()
            .saturating_mul(2)
            .min(max_candidate_rows);
        if new_count <= self.bucket_heads.len() {
            return Ok(());
        }

        let transient = self
            .resident_bytes()
            .saturating_add(new_count.saturating_mul(size_of::<u32>()));
        if !Self::total_fits(engine_resident_bytes, transient, max_total_bytes) {
            return Err(NativePackingCandidateSinkError::MemoryExceeded);
        }
        let mut grown = Vec::new();
        grown
            .try_reserve_exact(new_count)
            .map_err(|_| NativePackingCandidateSinkError::MemoryExceeded)?;
        grown.resize(new_count, EMPTY_BUCKET);
        for index in 0..self.candidates.len() {
            let bucket = candidate_bucket_from_key(
                self.candidates
                    .hash_key(index)
                    .expect("candidate index is in bounds"),
                new_count,
            );
            self.next_indices[index] = grown[bucket];
            grown[bucket] = index as u32;
        }
        self.bucket_heads = grown;
        Ok(())
    }

    pub(crate) fn insert(
        &mut self,
        mut candidate: CPackingCandidate,
        accepted_candidate_count: usize,
        engine_resident_bytes: usize,
        max_candidate_rows: usize,
        max_total_bytes: usize,
    ) -> Result<bool, NativePackingCandidateSinkError> {
        if accepted_candidate_count != self.candidates.len()
            || usize::from(candidate.operation_count) > C_PACKING_MAX_OPERATIONS
        {
            return Err(NativePackingCandidateSinkError::Invalid);
        }
        self.ensure_bucket_table(max_candidate_rows, engine_resident_bytes, max_total_bytes)?;

        let bucket = candidate_bucket(&candidate, self.bucket_heads.len());
        let mut existing = self.bucket_heads[bucket];
        while existing != EMPTY_BUCKET {
            let index = existing as usize;
            if self.candidates.exact_candidate_matches(index, &candidate) {
                return Ok(false);
            }
            existing = self.next_indices[index];
        }
        if self.candidates.len() >= max_candidate_rows {
            return Err(NativePackingCandidateSinkError::CandidateBudgetExceeded);
        }

        self.grow_buckets_if_needed(max_candidate_rows, engine_resident_bytes, max_total_bytes)?;
        self.next_indices
            .try_reserve(1)
            .map_err(|_| NativePackingCandidateSinkError::MemoryExceeded)?;
        let bucket = candidate_bucket(&candidate, self.bucket_heads.len());
        let candidate_id = (self.candidates.len() as u64) + 1;
        candidate.candidate_id = candidate_id;
        candidate.canonical_operation_set_id = candidate_id;
        let old_len = self.candidates.len();
        self.candidates
            .push(candidate)
            .map_err(|_| NativePackingCandidateSinkError::MemoryExceeded)?;
        if !Self::total_fits(
            engine_resident_bytes,
            self.resident_bytes(),
            max_total_bytes,
        ) {
            self.candidates.truncate(old_len);
            return Err(NativePackingCandidateSinkError::MemoryExceeded);
        }
        self.next_indices.push(self.bucket_heads[bucket]);
        self.bucket_heads[bucket] = old_len as u32;
        Ok(true)
    }
}

impl NativePackingCandidateConsumer for NativeCandidateReducer {
    fn consume(
        &mut self,
        candidate: CPackingCandidate,
        context: NativePackingCandidateContext,
    ) -> Result<bool, NativePackingCandidateSinkError> {
        self.insert(
            candidate,
            context.accepted_candidate_count,
            context.engine_resident_bytes,
            context.max_candidate_rows,
            context.max_total_bytes,
        )
    }

    fn resident_bytes(&self) -> usize {
        NativeCandidateReducer::resident_bytes(self)
    }
}

fn candidate_bucket(candidate: &CPackingCandidate, bucket_count: usize) -> usize {
    candidate_bucket_from_key(
        candidate.operation_set_key
            ^ candidate.tiling_key.rotate_left(17)
            ^ candidate.shape_key.rotate_left(31),
        bucket_count,
    )
}

fn candidate_bucket_from_key(mut key: u64, bucket_count: usize) -> usize {
    key ^= key >> 33;
    key = key.wrapping_mul(0xff51_afd7_ed55_8ccd);
    key ^= key >> 33;
    (key as usize) % bucket_count
}
