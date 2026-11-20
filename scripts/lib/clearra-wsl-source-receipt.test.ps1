$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'clearra-runtime-environment.ps1')

$sha = 'a' * 64
$otherSha = 'b' * 64
$workspace = "/home/clearra/.local/share/Clearra/workspaces/$sha/source"
$prefix = 'clearra_wsl_source_receipt_v1=Clearra-Build'
$receipt = "$prefix`t$workspace`t$sha`t317"
$body = ConvertFrom-ClearraWslSourceReceipt "notice`r`n$receipt`r`nCLEARRA_WSL_RESULT=success`n"
if ($body.distribution -cne 'Clearra-Build' -or $body.workspace -cne $workspace -or
    $body.source_digest -cne $sha -or $body.source_file_count -ne 317 -or
    -not $body.sync_performed -or $body.filesystem -cne 'wsl-ext4') {
    throw 'The Clearra-Build receipt did not retain the public sync fields.'
}

$badReceipts = @(
    'no receipt',
    "source_file_count=317`nclearra_wsl_source=$workspace",
    "$receipt`n$receipt",
    "$receipt`n$($receipt.Replace('_v1=', '_v2='))",
    "$receipt`n $receipt",
    $receipt.Replace('=Clearra-Build', '=Ubuntu'),
    $receipt.Replace('/home/clearra/', '/home/other/'),
    "$prefix`t$workspace`t$otherSha`t317",
    "$prefix`t$workspace`t$sha`t0",
    "$prefix`t$workspace`t$sha`t2147483648",
    "$receipt`textra=1"
)
foreach ($bad in $badReceipts) {
    $rejected = $false
    try {
        [void](ConvertFrom-ClearraWslSourceReceipt $bad)
    } catch {
        $rejected = $true
    }
    if (-not $rejected) { throw 'An invalid or duplicate Clearra-Build receipt was accepted.' }
}
Write-Output "source receipt parser: 1 valid and $($badReceipts.Count) rejected cases"
