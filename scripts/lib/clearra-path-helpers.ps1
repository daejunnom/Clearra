# Canonical policy for internal build, cache, diagnostic, and report paths.
$script:ClearraPathPolicyRepositoryRoot = [System.IO.Path]::GetFullPath(
    (Join-Path $PSScriptRoot "../..")
)
. (Join-Path $PSScriptRoot 'clearra-local-diagnostics-policy.ps1')
if ($null -eq (Get-Variable -Name ClearraTransientBuildSlotLocks -Scope Script -ErrorAction SilentlyContinue)) {
    $script:ClearraTransientBuildSlotLocks = @{}
}

function Resolve-ClearraRoot {
    return $script:ClearraPathPolicyRepositoryRoot
}
function Test-StartTestsWindows {
    return [System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT
}
. (Join-Path $PSScriptRoot 'clearra-build-path-policy.ps1')
function Get-ClearraArtifactRoot {
    return (Get-ClearraCanonicalBuildRoot)
}
function Get-ClearraReportRoot {
    $base = $null
    if (Test-StartTestsWindows) {
        $base = $env:LOCALAPPDATA
        if ([string]::IsNullOrWhiteSpace($base)) {
            $base = [System.Environment]::GetFolderPath("LocalApplicationData")
        }
    } else {
        $base = $env:XDG_STATE_HOME
    }
    if ([string]::IsNullOrWhiteSpace($base)) {
        $base = [System.IO.Path]::GetTempPath()
    }
    $path = [System.IO.Path]::GetFullPath((Join-Path $base "Clearra/reports"))
    Assert-ClearraPathOutsideRepository $path | Out-Null
    return $path
}
function Test-ClearraPathInsideRepository(
    [string]$Path,
    [string]$RepositoryRoot = $script:ClearraPathPolicyRepositoryRoot
) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $false
    }
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    )
    $candidate = [System.IO.Path]::GetFullPath($Path)
    $comparison = if (Test-StartTestsWindows) {
        [System.StringComparison]::OrdinalIgnoreCase
    } else {
        [System.StringComparison]::Ordinal
    }
    if ($candidate.Equals($repository, $comparison)) {
        return $true
    }
    $prefix = $repository + [System.IO.Path]::DirectorySeparatorChar
    return $candidate.StartsWith($prefix, $comparison)
}
function Assert-ClearraPathOutsideRepository(
    [string]$Path,
    [string]$RepositoryRoot = $script:ClearraPathPolicyRepositoryRoot
) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw "Internal artifact path must not be empty."
    }
    $candidate = [System.IO.Path]::GetFullPath($Path)
    if (Test-ClearraPathInsideRepository $candidate $RepositoryRoot) {
        throw "Internal artifact and report paths must be outside the repository: $candidate"
    }
    return $candidate
}
function Resolve-ClearraReportPath(
    [string]$ReportPath,
    [string]$RepositoryRoot = $script:ClearraPathPolicyRepositoryRoot
) {
    if ([string]::IsNullOrWhiteSpace($ReportPath)) {
        return $null
    }
    $candidate = if ([System.IO.Path]::IsPathRooted($ReportPath)) {
        $ReportPath
    } else {
        Join-Path (Get-ClearraReportRoot) $ReportPath
    }
    return (Assert-ClearraPathOutsideRepository $candidate $RepositoryRoot)
}
function Resolve-ClearraArtifactPath(
    [string]$ArtifactPath,
    [string]$RepositoryRoot = (Resolve-ClearraBuildSourceRoot)
) {
    if ([string]::IsNullOrWhiteSpace($ArtifactPath)) {
        throw "Internal artifact path must not be empty."
    }
    $transaction = Get-ClearraBuildTransactionRoot -RepositoryRoot $RepositoryRoot
    $candidate = if ([System.IO.Path]::IsPathRooted($ArtifactPath)) {
        $ArtifactPath
    } else {
        Join-Path $transaction $ArtifactPath
    }
    return (Assert-ClearraPathInBuildTransaction $candidate $transaction $RepositoryRoot)
}
function Assert-ClearraRepositoryArtifactPolicy(
    [string]$RepositoryRoot = $script:ClearraPathPolicyRepositoryRoot
) {
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot)
    Assert-ClearraLocalToolDirectoryPolicy $repository
    foreach ($name in @("target", "build")) {
        $forbidden = Join-Path $repository $name
        if (Test-Path -LiteralPath $forbidden) {
            throw "Repository-local artifact directory is forbidden: $forbidden"
        }
    }
    $cargoRoot = Join-Path $repository ".cargo"
    if (Test-Path -LiteralPath $cargoRoot) {
        $cargoTargets = @(Get-ChildItem -LiteralPath $cargoRoot -Directory -Filter "target*" -Force)
        if ($cargoTargets.Count -gt 0) {
            throw "Repository-local Cargo target directory is forbidden: $($cargoTargets[0].FullName)"
        }
    }
}
function Remove-ClearraRepositoryLocalBuildArtifacts(
    [string]$RepositoryRoot = $script:ClearraPathPolicyRepositoryRoot
) {
    throw 'Repository-local legacy cleanup is forbidden during build initialization; use an explicit reviewed cleanup plan.'
}
function Get-StartTestsTransientBuildRoots {
    return [string[]]@((Get-ClearraBuildTransactionRoot))
}
function New-TransientBuildDir([string]$Prefix) {
    $base = (Get-StartTestsTransientBuildRoots | Select-Object -First 1)
    if ([string]::IsNullOrWhiteSpace($base)) {
        throw "No transient build root is available; pass -CoreCBuildDir explicitly."
    }
    if ($Prefix -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
        throw "Transient build prefix contains unsupported path characters: $Prefix"
    }
    $slotRoot = [System.IO.Path]::GetFullPath((Join-Path $base 'transient'))
    $path = [System.IO.Path]::GetFullPath((Join-Path $slotRoot $Prefix))
    Assert-ClearraPathInBuildTransaction $path $base (Resolve-ClearraBuildSourceRoot) | Out-Null
    Ensure-ClearraBuildArtifactCache
    New-Item -ItemType Directory -Force -Path $slotRoot | Out-Null
    if ($script:ClearraTransientBuildSlotLocks.ContainsKey($path)) {
        throw "Transient build slot is already active in this process: $path"
    }

    $lockPath = Join-Path $slotRoot ".$Prefix.lock"
    $deadline = [DateTime]::UtcNow.AddMinutes(30)
    $lock = $null
    do {
        try {
            $lock = [System.IO.File]::Open(
                $lockPath,
                [System.IO.FileMode]::OpenOrCreate,
                [System.IO.FileAccess]::ReadWrite,
                [System.IO.FileShare]::None
            )
        } catch [System.IO.IOException] {
            if ([DateTime]::UtcNow -ge $deadline) {
                throw "Timed out waiting for transient build slot: $path"
            }
            Start-Sleep -Milliseconds 250
        }
    } while ($null -eq $lock)

    try {
        if (Test-Path -LiteralPath $path) {
            Assert-ClearraBuildTreeNoReparse $path
            Remove-Item -LiteralPath $path -Recurse -Force
        }
        New-Item -ItemType Directory -Force -Path $path | Out-Null
        $script:ClearraTransientBuildSlotLocks[$path] = $lock
        return $path
    } catch {
        $lock.Dispose()
        Remove-Item -LiteralPath $lockPath -Force -ErrorAction SilentlyContinue
        throw
    }
}
function Get-StartTestsPersistentBuildDir([string]$Name) {
    $base = (Get-StartTestsTransientBuildRoots | Select-Object -First 1)
    if ([string]::IsNullOrWhiteSpace($base)) {
        throw "No persistent build root is available; pass -CoreCBuildDir explicitly."
    }
    $path = Resolve-ClearraArtifactPath (Join-Path $base $Name)
    Ensure-ClearraBuildArtifactCache
    New-Item -ItemType Directory -Force -Path $path | Out-Null
    return $path
}

function Get-ClearraCargoTargetDir {
    return (Get-StartTestsPersistentBuildDir "cargo-target")
}
function Assert-ClearraCanonicalCargoTargetDir([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw "Cargo target directory must not be empty."
    }

    $canonical = Join-Path (Get-ClearraBuildTransactionRoot) 'cargo-target'
    Assert-ClearraCanonicalBuildPath $Path (Resolve-ClearraBuildSourceRoot) | Out-Null
    $candidate = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    $comparison = if (Test-StartTestsWindows) {
        [System.StringComparison]::OrdinalIgnoreCase
    } else {
        [System.StringComparison]::Ordinal
    }
    if (-not $candidate.Equals($canonical, $comparison)) {
        throw "All Clearra Cargo tasks must share the canonical target directory: $canonical"
    }
    return $canonical
}
function Remove-TransientBuildDir([string]$BuildDir) {
    if ([string]::IsNullOrWhiteSpace($BuildDir)) {
        return
    }
    $buildPath = [System.IO.Path]::GetFullPath($BuildDir)
    $isAllowed = $false
    $lockPath = $null
    foreach ($root in Get-StartTestsTransientBuildRoots) {
        if ([string]::IsNullOrWhiteSpace($root)) {
            continue
        }
        $rootPath = [System.IO.Path]::GetFullPath($root)
        $comparison = if (Test-StartTestsWindows) {
            [System.StringComparison]::OrdinalIgnoreCase
        } else {
            [System.StringComparison]::Ordinal
        }
        $slotRoot = [System.IO.Path]::GetFullPath((Join-Path $rootPath 'transient')).TrimEnd('\', '/')
        $parent = [System.IO.Path]::GetFullPath((Split-Path -Parent $buildPath)).TrimEnd('\', '/')
        if ($parent.Equals($slotRoot, $comparison) -and
            $script:ClearraTransientBuildSlotLocks.ContainsKey($buildPath)) {
            $isAllowed = $true
            $lockPath = Join-Path $slotRoot ".$([System.IO.Path]::GetFileName($buildPath)).lock"
            break
        }
    }

    try {
        if ($isAllowed) {
            Assert-ClearraPathInBuildTransaction $buildPath (Get-ClearraBuildTransactionRoot) (Resolve-ClearraBuildSourceRoot) | Out-Null
            if (Test-Path -LiteralPath $buildPath) { Assert-ClearraBuildTreeNoReparse $buildPath }
            Remove-Item -LiteralPath $buildPath -Recurse -Force -ErrorAction SilentlyContinue
        }
    } finally {
        if ($script:ClearraTransientBuildSlotLocks.ContainsKey($buildPath)) {
            $script:ClearraTransientBuildSlotLocks[$buildPath].Dispose()
            $script:ClearraTransientBuildSlotLocks.Remove($buildPath)
        }
        if (-not [string]::IsNullOrWhiteSpace($lockPath)) {
            Remove-Item -LiteralPath $lockPath -Force -ErrorAction SilentlyContinue
        }
    }
}
function Resolve-CoreCBuildDirForStartTests(
    [string]$Root,
    [bool]$Keep,
    [string]$Requested
) {
    if (-not [string]::IsNullOrWhiteSpace($Requested)) {
        return (Resolve-ClearraArtifactPath $Requested $Root)
    }
    if ($Keep) {
        return (Get-StartTestsPersistentBuildDir "core-c-cache")
    }
    return (New-TransientBuildDir "clearra-core-c")
}

. (Join-Path $PSScriptRoot 'clearra-artifact-cache.ps1')
