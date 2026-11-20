$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'clearra-build-wsl-dispatch.ps1')
. (Join-Path $PSScriptRoot 'clearra-runtime-environment.ps1')

$sha = 'a' * 64
$otherSha = 'b' * 64
$distribution = 'Ubuntu 24.04'
$workspace = "/home/worker/.local/share/Clearra/workspaces/$sha/source"
$prefix = "clearra_wsl_source_receipt_v2=$distribution"
$receipt = "$prefix`t$workspace`t$sha`t317"
$body = ConvertFrom-ClearraWslSourceReceipt "notice`r`n$receipt`r`nCLEARRA_WSL_RESULT=success`n"
if ($body.distribution -cne $distribution -or $body.workspace -cne $workspace -or
    $body.source_digest -cne $sha -or $body.source_file_count -ne 317 -or
    -not $body.sync_performed -or $body.filesystem -cne 'wsl-ext4') {
    throw 'The WSL receipt did not retain the public sync fields.'
}
[void](ConvertFrom-ClearraWslSourceReceipt $receipt 'ubuntu 24.04')

$badReceipts = @(
    'no receipt',
    "source_file_count=317`nclearra_wsl_source=$workspace",
    "$receipt`n$receipt",
    "$receipt`n$($receipt.Replace('_v2=', '_v1='))",
    "$receipt`n $receipt",
    $receipt.Replace("=$distribution", '=Other Distribution'),
    $receipt.Replace("=$distribution", '=--shutdown'),
    $receipt.Replace("=$distribution", '='),
    $receipt.Replace("=$distribution", '=- Invalid'),
    $receipt.Replace('/home/worker/', '/home/../worker/'),
    $receipt.Replace('/home/worker/', '/home//worker/'),
    $receipt.Replace('/home/worker/', 'home/worker/'),
    "$prefix`t$workspace`t$otherSha`t317",
    "$prefix`t$workspace`t$sha`t0",
    "$prefix`t$workspace`t$sha`t2147483648",
    "$receipt`textra=1"
)
foreach ($bad in $badReceipts) {
    $rejected = $false
    try {
        [void](ConvertFrom-ClearraWslSourceReceipt $bad $distribution)
    } catch {
        $rejected = $true
    }
    if (-not $rejected) { throw 'An invalid or duplicate WSL receipt was accepted.' }
}
Write-Output "source receipt parser: 2 valid and $($badReceipts.Count) rejected cases"
