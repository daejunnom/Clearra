# Runtime environment selection is separate from the Windows application-control
# execution surface. Selection is explicit; it is never a backend fallback.

function Resolve-ClearraRuntimeEnvironment([AllowNull()][string]$Environment) {
    $candidate = if (-not [string]::IsNullOrWhiteSpace($Environment)) {
        $Environment
    } elseif (-not [string]::IsNullOrWhiteSpace($env:CLEARRA_RUNTIME_ENVIRONMENT)) {
        $env:CLEARRA_RUNTIME_ENVIRONMENT
    } else {
        'auto'
    }
    $candidate = $candidate.Trim().ToLowerInvariant()
    if ($candidate -notin @('auto', 'windows', 'wsl', 'wasm')) {
        throw "Unknown Clearra runtime environment '$candidate'. Expected auto, windows, wsl, or wasm."
    }
    if ($candidate -ne 'auto') {
        return $candidate
    }
    if ($env:WSL_DISTRO_NAME -or $env:WSL_INTEROP) {
        return 'wsl'
    }
    if ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT) {
        return 'windows'
    }
    throw 'Automatic runtime selection supports Windows and WSL only; select wasm explicitly for browser artifacts.'
}

function Assert-ClearraRuntimeEnvironmentAvailable(
    [string]$Environment,
    [string]$WslDistribution = 'Ubuntu'
) {
    $resolved = Resolve-ClearraRuntimeEnvironment $Environment
    switch ($resolved) {
        'windows' {
            if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
                throw 'The windows runtime environment requires Windows.'
            }
        }
        'wsl' {
            if ($env:WSL_DISTRO_NAME -or $env:WSL_INTEROP) {
                break
            }
            $wsl = Get-Command 'wsl.exe' -ErrorAction SilentlyContinue
            if ($null -eq $wsl) {
                throw 'The requested WSL runtime environment is unavailable.'
            }
            $previousPreference = $ErrorActionPreference
            $ErrorActionPreference = 'Continue'
            try {
                & $wsl.Source -d $WslDistribution -- bash -lc 'test -n "$WSL_DISTRO_NAME" && test "$(stat -f -c %T "$HOME")" != v9fs' 2>$null
                $exitCode = $LASTEXITCODE
            } finally {
                $ErrorActionPreference = $previousPreference
            }
            if ($exitCode -ne 0) {
                throw "The requested WSL2 distribution is unavailable or has no Linux filesystem: $WslDistribution"
            }
        }
        'wasm' {
            if ($null -eq (Get-Command 'node' -ErrorAction SilentlyContinue)) {
                throw "The local WASM command host requires 'node' on PATH; deployed browser execution does not require Cargo or wasm-bindgen."
            }
        }
    }
    return $resolved
}

function Get-ClearraStableDigest([string[]]$Lines) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes(($Lines -join "`n"))
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Get-ClearraWslSourceManifest([string]$RepositoryRoot) {
    $root = [System.IO.Path]::GetFullPath($RepositoryRoot).TrimEnd('\', '/')
    $entries = [System.Collections.Generic.List[object]]::new()
    foreach ($file in Get-ClearraBuildInputFiles $root) {
        $relative = $file.FullName.Substring($root.Length).TrimStart('\', '/').Replace('\', '/')
        $entries.Add([pscustomobject]@{
                relative_path = $relative
                full_path = $file.FullName
                sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                size = $file.Length
            })
    }
    $cargoConfig = Join-Path $root '.cargo/config.toml'
    if (Test-Path -LiteralPath $cargoConfig -PathType Leaf) {
        $file = [System.IO.FileInfo]::new($cargoConfig)
        $entries.Add([pscustomobject]@{
                relative_path = '.cargo/config.toml'
                full_path = $file.FullName
                sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                size = $file.Length
            })
    }
    $ordered = @($entries | Sort-Object relative_path -Unique)
    $digestLines = @($ordered | ForEach-Object {
            "$($_.relative_path)|$($_.size)|$($_.sha256)"
        })
    return [pscustomobject]@{
        files = $ordered
        source_digest = Get-ClearraStableDigest $digestLines
    }
}

function Invoke-ClearraWslTarArchive(
    [string]$WslDistribution,
    [string]$ArchivePath,
    [string]$LinuxDestination
) {
    if ($WslDistribution -notmatch '^[A-Za-z0-9._-]+$') {
        throw "Unsafe WSL distribution name: $WslDistribution"
    }
    if ($LinuxDestination -notmatch '^/home/[^/]+/\.local/share/Clearra/workspaces/[0-9a-f]{16}/source\.next$') {
        throw "Unsafe WSL extraction destination: $LinuxDestination"
    }
    $archive = [System.IO.Path]::GetFullPath($ArchivePath)
    if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
        throw "WSL source archive does not exist: $archive"
    }
    $wsl = (Get-Command 'wsl.exe' -ErrorAction Stop).Source
    $command = '""{0}" -d {1} -- tar -xf - -C "{2}" < "{3}""' -f `
        $wsl, $WslDistribution, $LinuxDestination, $archive
    & $env:ComSpec /d /s /c $command
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to stream the source package into WSL (exit $LASTEXITCODE)."
    }
}

function Copy-ClearraWslFileToWindows(
    [string]$WslDistribution,
    [string]$LinuxSource,
    [string]$WindowsDestination,
    [switch]$AllowEmpty
) {
    if ($WslDistribution -notmatch '^[A-Za-z0-9._-]+$') {
        throw "Unsafe WSL distribution name: $WslDistribution"
    }
    if ($LinuxSource -notmatch '^/home/[A-Za-z0-9._-]+/[A-Za-z0-9._/-]+$' -or
        $LinuxSource -match '(^|/)\.\.(/|$)') {
        throw "Unsafe WSL artifact source: $LinuxSource"
    }
    $destination = [System.IO.Path]::GetFullPath($WindowsDestination)
    $parent = Split-Path -Parent $destination
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $temporary = "$destination.partial-$PID-$([Guid]::NewGuid().ToString('N'))"
    $wsl = (Get-Command 'wsl.exe' -ErrorAction Stop).Source
    try {
        $command = '""{0}" -d {1} -- cat -- "{2}" > "{3}""' -f `
            $wsl, $WslDistribution, $LinuxSource, $temporary
        & $env:ComSpec /d /s /c $command
        if ($LASTEXITCODE -ne 0 -or
            -not (Test-Path -LiteralPath $temporary -PathType Leaf) -or
            ((Get-Item -LiteralPath $temporary).Length -eq 0 -and -not $AllowEmpty.IsPresent)) {
            throw "Failed to stream the WSL artifact to Windows (exit $LASTEXITCODE)."
        }
        Move-Item -LiteralPath $temporary -Destination $destination -Force
    } finally {
        if (Test-Path -LiteralPath $temporary) {
            Remove-Item -LiteralPath $temporary -Force
        }
    }
    return $destination
}

function Sync-ClearraWslExt4Workspace(
    [string]$RepositoryRoot,
    [string]$WslDistribution = 'Ubuntu'
) {
    Assert-ClearraRuntimeEnvironmentAvailable 'wsl' $WslDistribution | Out-Null
    $root = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $manifest = Get-ClearraWslSourceManifest $root
    $workspaceId = (Get-ClearraStableDigest @($root.ToLowerInvariant())).Substring(0, 16)
    $linuxHome = (& wsl.exe -d $WslDistribution -- sh -c 'printf %s "$HOME"' | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $linuxHome -notmatch '^/home/[^/]+$') {
        throw "Could not resolve a safe Linux home in WSL: $linuxHome"
    }
    $linuxBase = "$linuxHome/.local/share/Clearra/workspaces/$workspaceId"
    $linuxWorkspace = "$linuxBase/source"
    if ($linuxBase -notmatch '^/home/[^/]+/\.local/share/Clearra/workspaces/[0-9a-f]{16}$') {
        throw "Refusing an unexpected WSL workspace path: $linuxBase"
    }
    $digestMarker = "$linuxWorkspace/.clearra-source-digest"
    & wsl.exe -d $WslDistribution -- test -f $digestMarker
    $markerExists = $LASTEXITCODE -eq 0
    $currentDigest = if ($markerExists) {
        (& wsl.exe -d $WslDistribution -- cat $digestMarker | Out-String).Trim()
    } else {
        ''
    }
    if ($LASTEXITCODE -eq 0 -and $currentDigest -eq $manifest.source_digest) {
        return [pscustomobject]@{
            distribution = $WslDistribution
            workspace = $linuxWorkspace
            source_digest = $manifest.source_digest
            source_file_count = $manifest.files.Count
            sync_performed = $false
            filesystem = 'wsl-ext4'
        }
    }

    $artifactRoot = Get-ClearraArtifactRoot
    New-Item -ItemType Directory -Force -Path $artifactRoot | Out-Null
    $transaction = Join-Path $artifactRoot "wsl-sync-$PID-$([Guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force -Path $transaction | Out-Null
    $listPath = Join-Path $transaction 'source-files.txt'
    $archivePath = Join-Path $transaction 'source.tar'
    $digestPath = Join-Path $transaction '.clearra-source-digest'
    try {
        [System.IO.File]::WriteAllLines(
            $listPath,
            [string[]]@($manifest.files | ForEach-Object { $_.relative_path }),
            [System.Text.UTF8Encoding]::new($false)
        )
        $tar = Get-Command 'tar.exe' -ErrorAction Stop
        & $tar.Source -cf $archivePath -C $root -T $listPath
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to create the WSL source package (exit $LASTEXITCODE)."
        }
        [System.IO.File]::WriteAllText(
            $digestPath,
            $manifest.source_digest + "`n",
            [System.Text.UTF8Encoding]::new($false)
        )
        & $tar.Source -rf $archivePath -C $transaction '.clearra-source-digest'
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to add the WSL source digest (exit $LASTEXITCODE)."
        }

        $nextWorkspace = "$linuxBase/source.next"
        & wsl.exe -d $WslDistribution -- mkdir -p -- $linuxBase
        if ($LASTEXITCODE -ne 0) { throw "Failed to create WSL workspace root: $linuxBase" }
        & wsl.exe -d $WslDistribution -- rm -rf -- $nextWorkspace
        if ($LASTEXITCODE -ne 0) { throw "Failed to clear WSL staging workspace: $nextWorkspace" }
        & wsl.exe -d $WslDistribution -- mkdir -p -- $nextWorkspace
        if ($LASTEXITCODE -ne 0) { throw "Failed to create WSL staging workspace: $nextWorkspace" }
        Invoke-ClearraWslTarArchive $WslDistribution $archivePath $nextWorkspace
        & wsl.exe -d $WslDistribution -- rm -rf -- $linuxWorkspace
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to replace the prior WSL workspace: $linuxWorkspace"
        }
        & wsl.exe -d $WslDistribution -- mv -- $nextWorkspace $linuxWorkspace
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to activate the WSL workspace: $linuxWorkspace"
        }
    } finally {
        if (Test-Path -LiteralPath $transaction) {
            Remove-Item -LiteralPath $transaction -Recurse -Force
        }
    }
    return [pscustomobject]@{
        distribution = $WslDistribution
        workspace = $linuxWorkspace
        source_digest = $manifest.source_digest
        source_file_count = $manifest.files.Count
        sync_performed = $true
        filesystem = 'wsl-ext4'
    }
}
