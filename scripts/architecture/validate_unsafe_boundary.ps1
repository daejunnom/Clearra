function Test-RustUnsafeBoundaryAllowed([string]$RelativePath) {
    $normalized = $RelativePath.Replace("\", "/").TrimStart(".", "/")
    if ($normalized.StartsWith("crates/clearra-core-ffi/src/raw/")) {
        return $true
    }

    if ($normalized -eq "crates/clearra-wasm-abi/src/lib.rs") {
        return $true
    }

    if ($normalized -in @(
            "crates/clearra-core-executor/src/performance/search_stage_profiler.rs",
            "crates/clearra-core-executor/src/performance/host_clock.rs",
            "crates/clearra-postprocess/src/score_batch/exact_scoring_execution_materializer.rs",
            "crates/clearra-webgpu/src/geometry_exact_cover_timing.rs"
        )) {
        return $true
    }

    foreach ($allowedOwner in @(
            "crates/clearra-core-ffi/src/memory/native_memory_bindings.rs",
            "crates/clearra-core-ffi/src/native/geometry_catalog.rs",
            "crates/clearra-core-ffi/src/native/geometry_solution_graph.rs"
        )) {
        $ownerStem = $allowedOwner.Substring(0, $allowedOwner.Length - 3)
        if ($normalized -eq $allowedOwner -or
            $normalized.StartsWith("${ownerStem}_functions/") -or
            $normalized.StartsWith("${ownerStem}_types/")) {
            return $true
        }
    }
    return $false
}function Test-RustProductionContainsUnsafeBoundary([string]$Contents) {
    $production = Get-RustProductionContents $Contents
    return $production -match '\bunsafe\b' -or
        $production -match 'extern\s+"C"' -or
        $production -match '\bNonNull\s*<' -or
        $production -match '\*(mut|const)\s+'
}function Invoke-UnsafeBoundaryArchitectureValidation() {
$coreFfiManifest = Read-Text "crates/clearra-core-ffi/Cargo.toml"
$coreFfiLib = Read-Text "crates/clearra-core-ffi/src/lib.rs"
$rawMod = Read-Text "crates/clearra-core-ffi/src/raw/mod.rs"
$nativeBindings = Read-Text "crates/clearra-core-ffi/src/memory/native_memory_bindings.rs"
$nativeCoreContext = Read-Text "crates/clearra-core-ffi/src/memory/native_core_context.rs"
foreach ($requiredMarker in @(
            "default = []",
            "native-memory-binding = []",
            'native-c-core = ["native-memory-binding"]'
        )) {
        if (-not $coreFfiManifest.Contains($requiredMarker)) {
            Add-ArchitectureError "clearra-core-ffi native features must pin marker '$requiredMarker'"
        }
    }
if ($coreFfiManifest.Contains('native-memory-binding = ["native-c-core"]')) {
        Add-ArchitectureError "native-memory-binding must not imply native-c-core; native-c-core depends on native-memory-binding"
    }
if ($coreFfiLib -like "*pub mod raw;*") {
        Add-ArchitectureError "clearra-core-ffi raw binding module must not be public API"
    }
if ($rawMod -notlike "*pub(crate) mod bindings;*") {
        Add-ArchitectureError "clearra-core-ffi raw bindings must be crate-private"
    }
foreach ($requiredMarker in @(
            "native_memory_binding_is_feature_gated",
            "native_binding_raw_pointers_are_private",
            "BindingUnavailable",
            "NativeMemContextHandle",
            "NativeScopeHandle"
        )) {
        if ($nativeBindings -notlike "*$requiredMarker*") {
            Add-ArchitectureError "native memory binding isolation must preserve marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            "native_core_context_default_build_returns_binding_unavailable",
            "NativeCoreContext::create().expect_err",
            "BindingUnavailable"
        )) {
        if ($nativeCoreContext -notlike "*$requiredMarker*") {
            Add-ArchitectureError "NativeCoreContext default unavailable contract must preserve marker '$requiredMarker'"
        }
    }
foreach ($file in Get-ProductionRustFiles) {
        $relativePath = Get-NormalizedRelativePath $file
        $contents = Get-Content -LiteralPath $file.FullName -Raw
        if ((Test-RustProductionContainsUnsafeBoundary $contents) -and
            -not (Test-RustUnsafeBoundaryAllowed $relativePath)) {
            Add-ArchitectureError "$relativePath must not contain unsafe/raw pointer boundary code outside clearra-core-ffi raw/native binding allowlist or a separately approved ABI/host-clock boundary"
        }
    }
foreach ($crateDir in @(
            "crates/clearra-cli/src",
            "crates/clearra-app/src",
            "crates/clearra-output/src",
            "crates/clearra-validation/src",
            "crates/clearra-coverage/src",
            "crates/clearra-objectives/src",
            "crates/clearra-replay/src",
            "crates/clearra-scoring/src",
            "crates/clearra-render/src",
            "crates/clearra-ui-schema/src"
        )) {
        foreach ($file in Get-ProductionRustFilesIn $crateDir) {
            $relativePath = Get-NormalizedRelativePath $file
            $contents = Get-Content -LiteralPath $file.FullName -Raw
            if (Test-RustProductionContainsUnsafeBoundary $contents) {
                Add-ArchitectureError "$relativePath must not contain unsafe, extern C, NonNull, or raw pointer boundary code"
            }
        }
    }
foreach ($file in Get-ProductionRustFilesIn "crates/clearra-core-ffi/src") {
        $relativePath = Get-NormalizedRelativePath $file
        $contents = Get-RustProductionContents (Get-Content -LiteralPath $file.FullName -Raw)
        if ($relativePath -notlike "crates/clearra-core-ffi/src/memory/native_memory_bindings.rs" -and
            $contents -match 'pub\s+(?:struct|type|fn|const|static)?\s*[^;\r\n=]*\*(?:mut|const)\s+CClr') {
            Add-ArchitectureError "$relativePath must not expose public C memory raw pointer API"
        }
        if ($contents -match 'pub\s+extern\s+"C"') {
            Add-ArchitectureError "$relativePath must not expose public extern C functions"
        }
    }
}
