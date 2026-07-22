function Invoke-ArchitectureResetValidation() {
foreach ($requiredPath in @(
        "CMakeLists.txt",
        "core-c/CMakeLists.txt",
        "core-c/include/clearra_core.h",
        "core-c/src/clearra_core.c",
        "core-c/tests/version_tests.c",
        "scripts/build-core-c.ps1",
        "scripts/build-core-c.sh",
        "scripts/run-c-core-tests.ps1",
        "docs/build-system.md",
        "crates/clearra-core-ffi/src/version.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M0 architecture reset required file is missing: $requiredPath"
        }
    }
if (Test-Path -LiteralPath (Join-Path $Root "build.rs")) {
        Add-ArchitectureError "virtual workspace root must not own build.rs; C core build wiring is owned by CMake plus scripts/lib/core-c-tests.ps1"
    }
$rootCargoToml = Read-Text "Cargo.toml"
if ($rootCargoToml -match '(?m)^\s*build\s*=') {
        Add-ArchitectureError "workspace root Cargo.toml must not declare a build script"
    }
foreach ($cargoToml in Get-ChildItem -Path (Join-Path $Root "crates") -Recurse -File -Filter Cargo.toml) {
        $contents = Get-Content -LiteralPath $cargoToml.FullName -Raw
        if ($contents -match '(?m)^\s*build\s*=\s*"build\.rs"\s*$') {
            Add-ArchitectureError "$($cargoToml.FullName) must not declare build = `"build.rs`""
        }
    }
$buildSystemDoc = Read-Text "docs/build-system.md"
foreach ($requiredMarker in @(
        'virtual workspace',
        'does not own a Cargo build.rs',
        'C core is built by CMake',
        'RUSTFLAGS',
        '-L native',
        'Cargo build scripts are forbidden',
        'The root Cargo.toml is a virtual workspace',
        'repository root does not own',
        'Cargo build.rs',
        'The C core is built by CMake',
        'scripts/clearra.ps1',
        'scripts/lib/core-c-tests.ps1',
        '#[link(name = "clearra_core", kind = "static")]',
        'RUSTFLAGS="-L native=<clearra_core_lib_dir>"',
        'root build.rs',
        'crate-local build.rs',
        'Cargo.toml build = "build.rs"',
        'automatic CMake invocation from a Cargo build script',
        'ManagedLocal',
        'BUILD_TESTING=OFF',
        'does not classify',
        'policy failure',
        'canonical `CARGO_TARGET_DIR`',
        'Win32_DeviceGuard.UsermodeCodeIntegrityPolicyEnforcementStatus',
        'E_WINDOWS_LOCAL_SOURCE_BUILD_BLOCKED',
        'Default product, test, desktop, and artifact commands never invoke `wsl.exe`',
        'separate browser product',
        'not a Rust crate metadata suffix',
        '<CARGO_TARGET_DIR>/.clearra-state/native-core-link.txt',
        'UTF-8',
        'invalidates only the',
        'debug and release artifacts for `clearra-core-ffi`',
        'unchanged native builds',
        'does not invoke WSL',
        'external artifact root is an incremental cache',
        'source or script change',
        'preserves the CMake and Cargo trees',
        'size budget is exceeded',
        'workspace/schema identity'
    )) {
        if ($buildSystemDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/build-system.md must document no-build.rs build policy marker '$requiredMarker'"
        }
    }
$nativeLinkHelpers = Read-Text "scripts/lib/clearra-native-helpers.ps1"
foreach ($requiredMarker in @(
        'function Sync-ClearraNativeCargoLinkState',
        "'.clearra-state'",
        "'native-core-link.txt'",
        "@('clean', '-p', 'clearra-core-ffi')",
        "@('clean', '-p', 'clearra-core-ffi', '--release')",
        'Set-Content -LiteralPath $statePath -Value $fingerprint -Encoding UTF8 -NoNewline'
    )) {
    if ($nativeLinkHelpers -notlike "*$requiredMarker*") {
        Add-ArchitectureError "native Cargo link cache must own selective invalidation marker '$requiredMarker'"
    }
}
if ($nativeLinkHelpers -match '(?i)-C\s+metadata\s*=\s*clearra_core') {
    Add-ArchitectureError "native C library identity must not enter global Rust -C metadata"
}
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
        "## Build Script Policy",
        "The handoff lists build.rs as an optional top-level build integration point",
        "The current repository policy intentionally does not use it",
        "C core build integration belongs to",
        "CMakeLists.txt",
        "core-c/CMakeLists.txt",
        "scripts/lib/core-c-tests.ps1",
        "scripts/clearra.ps1",
        "Cargo build scripts are forbidden in the standard workspace",
        "ManagedLocal builds the C library",
        "actual Windows error 4551",
        "Default product gates never invoke WSL"
    )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document 14.3 build script policy override marker '$requiredMarker'"
        }
    }
$workspaceMembers = Get-WorkspaceMembers
foreach ($requiredMember in @(
        "crates/clearra-problem",
        "crates/clearra-core-ffi",
        "crates/clearra-core-executor",
        "crates/clearra-replay"
    )) {
        if (-not $workspaceMembers.Contains($requiredMember)) {
            Add-ArchitectureError "M0 workspace must include new Rust shell crate member '$requiredMember'"
        }
    }
$rootCmake = Read-Text "CMakeLists.txt"
foreach ($requiredMarker in @("project(clearra LANGUAGES C)", "include(CTest)", "add_subdirectory(core-c)")) {
        if ($rootCmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "root CMakeLists.txt must expose M0 core-c root marker '$requiredMarker'"
        }
    }
if ($rootCmake -match '(?m)^\s*enable_testing\s*\(\s*\)') {
    Add-ArchitectureError "root CMakeLists.txt must let include(CTest) gate test registration through BUILD_TESTING"
}
$coreCmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
        "project(clearra_core_c LANGUAGES C)",
        "include(CTest)",
        "if(BUILD_TESTING)",
        "include(cmake/test_targets.cmake)"
    )) {
    if ($coreCmake -notlike "*$requiredMarker*") {
        Add-ArchitectureError "core-c/CMakeLists.txt must own BUILD_TESTING boundary marker '$requiredMarker'"
    }
}
$coreBuildSurface = @(
    $coreCmake
    Read-Text "core-c/cmake/library_target.cmake"
    Read-Text "core-c/cmake/test_targets.cmake"
) -join "`n"
foreach ($requiredMarker in @(
        "add_library(clearra_core STATIC",
        "CLEARRA_CORE_SPLIT_TESTS",
        "clearra_core_all_tests",
        'target_compile_definitions(${test_name}_object PRIVATE main=${test_main})',
        "clearra_core_version_tests",
        "board64_tests",
        "candidate_tests",
        "reachability_tests",
        "packing_tests",
        "buildup_tests"
    )) {
        if ($coreBuildSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c CMake modules must own M0 C build/test marker '$requiredMarker'"
        }
    }
$coreHeader = Read-Text "core-c/include/clearra_core.h"
foreach ($requiredMarker in @("CLEARRA_CORE_ABI_VERSION", "clearra_core_version", "clearra_core_abi_version")) {
        if ($coreHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c public header must expose M0 ABI marker '$requiredMarker'"
        }
    }
$coreSource = Read-Text "core-c/src/clearra_core.c"
foreach ($requiredMarker in @("clearra_core_version", "clearra_core_abi_version", "CLEARRA_CORE_ABI_VERSION")) {
        if ($coreSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c version source must implement M0 ABI marker '$requiredMarker'"
        }
    }
$versionTest = Read-Text "core-c/tests/version_tests.c"
foreach ($requiredMarker in @("clearra_core_version", "clearra_core_abi_version", "CLEARRA_CORE_ABI_VERSION")) {
        if ($versionTest -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c version test must verify M0 ABI marker '$requiredMarker'"
        }
    }
$buildCorePs = Read-Text "scripts/build-core-c.ps1"
foreach ($requiredMarker in @('$SourceDir = $Root', "cmake", "-S", "-B", "--build", "AllowMissingCompiler")) {
        if ($buildCorePs -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/build-core-c.ps1 must own M0 CMake build marker '$requiredMarker'"
        }
    }
$buildCoreSh = Read-Text "scripts/build-core-c.sh"
foreach ($requiredMarker in @('cmake -S "$ROOT_DIR"', "cmake --build", "Clearra/build/core-c-library-cache", "BUILD_TESTING=OFF")) {
        if ($buildCoreSh -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/build-core-c.sh must own M0 CMake build marker '$requiredMarker'"
        }
    }
$runCoreTests = Read-Text "scripts/run-c-core-tests.ps1"
foreach ($requiredMarker in @("build-core-c.ps1", "ctest --test-dir", "Total Tests:", "registered zero tests", "CMake tests degraded", "AllowMissingCompiler")) {
        if ($runCoreTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/run-c-core-tests.ps1 must own M0 CTest runner marker '$requiredMarker'"
        }
    }
$verifyScript = Read-Text "scripts/verify.ps1"
if ($verifyScript -notlike "*lib/core-c-tests.ps1*" -or
        $verifyScript -notlike "*Invoke-CoreCTest*" -or
        $verifyScript -notlike "*-BuildOnly:(-not `$trustedExecution)*") {
    Add-ArchitectureError "scripts/verify.ps1 must run M0 C core verification in-process through scripts/lib/core-c-tests.ps1"
}
if ($verifyScript -like "*-File*run-c-core-tests.ps1*") {
        Add-ArchitectureError "scripts/verify.ps1 must not spawn scripts/run-c-core-tests.ps1 as a nested PowerShell process"
    }
if ($verifyScript -like "*test_core_c.ps1*") {
        Add-ArchitectureError "scripts/verify.ps1 must not call the removed manual C test runner after M0"
    }
$ffiLib = Read-Text "crates/clearra-core-ffi/src/lib.rs"
foreach ($requiredMarker in @("pub mod version", "CLEARRA_CORE_ABI_VERSION", "CoreAbiVersion")) {
        if ($ffiLib -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi must export M0 ABI version marker '$requiredMarker'"
        }
    }
$ffiVersion = Read-Text "crates/clearra-core-ffi/src/version.rs"
foreach ($requiredMarker in @("CLEARRA_CORE_ABI_VERSION", "CLEARRA_CORE_VERSION", "CoreAbiVersion")) {
        if ($ffiVersion -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi version module must own M0 ABI marker '$requiredMarker'"
        }
    }
}

