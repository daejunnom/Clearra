param(
    [Parameter(Mandatory = $true)]
    [string[]]$Path,

    [string]$ReportPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot "lib/clearra-path-helpers.ps1")
$ResolvedReportPath = Resolve-ClearraReportPath $ReportPath $Root
function Test-HasZoneIdentifier([string]$LiteralPath) {
    try {
        $stream = Get-Item -LiteralPath $LiteralPath -Stream Zone.Identifier -ErrorAction SilentlyContinue
        return $null -ne $stream
    } catch {
        return $false
    }
}function Test-IsLikelyUserWritableBuildPath([string]$LiteralPath) {
    $normalized = $LiteralPath.Replace('/', '\').ToLowerInvariant()
    return (
        (Test-ClearraPathInsideRepository $LiteralPath $Root) -or
        $normalized -match '\\target\\' -or
        $normalized -match '\\\.cargo\\target-tests\\' -or
        $normalized -match '\\core-c-build\\' -or
        $normalized -match '\\debug\\' -or
        $normalized -match '\\release\\'
    )
}function Test-SensitiveFileName([string]$Name) {
    return (
        $Name -match '^\.env(\.|$)' -or
        $Name -match '(?i)service[-_]?account.*\.json$' -or
        $Name -match '(?i)(api[-_]?key|credential|credentials|private[-_]?key)' -or
        $Name -match '(?i)\.(pem|key)$'
    )
}function Get-RelativePathOrFullName([string]$LiteralPath) {
    try {
        return Resolve-Path -LiteralPath $LiteralPath -Relative
    } catch {
        return $LiteralPath
    }
}function Get-AuthenticodeDiagnostic([System.IO.FileSystemInfo]$Item) {
    if ($Item.PSIsContainer) {
        return $null
    }
    if ($Item.Extension -notin @(".exe", ".dll", ".ps1", ".psm1", ".psd1", ".msi")) {
        return $null
    }
    if ($null -eq (Get-Command Get-AuthenticodeSignature -ErrorAction SilentlyContinue)) {
        return $null
    }

    return Get-AuthenticodeSignature -LiteralPath $Item.FullName
}function Get-FileDiagnostic([string]$LiteralPath) {
    $item = Get-Item -LiteralPath $LiteralPath -ErrorAction Stop
    if (Test-SensitiveFileName $item.Name) {
        return [ordered]@{
            path = $item.FullName
            relative_path = Get-RelativePathOrFullName $item.FullName
            exists = $true
            skipped = $true
            error = "sensitive file diagnostics are disabled"
        }
    }

    $signature = Get-AuthenticodeDiagnostic $item

    $hash = $null
    if (-not $item.PSIsContainer) {
        $hash = Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256
    }

    [ordered]@{
        path = $item.FullName
        relative_path = Get-RelativePathOrFullName $item.FullName
        exists = $true
        extension = $item.Extension
        length = if ($item.PSIsContainer) { $null } else { $item.Length }
        last_write_time = $item.LastWriteTimeUtc.ToString("o")
        sha256 = if ($hash) { $hash.Hash } else { $null }
        zone_identifier_present = Test-HasZoneIdentifier $item.FullName
        user_writable_build_path = Test-IsLikelyUserWritableBuildPath $item.FullName
        authenticode_status = if ($signature) { [string]$signature.Status } else { $null }
        authenticode_status_message = if ($signature) { $signature.StatusMessage } else { $null }
        signer_subject = if ($signature -and $signature.SignerCertificate) {
            $signature.SignerCertificate.Subject
        } else {
            $null
        }
        signer_thumbprint = if ($signature -and $signature.SignerCertificate) {
            $signature.SignerCertificate.Thumbprint
        } else {
            $null
        }
    }
}
$results = foreach ($pathValue in $Path) {
    if (Test-Path -LiteralPath $pathValue) {
        Get-FileDiagnostic $pathValue
    } else {
        [ordered]@{
            path = $pathValue
            exists = $false
            error = "file does not exist"
        }
    }
}

if ($ResolvedReportPath) {
    $parent = Split-Path -Parent $ResolvedReportPath
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $results | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ResolvedReportPath -Encoding UTF8
}

$results | ConvertTo-Json -Depth 8
