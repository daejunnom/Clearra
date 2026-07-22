use clearra_core_domain::pruning::PruningEvidencePolicy;
use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken, resource::ResourceReport,
};

use crate::problem::CBuildUpProblem;
use crate::problem::CPackingProblem;

#[cfg(feature = "native-c-core")]
use super::CNativePruningProofLedger;
use super::{NativeCoreError, NativeGeometryCatalog, NativePruningLedger};

pub const C_NATIVE_GEOMETRY_TASK_MAX_OPERATIONS: usize = 15;
pub const C_NATIVE_GEOMETRY_PATH_MAX_OPERATIONS: usize = C_NATIVE_GEOMETRY_TASK_MAX_OPERATIONS;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativeBuildableGeometryStreamReport {
    pub generated_count: u64,
    pub buildable_count: u64,
    pub workspace_retained_bytes: usize,
    pub host_resident_bytes: usize,
    pub buildup_status: i32,
    pub truncation_reason: u16,
    pub complete: u8,
    pub candidate_buildable: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBuildableGeometryTaskOutcome {
    pub status: i32,
    pub report: CNativeBuildableGeometryStreamReport,
    pub pruning_ledger: NativePruningLedger,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeGeometrySolutionTask {
    pub family_ref: u32,
    pub prefix_row_ids: [u32; C_NATIVE_GEOMETRY_TASK_MAX_OPERATIONS],
    pub continuation_family_refs: [u32; C_NATIVE_GEOMETRY_TASK_MAX_OPERATIONS],
    pub prefix_count: u8,
    pub continuation_count: u8,
    pub reserved: [u8; 2],
}

impl NativeGeometrySolutionTask {
    pub fn prefix_row_ids(&self) -> &[u32] {
        &self.prefix_row_ids[..usize::from(self.prefix_count)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeGeometryPathSinkError {
    Invalid,
    CapacityExceeded,
    Cancelled,
}

pub trait NativeGeometryPathConsumer {
    fn consume(&mut self, skeleton_row_ids: &[u32]) -> Result<(), NativeGeometryPathSinkError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeGeometryGraphSearchOutcome {
    pub graph: NativeGeometrySolutionGraph,
    pub resource_report: ResourceReport,
    pub pruning_ledger: NativePruningLedger,
}

#[cfg(feature = "native-c-core")]
mod linked {
    use std::{ffi::c_void, ptr::NonNull, sync::Arc};

    use super::*;
    use crate::native::CNativeResourceReport;

    const PACKING_OK: i32 = 0;
    const PACKING_CANCELLED: i32 = 7;

    struct NativeGeometrySolutionGraphInner {
        pointer: NonNull<c_void>,
        catalog: NativeGeometryCatalog,
        resource_report: ResourceReport,
    }

    unsafe impl Send for NativeGeometrySolutionGraphInner {}
    unsafe impl Sync for NativeGeometrySolutionGraphInner {}

    impl Drop for NativeGeometrySolutionGraphInner {
        fn drop(&mut self) {
            let mut pointer = self.pointer.as_ptr();
            crate::raw::bindings::geometry_solution_graph::release(&mut pointer);
        }
    }

    #[derive(Clone)]
    pub struct NativeGeometrySolutionGraph {
        inner: Arc<NativeGeometrySolutionGraphInner>,
    }

    impl NativeGeometrySolutionGraph {
        pub(crate) fn search_with_pruning_policy(
            catalog: NativeGeometryCatalog,
            problem: &CPackingProblem,
            cancellation: &ExecutionCancellationToken,
            evidence_policy: PruningEvidencePolicy,
        ) -> Result<NativeGeometryGraphSearchOutcome, NativeCoreError> {
            let _control =
                crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
            let mut report = CNativeResourceReport::default();
            let mut pruning_ledger = CNativePruningProofLedger::default();
            let evidence_policy = match evidence_policy {
                PruningEvidencePolicy::BestEffort => 1,
                PruningEvidencePolicy::CompleteRequired => 2,
            };
            let (status, pointer) = crate::raw::bindings::geometry_solution_graph::search(
                catalog.raw_pointer(),
                problem,
                evidence_policy,
                &mut report,
                &mut pruning_ledger,
            );
            if status == PACKING_CANCELLED || cancellation.is_cancelled() {
                return Err(NativeCoreError::ExecutionCancelled);
            }
            let resource_report = report.to_domain();
            if status != PACKING_OK {
                return Err(NativeCoreError::packing_with_resource_report(
                    status,
                    resource_report,
                ));
            }
            let pointer = NonNull::new(pointer).ok_or(NativeCoreError::PackingStatus(1))?;
            let graph = Self {
                inner: Arc::new(NativeGeometrySolutionGraphInner {
                    pointer,
                    catalog,
                    resource_report: resource_report.clone(),
                }),
            };
            Ok(NativeGeometryGraphSearchOutcome {
                graph,
                resource_report,
                pruning_ledger: pruning_ledger
                    .to_owned_report()
                    .map_err(NativeCoreError::InvalidPruningLedger)?,
            })
        }

        pub fn catalog(&self) -> &NativeGeometryCatalog {
            &self.inner.catalog
        }

        pub fn resource_report(&self) -> &ResourceReport {
            &self.inner.resource_report
        }

        pub fn resident_bytes(&self) -> usize {
            crate::raw::bindings::geometry_solution_graph::resident_bytes(
                self.inner.pointer.as_ptr(),
            )
        }

        pub fn node_count(&self) -> u32 {
            crate::raw::bindings::geometry_solution_graph::node_count(self.inner.pointer.as_ptr())
        }

        pub fn split_tasks(
            &self,
            desired_task_count: usize,
        ) -> Result<(Vec<NativeGeometrySolutionTask>, usize), NativeCoreError> {
            let capacity = desired_task_count.clamp(1, u32::MAX as usize);
            crate::raw::bindings::geometry_solution_graph::split_tasks(
                self.inner.pointer.as_ptr(),
                capacity,
            )
            .map_err(NativeCoreError::PackingStatus)
        }

        pub fn stream_task_paths(
            &self,
            task: &NativeGeometrySolutionTask,
            cancellation: &ExecutionCancellationToken,
            consumer: &mut dyn NativeGeometryPathConsumer,
        ) -> Result<u64, NativeCoreError> {
            let _control =
                crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
            let mut emitted_count = 0u64;
            let status = {
                let mut sink =
                    crate::raw::geometry_path_sink::NativeGeometryPathSinkHandle::new(consumer);
                crate::raw::bindings::geometry_solution_graph::stream_task_paths(
                    self.inner.pointer.as_ptr(),
                    task,
                    sink.as_mut(),
                    &mut emitted_count,
                )
            };
            if status == PACKING_CANCELLED || cancellation.is_cancelled() {
                return Err(NativeCoreError::ExecutionCancelled);
            }
            if status != PACKING_OK {
                return Err(NativeCoreError::PackingStatus(status));
            }
            Ok(emitted_count)
        }

        pub fn stream_buildable_task(
            &self,
            task: &NativeGeometrySolutionTask,
            packing_problem: &CPackingProblem,
            buildup_scratch: &mut CBuildUpProblem,
            buildup_workspace: &mut crate::native::NativeBuildUpWorkspace,
            cancellation: &ExecutionCancellationToken,
            evidence_policy: PruningEvidencePolicy,
            consumer: &mut dyn crate::native::NativePackingCandidateConsumer,
        ) -> Result<NativeBuildableGeometryTaskOutcome, NativeCoreError> {
            let _control =
                crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
            let workspace = buildup_workspace.raw_pointer()?;
            let mut report = CNativeBuildableGeometryStreamReport::default();
            let mut pruning_ledger = CNativePruningProofLedger::default();
            let native_evidence_policy = match evidence_policy {
                PruningEvidencePolicy::BestEffort => 1,
                PruningEvidencePolicy::CompleteRequired => 2,
            };
            let status = {
                let mut sink =
                    crate::raw::packing_candidate_sink::NativeCandidateSinkHandle::new(consumer);
                crate::raw::bindings::geometry_solution_graph::stream_buildable_task(
                    self.inner.pointer.as_ptr(),
                    self.catalog().raw_pointer(),
                    task,
                    packing_problem,
                    buildup_scratch,
                    workspace,
                    sink.as_mut(),
                    native_evidence_policy,
                    &mut pruning_ledger,
                    &mut report,
                )
            };
            if status == PACKING_CANCELLED || cancellation.is_cancelled() {
                return Err(NativeCoreError::ExecutionCancelled);
            }
            Ok(NativeBuildableGeometryTaskOutcome {
                status,
                report,
                pruning_ledger: pruning_ledger
                    .to_owned_report()
                    .map_err(NativeCoreError::InvalidPruningLedger)?,
            })
        }
    }

    impl core::fmt::Debug for NativeGeometrySolutionGraph {
        fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter
                .debug_struct("NativeGeometrySolutionGraph")
                .field("catalog_identity", &self.catalog().identity())
                .field("node_count", &self.node_count())
                .field("resident_bytes", &self.resident_bytes())
                .finish()
        }
    }

    impl PartialEq for NativeGeometrySolutionGraph {
        fn eq(&self, other: &Self) -> bool {
            Arc::ptr_eq(&self.inner, &other.inner)
        }
    }

    impl Eq for NativeGeometrySolutionGraph {}
}

#[cfg(feature = "native-c-core")]
pub use linked::NativeGeometrySolutionGraph;

#[cfg(not(feature = "native-c-core"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeGeometrySolutionGraph {
    _private: (),
}

#[cfg(not(feature = "native-c-core"))]
impl NativeGeometrySolutionGraph {
    pub fn catalog(&self) -> &NativeGeometryCatalog {
        unreachable!("a native geometry graph cannot exist without native-c-core")
    }

    pub const fn resident_bytes(&self) -> usize {
        0
    }

    pub fn split_tasks(
        &self,
        _desired_task_count: usize,
    ) -> Result<(Vec<NativeGeometrySolutionTask>, usize), NativeCoreError> {
        Err(NativeCoreError::Unavailable)
    }

    pub fn stream_task_paths(
        &self,
        _task: &NativeGeometrySolutionTask,
        _cancellation: &ExecutionCancellationToken,
        _consumer: &mut dyn NativeGeometryPathConsumer,
    ) -> Result<u64, NativeCoreError> {
        Err(NativeCoreError::Unavailable)
    }

    pub fn stream_buildable_task(
        &self,
        _task: &NativeGeometrySolutionTask,
        _packing_problem: &CPackingProblem,
        _buildup_scratch: &mut CBuildUpProblem,
        _buildup_workspace: &mut crate::native::NativeBuildUpWorkspace,
        _cancellation: &ExecutionCancellationToken,
        _evidence_policy: PruningEvidencePolicy,
        _consumer: &mut dyn crate::native::NativePackingCandidateConsumer,
    ) -> Result<NativeBuildableGeometryTaskOutcome, NativeCoreError> {
        Err(NativeCoreError::Unavailable)
    }
}

const _: () = assert!(core::mem::size_of::<NativeGeometrySolutionTask>() == 128);
// This report mirrors a native C ABI containing size_t fields. The WASM
// product never links this surface; its 32-bit layout is intentionally not
// asserted as a native ABI. Shared descriptor types remain available so the
// Rust workspace can compile without creating a native process fallback.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<CNativeBuildableGeometryStreamReport>() == 40);
