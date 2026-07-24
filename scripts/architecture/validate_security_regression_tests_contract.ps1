function Invoke-SecurityRegressionTestsContractValidation() {
$requiredFiles = @(
        "core-c/tests/memory_tests.c",
        "crates/clearra-core-ffi/src/buildup/build_variant_view.rs",
        "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_contract_tests.rs",
        "crates/clearra-render/src/lib.rs",
        "crates/clearra-gui-host/src/gui_host_contract_tests.rs",
        "crates/clearra-wasm/src/lib.rs",
        "scripts/clearra.ps1",
        "scripts/verify.ps1",
        "scripts/lib/architecture-validation-tasks.ps1",
        "docs/test-policy.md",
        "docs/architecture.md"
    )
foreach ($relativePath in $requiredFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $relativePath))) {
            Add-ArchitectureError "T5 Security Regression required file is missing: $relativePath"
        }
    }
$memoryTests = Read-Text "core-c/tests/memory_tests.c"
foreach ($requiredMarker in @(
        "memory_context_double_release_does_not_deref_freed_memory",
        "gpu_buffer_release_without_fence_rejected",
        "CLR_MEM_INVALID_STATE",
        "clr_gpu_buffer_set_fence_epoch"
    )) {
        if ($memoryTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c memory tests must keep T5 security regression marker '$requiredMarker'"
        }
    }
$gpuBufferLifetime = Read-Text "core-c/src/memory/clr_gpu_buffer_lifetime.c"
foreach ($requiredMarker in @(
        "fence_epoch_set",
        "fence_epoch == 0",
        "return CLR_MEM_INVALID_STATE"
    )) {
        if ($gpuBufferLifetime -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GPU buffer lifetime must reject release without explicit fence marker '$requiredMarker'"
        }
    }
$ffiBuildVariantView = Read-Text "crates/clearra-core-ffi/src/buildup/build_variant_view.rs"
foreach ($requiredMarker in @(
        "ffi_kick_evidence_count_exceeded_rejected_before_pointer_read",
        "ffi_build_variant_does_not_read_pointer_when_count_exceeds_limit",
        "C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT",
        "KickEvidenceCountExceeded",
        "from_raw_parts"
    )) {
        if ($ffiBuildVariantView -notlike "*$requiredMarker*") {
            Add-ArchitectureError "Rust FFI build variant view must keep T5 marker '$requiredMarker'"
        }
    }
if ($ffiBuildVariantView.IndexOf("if kick_evidence_count > C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT") -gt
        $ffiBuildVariantView.IndexOf("from_raw_parts")) {
        Add-ArchitectureError "ffi_kick_evidence_count_exceeded_rejected_before_pointer_read must check count before from_raw_parts"
    }
$gpuWorkerTests = Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_contract_tests.rs"
foreach ($requiredMarker in @(
        "gpu_worker_missing_memory_ticket_rejected",
        "gpu_worker_request_requires_memory_ticket",
        "gpu_worker_result_requires_memory_ticket",
        "gpu_unconfirmed_probability_rejected",
        "gpu_worker_unconfirmed_result_cannot_source_exact_probability",
        "GpuWorkerResultReducer::reduce",
        "GpuWorkerReduction::PrefilterOnly"
    )) {
        if ($gpuWorkerTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GPU worker tests must keep T5 security regression marker '$requiredMarker'"
        }
    }
$renderTests = Read-Text "crates/clearra-render/src/lib.rs"
foreach ($requiredMarker in @(
        "runtime_raw_svg_rejected",
        "invalid_runtime_raw_svg_allowed",
        '"runtime_raw_svg_allowed": true'
    )) {
        if ($renderTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "render tests must keep T5 raw SVG regression marker '$requiredMarker'"
        }
    }
$guiHostTests = Read-Text "crates/clearra-gui-host/src/gui_host_contract_tests.rs"
foreach ($requiredMarker in @(
        "gui_subprocess_forbidden",
        "gui_does_not_spawn_clearra_exe",
        'subprocess_execution(), "forbidden"',
        "gui_does_not_parse_cli_text",
        "gui_does_not_call_core_c_directly"
    )) {
        if ($guiHostTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GUI host tests must keep T5 subprocess boundary marker '$requiredMarker'"
        }
    }
$wasmTests = Read-Text "crates/clearra-wasm/src/lib.rs"
foreach ($requiredMarker in @(
        "wasm_user_shader_rejected",
        "reject_user_provided_wgsl",
        "E_WEBGPU_USER_PROVIDED_WGSL_REJECTED",
        "pre_reviewed_embedded_shader_only",
        "no_runtime_shader_injection"
    )) {
        if ($wasmTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "WASM/WebGPU tests must keep T5 user shader regression marker '$requiredMarker'"
        }
    }
$taskList = Read-Text "scripts/lib/architecture-validation-tasks.ps1"
foreach ($requiredMarker in @(
        "T5 Security Regression Tests",
        "Invoke-SecurityRegressionTestsContractValidation"
    )) {
        if ($taskList -notlike "*$requiredMarker*") {
            Add-ArchitectureError "architecture validation task list must include T5 marker '$requiredMarker'"
        }
    }
$clearraScript = Read-Text "scripts/clearra.ps1"
$verifyScript = Read-Text "scripts/verify.ps1"
foreach ($requiredMarker in @(
        '"Local"',
        '"Strict"',
        '"ReleaseAcceptance"',
        '"Validate"'
    )) {
        if ($clearraScript -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/clearra.ps1 must keep gate marker '$requiredMarker' for T5"
        }
    }
foreach ($requiredMarker in @(
        "Invoke-ArchitectureValidation",
        "VerifyArchitectureValidationStatus"
    )) {
        if ($verifyScript -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/verify.ps1 must keep architecture validation marker '$requiredMarker' for T5 Local gate"
        }
    }
$testPolicyDoc = Read-Text "docs/test-policy.md"
foreach ($requiredMarker in @(
        "T5 security regression tests",
        "memory_context_double_release_does_not_deref_freed_memory",
        "ffi_kick_evidence_count_exceeded_rejected_before_pointer_read",
        "gpu_worker_missing_memory_ticket_rejected",
        "gpu_buffer_release_without_fence_rejected",
        "wasm_user_shader_rejected",
        "release acceptance cannot pass"
    )) {
        if ($testPolicyDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/test-policy.md must document T5 marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
        "T5 Security Regression Tests",
        "security_regression_tests_are_part_of_Local_or_Strict_gate",
        "release_acceptance_cannot_pass_when_security_regressions_fail"
    )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document T5 marker '$requiredMarker'"
        }
    }
}
