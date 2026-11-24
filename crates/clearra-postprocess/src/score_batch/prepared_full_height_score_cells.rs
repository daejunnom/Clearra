//! One validated full-height source reused by bounded score tasks. Group
//! metadata is prepared once per compute worker, never per candidate/pattern.
use super::*;
use std::{ops::Range, sync::Arc};

#[derive(Debug)]
pub struct PreparedFullHeightScoreCells {
    batch: Arc<FullHeightExecutionBatch>,
    ranges: Vec<(usize, usize, u64)>,
    max_path_len: usize,
    cell_count: usize,
}

impl PreparedFullHeightScoreCells {
    /// `already_retained_bytes` includes the shared immutable batch owner.
    /// Only this source's metadata is newly allocated and admitted here.
    pub fn new_with_memory_limit(
        batch: Arc<FullHeightExecutionBatch>,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<Self, FullHeightScoreCellError> {
        let (candidate_count, max_path_len) = validated_shape(&batch)?;
        let cell_count = candidate_count
            .checked_mul(batch.execution().patterns().len())
            .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
        let projected = checked_metadata_bytes(candidate_count)
            .and_then(|bytes| already_retained_bytes.checked_add(bytes))
            .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
        ensure_limit(projected, max_memory_bytes)?;
        let mut ranges = Vec::new();
        ranges
            .try_reserve_exact(candidate_count)
            .map_err(|_| FullHeightScoreCellError::AllocationFailed)?;
        let actual = checked_metadata_bytes(ranges.capacity())
            .and_then(|bytes| already_retained_bytes.checked_add(bytes))
            .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
        ensure_limit(actual, max_memory_bytes)?;
        let graphs = batch.execution().graphs();
        let mut first = 0;
        while first < graphs.len() {
            let end = first
                + graphs[first..]
                    .iter()
                    .take_while(|graph| graph.candidate_id() == graphs[first].candidate_id())
                    .count();
            let identity =
                ExtendedTilingSolutionKey::parse_canonical(graphs[first].candidate_key())
                    .map_err(|_| FullHeightScoreCellError::InvalidEvidence)?;
            ranges.push((first, end, (1_u64 << identity.placement_count()) - 1));
            first = end;
        }
        Ok(Self {
            batch,
            ranges,
            max_path_len,
            cell_count,
        })
    }

    pub fn batch(&self) -> &FullHeightExecutionBatch {
        &self.batch
    }
    pub fn candidate_count(&self) -> usize {
        self.ranges.len()
    }
    pub fn pattern_count(&self) -> usize {
        self.batch.execution().patterns().len()
    }
    pub const fn cell_count(&self) -> usize {
        self.cell_count
    }

    /// Excludes the batch pointee shared with the original producer owner.
    pub fn checked_metadata_retained_bytes(&self) -> Option<u128> {
        checked_metadata_bytes(self.ranges.capacity())
    }

    /// Range ordinals are candidate-group × original pattern. Multiple source
    /// graphs of one candidate are reduced by the unchanged score/tie rule.
    /// This projection includes this prepared source's metadata exactly once;
    /// callers must exclude that metadata from `already_retained_bytes`.
    pub fn checked_range_memory_projection(
        &self,
        range: Range<usize>,
        profile_storage_bytes: u128,
        parallelism: usize,
    ) -> Option<FullHeightScoreCellMemoryProjection> {
        if range.start > range.end || range.end > self.cell_count {
            return None;
        }
        let cell_storage_bytes = (range.len() as u128).checked_mul(
            (core::mem::size_of::<ScoreCell>() + COMPACT_SCORE_CELL_TRACE_ID_BYTES) as u128,
        )?;
        let workers = parallel_cells::workspace_count(range.len(), parallelism);
        let scratch_bytes = checked_scratch_bytes(self.max_path_len)?
            .checked_mul(workers as u128)?
            .checked_add(parallel_cells::checked_control_bytes(0, workers)?)?
            .checked_add(self.checked_metadata_retained_bytes()?)?;
        Some(FullHeightScoreCellMemoryProjection {
            candidate_count: self.candidate_count(),
            pattern_count: self.pattern_count(),
            cell_capacity: range.len(),
            max_path_len: self.max_path_len,
            cell_storage_bytes,
            scratch_bytes,
            profile_storage_bytes,
            required_peak_bytes: cell_storage_bytes
                .checked_add(scratch_bytes)?
                .checked_add(profile_storage_bytes)?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn materialize_range_with_profile_and_memory_limit(
        &self,
        range: Range<usize>,
        policy: ScoreObjectivePolicy,
        profile: &clearra_scoring::profile::ScoreProfile,
        profile_storage_bytes: u128,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<FullHeightScoreCellMaterialization, FullHeightScoreCellError> {
        if control.is_cancelled() {
            return Err(FullHeightScoreCellError::Cancelled);
        }
        if !crate::score_profile_selection::score_profile_matches_policy(profile, policy) {
            return Err(FullHeightScoreCellError::InvalidEvidence);
        }
        let projection = self
            .checked_range_memory_projection(
                range.clone(),
                profile_storage_bytes,
                control.parallelism(),
            )
            .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
        parallel_cells::materialize(
            &self.batch,
            policy,
            profile,
            projection,
            range.start,
            Some(&self.ranges),
            control,
            already_retained_bytes,
            max_memory_bytes,
        )
    }
}

fn checked_metadata_bytes(capacity: usize) -> Option<u128> {
    (capacity as u128)
        .checked_mul(core::mem::size_of::<(usize, usize, u64)>() as u128)?
        .checked_add(core::mem::size_of::<PreparedFullHeightScoreCells>() as u128)
}
