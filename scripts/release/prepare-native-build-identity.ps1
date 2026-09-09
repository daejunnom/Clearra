param(
    [Parameter(Mandatory)]
    [string]$BuildDir,
    [Parameter(Mandatory)]
    [string]$OutputPath,
    [Parameter(Mandatory)]
    [string]$SourceCommit,
    [int]$Workers = [Math]::Max(1, [Environment]::ProcessorCount),
    [switch]$BuildTestOracle,
    [string]$GitHubEnvironmentPath = "",
    [string]$GitHubOutputPath = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Get-Sha256Text([string]$Value) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
    $hash = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString($hash.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
    } finally {
        $hash.Dispose()
    }
}

function Get-CommandOutput([string]$FileName, [string[]]$Arguments) {
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $lines = @(& $FileName @Arguments 2>&1 | ForEach-Object { $_.ToString() })
        if ($LASTEXITCODE -ne 0) {
            throw "$FileName $($Arguments -join ' ') failed with exit $LASTEXITCODE.`n$($lines -join "`n")"
        }
        return (($lines -join "`n").Trim())
    } finally {
        $ErrorActionPreference = $previousPreference
    }
}

function Get-CMakeValue([string]$Text, [string]$Name) {
    $match = [regex]::Match(
        $Text,
        "(?m)^$([regex]::Escape($Name))(?::[^=]+)?=(.*)$"
    )
    if ($match.Success) {
        return $match.Groups[1].Value.Trim()
    }
    return ""
}

function Get-CMakeSetValue([string]$Text, [string]$Name) {
    $pattern = '(?m)^set\(' + [regex]::Escape($Name) + '\s+"([^"]*)"\)'
    $match = [regex]::Match(
        $Text,
        $pattern
    )
    if ($match.Success) {
        return $match.Groups[1].Value.Trim()
    }
    return ""
}

function Get-OptionalFileHash([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path) -or
        -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return "unavailable"
    }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

if ($Workers -lt 1) {
    throw "-Workers must be at least 1."
}
if ($SourceCommit -notmatch '^[0-9a-f]{40}$') {
    throw "-SourceCommit must be an exact lowercase commit SHA."
}

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../.."))
$resolvedBuildDir = [System.IO.Path]::GetFullPath($BuildDir)
$resolvedOutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$head = (Get-CommandOutput "git" @("-C", $root, "rev-parse", "HEAD")).Trim()
if ($head -ne $SourceCommit) {
    throw "Native identity source mismatch: expected $SourceCommit, observed $head."
}

$configureArgs = [System.Collections.Generic.List[string]]::new()
$configureArgs.Add("-DCLEARRA_CORE_SPLIT_TESTS=OFF")
$configureArgs.Add("-DBUILD_TESTING=OFF")
if ($BuildTestOracle.IsPresent) {
    $configureArgs.Add("-DCLEARRA_BUILD_TEST_ORACLE=ON")
}

New-Item -ItemType Directory -Force -Path $resolvedBuildDir | Out-Null
$configureOutput = @(& cmake -S $root -B $resolvedBuildDir @($configureArgs) 2>&1)
if ($LASTEXITCODE -ne 0) {
    throw "Native identity CMake configure failed.`n$($configureOutput -join "`n")"
}
$buildOutput = @(& cmake --build $resolvedBuildDir --config Debug --parallel $Workers 2>&1)
if ($LASTEXITCODE -ne 0) {
    throw "Native identity CMake build failed.`n$($buildOutput -join "`n")"
}

$archive = @(
    "clearra_core.lib",
    "libclearra_core.a",
    "clearra_core.a"
) | ForEach-Object {
    Get-ChildItem -LiteralPath $resolvedBuildDir -Recurse -File -Filter $_ -ErrorAction SilentlyContinue
} | Select-Object -First 1
if ($null -eq $archive) {
    throw "Native identity could not find the clearra_core archive under $resolvedBuildDir."
}

$trackedInputs = @(
    "CMakeLists.txt",
    "Cargo.toml",
    "Cargo.lock",
    "cmake",
    "core-c",
    "crates/clearra-core-ffi",
    "scripts/lib/clearra-core-c-task-helpers.ps1",
    "scripts/lib/clearra-native-helpers.ps1",
    "scripts/lib/core-c-build.ps1",
    "scripts/lib/product-process-surface.ps1"
)
$trackedMaterial = Get-CommandOutput "git" (@("-C", $root, "ls-files", "--stage", "--") + $trackedInputs)
if ([string]::IsNullOrWhiteSpace($trackedMaterial)) {
    throw "Native identity tracked input set is empty."
}

$cachePath = Join-Path $resolvedBuildDir "CMakeCache.txt"
if (-not (Test-Path -LiteralPath $cachePath -PathType Leaf)) {
    throw "Native identity CMake cache is missing: $cachePath"
}
$cacheText = Get-Content -LiteralPath $cachePath -Raw
$compilerMetadataPath = Get-ChildItem `
    -LiteralPath (Join-Path $resolvedBuildDir "CMakeFiles") `
    -Recurse `
    -File `
    -Filter "CMakeCCompiler.cmake" `
    -ErrorAction SilentlyContinue |
    Select-Object -First 1 -ExpandProperty FullName
$compilerMetadata = if ([string]::IsNullOrWhiteSpace($compilerMetadataPath)) {
    ""
} else {
    Get-Content -LiteralPath $compilerMetadataPath -Raw
}

$compilerPath = Get-CMakeValue $cacheText "CMAKE_C_COMPILER"
if ([string]::IsNullOrWhiteSpace($compilerPath)) {
    $compilerPath = Get-CMakeSetValue $compilerMetadata "CMAKE_C_COMPILER"
}
$librarianPath = Get-CMakeValue $cacheText "CMAKE_AR"
if ([string]::IsNullOrWhiteSpace($librarianPath)) {
    $librarianPath = Get-CMakeSetValue $compilerMetadata "CMAKE_AR"
}
$compilerId = Get-CMakeSetValue $compilerMetadata "CMAKE_C_COMPILER_ID"
$compilerVersion = Get-CMakeSetValue $compilerMetadata "CMAKE_C_COMPILER_VERSION"
$rustcIdentity = Get-CommandOutput "rustc" @("-vV")
$cargoIdentity = Get-CommandOutput "cargo" @("-Vv")
$cmakeIdentity = Get-CommandOutput "cmake" @("--version")
$archiveHash = (Get-FileHash -LiteralPath $archive.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$trackedInputHash = Get-Sha256Text (($trackedMaterial -replace "`r`n", "`n") + "`n")
$configuration = "Debug|$($configureArgs -join '|')"
$generator = Get-CMakeValue $cacheText "CMAKE_GENERATOR"
$generatorInstance = Get-CMakeValue $cacheText "CMAKE_GENERATOR_INSTANCE"

$identityLines = @(
    "schema=clearra.native-build-identity.v1",
    "tracked_inputs_sha256=$trackedInputHash",
    "configuration=$configuration",
    "archive_sha256=$archiveHash",
    "rustc_sha256=$(Get-Sha256Text $rustcIdentity)",
    "cargo_sha256=$(Get-Sha256Text $cargoIdentity)",
    "cmake_sha256=$(Get-Sha256Text $cmakeIdentity)",
    "generator=$generator",
    "generator_instance=$generatorInstance",
    "compiler_id=$compilerId",
    "compiler_version=$compilerVersion",
    "compiler_binary_sha256=$(Get-OptionalFileHash $compilerPath)",
    "librarian_binary_sha256=$(Get-OptionalFileHash $librarianPath)"
)
$identityHash = Get-Sha256Text (($identityLines -join "`n") + "`n")

$document = [ordered]@{
    schema_version = "clearra.native-build-identity.v1"
    authority = "non-authoritative-build-input"
    source_commit = $SourceCommit
    identity_sha256 = $identityHash
    tracked_inputs_sha256 = $trackedInputHash
    tracked_input_count = @($trackedMaterial -split "`n" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    configuration = [ordered]@{
        build_type = "Debug"
        cmake_arguments = @($configureArgs)
        build_test_oracle = $BuildTestOracle.IsPresent
    }
    native_archive = [ordered]@{
        file_name = $archive.Name
        sha256 = $archiveHash
        size_bytes = [int64]$archive.Length
    }
    toolchain = [ordered]@{
        rustc_identity_sha256 = Get-Sha256Text $rustcIdentity
        cargo_identity_sha256 = Get-Sha256Text $cargoIdentity
        cmake_identity_sha256 = Get-Sha256Text $cmakeIdentity
        cmake_generator = $generator
        cmake_generator_instance = $generatorInstance
        c_compiler_id = $compilerId
        c_compiler_version = $compilerVersion
        c_compiler_binary_sha256 = Get-OptionalFileHash $compilerPath
        librarian_binary_sha256 = Get-OptionalFileHash $librarianPath
        runner_image_os = [string]$env:ImageOS
        runner_image_version = [string]$env:ImageVersion
    }
    runtime_paths = [ordered]@{
        native_library_directory = $archive.DirectoryName
        native_archive = $archive.FullName
    }
}

$outputDirectory = Split-Path -Parent $resolvedOutputPath
if (-not [string]::IsNullOrWhiteSpace($outputDirectory)) {
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
}
$json = $document | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText(
    $resolvedOutputPath,
    $json + "`n",
    [System.Text.UTF8Encoding]::new($false)
)

$compilerCacheNamespace = "clearra-native-v1-$identityHash"
if (-not [string]::IsNullOrWhiteSpace($GitHubEnvironmentPath)) {
    Add-Content -LiteralPath $GitHubEnvironmentPath -Value "SCCACHE_GHA_VERSION=$compilerCacheNamespace" -Encoding UTF8
}
if (-not [string]::IsNullOrWhiteSpace($GitHubOutputPath)) {
    Add-Content -LiteralPath $GitHubOutputPath -Value "identity_sha256=$identityHash" -Encoding UTF8
    Add-Content -LiteralPath $GitHubOutputPath -Value "compiler_cache_namespace=$compilerCacheNamespace" -Encoding UTF8
    Add-Content -LiteralPath $GitHubOutputPath -Value "native_library_directory=$($archive.DirectoryName)" -Encoding UTF8
    Add-Content -LiteralPath $GitHubOutputPath -Value "native_archive=$($archive.FullName)" -Encoding UTF8
}

Write-Output "native_build_identity=sealed identity_sha256=$identityHash tracked_inputs_sha256=$trackedInputHash archive_sha256=$archiveHash"
Write-Output "compiler_cache_namespace=$compilerCacheNamespace"
