# Independent C reference checkpoint isolation; no product GPU executor remains.
function Invoke-GpuHostReducerContractValidation() {
    $manifest = Read-Text "core-c/cmake/source_manifest.cmake"
    $testingBoundary = $manifest.IndexOf("if(BUILD_TESTING)")
    foreach ($checkpointSource in @(
            "src/gpu/gpu_backend.c",
            "src/gpu/gpu_readback_reduce.c",
            "src/gpu/gpu_host_confirm.c",
            "src/packing/cpu_packing_reference.c"
        )) {
        $sourceIndex = $manifest.IndexOf($checkpointSource)
        if ($testingBoundary -lt 0 -or $sourceIndex -lt $testingBoundary) {
            Add-ArchitectureError "legacy GPU materializing checkpoint must be BUILD_TESTING-only: $checkpointSource"
        }
    }

}
