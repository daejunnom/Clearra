# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Static validation owns the reviewed shader and public outcome shape. The
# executable WebGPU batch is run by scripts/webgpu-backend-check.ps1.

function Invoke-WebGpuBackendContractValidation() {
    foreach ($requiredFile in @(
        'crates/clearra-webgpu/Cargo.toml',
        'crates/clearra-webgpu/src/embedded_pattern_bitset_union.wgsl',
        'crates/clearra-webgpu/src/shader_contract.rs',
        'crates/clearra-webgpu/src/adapter_selection.rs',
        'crates/clearra-webgpu/src/webgpu_backend.rs',
        'crates/clearra-postprocess-gpu/src/postprocess_gpu_backend.rs',
        'crates/clearra-wasm/src/webgpu/webgpu_backend_report.rs',
        'packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts',
        'scripts/webgpu-backend-check.ps1'
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "U8 required WebGPU backend file missing: $requiredFile"
        }
    }

    foreach ($removedFile in @(
        'crates/clearra-wasm/src/webgpu/embedded_board64_packing.wgsl',
        'crates/clearra-wasm/src/webgpu/webgpu_shader_policy.rs',
        'crates/clearra-wasm/src/webgpu/webgpu_trust_gate.rs',
        'crates/clearra-webgpu/src/runtime_capability.rs',
        'core-c/kernels/cuda/packing_kernel.cu',
        'core-c/kernels/opencl/packing_kernel.cl',
        'core-c/kernels/portable/packing_kernel_ref.c'
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $removedFile)) {
            Add-ArchitectureError "Disconnected or placeholder GPU surface must be removed: $removedFile"
        }
    }

    $backend = Read-Text 'crates/clearra-webgpu/src/webgpu_backend.rs'
    foreach ($required in @(
        'pub enum WebGpuBatchOutcome', 'Connected(', 'Unavailable(',
        'RejectedMismatch(', 'create_compute_pipeline',
        'dispatch_workgroups', 'copy_buffer_to_buffer', 'expected_union',
        'DeterministicReferenceMatched', 'cpu_confirmed: true'
    )) {
        if (-not $backend.Contains($required)) {
            Add-ArchitectureError "WebGPU runtime backend is missing '$required'"
        }
    }

    $adapterSelection = Read-Text 'crates/clearra-webgpu/src/adapter_selection.rs'
    if (-not $adapterSelection.Contains('request_adapter')) {
        Add-ArchitectureError "WebGPU adapter selection is missing 'request_adapter'"
    }

    $shader = Read-Text 'crates/clearra-webgpu/src/embedded_pattern_bitset_union.wgsl'
    foreach ($required in @(
        '@group(0) @binding(0)', '@group(0) @binding(1)',
        '@compute @workgroup_size(64)', 'value |=', 'union_words[word_index] = value'
    )) {
        if (-not $shader.Contains($required)) {
            Add-ArchitectureError "Reviewed WebGPU shader is missing executable operation '$required'"
        }
    }

    $surface = @(
        $backend
        Read-Text 'crates/clearra-webgpu/src/shader_contract.rs'
        Read-Text 'crates/clearra-postprocess-gpu/src/post_gpu_result.rs'
        Read-Text 'crates/clearra-wasm/src/webgpu/webgpu_backend_report.rs'
    ) -join "`n"
    foreach ($forbidden in @(
        'PostGpuCapabilityState::Preview', 'PortableReference',
        'user_provided_wgsl_allowed: true', 'runtime_shader_injection_allowed: true',
        'can_source_exact_probability: true', 'placeholder shader', 'scaffold backend'
    )) {
        if ($surface.Contains($forbidden)) {
            Add-ArchitectureError "WebGPU product surface contains forbidden marker '$forbidden'"
        }
    }
}
