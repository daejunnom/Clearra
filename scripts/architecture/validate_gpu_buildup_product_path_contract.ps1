# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Keep functions side-effect free at load time; validation runs only when invoked.

function Invoke-GpuBuildUpProductPathContractValidation() {
$gpuPipelineDoc = Read-Text "docs/gpu-pipeline.md"
foreach ($requiredMarker in @(
            "GPU Worker Phase 5 CPU BuildUp Product Path",
            "GpuWorkerResult",
            "CPU confirm bridge",
            "confirmed candidate queue",
            "clearra_hybrid_buildup_dispatch_candidate",
            "clearra_hybrid_collect_build_variants_from_confirmed_queue",
            "verify_first",
            "enumerate_variants",
            "count_variants",
            "clearra_hybrid_coverage_rows_from_build_variants",
            "Rust TypedCoverageMatrix",
            "ObjectiveResult",
            "gpu_assisted_opening_2l_reaches_buildup",
            "gpu_assisted_buildvariant_count_matches_cpu_reference",
            "gpu_assisted_coverage_rows_match_cpu_reference",
            "gpu_verify_first_not_used_for_coverage"
        )) {
        if ($gpuPipelineDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/gpu-pipeline.md must document GPU BuildUp product path marker '$requiredMarker'"
        }
    }
foreach ($requiredPath in @(
            "core-c/src/scheduler/hybrid_buildup_dispatch.c",
            "core-c/src/scheduler/hybrid_buildup_dispatch.c",
            "core-c/src/scheduler/hybrid_buildup_dispatch.c",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_build_result_bridge.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_coverage_bridge.rs",
            "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_product_report.rs"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "GPU BuildUp product path required file is missing: $requiredPath"
        }
    }
$dispatch = Read-Text "core-c/src/scheduler/hybrid_buildup_dispatch.c"
foreach ($requiredMarker in @(
            "clearra_hybrid_buildup_dispatch_candidate",
            "CLEARRA_HYBRID_BUILDUP_VERIFY_FIRST",
            "CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS",
            "CLEARRA_HYBRID_BUILDUP_COUNT_VARIANTS",
            "clr_buildup_verify_first",
            "clr_buildup_enumerate_variants",
            "clr_buildup_count_variants"
        )) {
        if ($dispatch -notlike "*$requiredMarker*") {
            Add-ArchitectureError "hybrid_buildup_dispatch.c must preserve BuildUp dispatch marker '$requiredMarker'"
        }
    }
$collect = Read-Text "core-c/src/scheduler/hybrid_buildup_dispatch.c"
foreach ($requiredMarker in @(
            "clearra_hybrid_collect_build_variants_from_confirmed_queue",
            "can_enter_cpu_buildup_queue",
            "can_create_coverage_row != 0u",
            "candidate_is_solution != 0u",
            "append_buffer",
            "verify_first_used_for_coverage"
        )) {
        if ($collect -notlike "*$requiredMarker*") {
            Add-ArchitectureError "hybrid_build_variant_collect.c must preserve collection marker '$requiredMarker'"
        }
    }
$coverageBridge = Read-Text "core-c/src/scheduler/hybrid_buildup_dispatch.c"
foreach ($requiredMarker in @(
            "clearra_hybrid_coverage_rows_from_build_variants",
            "CLEARRA_HYBRID_BUILDUP_VERIFY_FIRST",
            "rejected_verify_first",
            "CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS",
            "clr_coverage_pattern_verification",
            "CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP",
            "clr_coverage_row_from_verified_build_variant_with_identity"
        )) {
        if ($coverageBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "hybrid_coverage_row_bridge.c must preserve coverage bridge marker '$requiredMarker'"
        }
    }
$rustSurface = @(
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_build_result_bridge.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_coverage_bridge.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_product_report.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "GpuWorkerBuildUpMode",
            "VerifyFirst",
            "EnumerateVariants",
            "CountVariants",
            "can_source_coverage_rows",
            "GpuWorkerCoverageBridge",
            "TypedCoverageMatrix",
            "GpuWorkerProductReport",
            "objective_ready"
        )) {
        if ($rustSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "Rust GPU worker product bridge must preserve marker '$requiredMarker'"
        }
    }
$schedulerTests = Get-SchedulerTestsValidationSurface
foreach ($requiredMarker in @(
            "gpu_assisted_opening_2l_reaches_buildup",
            "gpu_assisted_buildvariant_count_matches_cpu_reference",
            "gpu_assisted_coverage_rows_match_cpu_reference",
            "gpu_verify_first_not_used_for_coverage"
        )) {
        if ($schedulerTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scheduler tests must cover GPU BuildUp product path marker '$requiredMarker'"
        }
    }
$gpuWorkerTests = @(
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_contract_tests.rs"
        Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_product_path_tests.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "gpu_assisted_opening_2l_reaches_buildup",
            "gpu_assisted_buildvariant_count_matches_cpu_reference",
            "gpu_assisted_coverage_rows_match_cpu_reference",
            "gpu_verify_first_not_used_for_coverage"
        )) {
        if ($gpuWorkerTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "Rust GPU worker tests must cover BuildUp product path marker '$requiredMarker'"
        }
    }
$cmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
            "src/scheduler/hybrid_buildup_dispatch.c",
            "src/scheduler/hybrid_buildup_dispatch.c",
            "src/scheduler/hybrid_buildup_dispatch.c"
        )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must compile GPU BuildUp product path source '$requiredMarker'"
        }
    }
}
