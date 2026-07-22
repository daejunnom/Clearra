use core::{cmp::Ordering, mem::size_of};

use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::{
        NormalizedTilingSolutionError, PiecePlacementMask, StandardBoard64TilingIdentity,
        STANDARD_BOARD64_TILING_MAX_PLACEMENTS,
    },
};

use crate::packing_problem::{CPackingCandidate, CPackingOperation, C_PACKING_MAX_OPERATIONS};

const EMPTY_DICTIONARY_INDEX: u32 = u32::MAX;
const INITIAL_OPERATION_BUCKET_COUNT: usize = 256;
const CANDIDATES_PER_SEGMENT: usize = 1_024;
const OPERATION_REFS_PER_SEGMENT: usize = CANDIDATES_PER_SEGMENT * C_PACKING_MAX_OPERATIONS;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CompactPackingCandidate {
    candidate_id: u64,
    canonical_operation_set_id: u64,
    final_board: u64,
    shape_mask: u64,
    shape_key: u64,
    tiling_key: u64,
    operation_set_key: u64,
    operation_start: u32,
    geometry_variant_domains: u16,
    operation_count: u8,
    cleared_lines: u8,
}

// Candidate metadata is the hot retained surface. One entry fits one 64-byte
// cache line; exact operation tuples live once in the batch dictionary.
const _: () = assert!(size_of::<CompactPackingCandidate>() == 64);

#[derive(Clone, Debug, Eq, PartialEq)]
struct OperationDictionary {
    entries: Vec<CPackingOperation>,
    bucket_heads: Vec<u32>,
    next_indices: Vec<u32>,
}

impl OperationDictionary {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            bucket_heads: Vec::new(),
            next_indices: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn entry(&self, index: u16) -> Option<CPackingOperation> {
        self.entries.get(index as usize).copied()
    }

    fn resident_bytes(&self) -> usize {
        self.entries
            .capacity()
            .saturating_mul(size_of::<CPackingOperation>())
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

    fn resident_allocation_count(&self) -> usize {
        usize::from(self.entries.capacity() != 0)
            + usize::from(self.bucket_heads.capacity() != 0)
            + usize::from(self.next_indices.capacity() != 0)
    }

    fn intern(&mut self, operation: CPackingOperation) -> Result<u16, PackingCandidateBatchError> {
        self.ensure_buckets()?;
        if let Some(index) = self.find(operation) {
            return Ok(index);
        }
        self.grow_if_needed()?;

        let index = u16::try_from(self.entries.len())
            .map_err(|_| PackingCandidateBatchError::StorageOverflow)?;
        self.entries
            .try_reserve(1)
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        self.next_indices
            .try_reserve(1)
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        let bucket = operation_bucket(operation, self.bucket_heads.len());
        self.entries.push(operation);
        self.next_indices.push(self.bucket_heads[bucket]);
        self.bucket_heads[bucket] = u32::from(index);
        Ok(index)
    }

    fn find(&self, operation: CPackingOperation) -> Option<u16> {
        if self.bucket_heads.is_empty() {
            return None;
        }
        let bucket = operation_bucket(operation, self.bucket_heads.len());
        let mut index = self.bucket_heads[bucket];
        while index != EMPTY_DICTIONARY_INDEX {
            if self.entries[index as usize] == operation {
                return u16::try_from(index).ok();
            }
            index = self.next_indices[index as usize];
        }
        None
    }

    fn ensure_buckets(&mut self) -> Result<(), PackingCandidateBatchError> {
        if !self.bucket_heads.is_empty() {
            return Ok(());
        }
        self.bucket_heads
            .try_reserve_exact(INITIAL_OPERATION_BUCKET_COUNT)
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        self.bucket_heads
            .resize(INITIAL_OPERATION_BUCKET_COUNT, EMPTY_DICTIONARY_INDEX);
        Ok(())
    }

    fn grow_if_needed(&mut self) -> Result<(), PackingCandidateBatchError> {
        if self.entries.len().saturating_mul(4) < self.bucket_heads.len().saturating_mul(3) {
            return Ok(());
        }
        let new_count = self
            .bucket_heads
            .len()
            .checked_mul(2)
            .ok_or(PackingCandidateBatchError::StorageOverflow)?;
        let mut grown = Vec::new();
        grown
            .try_reserve_exact(new_count)
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        grown.resize(new_count, EMPTY_DICTIONARY_INDEX);
        for (index, operation) in self.entries.iter().copied().enumerate() {
            let bucket = operation_bucket(operation, new_count);
            self.next_indices[index] = grown[bucket];
            grown[bucket] = index as u32;
        }
        self.bucket_heads = grown;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackingCandidateSegment {
    candidates: Vec<CompactPackingCandidate>,
    operation_refs: Vec<u16>,
    operation_dictionary: OperationDictionary,
}

impl PackingCandidateSegment {
    fn try_new() -> Result<Self, PackingCandidateBatchError> {
        let mut candidates = Vec::new();
        candidates
            .try_reserve_exact(CANDIDATES_PER_SEGMENT)
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        let mut operation_refs = Vec::new();
        operation_refs
            .try_reserve_exact(OPERATION_REFS_PER_SEGMENT)
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        Ok(Self {
            candidates,
            operation_refs,
            operation_dictionary: OperationDictionary::new(),
        })
    }

    fn is_full(&self) -> bool {
        self.candidates.len() == CANDIDATES_PER_SEGMENT
    }

    fn push(&mut self, candidate: CPackingCandidate) -> Result<(), PackingCandidateBatchError> {
        let operation_count = usize::from(candidate.operation_count);
        if operation_count > C_PACKING_MAX_OPERATIONS {
            return Err(PackingCandidateBatchError::OperationCountExceeded);
        }
        let operation_start = u32::try_from(self.operation_refs.len())
            .map_err(|_| PackingCandidateBatchError::StorageOverflow)?;
        let compact_operation_count = u8::try_from(operation_count)
            .map_err(|_| PackingCandidateBatchError::OperationCountExceeded)?;

        debug_assert!(!self.is_full());
        debug_assert!(
            self.operation_refs.len().saturating_add(operation_count)
                <= self.operation_refs.capacity()
        );

        let (operations, geometry_variant_domains) = canonical_operation_set(
            &candidate.operations,
            operation_count,
            candidate.geometry_variant_domains,
        )?;

        let mut operation_indices = [0_u16; C_PACKING_MAX_OPERATIONS];
        for (offset, operation) in operations[..operation_count].iter().copied().enumerate() {
            operation_indices[offset] = self.operation_dictionary.intern(operation)?;
        }
        self.operation_refs
            .extend_from_slice(&operation_indices[..operation_count]);
        self.candidates.push(CompactPackingCandidate {
            candidate_id: candidate.candidate_id,
            canonical_operation_set_id: candidate.canonical_operation_set_id,
            final_board: candidate.final_board,
            shape_mask: candidate.shape_mask,
            shape_key: candidate.shape_key,
            tiling_key: candidate.tiling_key,
            operation_set_key: candidate.operation_set_key,
            operation_start,
            geometry_variant_domains,
            operation_count: compact_operation_count,
            cleared_lines: candidate.cleared_lines,
        });
        Ok(())
    }

    fn truncate_tail(&mut self, candidate_count: usize) {
        if candidate_count >= self.candidates.len() {
            return;
        }
        let operation_count = self.candidates[..candidate_count]
            .last()
            .map_or(0, |candidate| {
                candidate.operation_start as usize + usize::from(candidate.operation_count)
            });
        self.candidates.truncate(candidate_count);
        self.operation_refs.truncate(operation_count);
    }

    fn candidate_view(&self, index: usize) -> Option<PackingCandidateView<'_>> {
        let compact = self.candidates.get(index)?;
        let operation_start = compact.operation_start as usize;
        let operation_end = operation_start.checked_add(usize::from(compact.operation_count))?;
        let operation_refs = self.operation_refs.get(operation_start..operation_end)?;
        Some(PackingCandidateView {
            compact,
            operation_refs,
            operation_dictionary: &self.operation_dictionary,
        })
    }

    fn resident_allocation_count(&self) -> usize {
        usize::from(self.candidates.capacity() != 0)
            + usize::from(self.operation_refs.capacity() != 0)
            + self.operation_dictionary.resident_allocation_count()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CandidateLocation {
    segment_index: u32,
    candidate_index: u32,
}

// Canonical sorting moves only this compact index. Candidate cache lines and
// exact operation tuples stay in their worker-owned segment allocations.
const _: () = assert!(size_of::<CandidateLocation>() == 8);

/// Cache-oriented owned packing output.
///
/// Candidate metadata occupies one cache line and operation lists contain only
/// dictionary indices. The dictionary interns complete operation tuples, so a
/// hash collision can never merge distinct target-frame geometry or evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackingCandidateBatch {
    board_width: u16,
    board_height: u16,
    segments: Vec<PackingCandidateSegment>,
    candidate_order: Vec<CandidateLocation>,
}

impl PackingCandidateBatch {
    pub fn new(board_width: u16, board_height: u16) -> Result<Self, PackingCandidateBatchError> {
        validate_layout(board_width, board_height)?;
        Ok(Self {
            board_width,
            board_height,
            segments: Vec::new(),
            candidate_order: Vec::new(),
        })
    }

    pub fn from_candidates(
        board_width: u16,
        board_height: u16,
        candidates: impl IntoIterator<Item = CPackingCandidate>,
    ) -> Result<Self, PackingCandidateBatchError> {
        let mut batch = Self::new(board_width, board_height)?;
        for candidate in candidates {
            batch.push(candidate)?;
        }
        Ok(batch)
    }

    pub fn len(&self) -> usize {
        self.candidate_order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidate_order.is_empty()
    }

    pub fn board_width(&self) -> u16 {
        self.board_width
    }

    pub fn board_height(&self) -> u16 {
        self.board_height
    }

    pub fn operation_dictionary_len(&self) -> usize {
        self.segments
            .iter()
            .map(|segment| segment.operation_dictionary.len())
            .fold(0usize, usize::saturating_add)
    }

    pub fn operation_reference_count(&self) -> usize {
        self.segments
            .iter()
            .map(|segment| segment.operation_refs.len())
            .fold(0usize, usize::saturating_add)
    }

    pub fn candidate_metadata_resident_bytes(&self) -> usize {
        self.segments
            .iter()
            .map(|segment| {
                segment
                    .candidates
                    .capacity()
                    .saturating_mul(size_of::<CompactPackingCandidate>())
            })
            .fold(0usize, usize::saturating_add)
            .saturating_add(self.merge_index_resident_bytes())
    }

    pub fn merge_index_resident_bytes(&self) -> usize {
        self.segments
            .capacity()
            .saturating_mul(size_of::<PackingCandidateSegment>())
            .saturating_add(
                self.candidate_order
                    .capacity()
                    .saturating_mul(size_of::<CandidateLocation>()),
            )
    }

    pub fn operation_reference_resident_bytes(&self) -> usize {
        self.segments
            .iter()
            .map(|segment| {
                segment
                    .operation_refs
                    .capacity()
                    .saturating_mul(size_of::<u16>())
            })
            .fold(0usize, usize::saturating_add)
    }

    pub fn resident_bytes(&self) -> usize {
        self.candidate_metadata_resident_bytes()
            .saturating_add(self.operation_reference_resident_bytes())
            .saturating_add(
                self.segments
                    .iter()
                    .map(|segment| segment.operation_dictionary.resident_bytes())
                    .fold(0usize, usize::saturating_add),
            )
    }

    pub fn resident_allocation_count(&self) -> usize {
        usize::from(self.segments.capacity() != 0)
            + usize::from(self.candidate_order.capacity() != 0)
            + self
                .segments
                .iter()
                .map(PackingCandidateSegment::resident_allocation_count)
                .fold(0usize, usize::saturating_add)
    }

    pub fn candidate_at(&self, index: usize) -> Option<CPackingCandidate> {
        self.candidate_view(index)
            .map(PackingCandidateView::to_owned)
    }

    pub fn candidate_view(&self, index: usize) -> Option<PackingCandidateView<'_>> {
        let location = *self.candidate_order.get(index)?;
        self.segments
            .get(location.segment_index as usize)?
            .candidate_view(location.candidate_index as usize)
    }

    fn owned_candidate_from_view(view: PackingCandidateView<'_>) -> CPackingCandidate {
        let compact = *view.compact;
        let mut candidate = CPackingCandidate {
            candidate_id: compact.candidate_id,
            canonical_operation_set_id: compact.canonical_operation_set_id,
            final_board: compact.final_board,
            shape_mask: compact.shape_mask,
            shape_key: compact.shape_key,
            tiling_key: compact.tiling_key,
            operation_set_key: compact.operation_set_key,
            operation_count: u16::from(compact.operation_count),
            geometry_variant_domains: compact.geometry_variant_domains,
            cleared_lines: compact.cleared_lines,
            ..Default::default()
        };
        for (offset, operation) in view.operations().enumerate() {
            candidate.operations[offset] = operation;
        }
        candidate
    }

    pub fn iter(&self) -> PackingCandidateIter<'_> {
        PackingCandidateIter {
            batch: self,
            next_index: 0,
        }
    }

    pub fn push(&mut self, candidate: CPackingCandidate) -> Result<(), PackingCandidateBatchError> {
        if self
            .segments
            .last()
            .is_none_or(PackingCandidateSegment::is_full)
        {
            self.segments
                .try_reserve(1)
                .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
            self.segments.push(PackingCandidateSegment::try_new()?);
        }
        let segment_index = self.segments.len() - 1;
        let candidate_index = self.segments[segment_index].candidates.len();
        let location = CandidateLocation {
            segment_index: u32::try_from(segment_index)
                .map_err(|_| PackingCandidateBatchError::StorageOverflow)?,
            candidate_index: u32::try_from(candidate_index)
                .map_err(|_| PackingCandidateBatchError::StorageOverflow)?,
        };
        if self.candidate_order.len() == self.candidate_order.capacity() {
            self.candidate_order
                .try_reserve_exact(CANDIDATES_PER_SEGMENT)
                .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        }
        self.segments[segment_index].push(candidate)?;
        self.candidate_order.push(location);
        Ok(())
    }

    pub fn exact_candidate_matches(&self, index: usize, candidate: &CPackingCandidate) -> bool {
        let Some(view) = self.candidate_view(index) else {
            return false;
        };
        let compact = *view.compact;
        if compact.final_board != candidate.final_board
            || compact.shape_mask != candidate.shape_mask
            || compact.shape_key != candidate.shape_key
            || compact.tiling_key != candidate.tiling_key
            || compact.operation_set_key != candidate.operation_set_key
            || u16::from(compact.operation_count) != candidate.operation_count
            || compact.cleared_lines != candidate.cleared_lines
        {
            return false;
        }
        let operation_count = usize::from(candidate.operation_count);
        let Ok((operations, geometry_variant_domains)) = canonical_operation_set(
            &candidate.operations,
            operation_count,
            candidate.geometry_variant_domains,
        ) else {
            return false;
        };
        if compact.geometry_variant_domains != geometry_variant_domains {
            return false;
        }
        for (operation, expected) in view.operations().zip(operations[..operation_count].iter()) {
            if operation != *expected {
                return false;
            }
        }
        true
    }

    pub fn candidate_id(&self, index: usize) -> Option<u64> {
        self.candidate_view(index)
            .map(PackingCandidateView::candidate_id)
    }

    pub fn set_identity(
        &mut self,
        index: usize,
        candidate_id: u64,
        canonical_operation_set_id: u64,
    ) -> Result<(), PackingCandidateBatchError> {
        let location = *self
            .candidate_order
            .get(index)
            .ok_or(PackingCandidateBatchError::CandidateIndexOutOfBounds)?;
        let candidate = self
            .segments
            .get_mut(location.segment_index as usize)
            .and_then(|segment| {
                segment
                    .candidates
                    .get_mut(location.candidate_index as usize)
            })
            .ok_or(PackingCandidateBatchError::CandidateIndexOutOfBounds)?;
        candidate.candidate_id = candidate_id;
        candidate.canonical_operation_set_id = canonical_operation_set_id;
        Ok(())
    }

    pub fn hash_key(&self, index: usize) -> Option<u64> {
        let candidate = self.candidate_view(index)?.compact;
        Some(
            candidate.operation_set_key
                ^ candidate.tiling_key.rotate_left(17)
                ^ candidate.shape_key.rotate_left(31),
        )
    }

    pub fn append(&mut self, mut other: Self) -> Result<(), PackingCandidateBatchError> {
        if self.board_width != other.board_width || self.board_height != other.board_height {
            return Err(PackingCandidateBatchError::LayoutMismatch);
        }
        let segment_base = u32::try_from(self.segments.len())
            .map_err(|_| PackingCandidateBatchError::StorageOverflow)?;
        let total_segments = self
            .segments
            .len()
            .checked_add(other.segments.len())
            .ok_or(PackingCandidateBatchError::StorageOverflow)?;
        u32::try_from(total_segments).map_err(|_| PackingCandidateBatchError::StorageOverflow)?;
        self.segments
            .try_reserve(other.segments.len())
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        self.candidate_order
            .try_reserve(other.candidate_order.len())
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        for location in &mut other.candidate_order {
            location.segment_index = location
                .segment_index
                .checked_add(segment_base)
                .ok_or(PackingCandidateBatchError::StorageOverflow)?;
        }
        self.segments.append(&mut other.segments);
        self.candidate_order.append(&mut other.candidate_order);
        Ok(())
    }

    /// Moves worker-owned payload segments into one batch. Only the compact
    /// 8-byte candidate index is rewritten; candidate cache lines, operation
    /// references, and exact operation dictionaries keep their allocations.
    pub fn merge_batches(
        mut batches: Vec<Self>,
        board_width: u16,
        board_height: u16,
    ) -> Result<Self, PackingCandidateBatchError> {
        let mut merged = if batches.is_empty() {
            return Self::new(board_width, board_height);
        } else {
            batches.remove(0)
        };
        if merged.board_width != board_width || merged.board_height != board_height {
            return Err(PackingCandidateBatchError::LayoutMismatch);
        }
        let additional_segments = batches.iter().try_fold(0usize, |total, batch| {
            total
                .checked_add(batch.segments.len())
                .ok_or(PackingCandidateBatchError::StorageOverflow)
        })?;
        let additional_candidates = batches.iter().try_fold(0usize, |total, batch| {
            total
                .checked_add(batch.candidate_order.len())
                .ok_or(PackingCandidateBatchError::StorageOverflow)
        })?;
        merged
            .segments
            .try_reserve_exact(additional_segments)
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        merged
            .candidate_order
            .try_reserve_exact(additional_candidates)
            .map_err(|_| PackingCandidateBatchError::AllocationFailed)?;
        for batch in batches {
            merged.append(batch)?;
        }
        Ok(merged)
    }

    pub fn truncate(&mut self, candidate_count: usize) {
        if candidate_count >= self.candidate_order.len() {
            return;
        }
        while self.candidate_order.len() > candidate_count {
            let location = self
                .candidate_order
                .pop()
                .expect("candidate order length was checked");
            let segment_index = location.segment_index as usize;
            if let Some(segment) = self.segments.get_mut(segment_index) {
                if location.candidate_index as usize + 1 == segment.candidates.len() {
                    segment.truncate_tail(location.candidate_index as usize);
                }
            }
        }
        while self
            .segments
            .last()
            .is_some_and(|segment| segment.candidates.is_empty())
        {
            self.segments.pop();
        }
    }

    /// Places exact candidates in a backend-independent canonical order and
    /// assigns stable dense identities. Hashes are never used as equality.
    pub fn canonicalize_identities(&mut self) {
        let segments = &self.segments;
        self.candidate_order
            .sort_unstable_by(|left, right| compare_candidate_locations(segments, *left, *right));
        self.candidate_order.dedup_by(|right, left| {
            compare_candidate_locations(segments, *left, *right) == Ordering::Equal
        });
        for index in 0..self.candidate_order.len() {
            let identity = index as u64 + 1;
            let location = self.candidate_order[index];
            let candidate = &mut self.segments[location.segment_index as usize].candidates
                [location.candidate_index as usize];
            candidate.candidate_id = identity;
            candidate.canonical_operation_set_id = identity;
        }
    }
}

fn compare_candidate_locations(
    segments: &[PackingCandidateSegment],
    left: CandidateLocation,
    right: CandidateLocation,
) -> Ordering {
    let left_segment = &segments[left.segment_index as usize];
    let right_segment = &segments[right.segment_index as usize];
    let left_candidate = &left_segment.candidates[left.candidate_index as usize];
    let right_candidate = &right_segment.candidates[right.candidate_index as usize];
    (
        left_candidate.final_board,
        left_candidate.shape_mask,
        left_candidate.cleared_lines,
        left_candidate.operation_count,
        left_candidate.geometry_variant_domains,
    )
        .cmp(&(
            right_candidate.final_board,
            right_candidate.shape_mask,
            right_candidate.cleared_lines,
            right_candidate.operation_count,
            right_candidate.geometry_variant_domains,
        ))
        .then_with(|| {
            compact_candidate_operations(
                left_candidate,
                &left_segment.operation_refs,
                &left_segment.operation_dictionary,
            )
            .cmp(compact_candidate_operations(
                right_candidate,
                &right_segment.operation_refs,
                &right_segment.operation_dictionary,
            ))
        })
}

fn compact_candidate_operations<'a>(
    candidate: &CompactPackingCandidate,
    operation_refs: &'a [u16],
    operation_dictionary: &'a OperationDictionary,
) -> impl Iterator<Item = CPackingOperation> + 'a {
    let begin = candidate.operation_start as usize;
    let end = begin + usize::from(candidate.operation_count);
    operation_refs[begin..end].iter().map(|index| {
        operation_dictionary
            .entry(*index)
            .expect("canonical candidate operation reference is valid")
    })
}

#[derive(Clone, Copy)]
pub struct PackingCandidateView<'a> {
    compact: &'a CompactPackingCandidate,
    operation_refs: &'a [u16],
    operation_dictionary: &'a OperationDictionary,
}

impl<'a> PackingCandidateView<'a> {
    pub fn candidate_id(self) -> u64 {
        self.compact.candidate_id
    }

    pub fn canonical_operation_set_id(self) -> u64 {
        self.compact.canonical_operation_set_id
    }

    pub fn operation_count(self) -> usize {
        usize::from(self.compact.operation_count)
    }

    pub fn geometry_variant_domains(self) -> u16 {
        self.compact.geometry_variant_domains
    }

    pub fn cleared_lines(self) -> u8 {
        self.compact.cleared_lines
    }

    pub fn operations(self) -> impl ExactSizeIterator<Item = CPackingOperation> + 'a {
        self.operation_refs.iter().map(|index| {
            self.operation_dictionary
                .entry(*index)
                .expect("candidate operation reference must address its owned dictionary")
        })
    }

    pub fn to_owned(self) -> CPackingCandidate {
        PackingCandidateBatch::owned_candidate_from_view(self)
    }

    pub fn standard_board64_tiling_identity(
        self,
        initial_board_mask: u64,
    ) -> Result<StandardBoard64TilingIdentity, PackingCandidateIdentityError> {
        let mut placements = [None; STANDARD_BOARD64_TILING_MAX_PLACEMENTS];
        for (index, operation) in self.operations().enumerate() {
            let piece = piece_from_code(operation.piece).ok_or(
                PackingCandidateIdentityError::UnknownPieceCode(operation.piece),
            )?;
            placements[index] = Some(PiecePlacementMask::new(piece, operation.mask));
        }
        StandardBoard64TilingIdentity::from_placements(
            initial_board_mask,
            placements
                .into_iter()
                .take(self.operation_count())
                .flatten(),
        )
        .map_err(PackingCandidateIdentityError::InvalidTiling)
    }
}

impl CPackingCandidate {
    pub fn standard_board64_tiling_identity(
        &self,
        initial_board_mask: u64,
    ) -> Result<StandardBoard64TilingIdentity, PackingCandidateIdentityError> {
        let operation_count = usize::from(self.operation_count);
        if operation_count > C_PACKING_MAX_OPERATIONS {
            return Err(PackingCandidateIdentityError::InvalidTiling(
                NormalizedTilingSolutionError::TooManyPlacements {
                    count: operation_count,
                    capacity: STANDARD_BOARD64_TILING_MAX_PLACEMENTS,
                },
            ));
        }
        let mut placements = [None; STANDARD_BOARD64_TILING_MAX_PLACEMENTS];
        for (index, operation) in self.operations[..operation_count].iter().enumerate() {
            let piece = piece_from_code(operation.piece).ok_or(
                PackingCandidateIdentityError::UnknownPieceCode(operation.piece),
            )?;
            placements[index] = Some(PiecePlacementMask::new(piece, operation.mask));
        }
        StandardBoard64TilingIdentity::from_placements(
            initial_board_mask,
            placements.into_iter().take(operation_count).flatten(),
        )
        .map_err(PackingCandidateIdentityError::InvalidTiling)
    }
}

#[derive(Clone, Copy)]
pub struct PackingCandidateIter<'a> {
    batch: &'a PackingCandidateBatch,
    next_index: usize,
}

impl Iterator for PackingCandidateIter<'_> {
    type Item = CPackingCandidate;

    fn next(&mut self) -> Option<Self::Item> {
        let candidate = self.batch.candidate_at(self.next_index)?;
        self.next_index += 1;
        Some(candidate)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.batch.len().saturating_sub(self.next_index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PackingCandidateIter<'_> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingCandidateBatchError {
    InvalidLayout,
    OperationCountExceeded,
    InvalidGeometryVariantDomains,
    StorageOverflow,
    AllocationFailed,
    CandidateIndexOutOfBounds,
    LayoutMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingCandidateIdentityError {
    UnknownPieceCode(u8),
    InvalidTiling(NormalizedTilingSolutionError),
}

fn validate_layout(width: u16, height: u16) -> Result<(), PackingCandidateBatchError> {
    if width == 0 || height == 0 || u32::from(width) * u32::from(height) > 64 {
        Err(PackingCandidateBatchError::InvalidLayout)
    } else {
        Ok(())
    }
}

fn canonical_operation_set(
    source: &[CPackingOperation; C_PACKING_MAX_OPERATIONS],
    operation_count: usize,
    geometry_variant_domains: u16,
) -> Result<([CPackingOperation; C_PACKING_MAX_OPERATIONS], u16), PackingCandidateBatchError> {
    if operation_count > C_PACKING_MAX_OPERATIONS {
        return Err(PackingCandidateBatchError::OperationCountExceeded);
    }
    let valid_domain_bits = if operation_count == 0 {
        0
    } else {
        (1_u16 << operation_count) - 1
    };
    if geometry_variant_domains & !valid_domain_bits != 0 {
        return Err(PackingCandidateBatchError::InvalidGeometryVariantDomains);
    }

    let mut entries = [(CPackingOperation::default(), false); C_PACKING_MAX_OPERATIONS];
    for index in 0..operation_count {
        entries[index] = (
            source[index],
            geometry_variant_domains & (1_u16 << index) != 0,
        );
    }
    entries[..operation_count].sort_unstable();

    let mut operations = [CPackingOperation::default(); C_PACKING_MAX_OPERATIONS];
    let mut canonical_domains = 0_u16;
    for (index, (operation, has_geometry_domain)) in
        entries[..operation_count].iter().copied().enumerate()
    {
        operations[index] = operation;
        if has_geometry_domain {
            canonical_domains |= 1_u16 << index;
        }
    }
    Ok((operations, canonical_domains))
}

fn operation_bucket(operation: CPackingOperation, bucket_count: usize) -> usize {
    debug_assert!(bucket_count.is_power_of_two());
    (operation_hash(operation) as usize) & (bucket_count - 1)
}

fn operation_hash(operation: CPackingOperation) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in [
        u64::from(operation.piece),
        u64::from(operation.rotation),
        u64::from(operation.x as u8),
        u64::from(operation.y as u8),
        u64::from(operation.operation_id),
        u64::from(operation.required_deleted_row_mask),
        operation.mask,
    ] {
        hash ^= value;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

const fn piece_from_code(code: u8) -> Option<PieceKind> {
    match code {
        crate::problem::C_PIECE_I => Some(PieceKind::I),
        crate::problem::C_PIECE_O => Some(PieceKind::O),
        crate::problem::C_PIECE_T => Some(PieceKind::T),
        crate::problem::C_PIECE_S => Some(PieceKind::S),
        crate::problem::C_PIECE_Z => Some(PieceKind::Z),
        crate::problem::C_PIECE_J => Some(PieceKind::J),
        crate::problem::C_PIECE_L => Some(PieceKind::L),
        _ => None,
    }
}
