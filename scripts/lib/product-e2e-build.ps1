# Product E2E build and binary resolution stage.

$script:ProductE2ENativeLibraryDir = ''

function Remove-StaleProductE2EClearraCliBinary {
    $stalePaths = @(
        (Join-Path (Get-ClearraCargoTargetDir) "debug/clearra-cli.exe")
    )

    foreach ($stalePath in $stalePaths) {
        if (Test-Path -LiteralPath $stalePath) {
            Remove-Item -LiteralPath $stalePath -Force
        }
    }
}

function Resolve-ProductE2EBinary {
    if (-not [string]::IsNullOrWhiteSpace($ExePath)) {
        return $ExePath
    }

    $candidates = @()
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        $candidates += (Join-Path $env:CARGO_TARGET_DIR "debug/clearra.exe")
        $candidates += (Join-Path $env:CARGO_TARGET_DIR "debug/clearra")
    }

    $cacheTarget = Get-ClearraCargoTargetDir
    $candidates += (Join-Path $cacheTarget "debug/clearra.exe")
    $candidates += (Join-Path $cacheTarget "debug/clearra")

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    return (Join-Path (Get-ClearraCargoTargetDir) "debug/clearra.exe")
}

function Get-ProductE2ECoreBuildDir {
    $buildDir = Resolve-ClearraArtifactPath 'core-c-library-cache' $Root
    New-Item -ItemType Directory -Force -Path $buildDir | Out-Null
    return $buildDir
}

function Find-ProductE2ENativeLibraryDir([string]$BuildDir) {
    foreach ($libraryName in @('clearra_core.lib', 'libclearra_core.a', 'clearra_core.a')) {
        $library = Get-ChildItem -LiteralPath $BuildDir -Recurse -File -Filter $libraryName `
            -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($null -ne $library) {
            return $library.DirectoryName
        }
    }
    return $null
}

function Resolve-ProductE2ENativeLibraryDir {
    if (-not [string]::IsNullOrWhiteSpace($script:ProductE2ENativeLibraryDir)) {
        return $script:ProductE2ENativeLibraryDir
    }
    if ($null -eq (Get-Command 'cmake' -ErrorAction SilentlyContinue)) {
        throw 'Product E2E requires CMake to build the native core backend.'
    }

    $buildDir = Get-ProductE2ECoreBuildDir
    $configureOutput = & cmake -S $Root -B $buildDir `
        '-DCLEARRA_CORE_SPLIT_TESTS=OFF' '-DBUILD_TESTING=OFF' 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Product E2E native C configure failed.`n$($configureOutput -join "`n")"
    }
    $buildOutput = & cmake --build $buildDir --config Debug --parallel 2 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Product E2E native C build failed.`n$($buildOutput -join "`n")"
    }
    $script:ProductE2ENativeLibraryDir = Find-ProductE2ENativeLibraryDir $buildDir
    if ([string]::IsNullOrWhiteSpace($script:ProductE2ENativeLibraryDir)) {
        throw "Product E2E could not find clearra_core under $buildDir"
    }
    return $script:ProductE2ENativeLibraryDir
}
