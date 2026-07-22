# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Generic/custom GPU runtime is Unsupported in the default product. Its future
# implementation belongs in a default-off package, not the standard ABI.

function Invoke-GenericGpuDescriptorContractValidation() {
    foreach ($removedFile in @(
        'core-c/src/gpu/generic_gpu_descriptor.c',
        'crates/clearra-core-ffi/src/gpu/generic_gpu_descriptor_view.rs',
        'crates/clearra-core-ffi/src/gpu/gpu_buildup_subset_view.rs'
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $removedFile)) {
            Add-ArchitectureError "Disconnected generic GPU runtime must not ship by default: $removedFile"
        }
    }

    $defaultSurface = @(
        Read-Text 'core-c/include/clr_gpu.h'
        Read-Text 'crates/clearra-core-ffi/src/gpu/mod.rs'
        Read-Text 'crates/clearra-webgpu/src/lib.rs'
    ) -join "`n"
    foreach ($forbidden in @(
        'GenericGpuPackingDescriptor', 'GenericOperationTableGpuView',
        'GenericBoardMaskGpuView', 'GenericPieceAreaMultisetGpuView',
        'ClearraGpuBuildUpSubsetRequest', 'ClearraGpuBuildUpSubsetReport'
    )) {
        if ($defaultSurface -like "*$forbidden*") {
            Add-ArchitectureError "Default GPU ABI exposes disconnected generic surface '$forbidden'"
        }
    }

    $capabilities = Read-Text 'crates/clearra-validation/src/capability/mvp3_capability_registry.rs'
    foreach ($required in @('GenericGpuDescriptor', 'generic_gpu_descriptor_not_connected')) {
        if ($capabilities -notlike "*$required*") {
            Add-ArchitectureError "MVP3 capability report must disclose '$required'"
        }
    }
}
