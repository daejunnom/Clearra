function Invoke-SecurityRegressionTestsContractValidation() {
$requiredFiles = @(
        "core-c/tests/memory_tests.c",
        "crates/clearra-core-ffi/src/buildup/build_variant_view.rs",
        "crates/clearra-core-ffi/src/buildup/build_variant_view_tests.rs",
        "crates/clearra-core-ffi/src/raw/native_slice.rs",
        "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_contract_tests.rs",
        "crates/clearra-render/src/lib_tests.rs",
        "crates/clearra-render/src/skin/skin_manifest_validator.rs",
        "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs",
        "crates/clearra-gui-host/tests/desktop_cli_boundary.rs",
        "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts",
        "packages/clearra-ui/src/lib/stores/desktopJobStore.ts",
        "crates/clearra-wasm/src/lib_tests.rs",
        "crates/clearra-webgpu/src/shader_contract.rs",
        "crates/clearra-webgpu/src/shader_contract_tests.rs",
        "scripts/architecture/validate_host_runtime_contract.ps1",
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
        if ($memoryTests.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "core-c memory tests must keep T5 security regression marker '$requiredMarker'"
        }
    }
$gpuBufferLifetime = Read-Text "core-c/src/memory/clr_gpu_buffer_lifetime.c"
foreach ($requiredMarker in @(
        "fence_epoch_set",
        "fence_epoch == 0",
        "return CLR_MEM_INVALID_STATE"
    )) {
        if ($gpuBufferLifetime.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "GPU buffer lifetime must reject release without explicit fence marker '$requiredMarker'"
        }
    }
$ffiBuildVariantView = Read-PhysicalText "crates/clearra-core-ffi/src/buildup/build_variant_view.rs"
$ffiBuildVariantTests = Read-PhysicalText "crates/clearra-core-ffi/src/buildup/build_variant_view_tests.rs"
$ffiNativeSlice = Read-PhysicalText "crates/clearra-core-ffi/src/raw/native_slice.rs"
foreach ($requiredMarker in @(
        "ffi_kick_evidence_count_exceeded_rejected_before_pointer_read",
        "ffi_build_variant_does_not_read_pointer_when_count_exceeds_limit"
    )) {
        if ($ffiBuildVariantTests.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Rust FFI build variant tests must keep T5 marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
        "C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT",
        "KickEvidenceCountExceeded",
        "copy_native_slice"
    )) {
        if ($ffiBuildVariantView.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Rust FFI build variant view must keep T5 marker '$requiredMarker'"
        }
    }
$ffiKickCountCheckIndex = $ffiBuildVariantView.IndexOf(
    "if kick_evidence_count > C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT",
    [System.StringComparison]::Ordinal
)
$ffiKickPointerCopyIndex = $ffiBuildVariantView.IndexOf(
    "copy_native_slice(native.kick_evidence, kick_evidence_count)",
    [System.StringComparison]::Ordinal
)
if ($ffiKickCountCheckIndex -lt 0 -or
    $ffiKickPointerCopyIndex -lt 0 -or
    $ffiKickCountCheckIndex -gt $ffiKickPointerCopyIndex) {
        Add-ArchitectureError "ffi_kick_evidence_count_exceeded_rejected_before_pointer_read must bound count before copy_native_slice"
    }
foreach ($requiredMarker in @(
        "pub(crate) fn copy_native_slice<T: Copy>",
        "NonNull::new",
        "core::slice::from_raw_parts",
        ".to_vec()"
    )) {
        if ($ffiNativeSlice.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Rust FFI native slice boundary must keep bounded copied-slice marker '$requiredMarker'"
        }
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
        if ($gpuWorkerTests.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "GPU worker tests must keep T5 security regression marker '$requiredMarker'"
        }
    }
$renderTests = Read-PhysicalText "crates/clearra-render/src/lib_tests.rs"
foreach ($requiredMarker in @(
        "runtime_raw_svg_rejected",
        "invalid_runtime_raw_svg_allowed",
        'manifest["runtime_raw_svg_allowed"] = serde_json::json!(true)',
        'manifest.contains("\"runtime_raw_svg_allowed\": false")'
    )) {
        if ($renderTests.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "render tests must keep T5 raw SVG regression marker '$requiredMarker'"
        }
    }
$renderManifestValidator = Read-PhysicalText "crates/clearra-render/src/skin/skin_manifest_validator.rs"
foreach ($requiredMarker in @('require_bool(manifest, "runtime_raw_svg_allowed", false)', "fn require_bool", 'Some(actual) if actual == expected => Ok(())', 'Some(_) => Err(format!("invalid_{field}"))')) {
    if ($renderManifestValidator.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
        Add-ArchitectureError "render manifest validator must reject runtime raw SVG marker '$requiredMarker'"
    }
}
$desktopCliBoundaryTests = Read-PhysicalText "crates/clearra-gui-host/tests/desktop_cli_boundary.rs"
foreach ($requiredMarker in @(
        "production_entrypoint_accepts_only_the_complete_cli_envelope",
        "production_entrypoint_preserves_literal_exact_argv_without_shell_interpretation",
        "production_entrypoint_rejects_nul_in_exact_argv",
        "clearra-cli/CommandRequest",
        "clearra-app/AppRequest",
        "non-CLI desktop envelope must fail closed"
    )) {
        if ($desktopCliBoundaryTests.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Desktop CLI production boundary tests must keep T5 marker '$requiredMarker'"
        }
    }
$desktopBridge = Read-PhysicalText "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
$desktopClient = Read-PhysicalText "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts"
$desktopJobStore = Read-PhysicalText "packages/clearra-ui/src/lib/stores/desktopJobStore.ts"
foreach ($requiredMarker in @("CliCommandParser::parse_tokens", "response.to_host_response()", "serde_json::to_string")) {
    if ($desktopBridge.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
        Add-ArchitectureError "Desktop CLI production bridge must keep typed request/response marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("export type ClearraDesktopAppResponse", "runtime_identity: ClearraProductBuildIdentity", "capability_report:", "app_request_boundary: string", "JSON.parse(response) as ClearraDesktopAppResponse")) {
    if ($desktopClient.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
        Add-ArchitectureError "Desktop client must keep typed AppResponse marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("requireCompleteDesktopCliRequest", "clearra-cli/CommandRequest", "unexpectedField")) {
    if ($desktopJobStore.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
        Add-ArchitectureError "Desktop job store must keep complete CLI-only envelope marker '$requiredMarker'"
    }
}
$hostRuntimeValidator = Read-PhysicalText "scripts/architecture/validate_host_runtime_contract.ps1"
foreach ($requiredMarker in @(
        '$clearraExecutablePattern',
        "clearra.execution-resource-authority.v1",
        'Command::new("clearra.exe")',
        "R host runtime contract forbids executable token 'clearra.exe'",
        "std::process::Command",
        "run_with_args",
        "clearra_packing_",
        "clr_buildup_",
        "clearra_board64_"
    )) {
        if ($hostRuntimeValidator.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Desktop host static security validation must keep T5 marker '$requiredMarker'"
        }
    }
$wasmTests = Read-PhysicalText "crates/clearra-wasm/src/lib_tests.rs"
foreach ($requiredMarker in @(
        "wasm_user_shader_rejected",
        "E_WASM_COMMAND_UNSUPPORTED",
        "user_shader_allowed",
        "runtime_shader_injection_allowed"
    )) {
        if ($wasmTests.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "WASM command tests must keep T5 user shader rejection marker '$requiredMarker'"
        }
    }
$webGpuShaderSurface = @(
    Read-PhysicalText "crates/clearra-webgpu/src/shader_contract.rs"
    Read-PhysicalText "crates/clearra-webgpu/src/shader_contract_tests.rs"
) -join "`n"
foreach ($requiredMarker in @(
        "webgpu_user_shader_rejected",
        "reject_user_provided_wgsl",
        "E_WEBGPU_USER_PROVIDED_WGSL_REJECTED",
        "embedded_reviewed",
        "no_runtime_shader_injection"
    )) {
        if ($webGpuShaderSurface.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "WebGPU shader contract must keep T5 embedded-only marker '$requiredMarker'"
        }
    }
$taskList = Read-Text "scripts/lib/architecture-validation-tasks.ps1"
foreach ($requiredMarker in @(
        "T5 Security Regression Tests",
        "Invoke-SecurityRegressionTestsContractValidation"
    )) {
        if ($taskList.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
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
        if ($clearraScript.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "scripts/clearra.ps1 must keep gate marker '$requiredMarker' for T5"
        }
    }
foreach ($requiredMarker in @(
        "Invoke-ArchitectureValidation",
        '$script:VerifyArchitectureStatus = "failed"',
        'if ($result.Status -eq "Failed")',
        '$script:VerifyArchitectureStatus = "passed"'
    )) {
        if ($verifyScript.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
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
        if ($testPolicyDoc.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "docs/test-policy.md must document T5 marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
        "T5 Security Regression Tests",
        "security_regression_tests_are_part_of_Local_or_Strict_gate",
        "release_acceptance_cannot_pass_when_security_regressions_fail"
    )) {
        if ($architectureDoc.IndexOf($requiredMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "docs/architecture.md must document T5 marker '$requiredMarker'"
        }
    }
}
