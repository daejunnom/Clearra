function Write-ProductE2EReport {
    if ([string]::IsNullOrWhiteSpace($ReportPath)) {
        return
    }
    $resolved = Resolve-ClearraReportPath $ReportPath $Root
    $parent = Split-Path -Parent $resolved
    if (-not [string]::IsNullOrWhiteSpace($parent) -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }

    [pscustomobject]@{
        schema_version = 1
        kind = "clearra-product-e2e-report"
        generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        case_count = $ProductResults.Count
        failed_count = @($ProductResults.ToArray() | Where-Object { $_.status -ne "passed" }).Count
        cases = @($ProductResults.ToArray())
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $resolved -Encoding UTF8
}
