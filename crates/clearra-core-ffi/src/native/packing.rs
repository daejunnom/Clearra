#[cfg(feature = "native-c-core")]
use super::{CNativePruningProofLedger, CNativeResourceReport};
use super::{
    NativeCoreError, NativePackingOutcome, NativePackingStreamOutcome,
    C_NATIVE_PACKING_MAX_CANDIDATES, C_NATIVE_PACKING_MAX_PIECES,
};
use crate::{
    native::NativePackingCandidateConsumer,
    packing_problem::{CPackingCandidate, CPackingOperation, C_PACKING_MAX_OPERATIONS},
    problem::CPackingProblem,
    PackingCandidateBatch,
};
use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_domain::pruning::PruningEvidencePolicy;

const C_PACKING_STATUS_OK: i32 = 0;
#[cfg(feature = "native-c-core")]
const C_PACKING_STATUS_CAPACITY_EXCEEDED: i32 = 6;
#[cfg(feature = "native-c-core")]
const C_PACKING_STATUS_CANCELLED: i32 = 7;

#[repr(C)]
#[derive(Debug, Eq, PartialEq)]
pub struct CNativePackingCandidateBuffer {
    pub count: u16,
    pub final_boards: [u64; C_NATIVE_PACKING_MAX_CANDIDATES],
    pub shape_masks: [u64; C_NATIVE_PACKING_MAX_CANDIDATES],
    pub shape_keys: [u64; C_NATIVE_PACKING_MAX_CANDIDATES],
    pub tiling_keys: [u64; C_NATIVE_PACKING_MAX_CANDIDATES],
    pub operation_set_keys: [u64; C_NATIVE_PACKING_MAX_CANDIDATES],
    pub placed_counts: [u8; C_NATIVE_PACKING_MAX_CANDIDATES],
    pub cleared_lines: [u8; C_NATIVE_PACKING_MAX_CANDIDATES],
    pub geometry_variant_domains: [u16; C_NATIVE_PACKING_MAX_CANDIDATES],
    pub pieces: [[u8; C_NATIVE_PACKING_MAX_CANDIDATES]; C_NATIVE_PACKING_MAX_PIECES],
    pub rotations: [[u8; C_NATIVE_PACKING_MAX_CANDIDATES]; C_NATIVE_PACKING_MAX_PIECES],
    pub xs: [[i8; C_NATIVE_PACKING_MAX_CANDIDATES]; C_NATIVE_PACKING_MAX_PIECES],
    pub ys: [[i8; C_NATIVE_PACKING_MAX_CANDIDATES]; C_NATIVE_PACKING_MAX_PIECES],
    pub operation_ids: [[u16; C_NATIVE_PACKING_MAX_CANDIDATES]; C_NATIVE_PACKING_MAX_PIECES],
    pub operation_deleted_row_masks:
        [[u16; C_NATIVE_PACKING_MAX_CANDIDATES]; C_NATIVE_PACKING_MAX_PIECES],
    pub operation_masks: [[u64; C_NATIVE_PACKING_MAX_CANDIDATES]; C_NATIVE_PACKING_MAX_PIECES],
}

impl CNativePackingCandidateBuffer {
    pub fn to_candidates(&self) -> Vec<CPackingCandidate> {
        (0..usize::from(self.count))
            .map(|index| self.candidate_at(index))
            .collect()
    }
}
impl CNativePackingCandidateBuffer {
    fn candidate_at(&self, index: usize) -> CPackingCandidate {
        let operation_count = usize::from(self.placed_counts[index]).min(C_PACKING_MAX_OPERATIONS);
        let mut candidate = CPackingCandidate {
            candidate_id: (index as u64) + 1,
            canonical_operation_set_id: (index as u64) + 1,
            final_board: self.final_boards[index],
            shape_mask: self.shape_masks[index],
            shape_key: self.shape_keys[index],
            tiling_key: self.tiling_keys[index],
            operation_set_key: self.operation_set_keys[index],
            operation_count: operation_count as u16,
            geometry_variant_domains: self.geometry_variant_domains[index],
            cleared_lines: self.cleared_lines[index],
            ..Default::default()
        };

        for operation_index in 0..operation_count {
            candidate.operations[operation_index] = CPackingOperation {
                piece: self.pieces[operation_index][index],
                rotation: self.rotations[operation_index][index],
                x: self.xs[operation_index][index],
                y: self.ys[operation_index][index],
                operation_id: self.operation_ids[operation_index][index],
                required_deleted_row_mask: self.operation_deleted_row_masks[operation_index][index],
                mask: self.operation_masks[operation_index][index],
            };
        }
        candidate
    }
}

#[cfg(feature = "native-c-core")]
mod linked {
    use super::*;

    pub(crate) fn generate_packing_candidates(
        problem: &CPackingProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativePackingOutcome, NativeCoreError> {
        generate_packing_candidates_with_partition(problem, None, cancellation)
    }

    pub(crate) fn generate_packing_candidates_partition(
        problem: &CPackingProblem,
        partition_index: u16,
        partition_count: u16,
        partition_depth: u8,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativePackingOutcome, NativeCoreError> {
        generate_packing_candidates_with_partition(
            problem,
            Some((partition_index, partition_count, partition_depth)),
            cancellation,
        )
    }

    fn generate_packing_candidates_with_partition(
        problem: &CPackingProblem,
        partition: Option<(u16, u16, u8)>,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativePackingOutcome, NativeCoreError> {
        let mut reducer =
            super::super::packing_candidate_sink::NativeCandidateReducer::new(problem)
                .map_err(|_| NativeCoreError::PackingStatus(1))?;
        let streamed = stream_packing_candidates_with_partition(
            problem,
            partition,
            cancellation,
            &mut reducer,
        )?;
        Ok(NativePackingOutcome {
            status: streamed.status,
            candidates: reducer.into_candidates(),
            resource_report: streamed.resource_report,
            pruning_ledger: streamed.pruning_ledger,
        })
    }

    pub(crate) fn stream_packing_candidates(
        problem: &CPackingProblem,
        cancellation: &ExecutionCancellationToken,
        consumer: &mut dyn NativePackingCandidateConsumer,
    ) -> Result<NativePackingStreamOutcome, NativeCoreError> {
        stream_packing_candidates_with_partition(problem, None, cancellation, consumer)
    }

    pub(crate) fn stream_packing_candidates_partition(
        problem: &CPackingProblem,
        partition_index: u16,
        partition_count: u16,
        partition_depth: u8,
        cancellation: &ExecutionCancellationToken,
        consumer: &mut dyn NativePackingCandidateConsumer,
    ) -> Result<NativePackingStreamOutcome, NativeCoreError> {
        stream_packing_candidates_with_partition(
            problem,
            Some((partition_index, partition_count, partition_depth)),
            cancellation,
            consumer,
        )
    }

    fn stream_packing_candidates_with_partition(
        problem: &CPackingProblem,
        partition: Option<(u16, u16, u8)>,
        cancellation: &ExecutionCancellationToken,
        consumer: &mut dyn NativePackingCandidateConsumer,
    ) -> Result<NativePackingStreamOutcome, NativeCoreError> {
        let _execution_control =
            crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
        let mut resource_report = CNativeResourceReport::default();
        let mut pruning_ledger = CNativePruningProofLedger::default();
        let status = {
            let mut sink =
                crate::raw::packing_candidate_sink::NativeCandidateSinkHandle::new(consumer);
            match partition {
                Some((index, count, depth)) => {
                    crate::raw::bindings::generate_packing_candidates_prefix_partition_to_sink(
                        problem,
                        index,
                        count,
                        depth,
                        sink.as_mut(),
                        &mut resource_report,
                        &mut pruning_ledger,
                    )
                }
                None => crate::raw::bindings::generate_packing_candidates_to_sink(
                    problem,
                    sink.as_mut(),
                    &mut resource_report,
                    &mut pruning_ledger,
                ),
            }
        };
        if status == C_PACKING_STATUS_CANCELLED || cancellation.is_cancelled() {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        if status == C_PACKING_STATUS_OK
            || status == C_PACKING_STATUS_CAPACITY_EXCEEDED && resource_report.truncated != 0
        {
            Ok(NativePackingStreamOutcome {
                status,
                resource_report: resource_report.to_domain(),
                pruning_ledger: pruning_ledger
                    .to_owned_report()
                    .map_err(NativeCoreError::InvalidPruningLedger)?,
            })
        } else {
            Err(NativeCoreError::PackingStatus(status))
        }
    }
}

#[cfg(feature = "native-c-core")]
pub(crate) use linked::generate_packing_candidates;
#[cfg(feature = "native-c-core")]
pub(crate) use linked::generate_packing_candidates_partition;
#[cfg(feature = "native-c-core")]
pub(crate) use linked::stream_packing_candidates;
#[cfg(feature = "native-c-core")]
pub(crate) use linked::stream_packing_candidates_partition;

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn generate_packing_candidates(
    _problem: &CPackingProblem,
    _cancellation: &ExecutionCancellationToken,
) -> Result<NativePackingOutcome, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn generate_packing_candidates_partition(
    _problem: &CPackingProblem,
    _partition_index: u16,
    _partition_count: u16,
    _partition_depth: u8,
    _cancellation: &ExecutionCancellationToken,
) -> Result<NativePackingOutcome, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn stream_packing_candidates(
    _problem: &CPackingProblem,
    _cancellation: &ExecutionCancellationToken,
    _consumer: &mut dyn NativePackingCandidateConsumer,
) -> Result<NativePackingStreamOutcome, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn stream_packing_candidates_partition(
    _problem: &CPackingProblem,
    _partition_index: u16,
    _partition_count: u16,
    _partition_depth: u8,
    _cancellation: &ExecutionCancellationToken,
    _consumer: &mut dyn NativePackingCandidateConsumer,
) -> Result<NativePackingStreamOutcome, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(feature = "native-c-core")]
pub(crate) fn generate_packing_candidates_with_pruning_policy(
    problem: &CPackingProblem,
    evidence_policy: PruningEvidencePolicy,
) -> Result<NativePackingOutcome, NativeCoreError> {
    let mut buffer = crate::raw::owned_packing_buffer::new_zeroed_packing_candidate_buffer();
    let mut resource_report = CNativeResourceReport::default();
    let mut pruning_ledger = CNativePruningProofLedger::default();
    let policy_code = match evidence_policy {
        PruningEvidencePolicy::BestEffort => 1,
        PruningEvidencePolicy::CompleteRequired => 2,
    };
    let status = crate::raw::bindings::generate_packing_candidates_with_pruning_policy(
        problem,
        buffer.as_mut(),
        &mut resource_report,
        policy_code,
        &mut pruning_ledger,
    );
    if status == C_PACKING_STATUS_OK
        || status == C_PACKING_STATUS_CAPACITY_EXCEEDED && resource_report.truncated != 0
    {
        Ok(NativePackingOutcome {
            status,
            candidates: PackingCandidateBatch::from_candidates(
                problem.board.width,
                if problem.board.search_height == 0 {
                    problem.board.visible_height
                } else {
                    problem.board.search_height
                },
                buffer.to_candidates(),
            )
            .map_err(|_| NativeCoreError::PackingStatus(1))?,
            resource_report: resource_report.to_domain(),
            pruning_ledger: pruning_ledger
                .to_owned_report()
                .map_err(NativeCoreError::InvalidPruningLedger)?,
        })
    } else {
        Err(NativeCoreError::PackingStatus(status))
    }
}

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn generate_packing_candidates_with_pruning_policy(
    _problem: &CPackingProblem,
    _evidence_policy: PruningEvidencePolicy,
) -> Result<NativePackingOutcome, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(test)]
#[path = "packing_tests.rs"]
mod tests;
