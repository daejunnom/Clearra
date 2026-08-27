#[cfg(feature = "native-c-core")]
use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
#[cfg(feature = "native-c-core")]
use clearra_core_domain::pruning::PruningEvidencePolicy;
#[cfg(feature = "native-c-core")]
use clearra_core_domain::resource::{
    ExecutionAvailability, ExecutionAvailabilityReason, ResourceReport,
};

#[cfg(all(feature = "native-c-core", any(test, feature = "test-support")))]
use crate::packing_problem::CPackingCandidate;
#[cfg(feature = "native-c-core")]
use crate::problem::CBuildUpProblem;
use crate::problem::CPackingProblem;

use super::NativeCoreError;
#[cfg(feature = "native-c-core")]
use super::NativePackingCandidateConsumer;
#[cfg(feature = "native-c-core")]
use super::NativePackingStreamOutcome;
#[cfg(all(feature = "native-c-core", any(test, feature = "test-support")))]
use super::{
    NativeGeometryMaterializationOutcome, NativeGeometryStreamOutcome, NativePackingOutcome,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativeGeometryCatalogIdentity {
    pub board_layout_id: u64,
    pub compact_universe_digest: u64,
    pub target_geometry_digest: u64,
    pub piece_catalog_id: u64,
    pub skeleton_projection_version: u64,
    pub rule_capability_id: u64,
    pub realization_table_digest: u64,
    pub support_table_digest: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct CNativeGeometryCatalogView {
    pub identity: NativeGeometryCatalogIdentity,
    pub skeleton_cell_masks: *const u64,
    pub skeleton_piece_kinds: *const u32,
    pub skeleton_realization_offsets: *const u32,
    pub skeleton_realization_counts: *const u32,
    pub cell_support_offsets: *const u32,
    pub cell_support_row_ids: *const u32,
    pub skeleton_count: u32,
    pub realization_count: u32,
    pub support_entry_count: u32,
    pub cell_count: u32,
}

impl Default for CNativeGeometryCatalogView {
    fn default() -> Self {
        Self {
            identity: NativeGeometryCatalogIdentity::default(),
            skeleton_cell_masks: core::ptr::null(),
            skeleton_piece_kinds: core::ptr::null(),
            skeleton_realization_offsets: core::ptr::null(),
            skeleton_realization_counts: core::ptr::null(),
            cell_support_offsets: core::ptr::null(),
            cell_support_row_ids: core::ptr::null(),
            skeleton_count: 0,
            realization_count: 0,
            support_entry_count: 0,
            cell_count: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NativeGeometryCatalogView<'a> {
    identity: NativeGeometryCatalogIdentity,
    skeleton_cell_masks: &'a [u64],
    skeleton_piece_kinds: &'a [u32],
    skeleton_realization_offsets: &'a [u32],
    skeleton_realization_counts: &'a [u32],
    cell_support_offsets: &'a [u32],
    cell_support_row_ids: &'a [u32],
    realization_count: u32,
}

impl<'a> NativeGeometryCatalogView<'a> {
    pub const fn identity(self) -> NativeGeometryCatalogIdentity {
        self.identity
    }

    pub const fn skeleton_cell_masks(self) -> &'a [u64] {
        self.skeleton_cell_masks
    }

    pub const fn skeleton_piece_kinds(self) -> &'a [u32] {
        self.skeleton_piece_kinds
    }

    pub const fn skeleton_realization_offsets(self) -> &'a [u32] {
        self.skeleton_realization_offsets
    }

    pub const fn skeleton_realization_counts(self) -> &'a [u32] {
        self.skeleton_realization_counts
    }

    pub const fn cell_support_offsets(self) -> &'a [u32] {
        self.cell_support_offsets
    }

    pub const fn cell_support_row_ids(self) -> &'a [u32] {
        self.cell_support_row_ids
    }

    pub const fn realization_count(self) -> u32 {
        self.realization_count
    }
}

#[cfg(feature = "native-c-core")]
mod linked {
    use std::{
        collections::VecDeque,
        ffi::c_void,
        ptr::NonNull,
        sync::{Arc, Mutex, OnceLock},
    };

    use super::*;
    #[cfg(any(test, feature = "test-support"))]
    use crate::native::NativeCandidateReducer;
    use crate::native::{
        CNativeBuildableGeometryStreamReport, CNativePruningProofLedger, CNativeResourceReport,
        NativeBuildUpWorkspace, NativeBuildableGeometryTaskOutcome, NativePruningLedger,
    };

    const C_PACKING_STATUS_OK: i32 = 0;
    const C_PACKING_STATUS_CAPACITY_EXCEEDED: i32 = 6;
    const C_PACKING_STATUS_CANCELLED: i32 = 7;
    const C_PRUNING_EVIDENCE_BEST_EFFORT: i32 = 1;
    const MAX_CACHED_GEOMETRY_CATALOGS: usize = 8;
    const NATIVE_MEMORY_BUDGET_QUANTUM_BYTES: u128 = 1024 * 1024;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct NativeGeometryCatalogCacheKey {
        width: u16,
        height: u16,
        initial_mask: u64,
        goal_region_mask: u64,
        required_fill_mask: u64,
        forbidden_mask: u64,
        piece_set_profile_id: u32,
        rule_profile_id: u32,
        kick_profile_id: u32,
    }

    impl NativeGeometryCatalogCacheKey {
        fn from_problem(problem: &CPackingProblem) -> Self {
            Self {
                width: problem.board.width,
                height: if problem.board.search_height == 0 {
                    problem.board.visible_height
                } else {
                    problem.board.search_height
                },
                initial_mask: problem.board.initial_mask,
                goal_region_mask: problem.goal_region_mask,
                required_fill_mask: problem.required_fill_mask,
                forbidden_mask: problem.forbidden_mask,
                piece_set_profile_id: problem.rule.piece_set_profile_id,
                rule_profile_id: problem.rule.rule_profile_id,
                kick_profile_id: problem.rule.kick_profile_id,
            }
        }
    }

    /// Immutable native geometry catalog. The C allocation is shared read-only
    /// by every partition and released only after the last Rust owner drops it.
    struct NativeGeometryCatalogInner {
        pointer: NonNull<c_void>,
        compile_resource_report: ResourceReport,
        pruning_ledger: NativePruningLedger,
        identity: NativeGeometryCatalogIdentity,
    }

    // C freezes every catalog-owned array before publishing the handle. Search
    // calls only read the catalog and keep all mutable state worker-local.
    unsafe impl Send for NativeGeometryCatalogInner {}
    unsafe impl Sync for NativeGeometryCatalogInner {}

    #[derive(Clone)]
    pub struct NativeGeometryCatalog {
        inner: Arc<NativeGeometryCatalogInner>,
    }

    fn convert_pruning_ledger_or_release_unpublished_catalog(
        pointer: &mut *mut c_void,
        pruning_ledger: &CNativePruningProofLedger,
        release: impl FnOnce(&mut *mut c_void),
    ) -> Result<NativePruningLedger, NativeCoreError> {
        match pruning_ledger.to_owned_report() {
            Ok(ledger) => Ok(ledger),
            Err(error) => {
                release(pointer);
                debug_assert!((*pointer).is_null());
                Err(NativeCoreError::InvalidPruningLedger(error))
            }
        }
    }

    impl NativeGeometryCatalog {
        pub(crate) fn compile(problem: &CPackingProblem) -> Result<Self, NativeCoreError> {
            let mut resource_report = CNativeResourceReport::default();
            let mut pruning_ledger = CNativePruningProofLedger::default();
            let (status, pointer) = crate::raw::bindings::geometry_catalog::compile(
                problem,
                &mut resource_report,
                C_PRUNING_EVIDENCE_BEST_EFFORT,
                &mut pruning_ledger,
            );
            let resource_report = resource_report.to_domain();
            if status != C_PACKING_STATUS_OK {
                return Err(NativeCoreError::packing_with_resource_report(
                    status,
                    resource_report,
                ));
            }
            let pointer = NonNull::new(pointer).ok_or(NativeCoreError::PackingStatus(1))?;
            let mut raw_view = CNativeGeometryCatalogView::default();
            if !crate::raw::bindings::geometry_catalog::borrow_view(pointer.as_ptr(), &mut raw_view)
                || !raw_view_is_valid(&raw_view)
                || !unsafe { raw_view_contents_valid(&raw_view) }
            {
                let mut failed_pointer = pointer.as_ptr();
                crate::raw::bindings::geometry_catalog::release(&mut failed_pointer);
                return Err(NativeCoreError::PackingStatus(1));
            }
            let mut owned_pointer = pointer.as_ptr();
            let pruning_ledger = convert_pruning_ledger_or_release_unpublished_catalog(
                &mut owned_pointer,
                &pruning_ledger,
                crate::raw::bindings::geometry_catalog::release,
            )?;
            let pointer = NonNull::new(owned_pointer).ok_or(NativeCoreError::PackingStatus(1))?;
            Ok(Self {
                inner: Arc::new(NativeGeometryCatalogInner {
                    pointer,
                    compile_resource_report: resource_report,
                    pruning_ledger,
                    identity: raw_view.identity,
                }),
            })
        }

        pub fn compile_resource_report(&self) -> &ResourceReport {
            &self.inner.compile_resource_report
        }

        pub fn pruning_ledger(&self) -> &NativePruningLedger {
            &self.inner.pruning_ledger
        }

        pub fn identity(&self) -> NativeGeometryCatalogIdentity {
            self.inner.identity
        }

        pub fn resident_bytes(&self) -> usize {
            crate::raw::bindings::geometry_catalog::resident_bytes(self.inner.pointer.as_ptr())
        }

        /// Stable allocation token for foreign-buffer cache ownership. It is
        /// not a semantic identity and must never authorize deduplication.
        pub(crate) fn raw_pointer(&self) -> *const c_void {
            self.inner.pointer.as_ptr()
        }

        pub fn view(&self) -> NativeGeometryCatalogView<'_> {
            let mut raw = CNativeGeometryCatalogView::default();
            let valid = crate::raw::bindings::geometry_catalog::borrow_view(
                self.inner.pointer.as_ptr(),
                &mut raw,
            );
            debug_assert!(valid && raw_view_is_valid(&raw));
            unsafe { view_from_raw(&raw) }
        }

        pub fn stream_buildable_rows(
            &self,
            skeleton_row_ids: &[u32],
            packing_problem: &CPackingProblem,
            buildup_scratch: &mut CBuildUpProblem,
            buildup_workspace: &mut NativeBuildUpWorkspace,
            cancellation: &ExecutionCancellationToken,
            evidence_policy: PruningEvidencePolicy,
            consumer: &mut dyn NativePackingCandidateConsumer,
        ) -> Result<NativeBuildableGeometryTaskOutcome, NativeCoreError> {
            let _control =
                crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
            let mut workspace = buildup_workspace.raw_handle()?;
            let mut report = CNativeBuildableGeometryStreamReport::default();
            let mut pruning_ledger = CNativePruningProofLedger::default();
            let native_evidence_policy = match evidence_policy {
                PruningEvidencePolicy::BestEffort => 1,
                PruningEvidencePolicy::CompleteRequired => 2,
            };
            let status = {
                let mut sink =
                    crate::raw::packing_candidate_sink::NativeCandidateSinkHandle::new(consumer);
                crate::raw::bindings::geometry_solution_graph::stream_buildable_rows(
                    self.raw_pointer(),
                    skeleton_row_ids,
                    packing_problem,
                    buildup_scratch,
                    workspace.as_mut_ptr(),
                    sink.as_mut(),
                    native_evidence_policy,
                    &mut pruning_ledger,
                    &mut report,
                )
            };
            if status == C_PACKING_STATUS_CANCELLED || cancellation.is_cancelled() {
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

        #[cfg(any(test, feature = "test-support"))]
        pub fn generate_partition(
            &self,
            problem: &CPackingProblem,
            family_begin: u16,
            family_end: u16,
            partition_index: u16,
            partition_count: u16,
            partition_depth: u8,
            cancellation: &ExecutionCancellationToken,
        ) -> Result<NativePackingOutcome, NativeCoreError> {
            let mut reducer = NativeCandidateReducer::new(problem)
                .map_err(|_| NativeCoreError::PackingStatus(1))?;
            let streamed = self.stream_partition(
                problem,
                family_begin,
                family_end,
                partition_index,
                partition_count,
                partition_depth,
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

        pub fn stream_partition(
            &self,
            problem: &CPackingProblem,
            family_begin: u16,
            family_end: u16,
            partition_index: u16,
            partition_count: u16,
            partition_depth: u8,
            cancellation: &ExecutionCancellationToken,
            consumer: &mut dyn NativePackingCandidateConsumer,
        ) -> Result<NativePackingStreamOutcome, NativeCoreError> {
            let _execution_control =
                crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
            let mut resource_report = CNativeResourceReport::default();
            let status = {
                let mut sink =
                    crate::raw::packing_candidate_sink::NativeCandidateSinkHandle::new(consumer);
                crate::raw::bindings::geometry_catalog::search_partition_to_sink(
                    self.inner.pointer.as_ptr(),
                    problem,
                    family_begin,
                    family_end,
                    partition_index,
                    partition_count,
                    partition_depth,
                    sink.as_mut(),
                    &mut resource_report,
                )
            };
            if status == C_PACKING_STATUS_CANCELLED || cancellation.is_cancelled() {
                return Err(NativeCoreError::ExecutionCancelled);
            }
            if status != C_PACKING_STATUS_OK
                && !(status == C_PACKING_STATUS_CAPACITY_EXCEEDED && resource_report.truncated != 0)
            {
                return Err(NativeCoreError::PackingStatus(status));
            }
            Ok(NativePackingStreamOutcome {
                status,
                resource_report: resource_report.to_domain(),
                pruning_ledger: self.pruning_ledger().clone(),
            })
        }

        #[cfg(any(test, feature = "test-support"))]
        pub fn materialize_paths_to_consumer(
            &self,
            problem: &CPackingProblem,
            paths: &[crate::native::CNativePackingGeometryPath],
            cancellation: &ExecutionCancellationToken,
            consumer: &mut dyn NativePackingCandidateConsumer,
        ) -> Result<NativeGeometryStreamOutcome, NativeCoreError> {
            if paths.is_empty() {
                return Ok(NativeGeometryStreamOutcome {
                    status: C_PACKING_STATUS_OK,
                    resource_report: ResourceReport::complete(),
                });
            }
            let path_count =
                u32::try_from(paths.len()).map_err(|_| NativeCoreError::PackingStatus(1))?;
            let _execution_control =
                crate::raw::execution_control::NativeExecutionControlGuard::install(cancellation)?;
            let mut resource_report = CNativeResourceReport::default();
            let status = {
                let mut sink =
                    crate::raw::packing_candidate_sink::NativeCandidateSinkHandle::new(consumer);
                crate::raw::bindings::materialize_packing_catalog_paths(
                    self.inner.pointer.as_ptr(),
                    problem,
                    paths,
                    path_count,
                    sink.as_mut(),
                    &mut resource_report,
                )
            };
            if status == C_PACKING_STATUS_CANCELLED || cancellation.is_cancelled() {
                return Err(NativeCoreError::ExecutionCancelled);
            }
            if status == C_PACKING_STATUS_OK
                || status == C_PACKING_STATUS_CAPACITY_EXCEEDED && resource_report.truncated != 0
            {
                Ok(NativeGeometryStreamOutcome {
                    status,
                    resource_report: resource_report.to_domain(),
                })
            } else {
                Err(NativeCoreError::PackingStatus(status))
            }
        }

        #[cfg(any(test, feature = "test-support"))]
        pub fn materialize_paths(
            &self,
            problem: &CPackingProblem,
            paths: &[crate::native::CNativePackingGeometryPath],
            cancellation: &ExecutionCancellationToken,
        ) -> Result<NativeGeometryMaterializationOutcome, NativeCoreError> {
            let mut reducer = NativeCandidateReducer::new(problem)
                .map_err(|_| NativeCoreError::PackingStatus(1))?;
            let streamed =
                self.materialize_paths_to_consumer(problem, paths, cancellation, &mut reducer)?;
            Ok(NativeGeometryMaterializationOutcome {
                candidates: reducer.into_candidates(),
                resource_report: streamed.resource_report,
            })
        }

        #[cfg(any(test, feature = "test-support"))]
        pub fn materialize_row_ids(
            &self,
            problem: &CPackingProblem,
            skeleton_row_ids: &[u32],
        ) -> Result<CPackingCandidate, NativeCoreError> {
            if skeleton_row_ids.is_empty()
                || skeleton_row_ids.len() > crate::native::C_NATIVE_PACKING_MAX_PIECES
            {
                return Err(NativeCoreError::PackingStatus(1));
            }
            let mut candidate =
                crate::raw::packing_candidate_sink::CNativePackingCandidateView::default();
            let status = crate::raw::bindings::materialize_packing_catalog_row_ids(
                self.inner.pointer.as_ptr(),
                problem,
                skeleton_row_ids,
                &mut candidate,
            );
            if status != C_PACKING_STATUS_OK {
                return Err(NativeCoreError::PackingStatus(status));
            }
            Ok(candidate.to_candidate())
        }
    }

    impl core::fmt::Debug for NativeGeometryCatalog {
        fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter
                .debug_struct("NativeGeometryCatalog")
                .field("identity", &self.identity())
                .finish_non_exhaustive()
        }
    }

    impl PartialEq for NativeGeometryCatalog {
        fn eq(&self, other: &Self) -> bool {
            self.identity() == other.identity()
        }
    }

    impl Eq for NativeGeometryCatalog {}

    impl Drop for NativeGeometryCatalogInner {
        fn drop(&mut self) {
            let mut pointer = self.pointer.as_ptr();
            crate::raw::bindings::geometry_catalog::release(&mut pointer);
            debug_assert!(pointer.is_null());
        }
    }

    fn raw_view_is_valid(view: &CNativeGeometryCatalogView) -> bool {
        let skeleton_count = view.skeleton_count as usize;
        let support_count = view.support_entry_count as usize;
        view.cell_count <= 64
            && view.cell_count > 0
            && (skeleton_count == 0
                || !view.skeleton_cell_masks.is_null()
                    && !view.skeleton_piece_kinds.is_null()
                    && !view.skeleton_realization_offsets.is_null()
                    && !view.skeleton_realization_counts.is_null())
            && !view.cell_support_offsets.is_null()
            && (support_count == 0 || !view.cell_support_row_ids.is_null())
    }

    unsafe fn raw_view_contents_valid(view: &CNativeGeometryCatalogView) -> bool {
        let catalog = unsafe { view_from_raw(view) };
        let offsets = catalog.cell_support_offsets();
        offsets.first() == Some(&0)
            && offsets.last() == Some(&view.support_entry_count)
            && offsets.windows(2).all(|pair| pair[0] <= pair[1])
            && catalog
                .cell_support_row_ids()
                .iter()
                .all(|row| *row < view.skeleton_count)
            && catalog
                .skeleton_realization_offsets()
                .iter()
                .zip(catalog.skeleton_realization_counts())
                .all(|(offset, count)| {
                    offset
                        .checked_add(*count)
                        .is_some_and(|end| end <= view.realization_count)
                })
    }

    unsafe fn view_from_raw<'a>(
        view: &CNativeGeometryCatalogView,
    ) -> NativeGeometryCatalogView<'a> {
        let skeleton_count = view.skeleton_count as usize;
        let support_count = view.support_entry_count as usize;
        NativeGeometryCatalogView {
            identity: view.identity,
            skeleton_cell_masks: unsafe {
                slice_from_raw_parts(view.skeleton_cell_masks, skeleton_count)
            },
            skeleton_piece_kinds: unsafe {
                slice_from_raw_parts(view.skeleton_piece_kinds, skeleton_count)
            },
            skeleton_realization_offsets: unsafe {
                slice_from_raw_parts(view.skeleton_realization_offsets, skeleton_count)
            },
            skeleton_realization_counts: unsafe {
                slice_from_raw_parts(view.skeleton_realization_counts, skeleton_count)
            },
            cell_support_offsets: unsafe {
                slice_from_raw_parts(view.cell_support_offsets, view.cell_count as usize + 1)
            },
            cell_support_row_ids: unsafe {
                slice_from_raw_parts(view.cell_support_row_ids, support_count)
            },
            realization_count: view.realization_count,
        }
    }

    unsafe fn slice_from_raw_parts<'a, T>(pointer: *const T, len: usize) -> &'a [T] {
        let pointer = if len == 0 {
            core::ptr::NonNull::<T>::dangling().as_ptr()
        } else {
            pointer
        };
        unsafe { core::slice::from_raw_parts(pointer, len) }
    }

    fn catalog_cache(
    ) -> &'static Mutex<VecDeque<(NativeGeometryCatalogCacheKey, NativeGeometryCatalog)>> {
        static CACHE: OnceLock<
            Mutex<VecDeque<(NativeGeometryCatalogCacheKey, NativeGeometryCatalog)>>,
        > = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
    }

    fn catalog_fits_request_budget(
        problem: &CPackingProblem,
        catalog: &NativeGeometryCatalog,
    ) -> bool {
        match configured_request_memory_bytes(problem) {
            Some(max_bytes) => catalog.resident_bytes() as u128 <= max_bytes,
            None => true,
        }
    }

    fn configured_request_memory_bytes(problem: &CPackingProblem) -> Option<u128> {
        (problem.budget.has_max_memory_mib != 0)
            .then(|| u128::from(problem.budget.max_memory_mib) * NATIVE_MEMORY_BUDGET_QUANTUM_BYTES)
    }

    fn memory_budget_error(required_memory_bytes: u128) -> NativeCoreError {
        NativeCoreError::PackingIncomplete {
            status: C_PACKING_STATUS_CAPACITY_EXCEEDED,
            resource_report: ResourceReport::admission_failure(
                ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded)
                    .with_required_memory_bytes(required_memory_bytes),
            ),
        }
    }

    fn ensure_request_memory_budget(
        problem: &CPackingProblem,
        required_memory_bytes: u128,
    ) -> Result<(), NativeCoreError> {
        if configured_request_memory_bytes(problem)
            .is_some_and(|max_bytes| required_memory_bytes > max_bytes)
        {
            return Err(memory_budget_error(required_memory_bytes));
        }
        Ok(())
    }

    pub(crate) fn compile(
        problem: &CPackingProblem,
    ) -> Result<NativeGeometryCatalog, NativeCoreError> {
        compile_optional_control(problem, None)
    }

    pub(crate) fn compile_with_cancellation(
        problem: &CPackingProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<NativeGeometryCatalog, NativeCoreError> {
        compile_optional_control(problem, Some(cancellation))
    }

    fn compile_optional_control(
        problem: &CPackingProblem,
        cancellation: Option<&ExecutionCancellationToken>,
    ) -> Result<NativeGeometryCatalog, NativeCoreError> {
        if cancellation.is_some_and(ExecutionCancellationToken::is_cancelled) {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        let key = NativeGeometryCatalogCacheKey::from_problem(problem);
        {
            let mut cache = catalog_cache()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(index) = cache.iter().position(|(cached, catalog)| {
                *cached == key && catalog_fits_request_budget(problem, catalog)
            }) {
                let entry = cache
                    .remove(index)
                    .expect("a located geometry catalog cache entry exists");
                let catalog = entry.1.clone();
                cache.push_back(entry);
                return Ok(catalog);
            }
        }
        let _execution_control = cancellation
            .map(crate::raw::execution_control::NativeExecutionControlGuard::install)
            .transpose()?;
        // The C ABI accepts memory limits in whole MiB. A configured zero-MiB
        // request must therefore fail before its first catalog allocation;
        // every non-zero request gives the C allocator at least one complete
        // budget quantum, which it enforces for each subsequent allocation.
        ensure_request_memory_budget(problem, NATIVE_MEMORY_BUDGET_QUANTUM_BYTES)?;
        let catalog = NativeGeometryCatalog::compile(problem)?;
        // The C allocator accounts catalog-owned buffers, but this boundary is
        // also the cache/publication owner. Reject and drop an over-cap result
        // before it can enter the process-wide cache.
        ensure_request_memory_budget(problem, catalog.resident_bytes() as u128)?;
        if cancellation.is_some_and(ExecutionCancellationToken::is_cancelled) {
            return Err(NativeCoreError::ExecutionCancelled);
        }
        let mut cache = catalog_cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(index) = cache.iter().position(|(cached, cached_catalog)| {
            *cached == key && catalog_fits_request_budget(problem, cached_catalog)
        }) {
            let entry = cache
                .remove(index)
                .expect("a located geometry catalog cache entry exists");
            let reused = entry.1.clone();
            cache.push_back(entry);
            return Ok(reused);
        }
        if cache.len() == MAX_CACHED_GEOMETRY_CATALOGS {
            cache.pop_front();
        }
        cache.push_back((key, catalog.clone()));
        Ok(catalog)
    }

    #[cfg(test)]
    mod tests {
        use std::cell::Cell;

        use super::*;

        #[test]
        fn invalid_compile_ledger_releases_unpublished_catalog_exactly_once() {
            let mut pointer = NonNull::<u8>::dangling().as_ptr().cast::<c_void>();
            let expected_pointer = pointer;
            let release_count = Cell::new(0_usize);

            let result = convert_pruning_ledger_or_release_unpublished_catalog(
                &mut pointer,
                &CNativePruningProofLedger::default(),
                |released_pointer| {
                    assert_eq!(*released_pointer, expected_pointer);
                    release_count.set(release_count.get() + 1);
                    *released_pointer = core::ptr::null_mut();
                },
            );

            assert!(matches!(
                result,
                Err(NativeCoreError::InvalidPruningLedger(_))
            ));
            assert_eq!(release_count.get(), 1);
            assert!(pointer.is_null());
        }

        #[test]
        fn configured_zero_mib_is_rejected_before_native_catalog_allocation() {
            let mut problem = CPackingProblem::default();
            problem.budget.has_max_memory_mib = 1;
            problem.budget.max_memory_mib = 0;

            let error = ensure_request_memory_budget(&problem, NATIVE_MEMORY_BUDGET_QUANTUM_BYTES)
                .expect_err("zero MiB cannot authorize the first allocation");
            let NativeCoreError::PackingIncomplete {
                resource_report, ..
            } = error
            else {
                panic!("expected typed resource failure");
            };
            let availability = resource_report.execution_availability();
            assert_eq!(
                availability.reason(),
                Some(ExecutionAvailabilityReason::MemoryBudgetExceeded)
            );
            assert_eq!(
                availability.required_memory_bytes(),
                Some(NATIVE_MEMORY_BUDGET_QUANTUM_BYTES)
            );
            assert!(!resource_report.execution_started());
            assert!(!resource_report.result_complete());
        }

        #[test]
        fn configured_catalog_budget_uses_the_native_mib_unit_exactly() {
            let mut problem = CPackingProblem::default();
            problem.budget.has_max_memory_mib = 1;
            problem.budget.max_memory_mib = 2;

            assert_eq!(
                configured_request_memory_bytes(&problem),
                Some(2 * NATIVE_MEMORY_BUDGET_QUANTUM_BYTES)
            );
            assert!(
                ensure_request_memory_budget(&problem, 2 * NATIVE_MEMORY_BUDGET_QUANTUM_BYTES)
                    .is_ok()
            );
            assert!(ensure_request_memory_budget(
                &problem,
                2 * NATIVE_MEMORY_BUDGET_QUANTUM_BYTES + 1
            )
            .is_err());
        }
    }
}

#[cfg(feature = "native-c-core")]
pub use linked::NativeGeometryCatalog;

#[cfg(feature = "native-c-core")]
pub(crate) use linked::compile;
#[cfg(feature = "native-c-core")]
pub(crate) use linked::compile_with_cancellation;

#[cfg(not(feature = "native-c-core"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeGeometryCatalog {
    _private: (),
}

#[cfg(not(feature = "native-c-core"))]
impl NativeGeometryCatalog {
    pub(crate) const fn raw_pointer(&self) -> *const core::ffi::c_void {
        core::ptr::null()
    }

    pub fn compile_resource_report(&self) -> &clearra_core_domain::resource::ResourceReport {
        static REPORT: std::sync::OnceLock<clearra_core_domain::resource::ResourceReport> =
            std::sync::OnceLock::new();
        REPORT.get_or_init(clearra_core_domain::resource::ResourceReport::complete)
    }

    pub const fn resident_bytes(&self) -> usize {
        0
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn generate_partition(
        &self,
        _problem: &CPackingProblem,
        _family_begin: u16,
        _family_end: u16,
        _partition_index: u16,
        _partition_count: u16,
        _partition_depth: u8,
        _cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
    ) -> Result<super::NativePackingOutcome, NativeCoreError> {
        Err(NativeCoreError::Unavailable)
    }

    pub fn stream_partition(
        &self,
        _problem: &CPackingProblem,
        _family_begin: u16,
        _family_end: u16,
        _partition_index: u16,
        _partition_count: u16,
        _partition_depth: u8,
        _cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
        _consumer: &mut dyn super::NativePackingCandidateConsumer,
    ) -> Result<super::NativePackingStreamOutcome, NativeCoreError> {
        Err(NativeCoreError::Unavailable)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn materialize_row_ids(
        &self,
        _problem: &CPackingProblem,
        _skeleton_row_ids: &[u32],
    ) -> Result<crate::packing_problem::CPackingCandidate, NativeCoreError> {
        Err(NativeCoreError::Unavailable)
    }
}

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn compile(
    _problem: &CPackingProblem,
) -> Result<NativeGeometryCatalog, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}

#[cfg(not(feature = "native-c-core"))]
pub(crate) fn compile_with_cancellation(
    _problem: &CPackingProblem,
    _cancellation: &clearra_core_domain::execution_cancellation::ExecutionCancellationToken,
) -> Result<NativeGeometryCatalog, NativeCoreError> {
    Err(NativeCoreError::Unavailable)
}
