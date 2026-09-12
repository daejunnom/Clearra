# Pure validation shared by all managed build entry points. Never creates paths.
. (Join-Path $PSScriptRoot 'clearra-build-metadata-paths.ps1')
function Get-ClearraBuildPathComparison {
    if (Test-StartTestsWindows) { return [StringComparison]::OrdinalIgnoreCase }
    return [StringComparison]::Ordinal
}

function Assert-ClearraNoReparseBuildPath([string]$Path) {
    $candidate = [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    if (Test-StartTestsWindows) {
        $drive = [IO.Path]::GetPathRoot($candidate)
        if ($drive -notmatch '^[A-Za-z]:\\$' -or $candidate.Substring($drive.Length).Contains(':')) {
            throw "Build paths must use a local drive without alternate streams: $candidate"
        }
    }
    $ancestor = $candidate
    while (-not [string]::IsNullOrWhiteSpace($ancestor)) {
        $entry = Get-Item -LiteralPath $ancestor -Force -ErrorAction SilentlyContinue
        if ($null -ne $entry -and ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Build paths refuse reparse points and symbolic links: $ancestor"
        }
        $parent = [IO.Path]::GetDirectoryName($ancestor)
        if ($parent -eq $ancestor) { break }
        $ancestor = $parent
    }
    return $candidate
}

function Get-ClearraCanonicalBuildRoot {
    if (Test-StartTestsWindows) {
        $base = $env:LOCALAPPDATA
        if ([string]::IsNullOrWhiteSpace($base)) {
            $base = [Environment]::GetFolderPath('LocalApplicationData')
        }
        if ([string]::IsNullOrWhiteSpace($base)) { throw 'Windows LOCALAPPDATA is required.' }
        $root = Join-Path $base 'Clearra/build'
    } elseif (-not [string]::IsNullOrWhiteSpace($env:WSL_DISTRO_NAME) -or
              -not [string]::IsNullOrWhiteSpace($env:WSL_INTEROP)) {
        # WSL may not silently allocate another cache inside its VHDX.
        $windowsBase = @(& cmd.exe /d /c 'echo %LOCALAPPDATA%' 2>$null)
        if ($LASTEXITCODE -ne 0 -or $windowsBase.Count -ne 1 -or
            ([string]$windowsBase[0]).Trim() -notmatch '^[A-Za-z]:\\') {
            throw 'Cannot resolve Windows LOCALAPPDATA from WSL; use the managed Windows launcher.'
        }
        $windowsRoot = ([string]$windowsBase[0]).Trim().TrimEnd('\') + '\Clearra\build'
        $mapped = @(& wslpath -u $windowsRoot 2>$null)
        if ($LASTEXITCODE -ne 0 -or $mapped.Count -ne 1 -or ([string]$mapped[0]) -notmatch '^/mnt/[a-z]/') {
            throw 'WSL builds require the mounted Windows Clearra/build root.'
        }
        $root = [string]$mapped[0]
    } else {
        $base = $env:XDG_CACHE_HOME
        if ([string]::IsNullOrWhiteSpace($base)) {
            if ([string]::IsNullOrWhiteSpace($env:HOME)) { throw 'A Linux cache home is required.' }
            $base = Join-Path $env:HOME '.cache'
        }
        $root = Join-Path $base 'Clearra/build'
    }
    $root = Assert-ClearraNoReparseBuildPath $root
    if (-not [string]::IsNullOrWhiteSpace($env:CLEARRA_BUILD_ROOT)) {
        $configured = [IO.Path]::GetFullPath($env:CLEARRA_BUILD_ROOT).TrimEnd('\', '/')
        if (-not $root.Equals($configured, (Get-ClearraBuildPathComparison))) {
            throw "CLEARRA_BUILD_ROOT must equal the platform canonical root: $root"
        }
    }
    return $root
}

function Get-ClearraCanonicalSourceRoot([string]$RepositoryRoot = (Resolve-ClearraRoot), [switch]$AllowMissing) {
    $source = Assert-ClearraNoReparseBuildPath $RepositoryRoot
    if (-not $AllowMissing -and -not (Test-Path -LiteralPath $source -PathType Container)) {
        throw "The build source root does not exist: $source"
    }
    return $source
}

function Get-ClearraBuildSourceIdentity([string]$RepositoryRoot = (Resolve-ClearraRoot)) {
    return (Get-ClearraBuildMetadataSourceIdentity (Get-ClearraCanonicalSourceRoot $RepositoryRoot -AllowMissing))
}

function Assert-ClearraCanonicalBuildPath(
    [string]$Path,
    [string]$RepositoryRoot = (Resolve-ClearraRoot),
    [switch]$AllowRoot
) {
    if ([string]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathRooted($Path)) {
        throw 'An absolute canonical build path is required.'
    }
    $root = Get-ClearraCanonicalBuildRoot
    $candidate = Assert-ClearraNoReparseBuildPath $Path
    $comparison = Get-ClearraBuildPathComparison
    $prefix = $root + [IO.Path]::DirectorySeparatorChar
    if (-not $candidate.StartsWith($prefix, $comparison) -and
        -not ($AllowRoot -and $candidate.Equals($root, $comparison))) {
        throw "Build path is outside the canonical Clearra/build root: $candidate"
    }
    Assert-ClearraPathOutsideRepository $candidate $RepositoryRoot | Out-Null
    return $candidate
}

function Assert-ClearraPathInBuildTransaction(
    [string]$Path,
    [string]$TransactionRoot,
    [string]$RepositoryRoot = (Resolve-ClearraRoot)
) {
    $candidate = Assert-ClearraCanonicalBuildPath $Path $RepositoryRoot
    $transaction = Assert-ClearraCanonicalBuildPath $TransactionRoot $RepositoryRoot
    $comparison = Get-ClearraBuildPathComparison
    if (-not $candidate.StartsWith($transaction + [IO.Path]::DirectorySeparatorChar, $comparison)) {
        throw "Build path does not belong to the selected transaction: $candidate"
    }
    return $candidate
}

function Assert-ClearraRequestedBuildPath(
    [string]$Path,
    [string]$RepositoryRoot = (Resolve-ClearraBuildSourceRoot),
    [ValidateSet('experiment','product')][string]$Purpose = (Get-ClearraBuildPurpose)
) {
    if ([string]::IsNullOrWhiteSpace($Path)) { return $null }
    $source = Get-ClearraCanonicalSourceRoot $RepositoryRoot
    Assert-ClearraBuildEnvironmentBeforeMutation $source
    $absolute = [IO.Path]::IsPathRooted($Path)
    if ($absolute) {
        Assert-ClearraCanonicalBuildPath $Path $source | Out-Null
    } else {
        # Relative requests are transaction-local names, not navigational paths.
        # This check also works before a product generation has been selected.
        foreach ($segment in $Path.Replace('\','/').Split('/')) {
            if ([string]::IsNullOrWhiteSpace($segment) -or $segment -in @('.','..') -or
                $segment.IndexOfAny([IO.Path]::GetInvalidFileNameChars()) -ge 0 -or $segment.Contains(':')) {
                throw 'A requested build path must contain only transaction-local directory names.'
            }
        }
    }
    $selected = $null -ne $script:ClearraBuildTransaction -or
        -not [string]::IsNullOrWhiteSpace($env:CLEARRA_BUILD_SESSION_ID)
    if ($Purpose -eq 'product' -and -not $selected) {
        if ($absolute) { throw 'An absolute product build path requires an already selected product generation.' }
        Assert-ClearraCanonicalBuildPath (Get-ClearraCanonicalBuildRoot) $source -AllowRoot | Out-Null
        return $Path
    }
    $transaction = Get-ClearraBuildTransactionRoot -RepositoryRoot $source -Purpose $Purpose
    $candidate = if ($absolute) { $Path } else { Join-Path $transaction $Path }
    return (Assert-ClearraPathInBuildTransaction $candidate $transaction $source)
}
