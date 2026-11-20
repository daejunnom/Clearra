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
    [string]$WslDistribution
) {
    $resolved = Resolve-ClearraRuntimeEnvironment $Environment
    switch ($resolved) {
        'windows' {
            if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
                throw 'The windows runtime environment requires Windows.'
            }
        }
        'wsl' {
            [void](New-ClearraWslDistributionArguments $WslDistribution)
            if (($env:WSL_DISTRO_NAME -or $env:WSL_INTEROP) -and (
                [string]::IsNullOrEmpty($WslDistribution) -or
                [string]::Equals($env:WSL_DISTRO_NAME, $WslDistribution, [StringComparison]::OrdinalIgnoreCase)
            )) {
                break
            }
            $repositoryRoot = [System.IO.Path]::GetFullPath(
                (Join-Path $script:ClearraWslDispatchRoot '../..')
            )
            [void](Get-ClearraManageExecutable -RepositoryRoot $repositoryRoot)
        }
        'wasm' {
            if ($null -eq (Get-Command 'node' -ErrorAction SilentlyContinue)) {
                throw "The local WASM command host requires 'node' on PATH; deployed browser execution does not require Cargo or wasm-bindgen."
            }
        }
    }
    return $resolved
}

function ConvertFrom-ClearraWslSourceReceipt(
    [string]$Text,
    [string]$ExpectedDistribution
) {
    # Accept only one complete record on its own line. Runtime/log output
    # is not authority for any subset of these fields.
    $lines = @([regex]::Split($Text, '\r\n|\n|\r') | Where-Object {
        $_ -match '^\s*clearra_wsl_source_receipt'
    })
    if ($lines.Count -ne 1) {
        throw 'Managed WSL synchronization did not emit exactly one bounded source receipt.'
    }
    $pattern = '\Aclearra_wsl_source_receipt_v2=(?<distribution>[^\p{Cc}]{1,255})\t(?<workspace>/[^\p{Cc}]{1,4096}/Clearra/wsl/workspaces/(?<key>[0-9a-f]{64})/source)\t(?<digest>[0-9a-f]{64})\t(?<count>[1-9][0-9]{0,9})\z'
    $receipt = [regex]::Match($lines[0], $pattern)
    $count = 0
    if (-not $receipt.Success -or
        $receipt.Groups['key'].Value -cne $receipt.Groups['digest'].Value -or
        $receipt.Groups['workspace'].Value -cmatch '\A/mnt(?:/|$)|//|/(?:\.{1,2})(?:/|$)|\\' -or
        -not [int]::TryParse($receipt.Groups['count'].Value, [ref]$count)) {
        throw 'Managed WSL synchronization emitted an invalid source receipt.'
    }
    $distribution = $receipt.Groups['distribution'].Value
    [void](New-ClearraWslDistributionArguments $distribution)
    if (-not [string]::IsNullOrEmpty($ExpectedDistribution) -and
        -not [string]::Equals($distribution, $ExpectedDistribution, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Managed WSL source receipt does not identify the selected distribution.'
    }
    return [pscustomobject]@{
        distribution = $distribution
        workspace = $receipt.Groups['workspace'].Value
        source_digest = $receipt.Groups['digest'].Value
        source_file_count = $count
        sync_performed = $true
        filesystem = 'wsl-ext4'
    }
}

function Sync-ClearraWslExt4Workspace(
    [string]$RepositoryRoot,
    [string]$WslDistribution
) {
    $root = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $manager = Get-ClearraManageExecutable -RepositoryRoot $root
    $arguments = New-ClearraManagedWslEntryArguments `
        -RepositoryRoot $root -Entry 'sync-workspace' -Distribution $WslDistribution
    $output = @(& $manager @arguments 2>&1)
    $exitCode = $LASTEXITCODE
    $text = ($output | ForEach-Object { $_.ToString() }) -join "`n"
    if ($exitCode -ne 0) { throw "Managed WSL source synchronization failed ($exitCode).`n$text" }
    return ConvertFrom-ClearraWslSourceReceipt $text $WslDistribution
}
