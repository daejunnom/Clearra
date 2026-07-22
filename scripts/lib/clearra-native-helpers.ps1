# This file is dot-sourced by clearra-start-helpers.ps1.
function Join-StartTestRustFlags([string]$ExistingFlags, [string]$AdditionalFlags) {
    if ([string]::IsNullOrWhiteSpace($ExistingFlags)) {
        return $AdditionalFlags
    }
    if ([string]::IsNullOrWhiteSpace($AdditionalFlags)) {
        return $ExistingFlags
    }
    return "$ExistingFlags $AdditionalFlags"
}

function Test-CoreCLibraryDir([string]$Directory) {
    if ([string]::IsNullOrWhiteSpace($Directory) -or
        -not (Test-Path -LiteralPath $Directory -PathType Container)) {
        return $false
    }
    foreach ($libraryName in @("clearra_core.lib", "libclearra_core.a", "clearra_core.a")) {
        if (Test-Path -LiteralPath (Join-Path $Directory $libraryName) -PathType Leaf) {
            return $true
        }
    }
    return $false
}

function Get-ClearraNativeCoreLibraryPath([string]$LibraryDirectory) {
    if (-not (Test-CoreCLibraryDir $LibraryDirectory)) {
        throw "Native C library directory is invalid: $LibraryDirectory"
    }
    $libraryPath = @(
        "clearra_core.lib",
        "libclearra_core.a",
        "clearra_core.a"
    ) | ForEach-Object { Join-Path $LibraryDirectory $_ } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($libraryPath)) {
        throw "Native C library file is missing under: $LibraryDirectory"
    }
    return [System.IO.Path]::GetFullPath($libraryPath)
}

function Get-ClearraNativeRustLinkFlags([string]$LibraryDirectory) {
    Get-ClearraNativeCoreLibraryPath $LibraryDirectory | Out-Null
    return "-L native=$([System.IO.Path]::GetFullPath($LibraryDirectory))"
}

function Add-ClearraWindowsNativeRustLinkFlags(
    [AllowNull()][string]$ExistingFlags,
    [string]$LibraryDirectory
) {
    return Join-StartTestRustFlags `
        $ExistingFlags `
        (Get-ClearraNativeRustLinkFlags $LibraryDirectory)
}

function Sync-ClearraNativeCargoLinkState {
    param(
        [Parameter(Mandatory)]
        [string]$LibraryDirectory,
        [Parameter(Mandatory)]
        [string]$CargoTargetDirectory,
        [string]$CargoPath = 'cargo',
        [string]$WorkspaceRoot = ''
    )

    $targetDirectory = Assert-ClearraCanonicalCargoTargetDir $CargoTargetDirectory
    $libraryPath = Get-ClearraNativeCoreLibraryPath $LibraryDirectory
    $libraryHash = (Get-FileHash -LiteralPath $libraryPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $fingerprint = "$libraryPath|$libraryHash"
    $stateDirectory = Join-Path $targetDirectory '.clearra-state'
    $statePath = Join-Path $stateDirectory 'native-core-link.txt'
    $previousFingerprint = if (Test-Path -LiteralPath $statePath -PathType Leaf) {
        (Get-Content -LiteralPath $statePath -Raw).Trim()
    } else {
        ''
    }
    if ($previousFingerprint -eq $fingerprint) {
        Write-Output '[native-link-cache] reused | package=clearra-core-ffi'
        return
    }

    $root = if ([string]::IsNullOrWhiteSpace($WorkspaceRoot)) {
        [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
    } else {
        [System.IO.Path]::GetFullPath($WorkspaceRoot)
    }
    New-Item -ItemType Directory -Force -Path $stateDirectory | Out-Null
    $previousTarget = $env:CARGO_TARGET_DIR
    $previousPreference = $ErrorActionPreference
    $env:CARGO_TARGET_DIR = $targetDirectory
    $ErrorActionPreference = 'Continue'
    Push-Location $root
    try {
        $cleanOutput = [System.Collections.Generic.List[string]]::new()
        $cleanExitCode = 0
        foreach ($profileArguments in @(
            @('clean', '-p', 'clearra-core-ffi'),
            @('clean', '-p', 'clearra-core-ffi', '--release')
        )) {
            $profileOutput = @(& $CargoPath @profileArguments 2>&1)
            foreach ($line in $profileOutput) {
                $cleanOutput.Add($line.ToString())
            }
            if ($LASTEXITCODE -ne 0) {
                $cleanExitCode = $LASTEXITCODE
                break
            }
        }
    }
    finally {
        Pop-Location
        $ErrorActionPreference = $previousPreference
        if ([string]::IsNullOrWhiteSpace($previousTarget)) {
            Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousTarget
        }
    }
    if ($cleanExitCode -ne 0) {
        $excerpt = @($cleanOutput | Select-Object -Last 40) -join "`n"
        throw "Selective native Cargo cache invalidation failed with exit code $cleanExitCode`n$excerpt"
    }
    Set-Content -LiteralPath $statePath -Value $fingerprint -Encoding UTF8 -NoNewline
    Write-Output '[native-link-cache] refreshed | package=clearra-core-ffi'
}

function Find-CoreCLibraryDir([string]$BuildDir) {
    foreach ($candidate in @(
        $BuildDir,
        (Join-Path $BuildDir "core-c"),
        (Join-Path $BuildDir "Debug"),
        (Join-Path $BuildDir "Release"),
        (Join-Path $BuildDir "RelWithDebInfo"),
        (Join-Path $BuildDir "core-c/Debug"),
        (Join-Path $BuildDir "core-c/Release"),
        (Join-Path $BuildDir "core-c/RelWithDebInfo")
    )) {
        if (Test-CoreCLibraryDir $candidate) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    return $null
}

function Invoke-NativeCargoCommand(
    [string]$Label,
    [string[]]$Arguments
) {
    if ($VerboseLog.IsPresent) {
        Write-Output "==> $CargoPath $($Arguments -join ' ')"
    }
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = @(& $CargoPath @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($VerboseLog.IsPresent) {
        $output | ForEach-Object { Write-Output $_.ToString() }
    }
    if ($exitCode -ne 0) {
        $excerpt = @($output | Select-Object -Last $OutputExcerptLines) -join "`n"
        throw "$Label failed with exit code $exitCode`n$excerpt"
    }
    Write-Output "[$Label] passed"
}

function Invoke-NativeLocalMode([string]$Root) {
    $trustedExecution = Test-ClearraTrustedExecutionSurface $ExecutionSurface
    $buildDir = if ([string]::IsNullOrWhiteSpace($CoreCBuildDir)) {
        $buildName = if ($trustedExecution) {
            "core-c-test-cache"
        } else {
            "core-c-library-cache"
        }
        Get-StartTestsPersistentBuildDir $buildName
    } else {
        Resolve-CoreCBuildDirForStartTests $Root $true $CoreCBuildDir
    }

    $coreResult = Invoke-CoreCTest `
        -BuildDir $buildDir `
        -Configuration "Debug" `
        -ConfigureArgs (Get-StartTestsCMakeConfigureArgs) `
        -BuildOnly:(-not $trustedExecution) `
        -Workers $Workers
    Write-CoreCTestStartSummary "NativeLocal" $coreResult
    if ($coreResult.Status -eq "Failed" -or $coreResult.Status -eq "Degraded") {
        throw "NativeLocal C core build failed: $($coreResult.Reason)"
    }

    $libDir = Find-CoreCLibraryDir $buildDir
    if ([string]::IsNullOrWhiteSpace($libDir)) {
        throw "NativeLocal could not find clearra_core static library under $buildDir"
    }

    if (-not $trustedExecution) {
        Write-Output "[native-c-core] C library built; ManagedLocal does not compile or launch Rust source artifacts"
        Write-Output "[native-c-core] gate summary | execution_surface=ManagedLocal | native_c_binding=not-built | rust_test_execution=not-built | c_core_test_execution=not-built | product_binary_launch=false | policy_fallback_used=false"
        return
    }

    $previousWindowsRustFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
    $previousCargoTargetDir = $env:CARGO_TARGET_DIR
    $env:CARGO_TARGET_DIR = Get-ClearraCargoTargetDir
    Sync-ClearraNativeCargoLinkState `
        -LibraryDirectory $libDir `
        -CargoTargetDirectory $env:CARGO_TARGET_DIR `
        -CargoPath $CargoPath `
        -WorkspaceRoot $Root
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS =
        Add-ClearraWindowsNativeRustLinkFlags $previousWindowsRustFlags $libDir
    try {
        Invoke-NativeCargoCommand `
            "native core-executor tests" `
            @(
                "test", "-p", "clearra-core-executor", "--lib",
                "--features", "native-c-core", "--", "--test-threads=1"
            )
        Write-Output "[native-c-core] gate summary | execution_surface=Trusted | native_c_binding=enabled | rust_test_execution=launched | c_core_test_execution=launched | policy_fallback_used=false"
    } finally {
        if ([string]::IsNullOrWhiteSpace($previousWindowsRustFlags)) {
            Remove-Item Env:\CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousWindowsRustFlags
        }
        if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
            Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousCargoTargetDir
        }
    }
}
