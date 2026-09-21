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
    [string]$WslDistribution = 'Clearra-Build'
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
            if ($WslDistribution -cne 'Clearra-Build') {
                throw 'Only the managed Clearra-Build distribution is available to Clearra.'
            }
            if ($null -eq (Get-Command 'python' -ErrorAction SilentlyContinue)) {
                throw 'The managed WSL runtime requires Python on the Windows host.'
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

function Sync-ClearraWslExt4Workspace(
    [string]$RepositoryRoot,
    [string]$WslDistribution = 'Clearra-Build'
) {
    if ($WslDistribution -cne 'Clearra-Build') {
        throw 'Only the managed Clearra-Build distribution is available to Clearra.'
    }
    $root = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $python = (Get-Command 'python' -ErrorAction Stop).Source
    $arguments = New-ClearraManagedWslEntryArguments `
        -RepositoryRoot $root -Entry 'sync-workspace'
    $output = @(& $python @arguments 2>&1)
    $exitCode = $LASTEXITCODE
    $text = ($output | ForEach-Object { $_.ToString() }) -join "`n"
    if ($exitCode -ne 0) { throw "Managed WSL source synchronization failed ($exitCode).`n$text" }
    $workspaceMatch = [regex]::Match($text, '(?m)^clearra_wsl_source=(?<path>/home/clearra/\.local/share/Clearra/workspaces/[0-9a-f]{64}/source)\s*$')
    $digestMatch = [regex]::Match($text, '"source_digest"\s*:\s*"(?<digest>[0-9a-f]{64})"')
    $countMatch = [regex]::Match($text, '"source_file_count"\s*:\s*(?<count>[0-9]+)')
    if (-not $workspaceMatch.Success -or -not $digestMatch.Success -or -not $countMatch.Success) {
        throw 'Managed WSL synchronization did not emit its bounded source receipt.'
    }
    return [pscustomobject]@{
        distribution = 'Clearra-Build'
        workspace = $workspaceMatch.Groups['path'].Value
        source_digest = $digestMatch.Groups['digest'].Value
        source_file_count = [int]$countMatch.Groups['count'].Value
        sync_performed = $true
        filesystem = 'wsl-ext4'
    }
}
