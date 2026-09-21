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
    [string[]]$CommandArguments = @()
) {
    $root = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $manager = Join-Path $root 'scripts/management/clearra_manage.py'
    if (-not (Test-Path -LiteralPath $manager -PathType Leaf)) {
        throw 'The Clearra runtime manager is missing from the selected repository.'
    }
    foreach ($argument in $CommandArguments) {
        if ($null -eq $argument -or $argument -match '[\x00\r\n]') {
            throw 'Managed WSL entry arguments may not contain control characters.'
        }
    }
    return [string[]]@(
        '-B', $manager,
        'runtime', 'wsl', 'run', '--entry', $Entry, '--'
    ) + [string[]]$CommandArguments
}
