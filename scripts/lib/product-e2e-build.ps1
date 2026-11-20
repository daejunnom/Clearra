# Product E2E build and binary resolution stage.

function Resolve-ProductE2EBinary {
    if (-not [string]::IsNullOrWhiteSpace($ExePath)) {
        return $ExePath
    }

    $candidates = @()
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        $candidates += (Join-Path $env:CARGO_TARGET_DIR "release/clearra.exe")
        $candidates += (Join-Path $env:CARGO_TARGET_DIR "release/clearra")
    }

    $cacheTarget = Get-ClearraCargoTargetDir
    $candidates += (Join-Path $cacheTarget "release/clearra.exe")
    $candidates += (Join-Path $cacheTarget "release/clearra")

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    return (Join-Path (Get-ClearraCargoTargetDir) "release/clearra.exe")
}
