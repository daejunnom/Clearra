function Get-ClearraManageExecutable(
    [Parameter(Mandatory = $true)][string]$RepositoryRoot
) {
    $root = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $name = if ($IsWindows -or $env:OS -eq 'Windows_NT') {
        'clearra-manage.exe'
    } else {
        'clearra-manage'
    }
    $candidates = @()
    if (-not [string]::IsNullOrWhiteSpace($env:CLEARRA_MANAGE_BIN)) {
        $candidates += $env:CLEARRA_MANAGE_BIN
    }
    $candidates += @(
        (Join-Path $root "build/cargo/default/release/$name"),
        (Join-Path $root "build/cargo/default/debug/$name"),
        (Join-Path $root "build/tools/clearra-manage/host/release/$name")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return [System.IO.Path]::GetFullPath($candidate)
        }
    }
    $command = Get-Command 'clearra-manage' -ErrorAction SilentlyContinue
    if ($null -ne $command) { return $command.Source }
    throw @"
The Rust Clearra manager is not built. Run:
  cargo build --locked -p clearra-manage --release
The binary will be written under build/cargo/default/release.
"@
}
