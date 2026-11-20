Set-StrictMode -Version Latest

# Pure checks. A process must be fresh and reachable in the same exact job
# snapshot. PID alone is insufficient when a process exited between samples.
function Test-ClearraDesktopOwnedDescendant {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][System.Collections.IDictionary]$Processes,
        [Parameter(Mandatory)][int]$ProcessId,
        [Parameter(Mandatory)][int]$OwnerId,
        [Parameter(Mandatory)][datetime]$FreshSinceUtc,
        [int]$ExpectedOwnerPid = 0
    )
    if ($ProcessId -eq $OwnerId -or -not $Processes.Contains($OwnerId)) { return $false }
    $seen = [System.Collections.Generic.HashSet[int]]::new()
    $next = $ProcessId
    $childTime = [datetime]::MaxValue
    while ($next -ne 0 -and $seen.Add($next) -and $Processes.Contains($next)) {
        $process = $Processes[$next]
        if (-not $process.InJob -or $process.StartedUtc -lt $FreshSinceUtc -or
            $process.StartedUtc -gt $childTime) { return $false }
        if ($next -eq $OwnerId) {
            return $ExpectedOwnerPid -eq 0 -or $process.ParentId -eq $ExpectedOwnerPid
        }
        $childTime = $process.StartedUtc
        $next = $process.ParentId
    }
    return $false
}

function Get-ClearraDesktopWebViewArgumentEvidence {
    [CmdletBinding()]
    param([Parameter(Mandatory)][AllowEmptyCollection()][string[]]$Arguments)
    # Chromium flags before '--' only. Ambiguous/repeated selectors cannot
    # prove an isolated WebView even when one spelling happens to match.
    $types = [System.Collections.Generic.List[string]]::new()
    $profiles = [System.Collections.Generic.List[string]]::new()
    for ($i = 1; $i -lt $Arguments.Count; $i++) {
        $arg = $Arguments[$i]
        if ($arg -ceq '--') { break }
        if ($arg.StartsWith('--type=', [StringComparison]::Ordinal)) {
            $types.Add($arg.Substring('--type='.Length))
        } elseif ($arg.StartsWith('--user-data-dir=', [StringComparison]::Ordinal)) {
            $profiles.Add($arg.Substring('--user-data-dir='.Length))
        } elseif ($arg -ceq '--user-data-dir') {
            if ($i + 1 -ge $Arguments.Count -or $Arguments[$i + 1].StartsWith('--', [StringComparison]::Ordinal)) {
                return $null
            }
            $i++
            $profiles.Add($Arguments[$i])
        } elseif ($arg -ceq '--type') {
            return $null
        }
    }
    if ($types.Count -gt 1 -or $profiles.Count -gt 1 -or
        $types.Contains('') -or $profiles.Contains('')) { return $null }
    return @{
        Role = if ($types.Count -eq 0) { 'browser' } else { $types[0] }
        Profile = if ($profiles.Count -eq 0) { $null } else { $profiles[0] }
    }
}

function Test-ClearraDesktopProfileMatch([string]$Actual, [string]$Expected) {
    if ([string]::IsNullOrWhiteSpace($Actual) -or [string]::IsNullOrWhiteSpace($Expected)) {
        return $false
    }
    try {
        if (-not [IO.Path]::IsPathFullyQualified($Actual) -or -not [IO.Path]::IsPathFullyQualified($Expected)) {
            return $false
        }
        $fullActual = [IO.Path]::GetFullPath($Actual).TrimEnd('\', '/')
        $fullExpected = [IO.Path]::GetFullPath($Expected).TrimEnd('\', '/')
        $comparison = [StringComparison]::OrdinalIgnoreCase
        # No unresolved path aliases in runtime evidence (e.g. a path with
        # an earlier directory junction followed by '..').
        return [string]::Equals($Actual.TrimEnd('\', '/'), $fullActual, $comparison) -and
            [string]::Equals($fullActual, $fullExpected, $comparison)
    } catch { return $false }
}

function Get-ClearraDesktopStartupObservation {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][System.Collections.IDictionary]$Processes,
        [Parameter(Mandatory)][int]$OwnerId,
        [Parameter(Mandatory)][int]$SupervisorChildId,
        [Parameter(Mandatory)][datetime]$FreshSinceUtc,
        [Parameter(Mandatory)][string]$IsolatedProfile,
        [Parameter(Mandatory)][bool]$WindowObserved
    )
    # These are sampled facts, not authority that an application page loaded.
    $ownsProcess = $Processes.Contains($OwnerId) -and $Processes[$OwnerId].InJob -and
        $Processes[$OwnerId].StartedUtc -ge $FreshSinceUtc -and
        $Processes[$OwnerId].ParentId -eq $SupervisorChildId
    $ownsRenderer = $false
    if ($ownsProcess) {
        $browsers = @{}
        foreach ($item in $Processes.Values) {
            if ($item.Name -ieq 'msedgewebview2.exe' -and $item.Role -ceq 'browser' -and
                (Test-ClearraDesktopProfileMatch $item.Profile $IsolatedProfile) -and
                (Test-ClearraDesktopOwnedDescendant $Processes $item.Id $OwnerId $FreshSinceUtc $SupervisorChildId)) {
                $browsers[$item.Id] = $true
            }
        }
        foreach ($item in $Processes.Values) {
            if ($item.Name -ine 'msedgewebview2.exe' -or $item.Role -cne 'renderer' -or
                ($null -ne $item.Profile -and -not (Test-ClearraDesktopProfileMatch $item.Profile $IsolatedProfile))) {
                continue
            }
            foreach ($browserId in $browsers.Keys) {
                if (Test-ClearraDesktopOwnedDescendant $Processes $item.Id $browserId $FreshSinceUtc) {
                    $ownsRenderer = $true
                    break
                }
            }
            if ($ownsRenderer) { break }
        }
    }
    return @{
        gui_process_owned = $ownsProcess
        visible_clearra_window = $ownsProcess -and $WindowObserved
        isolated_webview2_renderer_owned = $ownsRenderer
    }
}

function Test-ClearraNativeX64DesktopHost([bool]$Windows, [string]$ProcessArchitecture, [string]$OsArchitecture) {
    return $Windows -and $ProcessArchitecture -ceq 'X64' -and $OsArchitecture -ceq 'X64'
}

function Get-ClearraDesktopStartupReadinessReason([hashtable]$Observation) {
    if (-not $Observation.gui_process_owned) { return 'gui-process-not-job-owned' }
    if (-not $Observation.visible_clearra_window) { return 'gui-window-not-observed' }
    if (-not $Observation.isolated_webview2_renderer_owned) { return 'isolated-webview2-renderer-not-observed' }
    return 'verified'
}

Export-ModuleMember -Function Test-ClearraDesktopOwnedDescendant, Get-ClearraDesktopWebViewArgumentEvidence,
    Get-ClearraDesktopStartupObservation, Get-ClearraDesktopStartupReadinessReason, Test-ClearraNativeX64DesktopHost
