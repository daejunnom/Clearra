function Invoke-SecurityArchitectureValidation() {
$securityFixMapPath = "docs/security-fix-map.md"
if (-not (Test-Path -LiteralPath (Join-Path $Root $securityFixMapPath))) {
        Add-ArchitectureError "S0 security inventory is missing: $securityFixMapPath"
        return
    }
$securityFixMap = Read-Text $securityFixMapPath
$architectureDoc = Read-Text "docs/architecture.md"
$mvpScopeDoc = Read-Text "docs/mvp-scope.md"
$coreSecurityGate = Read-Text "crates/clearra-validation/src/validators/core_security_gate.rs"
$ffiBuildVariantView = Read-Text "crates/clearra-core-ffi/src/buildup/build_variant_view.rs"
$invariantTests = Read-Text "crates/clearra-invariant-tests/tests/workspace_invariant_tests.rs"
$memoryTests = Read-Text "core-c/tests/memory_tests.c"
$gpuWorkerHeader = Read-Text "core-c/include/clr_gpu_worker.h"
$gpuWorkerTests = Read-Text "core-c/tests/gpu_worker_tests.c"
$gpuWorkerRequestView = Read-Text "crates/clearra-core-ffi/src/gpu/gpu_worker_request_view.rs"
$nativeLeakReport = Read-Text "crates/clearra-core-ffi/src/memory/native_leak_report.rs"
$nativeScope = Read-Text "crates/clearra-core-ffi/src/memory/native_scope.rs"
$gpuWorkerResult = Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_result.rs"
$gpuWorkerContractTests = Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_contract_tests.rs"
$gpuWorkerDiagnostic = Read-Text "crates/clearra-validation/src/diagnostic/gpu_worker_diagnostic.rs"
$pcJsonContract = Read-Text "crates/clearra-output/src/json/pc_json_contract.rs"
$backendGpuWorkerContract = Read-Text "crates/clearra-output/src/json/backend_gpu_worker_contract.rs"
$gpuValidationSurface = @(
        "scripts/architecture/validate_gpu_pipeline.ps1",
        "scripts/architecture/validate_gpu_product_equivalence_contract.ps1",
        "scripts/architecture/validate_gpu_stage_f_visibility.ps1",
        "scripts/architecture/validate_backend_policy_contract.ps1",
        "docs/gpu-pipeline.md",
        "docs/architecture.md"
    ) | ForEach-Object { Read-Text $_ }
$gpuValidationSurface = $gpuValidationSurface -join "`n"
$renderValidationSurface = @(
        "scripts/architecture/validate_output_contract.ps1",
        "docs/mvp-scope.md",
        "docs/security-fix-map.md"
    ) | ForEach-Object { Read-Text $_ }
$renderValidationSurface = $renderValidationSurface -join "`n"
$guiValidationSurface = @(
        "scripts/architecture/validate_workspace_surface_ui_gui.ps1",
        "docs/gui.md",
        "docs/gui-host.md",
        "docs/security-fix-map.md"
    ) | ForEach-Object { Read-Text $_ }
$guiValidationSurface = $guiValidationSurface -join "`n"
foreach ($requiredRiskId in @(
            "SEC-C-MEM-001",
            "SEC-C-MEM-002",
            "SEC-C-MEM-003",
            "SEC-FFI-001",
            "SEC-FFI-002",
            "SEC-GPU-001",
            "SEC-GPU-002",
            "SEC-COV-001",
            "SEC-REN-001",
            "SEC-SVG-001",
            "SEC-GUI-001",
            "SEC-WASM-001"
        )) {
        if ($securityFixMap -notlike "*$requiredRiskId*") {
            Add-ArchitectureError "docs/security-fix-map.md must track known S0 risk '$requiredRiskId'"
        }
    }
foreach ($requiredMarker in @(
            "security_fix_map_mentions_all_known_risks",
            "architecture_validation_rejects_silent_gpu_fallback",
            "architecture_validation_rejects_runtime_raw_svg",
            "architecture_validation_rejects_gui_subprocess",
            "architecture_validation_rejects_unbounded_ffi_pointer_count",
            "architecture_validation_rejects_unsafe_outside_core_ffi_raw",
            "capacity_exceeded_must_not_truncate_without_diagnostic",
            "mvp_out_of_scope_features_must_not_appear_supported",
            "scope_gpu_buffer_release_queue_lifetime_contract",
            "gpu_worker_request_result_require_scope_epoch_and_byte_budget",
            "memory_leak_report_snapshot_exposes_pending_gpu_buffer_releases",
            "batch_scope_abort_releases_allocations",
            "search_scope_release_releases_child_batch_scopes",
            "memory_context_release_uses_pointer_to_pointer_api",
            "memory_context_double_release_does_not_deref_freed_memory"
        )) {
        $surface = "$securityFixMap`n$architectureDoc`n$mvpScopeDoc"
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "S0 security docs must pin marker '$requiredMarker'"
        }
    }
foreach ($requiredColumn in @(
            "Risk ID",
            "위험 설명",
            "현재 파일",
            "영향 범위",
            "수정 단계",
            "수정 후 테스트",
            "관련 diagnostic code",
            "release 차단 여부"
        )) {
        if ($securityFixMap -notlike "*$requiredColumn*") {
            Add-ArchitectureError "docs/security-fix-map.md must include table column '$requiredColumn'"
        }
    }
foreach ($requiredMarker in @(
            "security_fix_map_mentions_all_known_risks",
            "SEC-C-MEM-001",
            "SEC-C-MEM-002",
            "SEC-C-MEM-003",
            "SEC-FFI-001",
            "SEC-FFI-002",
            "SEC-GPU-001",
            "SEC-GPU-002",
            "SEC-COV-001",
            "SEC-REN-001",
            "SEC-SVG-001",
            "SEC-GUI-001",
            "SEC-WASM-001"
        )) {
        if ($invariantTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-invariant-tests must pin S0 security inventory marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT",
            "KickEvidenceCountExceeded",
            "MissingKickEvidencePointer",
            "ffi_build_variant_rejects_kick_evidence_count_above_c_limit",
            "ffi_build_variant_rejects_missing_kick_evidence_pointer",
            "ffi_build_variant_does_not_read_pointer_when_count_exceeds_limit",
            "ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape"
        )) {
        if ($ffiBuildVariantView -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CBuildVariantView must reject unbounded FFI pointer/count marker '$requiredMarker'"
        }
    }
$ffiDiagnosticSurface = @(
        "crates/clearra-validation/src/diagnostic/diagnostic_code.rs",
        "crates/clearra-validation/src/diagnostic/diagnostic_code_string.rs",
        "crates/clearra-validation/src/validators/core_security_gate.rs",
        "crates/clearra-validation/src/validators/core_security_gate_tests.rs",
        "docs/security-fix-map.md"
    ) | ForEach-Object { Read-Text $_ }
$ffiDiagnosticSurface = $ffiDiagnosticSurface -join "`n"
foreach ($requiredMarker in @(
            "ECoreFfiBufferBounds",
            "E_CORE_FFI_BUFFER_BOUNDS",
            "ECoreInvalidNativeView",
            "E_CORE_INVALID_NATIVE_VIEW",
            "E_KICK_EVIDENCE_BUFFER_EXHAUSTED",
            "hybrid_collect_rejects_kick_evidence_count_over_limit"
        )) {
        if ($ffiDiagnosticSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "S3 FFI pointer/count diagnostics must preserve marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "gpu_buffer_release_before_fence_is_deferred",
            "gpu_buffer_release_before_fence_deferred",
            "release_queue_drain_after_epoch_releases_gpu_buffer",
            "batch_scope_abort_releases_allocations",
            "search_scope_release_releases_child_batch_scopes",
            "memory_leak_report_counts_pending_gpu_buffers"
        )) {
        if ($memoryTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "S2 memory lifetime tests must include marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "request_id",
            "memory_ticket_id",
            "fence_epoch",
            "cpu_confirm_required",
            "scope_epoch",
            "byte_budget"
        )) {
        $surface = "$gpuWorkerHeader`n$gpuWorkerRequestView`n$gpuWorkerResult`n$gpuWorkerTests`n$gpuWorkerContractTests"
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GPU worker request/result lifetime contract must preserve '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "gpu_worker_request_requires_memory_ticket",
            "gpu_worker_result_requires_memory_ticket",
            "gpu_worker_unconfirmed_result_cannot_source_exact_probability",
            "native_batch_scope_drop_releases_c_scope",
            "owned_snapshot_survives_scope_release",
            "borrowed_view_cannot_escape_scope"
        )) {
        $surface = "$gpuWorkerContractTests`n$nativeScope"
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "S2 Rust lifetime contract tests must include marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "EGpuWorkerMemoryTicketMissing",
            "EGpuBufferFenceMissing",
            "WGpuBufferReleaseDeferred",
            "WPendingReleaseQueueNotDrained",
            "WMemoryPressureHigh",
            "pending_gpu_buffer_releases",
            "double_releases",
            "canary_failures",
            "poison_detections"
        )) {
        $surface = "$gpuWorkerDiagnostic`n$nativeLeakReport`n$pcJsonContract`n$backendGpuWorkerContract"
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "S2 diagnostics/output must preserve marker '$requiredMarker'"
        }
    }
if ($gpuValidationSurface -notlike "*--no-backend-fallback prevents silent CPU fallback*" -and
        $gpuValidationSurface -notlike "*architecture_validation_rejects_silent_gpu_fallback*") {
        Add-ArchitectureError "architecture_validation_rejects_silent_gpu_fallback must be enforced by GPU/backend policy validation"
    }
foreach ($requiredMarker in @(
            "runtime_raw_svg_allowed`": false",
            "Runtime raw SVG rendering is forbidden",
            "architecture_validation_rejects_runtime_raw_svg"
        )) {
        if ($renderValidationSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "architecture_validation_rejects_runtime_raw_svg must pin marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "subprocess_execution=forbidden",
            "GUI direct C core calls are forbidden",
            "architecture_validation_rejects_gui_subprocess"
        )) {
        if ($guiValidationSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "architecture_validation_rejects_gui_subprocess must pin marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "ECoverageCapacityExceeded",
            "EBuildUpVariantEnumerationTruncated",
            "EScoreMatrixCapacityExceeded",
            "ESpinCoverageCapacityExceeded",
            "WObservedQueueProbabilityIncomplete",
            "WTraceRetentionTruncated",
            "E_COVERAGE_CAPACITY_EXCEEDED",
            "E_BUILDUP_VARIANT_ENUMERATION_TRUNCATED",
            "W_OBSERVED_QUEUE_PROBABILITY_INCOMPLETE",
            "W_TRACE_RETENTION_TRUNCATED",
            "build_up_count_reports_truncation",
            "enumerate_variants_sets_count_complete_false_when_truncated",
            "output_distinguishes_total_solution_count_and_retained_trace_count",
            "observed_queue_truncation_is_not_renormalized",
            "coverage_capacity_exceeded_is_error_not_success",
            "capacity_exceeded_must_not_truncate_without_diagnostic"
        )) {
        $capacitySurface = "$coreSecurityGate`n$securityFixMap`n$mvpScopeDoc"
        if ($capacitySurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "capacity exceeded must report diagnostic marker '$requiredMarker'"
        }
    }
    $templateImport = Read-Text "crates/clearra-build-coverage/src/template/template_import.rs"
$templateJsonReader = Read-Text "crates/clearra-build-coverage/src/template/template_json_reader.rs"
$templateImportProduction = Get-RustProductionContents $templateImport
$templateJsonReaderProduction = Get-RustProductionContents $templateJsonReader
if ($templateImportProduction -match 'pub\s+fn\s+from_json\s*\([^)]*\)\s*->\s*Result<\s*Self\s*,' -and
        $templateJsonReaderProduction -notlike "*CellCoord::new(x, y, board_size)*") {
        Add-ArchitectureError "TemplateImport::from_json returns BuildTemplate through TemplateJsonReader, so JSON cells must be bounds-checked with CellCoord::new before template construction"
    }
}
