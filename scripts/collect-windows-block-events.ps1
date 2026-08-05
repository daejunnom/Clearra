param(
    [int]$Minutes = 60,

    [int]$MaxEventsPerLog = 50,

    [int]$MessageExcerptLength = 800,

    [string]$ReportPath,

    [string[]]$Keywords = @(
        "Clearra",
        "target",
        ".cargo",
        "core-c-build",
        "clearra.exe",
        "clearra-cli.exe"
    )
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot "lib/clearra-path-helpers.ps1")
$ResolvedReportPath = Resolve-ClearraReportPath $ReportPath $Root
function Get-BlockEventLogs {
    $patterns = @("*CodeIntegrity*", "*SmartScreen*", "*AppControl*")
    $seen = New-Object System.Collections.Generic.HashSet[string]
    $logs = New-Object System.Collections.Generic.List[object]

    foreach ($pattern in $patterns) {
        $matchedLogs = @(Get-WinEvent -ListLog $pattern -ErrorAction SilentlyContinue)
        foreach ($log in $matchedLogs) {
            if ($null -eq $log.LogName -or -not $seen.Add([string]$log.LogName)) {
                continue
            }
            $logs.Add([ordered]@{
                log_name = [string]$log.LogName
                record_count = $log.RecordCount
            })
        }
    }

    return $logs.ToArray()
}function Get-MatchedKeywords([string]$Message, [string[]]$KeywordList) {
    $matches = New-Object System.Collections.Generic.List[string]
    foreach ($keyword in $KeywordList) {
        if ([string]::IsNullOrWhiteSpace($keyword)) {
            continue
        }
        if ($Message.IndexOf($keyword, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
            $matches.Add($keyword)
        }
    }
    return $matches.ToArray()
}function Get-MessageExcerpt([string]$Message, [int]$MaxLength) {
    if ([string]::IsNullOrWhiteSpace($Message)) {
        return ""
    }
    $singleLine = (($Message -replace "\s+", " ").Trim())
    if ($MaxLength -le 0 -or $singleLine.Length -le $MaxLength) {
        return $singleLine
    }
    return "$($singleLine.Substring(0, $MaxLength))..."
}function Get-RecentLogEvents(
    [string]$LogName,
    [datetime]$Since,
    [int]$Limit
) {
    try {
        return @{
            error = $null
            events = @(Get-WinEvent -FilterHashtable @{ LogName = $LogName; StartTime = $Since } -MaxEvents $Limit -ErrorAction Stop)
        }
    } catch {
        $message = $_.Exception.Message
        if ($message -like "*No events were found*" -or
            $message -like "*specified selection criteria*" -or
            $message -like "*지정된 선택 조건*") {
            return @{
                error = $null
                events = @()
            }
        }
        return @{
            error = $message
            events = @()
        }
    }
}
$since = (Get-Date).AddMinutes(-1 * $Minutes)
$availableLogs = @(Get-BlockEventLogs)
$matchedEvents = New-Object System.Collections.Generic.List[object]
$logErrors = New-Object System.Collections.Generic.List[object]

foreach ($log in $availableLogs) {
    $queryResult = Get-RecentLogEvents ([string]$log.log_name) $since $MaxEventsPerLog
    if ($null -ne $queryResult.error) {
        $logErrors.Add([ordered]@{
            log_name = [string]$log.log_name
            error = [string]$queryResult.error
        })
        continue
    }

    foreach ($event in @($queryResult.events)) {
        $message = if ($null -eq $event.Message) { "" } else { [string]$event.Message }
        $matchedKeywords = @(Get-MatchedKeywords $message $Keywords)
        if ($matchedKeywords.Count -eq 0) {
            continue
        }

        $matchedEvents.Add([ordered]@{
            time_created = if ($null -eq $event.TimeCreated) { $null } else { $event.TimeCreated.ToUniversalTime().ToString("o") }
            id = $event.Id
            provider_name = $event.ProviderName
            log_name = [string]$log.log_name
            matched_keywords = $matchedKeywords
            message_excerpt = Get-MessageExcerpt $message $MessageExcerptLength
        })
    }
}

$availableLogItems = @($availableLogs)
$logErrorItems = @($logErrors.ToArray())
$matchedEventItems = @($matchedEvents.ToArray())
$keywordItems = @($Keywords)

$report = [ordered]@{
    status = "ok"
    evidence_role = "supporting-only"
    events_are_not_conclusive = $true
    minutes = $Minutes
    since_utc = $since.ToUniversalTime().ToString("o")
    max_events_per_log = $MaxEventsPerLog
    keyword_filter = $keywordItems
    available_log_count = $availableLogItems.Count
    available_logs = $availableLogItems
    log_error_count = $logErrorItems.Count
    log_errors = $logErrorItems
    matched_event_count = $matchedEventItems.Count
    matched_events = $matchedEventItems
}

$json = $report | ConvertTo-Json -Depth 10

if (-not [string]::IsNullOrWhiteSpace($ResolvedReportPath)) {
    $reportDirectory = Split-Path -Parent $ResolvedReportPath
    if (-not [string]::IsNullOrWhiteSpace($reportDirectory) -and
        -not (Test-Path -LiteralPath $reportDirectory)) {
        New-Item -ItemType Directory -Path $reportDirectory | Out-Null
    }

    $json | Set-Content -LiteralPath $ResolvedReportPath -Encoding UTF8
}

$json
