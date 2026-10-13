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
    $forbiddenTarget = Join-Path $repository "target"
    if (Test-Path -LiteralPath $forbiddenTarget) {
        throw "Repository-local artifact directory is forbidden: $forbiddenTarget"
    }

    # Repository build staging is a declared management root and is excluded
    # from compiler source snapshots. Its presence is valid only while the
    # manifest, ignore boundary, and Git ownership all retain that contract.
    $repositoryBuild = Join-Path $repository "build"
    if (Test-Path -LiteralPath $repositoryBuild) {
        $repositoryBuildEntry = Get-Item -LiteralPath $repositoryBuild -Force
        if (-not $repositoryBuildEntry.PSIsContainer -or
            ($repositoryBuildEntry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Repository-local build root must be a real directory: $repositoryBuild"
        }
        $policyPath = Join-Path $repository "config/clearra-management.v1.json"
        if (-not (Test-Path -LiteralPath $policyPath -PathType Leaf)) {
            throw "Repository-local build directory is not declared by the management policy: $repositoryBuild"
        }
        try {
            $policy = Get-Content -LiteralPath $policyPath -Raw | ConvertFrom-Json
        } catch {
            throw "Repository-local build policy cannot be parsed: $policyPath"
        }
        $repositoryRoots = $policy.PSObject.Properties['repository_roots']
        $buildDeclarations = @()
        if ($null -ne $repositoryRoots) {
            $buildDeclarations = @($repositoryRoots.Value | Where-Object {
                $_.id -ceq 'build-publication' -and $_.path -ceq 'build'
            })
        }
        if ($buildDeclarations.Count -ne 1) {
            throw "Repository-local build directory is not declared exactly once: $repositoryBuild"
        }

        $gitIgnorePath = Join-Path $repository '.gitignore'
        if (-not (Test-Path -LiteralPath $gitIgnorePath -PathType Leaf) -or
            (Get-Content -LiteralPath $gitIgnorePath -Raw) -notmatch '(?m)^/build/\s*$') {
            throw "Repository-local build directory must remain root-ignored: $repositoryBuild"
        }
        $trackedBuildEntries = @(& git -C $repository ls-files -- build 2>$null)
        if ($LASTEXITCODE -ne 0) {
            throw "Could not verify repository-local build ownership: $repositoryBuild"
        }
        if ($trackedBuildEntries.Count -ne 0) {
            throw "Repository-local build output must not be tracked: $($trackedBuildEntries[0])"
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
        throw 'No managed transient build root is available.'
    }
    if ($Prefix -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
        throw "Transient build prefix contains unsupported path characters: $Prefix"
    }
    $slotRoot = [IO.Path]::GetFullPath((Join-Path $base 'transient'))
    $path = [IO.Path]::GetFullPath((Join-Path $slotRoot $Prefix))
    Assert-ClearraPathInBuildTransaction $path $base (Resolve-ClearraBuildSourceRoot) | Out-Null
    Ensure-ClearraBuildArtifactCache
    if ($script:ClearraTransientBuildSlotLocks.ContainsKey($path)) {
        throw "Transient build slot is already active in this process: $path"
    }
    # The shared helper validates the live build transaction, binds the lease
    # to this PowerShell parent and refuses interrupted/unknown predecessors.
    $leaseText = @(& node (Join-Path $PSScriptRoot '../tools/managed-transient-directory.mjs') --acquire-for-parent $path)
    if ($LASTEXITCODE -ne 0) { throw "Transient build slot acquisition failed: $path" }
    $lease = ($leaseText -join "`n") | ConvertFrom-Json
    if ($lease.pid -ne $PID -or $lease.directory -ne $path -or $lease.state -ne 'active') {
        throw 'Transient parent handoff identity mismatch; lease is preserved.'
    }
    $script:ClearraTransientBuildSlotLocks[$path] = $lease
    return $path
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
    if ([string]::IsNullOrWhiteSpace($BuildDir)) { return }
    $buildPath = [IO.Path]::GetFullPath($BuildDir)
    if (-not $script:ClearraTransientBuildSlotLocks.ContainsKey($buildPath)) { return }
    $lease = $script:ClearraTransientBuildSlotLocks[$buildPath]
    $base = Get-ClearraBuildTransactionRoot
    Assert-ClearraPathInBuildTransaction $buildPath $base (Resolve-ClearraBuildSourceRoot) | Out-Null
    if ($lease.pid -ne $PID -or $lease.directory -ne $buildPath -or $lease.state -ne 'active') {
        throw 'Transient cleanup owner mismatch; lease is preserved.'
    }
    # Callers have finished their owned consumers before this explicit release.
    # A failed cleanup keeps both the on-disk lease and this process's binding.
    & node (Join-Path $PSScriptRoot '../tools/managed-transient-directory.mjs') --release-for-parent $buildPath $lease.token
    if ($LASTEXITCODE -ne 0) { throw "Transient cleanup failed; unresolved lease is preserved: $buildPath" }
    $script:ClearraTransientBuildSlotLocks.Remove($buildPath)
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
