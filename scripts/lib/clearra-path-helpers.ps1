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
function Get-ClearraArtifactRoot {
    $base = $null
    if (Test-StartTestsWindows) {
        $base = $env:LOCALAPPDATA
        if ([string]::IsNullOrWhiteSpace($base)) {
            $base = [System.Environment]::GetFolderPath("LocalApplicationData")
        }
    } else {
        $base = $env:XDG_CACHE_HOME
        if ([string]::IsNullOrWhiteSpace($base) -and
            -not [string]::IsNullOrWhiteSpace($env:HOME)) {
            $base = Join-Path $env:HOME ".cache"
        }
    }
    if ([string]::IsNullOrWhiteSpace($base)) {
        $base = [System.IO.Path]::GetTempPath()
    }
    $path = [System.IO.Path]::GetFullPath((Join-Path $base "Clearra/build"))
    Assert-ClearraPathOutsideRepository $path | Out-Null
    return $path
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
    [string]$RepositoryRoot = $script:ClearraPathPolicyRepositoryRoot
) {
    if ([string]::IsNullOrWhiteSpace($ArtifactPath)) {
        throw "Internal artifact path must not be empty."
    }
    $candidate = if ([System.IO.Path]::IsPathRooted($ArtifactPath)) {
        $ArtifactPath
    } else {
        Join-Path (Get-ClearraArtifactRoot) $ArtifactPath
    }
    return (Assert-ClearraPathOutsideRepository $candidate $RepositoryRoot)
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
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot).TrimEnd('\', '/')
    foreach ($name in @('target', 'build')) {
        $candidate = [System.IO.Path]::GetFullPath((Join-Path $repository $name))
        $expected = $repository + [System.IO.Path]::DirectorySeparatorChar + $name
        $comparison = if (Test-StartTestsWindows) {
            [System.StringComparison]::OrdinalIgnoreCase
        } else {
            [System.StringComparison]::Ordinal
        }
        if (-not $candidate.Equals($expected, $comparison)) {
            throw "Refusing to clean an unexpected repository-local build path: $candidate"
        }
        if (Test-Path -LiteralPath $candidate) {
            Remove-Item -LiteralPath $candidate -Recurse -Force
        }
    }
}
function Get-StartTestsTransientBuildRoots {
    return [string[]]@((Get-ClearraArtifactRoot))
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
    Assert-ClearraPathOutsideRepository $path | Out-Null
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
    Ensure-ClearraBuildArtifactCache
    $base = (Get-StartTestsTransientBuildRoots | Select-Object -First 1)
    if ([string]::IsNullOrWhiteSpace($base)) {
        throw "No persistent build root is available; pass -CoreCBuildDir explicitly."
    }
    $path = Resolve-ClearraArtifactPath (Join-Path $base $Name)
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

    $canonical = [System.IO.Path]::GetFullPath((Get-ClearraCargoTargetDir)).TrimEnd('\', '/')
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
