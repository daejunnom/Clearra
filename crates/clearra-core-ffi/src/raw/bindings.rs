#[cfg(feature = "native-c-core")]
mod linked {
    #[cfg(any(test, feature = "test-support"))]
    use crate::native::CNativePackingCandidateBuffer;
    use crate::raw::execution_control::CNativeExecutionControl;
    use crate::{
        gpu::{CNativeGpuBackendCapability, CNativeGpuDeviceRequest},
        native::{
            buildup_geometry_language::{
                CNativeBuildUpGeometryLanguageEdge, CNativeBuildUpGeometryLanguageEdgeV2,
                CNativeBuildUpGeometryLanguageNode, CNativeBuildUpGeometryLanguageNodeV2,
                CNativeBuildUpGeometryLanguageReport, CNativeBuildUpGeometryLanguageReportV2,
            },
            CNativeBuildUpCountLimits, CNativeBuildUpCountReport, CNativeBuildUpEnumerationLimits,
            CNativeBuildUpVerification, CNativeBuildVariantBuffer,
            CNativeBuildableGeometryStreamReport, CNativeGeometryCatalogView,
            CNativePruningProofLedger, CNativeResourceReport, NativeGeometrySolutionTask,
        },
        problem::{CBuildUpProblem, CPackingProblem},
        raw::geometry_path_sink::CNativeGeometryPathSink,
        raw::packing_candidate_sink::CNativePackingCandidateSink,
    };
    #[cfg(any(test, feature = "test-support"))]
    use crate::{
        native::CNativePackingGeometryPath,
        raw::packing_candidate_sink::CNativePackingCandidateView,
    };
    #[cfg(feature = "search-stage-profiling")]
    use std::os::raw::c_char;
    use std::{ffi::c_void, os::raw::c_int};

    #[link(name = "clearra_core", kind = "static")]
    unsafe extern "C" {
        pub fn clearra_core_abi_version() -> c_int;

        pub fn clr_execution_control_install(control: *const CNativeExecutionControl) -> c_int;
        pub fn clr_execution_control_clear();

        pub fn clearra_gpu_device_capability_query(
            request: CNativeGpuDeviceRequest,
            out_capability: *mut CNativeGpuBackendCapability,
        ) -> c_int;

        pub fn clearra_geometry_catalog_compile(
            problem: *const CPackingProblem,
            out_resource_report: *mut CNativeResourceReport,
            evidence_policy: c_int,
            out_pruning_ledger: *mut CNativePruningProofLedger,
            out_catalog: *mut *mut c_void,
        ) -> c_int;

        pub fn clearra_geometry_catalog_release(catalog: *mut *mut c_void);
        pub fn clearra_geometry_catalog_resident_bytes(catalog: *const c_void) -> usize;
        pub fn clearra_geometry_catalog_borrow_view(
            catalog: *const c_void,
            out_view: *mut CNativeGeometryCatalogView,
        ) -> bool;

        pub fn clearra_geometry_exact_cover_search_family_to_sink(
            catalog: *const c_void,
            problem: *const CPackingProblem,
            family_begin: u16,
            family_end: u16,
            partition_index: u16,
            partition_count: u16,
            partition_depth: u8,
            sink: *const CNativePackingCandidateSink,
            out_resource_report: *mut CNativeResourceReport,
        ) -> c_int;

        pub fn clearra_geometry_exact_cover_search_graph_with_pruning_ledger(
            catalog: *const c_void,
            problem: *const CPackingProblem,
            evidence_policy: c_int,
            out_graph: *mut *mut c_void,
            out_resource_report: *mut CNativeResourceReport,
            out_pruning_ledger: *mut CNativePruningProofLedger,
        ) -> c_int;
        pub fn clearra_geometry_solution_graph_release(graph: *mut *mut c_void);
        pub fn clearra_geometry_solution_graph_resident_bytes(graph: *const c_void) -> usize;
        pub fn clearra_geometry_solution_graph_node_count(graph: *const c_void) -> u32;
        pub fn clearra_geometry_solution_graph_split_tasks(
            graph: *const c_void,
            tasks: *mut NativeGeometrySolutionTask,
            task_capacity: u32,
            out_task_count: *mut u32,
            out_peak_scratch_bytes: *mut usize,
        ) -> c_int;
        pub fn clearra_geometry_solution_graph_stream_task_paths(
            graph: *const c_void,
            task: *const NativeGeometrySolutionTask,
            sink: *const CNativeGeometryPathSink,
            out_emitted_count: *mut u64,
        ) -> c_int;
        pub fn clearra_geometry_solution_graph_stream_buildable_task(
            graph: *const c_void,
            catalog: *const c_void,
            task: *const NativeGeometrySolutionTask,
            packing_problem: *const CPackingProblem,
            buildup_scratch: *mut CBuildUpProblem,
            buildup_workspace: *mut c_void,
            sink: *const CNativePackingCandidateSink,
            evidence_policy: c_int,
            out_pruning_ledger: *mut CNativePruningProofLedger,
            out_report: *mut CNativeBuildableGeometryStreamReport,
        ) -> c_int;
        pub fn clearra_geometry_catalog_rows_buildable_to_sink(
            catalog: *const c_void,
            skeleton_row_ids: *const u32,
            operation_count: u8,
            packing_problem: *const CPackingProblem,
            buildup_scratch: *mut CBuildUpProblem,
            buildup_workspace: *mut c_void,
            sink: *const CNativePackingCandidateSink,
            evidence_policy: c_int,
            out_pruning_ledger: *mut CNativePruningProofLedger,
            out_report: *mut CNativeBuildableGeometryStreamReport,
        ) -> c_int;

        #[cfg(any(test, feature = "test-support"))]
        pub fn clearra_packing_materialize_catalog_paths_to_sink(
            catalog: *const c_void,
            problem: *const CPackingProblem,
            paths: *const CNativePackingGeometryPath,
            path_count: u32,
            sink: *const CNativePackingCandidateSink,
            out_resource_report: *mut CNativeResourceReport,
        ) -> c_int;
        #[cfg(any(test, feature = "test-support"))]
        pub fn clearra_packing_materialize_catalog_row_ids(
            catalog: *const c_void,
            problem: *const CPackingProblem,
            skeleton_row_ids: *const u32,
            operation_count: u8,
            out_candidate: *mut CNativePackingCandidateView,
        ) -> c_int;

        pub fn clr_buildup_worker_verify_into_buffer(
            problem: *const CBuildUpProblem,
            out_buffer: *mut CNativeBuildVariantBuffer,
            out_verification: *mut CNativeBuildUpVerification,
        ) -> c_int;

        pub fn clr_buildup_verify_first(
            problem: *const CBuildUpProblem,
            out_first: *mut CNativeBuildVariantBuffer,
        ) -> c_int;

        pub fn clr_buildup_verify_first_with_workspace(
            problem: *const CBuildUpProblem,
            workspace: *mut c_void,
            out_first: *mut CNativeBuildVariantBuffer,
        ) -> c_int;

        pub fn clr_buildup_exists_with_workspace(
            problem: *const CBuildUpProblem,
            workspace: *mut c_void,
        ) -> c_int;

        pub fn clr_buildup_enumerate_variants(
            problem: *const CBuildUpProblem,
            limits: *const CNativeBuildUpEnumerationLimits,
            out_variants: *mut CNativeBuildVariantBuffer,
        ) -> c_int;

        pub fn clr_buildup_workspace_create() -> *mut c_void;
        pub fn clr_buildup_workspace_release(workspace: *mut c_void);
        pub fn clr_buildup_workspace_retained_bytes(workspace: *const c_void) -> usize;
        pub fn clr_buildup_enumerate_variants_with_workspace(
            problem: *const CBuildUpProblem,
            limits: *const CNativeBuildUpEnumerationLimits,
            workspace: *mut c_void,
            out_variants: *mut CNativeBuildVariantBuffer,
        ) -> c_int;
        pub fn clr_buildup_export_geometry_language_with_workspace(
            problem: *const CBuildUpProblem,
            workspace: *mut c_void,
            nodes: *mut CNativeBuildUpGeometryLanguageNode,
            node_capacity: usize,
            edges: *mut CNativeBuildUpGeometryLanguageEdge,
            edge_capacity: usize,
            out_report: *mut CNativeBuildUpGeometryLanguageReport,
        ) -> c_int;
        pub fn clr_buildup_prepare_geometry_language_v2_with_workspace(
            problem: *const CBuildUpProblem,
            workspace: *mut c_void,
            transition_mode: c_int,
            out_report: *mut CNativeBuildUpGeometryLanguageReportV2,
        ) -> c_int;
        pub fn clr_buildup_copy_prepared_geometry_language_v2(
            workspace: *const c_void,
            nodes: *mut CNativeBuildUpGeometryLanguageNodeV2,
            node_capacity: usize,
            edges: *mut CNativeBuildUpGeometryLanguageEdgeV2,
            edge_capacity: usize,
            out_report: *mut CNativeBuildUpGeometryLanguageReportV2,
        ) -> c_int;

        pub fn clr_buildup_count_variants(
            problem: *const CBuildUpProblem,
            limits: *const CNativeBuildUpCountLimits,
            out_report: *mut CNativeBuildUpCountReport,
        ) -> c_int;

        #[cfg(feature = "search-stage-profiling")]
        pub fn clr_search_stage_profile_create() -> *mut c_void;
        #[cfg(feature = "search-stage-profiling")]
        pub fn clr_search_stage_profile_release(profile: *mut c_void);
        #[cfg(feature = "search-stage-profiling")]
        pub fn clr_search_stage_profile_start(profile: *mut c_void) -> bool;
        #[cfg(feature = "search-stage-profiling")]
        pub fn clr_search_stage_profile_stop(profile: *mut c_void);
        #[cfg(feature = "search-stage-profiling")]
        pub fn clr_search_stage_profile_stage_count() -> usize;
        #[cfg(feature = "search-stage-profiling")]
        pub fn clr_search_profile_stage_name(stage: c_int) -> *const c_char;
        #[cfg(feature = "search-stage-profiling")]
        pub fn clr_search_stage_profile_duration_ns(profile: *const c_void, stage: usize) -> u64;
        #[cfg(feature = "search-stage-profiling")]
        pub fn clr_search_stage_profile_invocation_count(
            profile: *const c_void,
            stage: usize,
        ) -> u64;
        #[cfg(feature = "search-stage-profiling")]
        pub fn clr_search_stage_profile_work_item_count(
            profile: *const c_void,
            stage: usize,
        ) -> u64;

    }

    #[cfg(any(test, feature = "test-support"))]
    #[link(name = "clearra_core_test_oracle", kind = "static")]
    unsafe extern "C" {
        pub fn clearra_packing_enumerator_cpu_generate_problem_with_resource_report_pruning_policy_and_ledger(
            problem: *const CPackingProblem,
            out_buffer: *mut CNativePackingCandidateBuffer,
            out_resource_report: *mut CNativeResourceReport,
            evidence_policy: c_int,
            out_pruning_ledger: *mut CNativePruningProofLedger,
        ) -> c_int;

        pub fn clearra_packing_enumerator_cpu_generate_problem_to_sink_with_resource_report_and_pruning_ledger(
            problem: *const CPackingProblem,
            sink: *const CNativePackingCandidateSink,
            out_resource_report: *mut CNativeResourceReport,
            out_pruning_ledger: *mut CNativePruningProofLedger,
        ) -> c_int;

        pub fn clearra_packing_enumerator_cpu_generate_problem_prefix_partition_to_sink_with_resource_report_and_pruning_ledger(
            problem: *const CPackingProblem,
            partition_index: u16,
            partition_count: u16,
            partition_depth: u8,
            sink: *const CNativePackingCandidateSink,
            out_resource_report: *mut CNativeResourceReport,
            out_pruning_ledger: *mut CNativePruningProofLedger,
        ) -> c_int;
    }

    pub fn abi_version() -> i32 {
        unsafe { clearra_core_abi_version() }
    }

    pub fn install_execution_control(control: &CNativeExecutionControl) -> i32 {
        unsafe { clr_execution_control_install(control) }
    }

    pub fn clear_execution_control() {
        unsafe { clr_execution_control_clear() }
    }

    pub fn query_gpu_capability(
        request: CNativeGpuDeviceRequest,
        out_capability: &mut CNativeGpuBackendCapability,
    ) -> i32 {
        unsafe { clearra_gpu_device_capability_query(request, out_capability) }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn generate_packing_candidates_with_pruning_policy(
        problem: &CPackingProblem,
        out_buffer: &mut CNativePackingCandidateBuffer,
        out_resource_report: &mut CNativeResourceReport,
        evidence_policy: i32,
        out_pruning_ledger: &mut CNativePruningProofLedger,
    ) -> i32 {
        unsafe {
            clearra_packing_enumerator_cpu_generate_problem_with_resource_report_pruning_policy_and_ledger(
                problem,
                out_buffer,
                out_resource_report,
                evidence_policy,
                out_pruning_ledger,
            )
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn generate_packing_candidates_to_sink(
        problem: &CPackingProblem,
        sink: &mut CNativePackingCandidateSink,
        out_resource_report: &mut CNativeResourceReport,
        out_pruning_ledger: &mut CNativePruningProofLedger,
    ) -> i32 {
        unsafe {
            clearra_packing_enumerator_cpu_generate_problem_to_sink_with_resource_report_and_pruning_ledger(
                problem,
                sink,
                out_resource_report,
                out_pruning_ledger,
            )
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn generate_packing_candidates_prefix_partition_to_sink(
        problem: &CPackingProblem,
        partition_index: u16,
        partition_count: u16,
        partition_depth: u8,
        sink: &mut CNativePackingCandidateSink,
        out_resource_report: &mut CNativeResourceReport,
        out_pruning_ledger: &mut CNativePruningProofLedger,
    ) -> i32 {
        unsafe {
            clearra_packing_enumerator_cpu_generate_problem_prefix_partition_to_sink_with_resource_report_and_pruning_ledger(
                problem,
                partition_index,
                partition_count,
                partition_depth,
                sink,
                out_resource_report,
                out_pruning_ledger,
            )
        }
    }

    pub mod geometry_catalog {
        use super::*;

        pub fn compile(
            problem: &CPackingProblem,
            out_resource_report: &mut CNativeResourceReport,
            evidence_policy: i32,
            out_pruning_ledger: &mut CNativePruningProofLedger,
        ) -> (i32, *mut c_void) {
            let mut catalog = std::ptr::null_mut();
            let status = unsafe {
                clearra_geometry_catalog_compile(
                    problem,
                    out_resource_report,
                    evidence_policy,
                    out_pruning_ledger,
                    &mut catalog,
                )
            };
            (status, catalog)
        }

        pub fn release(catalog: &mut *mut c_void) {
            unsafe { clearra_geometry_catalog_release(catalog) }
        }

        pub fn resident_bytes(catalog: *const c_void) -> usize {
            unsafe { clearra_geometry_catalog_resident_bytes(catalog) }
        }

        pub fn borrow_view(
            catalog: *const c_void,
            out_view: &mut CNativeGeometryCatalogView,
        ) -> bool {
            unsafe { clearra_geometry_catalog_borrow_view(catalog, out_view) }
        }

        pub fn search_partition_to_sink(
            catalog: *const c_void,
            problem: &CPackingProblem,
            family_begin: u16,
            family_end: u16,
            partition_index: u16,
            partition_count: u16,
            partition_depth: u8,
            sink: &mut CNativePackingCandidateSink,
            out_resource_report: &mut CNativeResourceReport,
        ) -> i32 {
            unsafe {
                clearra_geometry_exact_cover_search_family_to_sink(
                    catalog,
                    problem,
                    family_begin,
                    family_end,
                    partition_index,
                    partition_count,
                    partition_depth,
                    sink,
                    out_resource_report,
                )
            }
        }
    }

    pub mod geometry_solution_graph {
        use super::*;

        pub fn search(
            catalog: *const c_void,
            problem: &CPackingProblem,
            evidence_policy: i32,
            out_resource_report: &mut CNativeResourceReport,
            out_pruning_ledger: &mut CNativePruningProofLedger,
        ) -> (i32, *mut c_void) {
            let mut graph = std::ptr::null_mut();
            let status = unsafe {
                clearra_geometry_exact_cover_search_graph_with_pruning_ledger(
                    catalog,
                    problem,
                    evidence_policy,
                    &mut graph,
                    out_resource_report,
                    out_pruning_ledger,
                )
            };
            (status, graph)
        }

        pub fn release(graph: &mut *mut c_void) {
            unsafe { clearra_geometry_solution_graph_release(graph) }
        }

        pub fn resident_bytes(graph: *const c_void) -> usize {
            unsafe { clearra_geometry_solution_graph_resident_bytes(graph) }
        }

        pub fn node_count(graph: *const c_void) -> u32 {
            unsafe { clearra_geometry_solution_graph_node_count(graph) }
        }

        pub fn split_tasks(
            graph: *const c_void,
            task_capacity: usize,
        ) -> Result<(Vec<NativeGeometrySolutionTask>, usize), i32> {
            let Ok(capacity) = u32::try_from(task_capacity) else {
                return Err(1);
            };
            if capacity == 0 {
                return Err(1);
            }

            let mut tasks = Vec::with_capacity(task_capacity);
            let mut task_count = 0u32;
            let mut peak_scratch_bytes = 0usize;
            let status = unsafe {
                clearra_geometry_solution_graph_split_tasks(
                    graph,
                    tasks.as_mut_ptr(),
                    capacity,
                    &mut task_count,
                    &mut peak_scratch_bytes,
                )
            };
            if status != 0 {
                return Err(status);
            }
            if task_count > capacity {
                return Err(1);
            }

            // The C boundary initializes every returned task before publishing
            // task_count. Keep the uninitialized allocation confined to raw FFI.
            unsafe {
                tasks.set_len(task_count as usize);
            }
            Ok((tasks, peak_scratch_bytes))
        }

        pub fn stream_task_paths(
            graph: *const c_void,
            task: &NativeGeometrySolutionTask,
            sink: &mut CNativeGeometryPathSink,
            out_emitted_count: &mut u64,
        ) -> i32 {
            unsafe {
                clearra_geometry_solution_graph_stream_task_paths(
                    graph,
                    task,
                    sink,
                    out_emitted_count,
                )
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub fn stream_buildable_task(
            graph: *const c_void,
            catalog: *const c_void,
            task: &NativeGeometrySolutionTask,
            packing_problem: &CPackingProblem,
            buildup_scratch: &mut CBuildUpProblem,
            buildup_workspace: *mut c_void,
            sink: &mut CNativePackingCandidateSink,
            evidence_policy: c_int,
            out_pruning_ledger: &mut CNativePruningProofLedger,
            out_report: &mut CNativeBuildableGeometryStreamReport,
        ) -> i32 {
            unsafe {
                clearra_geometry_solution_graph_stream_buildable_task(
                    graph,
                    catalog,
                    task,
                    packing_problem,
                    buildup_scratch,
                    buildup_workspace,
                    sink,
                    evidence_policy,
                    out_pruning_ledger,
                    out_report,
                )
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub fn stream_buildable_rows(
            catalog: *const c_void,
            skeleton_row_ids: &[u32],
            packing_problem: &CPackingProblem,
            buildup_scratch: &mut CBuildUpProblem,
            buildup_workspace: *mut c_void,
            sink: &mut CNativePackingCandidateSink,
            evidence_policy: c_int,
            out_pruning_ledger: &mut CNativePruningProofLedger,
            out_report: &mut CNativeBuildableGeometryStreamReport,
        ) -> i32 {
            let Ok(operation_count) = u8::try_from(skeleton_row_ids.len()) else {
                return 1;
            };
            unsafe {
                clearra_geometry_catalog_rows_buildable_to_sink(
                    catalog,
                    skeleton_row_ids.as_ptr(),
                    operation_count,
                    packing_problem,
                    buildup_scratch,
                    buildup_workspace,
                    sink,
                    evidence_policy,
                    out_pruning_ledger,
                    out_report,
                )
            }
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn materialize_packing_catalog_paths(
        catalog: *const c_void,
        problem: &CPackingProblem,
        paths: &[CNativePackingGeometryPath],
        path_count: u32,
        sink: &mut CNativePackingCandidateSink,
        out_resource_report: &mut CNativeResourceReport,
    ) -> i32 {
        unsafe {
            clearra_packing_materialize_catalog_paths_to_sink(
                catalog,
                problem,
                paths.as_ptr(),
                path_count,
                sink,
                out_resource_report,
            )
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn materialize_packing_catalog_row_ids(
        catalog: *const c_void,
        problem: &CPackingProblem,
        skeleton_row_ids: &[u32],
        out_candidate: &mut CNativePackingCandidateView,
    ) -> i32 {
        let Ok(operation_count) = u8::try_from(skeleton_row_ids.len()) else {
            return 1;
        };
        unsafe {
            clearra_packing_materialize_catalog_row_ids(
                catalog,
                problem,
                skeleton_row_ids.as_ptr(),
                operation_count,
                out_candidate,
            )
        }
    }

    pub fn verify_buildup_problem(
        problem: &CBuildUpProblem,
        out_buffer: &mut CNativeBuildVariantBuffer,
        out_verification: &mut CNativeBuildUpVerification,
    ) -> i32 {
        unsafe { clr_buildup_worker_verify_into_buffer(problem, out_buffer, out_verification) }
    }

    pub fn verify_first_buildup_problem(
        problem: &CBuildUpProblem,
        out_first: &mut CNativeBuildVariantBuffer,
    ) -> i32 {
        unsafe { clr_buildup_verify_first(problem, out_first) }
    }

    pub fn enumerate_buildup_variants(
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpEnumerationLimits,
        out_variants: &mut CNativeBuildVariantBuffer,
    ) -> i32 {
        unsafe { clr_buildup_enumerate_variants(problem, limits, out_variants) }
    }

    pub mod buildup_workspace {
        use super::*;

        pub fn create() -> *mut c_void {
            unsafe { clr_buildup_workspace_create() }
        }

        pub fn release(workspace: *mut c_void) {
            unsafe { clr_buildup_workspace_release(workspace) }
        }

        pub fn retained_bytes(workspace: *const c_void) -> usize {
            unsafe { clr_buildup_workspace_retained_bytes(workspace) }
        }

        pub fn enumerate(
            problem: &CBuildUpProblem,
            limits: &CNativeBuildUpEnumerationLimits,
            workspace: *mut c_void,
            out_variants: &mut CNativeBuildVariantBuffer,
        ) -> i32 {
            unsafe {
                clr_buildup_enumerate_variants_with_workspace(
                    problem,
                    limits,
                    workspace,
                    out_variants,
                )
            }
        }

        pub fn verify_first(
            problem: &CBuildUpProblem,
            workspace: *mut c_void,
            out_first: &mut CNativeBuildVariantBuffer,
        ) -> i32 {
            unsafe { clr_buildup_verify_first_with_workspace(problem, workspace, out_first) }
        }

        pub fn exists(problem: &CBuildUpProblem, workspace: *mut c_void) -> i32 {
            unsafe { clr_buildup_exists_with_workspace(problem, workspace) }
        }

        pub fn export_geometry_language(
            problem: &CBuildUpProblem,
            workspace: *mut c_void,
            nodes: *mut CNativeBuildUpGeometryLanguageNode,
            node_capacity: usize,
            edges: *mut CNativeBuildUpGeometryLanguageEdge,
            edge_capacity: usize,
            out_report: &mut CNativeBuildUpGeometryLanguageReport,
        ) -> i32 {
            unsafe {
                clr_buildup_export_geometry_language_with_workspace(
                    problem,
                    workspace,
                    nodes,
                    node_capacity,
                    edges,
                    edge_capacity,
                    out_report,
                )
            }
        }

        pub fn prepare_geometry_language_v2(
            problem: &CBuildUpProblem,
            workspace: *mut c_void,
            transition_mode: c_int,
            out_report: &mut CNativeBuildUpGeometryLanguageReportV2,
        ) -> i32 {
            unsafe {
                clr_buildup_prepare_geometry_language_v2_with_workspace(
                    problem,
                    workspace,
                    transition_mode,
                    out_report,
                )
            }
        }

        pub fn copy_prepared_geometry_language_v2(
            workspace: *mut c_void,
            nodes: *mut CNativeBuildUpGeometryLanguageNodeV2,
            node_capacity: usize,
            edges: *mut CNativeBuildUpGeometryLanguageEdgeV2,
            edge_capacity: usize,
            out_report: &mut CNativeBuildUpGeometryLanguageReportV2,
        ) -> i32 {
            unsafe {
                clr_buildup_copy_prepared_geometry_language_v2(
                    workspace,
                    nodes,
                    node_capacity,
                    edges,
                    edge_capacity,
                    out_report,
                )
            }
        }
    }

    pub fn count_buildup_variants(
        problem: &CBuildUpProblem,
        limits: &CNativeBuildUpCountLimits,
        out_report: &mut CNativeBuildUpCountReport,
    ) -> i32 {
        unsafe { clr_buildup_count_variants(problem, limits, out_report) }
    }

    #[cfg(feature = "search-stage-profiling")]
    pub mod search_profile {
        use super::*;

        pub fn create() -> *mut c_void {
            unsafe { clr_search_stage_profile_create() }
        }

        pub fn release(profile: *mut c_void) {
            unsafe { clr_search_stage_profile_release(profile) }
        }

        pub fn start(profile: *mut c_void) -> bool {
            unsafe { clr_search_stage_profile_start(profile) }
        }

        pub fn stop(profile: *mut c_void) {
            unsafe { clr_search_stage_profile_stop(profile) }
        }

        pub fn stage_count() -> usize {
            unsafe { clr_search_stage_profile_stage_count() }
        }

        pub fn stage_name(stage: usize) -> *const c_char {
            unsafe { clr_search_profile_stage_name(stage as c_int) }
        }

        pub fn duration_ns(profile: *const c_void, stage: usize) -> u64 {
            unsafe { clr_search_stage_profile_duration_ns(profile, stage) }
        }

        pub fn invocation_count(profile: *const c_void, stage: usize) -> u64 {
            unsafe { clr_search_stage_profile_invocation_count(profile, stage) }
        }

        pub fn work_item_count(profile: *const c_void, stage: usize) -> u64 {
            unsafe { clr_search_stage_profile_work_item_count(profile, stage) }
        }
    }
}

#[cfg(feature = "native-c-core")]
pub use linked::buildup_workspace;
#[cfg(feature = "native-c-core")]
pub use linked::geometry_catalog;
#[cfg(feature = "native-c-core")]
pub use linked::geometry_solution_graph;
#[cfg(feature = "search-stage-profiling")]
pub use linked::search_profile;
#[cfg(feature = "native-c-core")]
pub use linked::{
    abi_version, clear_execution_control, count_buildup_variants, enumerate_buildup_variants,
    install_execution_control, query_gpu_capability, verify_buildup_problem,
    verify_first_buildup_problem,
};
#[cfg(all(feature = "native-c-core", any(test, feature = "test-support")))]
pub use linked::{
    generate_packing_candidates_prefix_partition_to_sink, generate_packing_candidates_to_sink,
    generate_packing_candidates_with_pruning_policy, materialize_packing_catalog_paths,
    materialize_packing_catalog_row_ids,
};
