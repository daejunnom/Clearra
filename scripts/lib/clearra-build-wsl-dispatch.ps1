$script:ClearraWslDispatchRoot = Split-Path -Parent $PSCommandPath
. (Join-Path $script:ClearraWslDispatchRoot 'clearra-manage-command.ps1')

function New-ClearraManagedWslEntryArguments(
    [Parameter(Mandatory = $true)][string]$RepositoryRoot,
    [Parameter(Mandatory = $true)]
    [ValidateSet(
        'sync-workspace',
        'wasm-build',
        'core-c-tests',
        'native-cargo',
        'oracle-local-layers-v080',
        'pc-runtime-build-batch',
        'posix-syntax-audit'
    )]
    [string]$Entry,
    [string[]]$CommandArguments = @(),
    [string]$Distribution
) {
    $root = [System.IO.Path]::GetFullPath($RepositoryRoot)
    foreach ($argument in $CommandArguments) {
        if ($null -eq $argument -or $argument -match '[\x00\r\n]') {
            throw 'Managed WSL entry arguments may not contain control characters.'
        }
    }
    $timeoutArguments = if ($Entry -eq 'pc-runtime-build-batch') { @('--timeout', '0') } else { @() }
    return [string[]]@(
        '--root', $root, 'runtime', 'wsl', 'run'
    ) + @(New-ClearraWslDistributionArguments $Distribution) +
        [string[]]@('--entry', $Entry) + $timeoutArguments + @('--') + [string[]]$CommandArguments
}

function New-ClearraWslDistributionArguments([string]$Distribution) {
    if ([string]::IsNullOrEmpty($Distribution)) {
        return
    }
    if ($Distribution.Length -gt 255 -or $Distribution -match '^[\s-]|\s$|[\p{Cc}"\\/]') {
        throw 'Invalid WSL distribution selector.'
    }
    return '--distribution', $Distribution
}
