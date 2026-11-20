# Pure fixture tests. Never launches Clearra, WebView2, or a native probe.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'windows-desktop-startup-proof.psm1')

$script:passed = 0
function Assert-Equal($Actual, $Expected, [string]$Label) {
    if ($Actual -cne $Expected) { throw "${Label}: expected '$Expected', got '$Actual'" }
    $script:passed++
}

Assert-Equal (Test-ClearraNativeX64DesktopHost $true 'X64' 'X64') $true 'native x64 Windows'
Assert-Equal (Test-ClearraNativeX64DesktopHost $true 'X64' 'Arm64') $false 'emulated x64 is not a native host'
Assert-Equal (Test-ClearraNativeX64DesktopHost $true 'X86' 'X64') $false '32-bit process is not the required host'
Assert-Equal (Test-ClearraNativeX64DesktopHost $false 'X64' 'X64') $false 'non-Windows x64 cannot provide this evidence'

$caseRoot = [IO.Path]::Combine([IO.Path]::GetTempPath(), 'clearra startup proof fixture')
$profile = [IO.Path]::Combine($caseRoot, 'webview2')
$otherProfile = [IO.Path]::Combine($caseRoot, 'another-webview2')
$epoch = [datetime]::new(2026, 11, 20, 2, 0, 0, [DateTimeKind]::Utc)

function New-ProcessEntry(
    [int]$Id, [int]$ParentId, [string]$Name, [int]$Age, [bool]$InJob = $true,
    [string]$Role = '', [string]$Profile = ''
) {
    @{
        Id = $Id; ParentId = $ParentId; Name = $Name; StartedUtc = $epoch.AddMilliseconds($Age)
        InJob = $InJob
        Role = if ($Role -eq '') { $null } else { $Role }
        Profile = if ($Profile -eq '') { $null } else { $Profile }
    }
}
function New-HealthyTree {
    @{
        10 = (New-ProcessEntry 10 5 'pwsh.exe' -5)
        20 = (New-ProcessEntry 20 10 'Clearra-GUI-v0.9.0-windows-x86_64.exe' 0)
        30 = (New-ProcessEntry 30 20 'msedgewebview2.exe' 1 $true 'browser' $profile)
        31 = (New-ProcessEntry 31 30 'msedgewebview2.exe' 2 $true 'utility')
        40 = (New-ProcessEntry 40 31 'msedgewebview2.exe' 3 $true 'renderer')
    }
}
function Get-Readiness([hashtable]$Processes, [bool]$Visible = $true) {
    Get-ClearraDesktopStartupReadinessReason (
        Get-ClearraDesktopStartupObservation $Processes 20 10 $epoch $profile $Visible
    )
}

$processes = New-HealthyTree
Assert-Equal (Get-Readiness $processes) 'verified' 'fresh private renderer and visible window'
Assert-Equal (Get-Readiness $processes $false) 'gui-window-not-observed' 'renderer alone is not window evidence'

$processes = New-HealthyTree
$processes.Remove(40)
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'only browser and utility'
$processes = New-HealthyTree
$processes[40].InJob = $false
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'external renderer'
$processes = New-HealthyTree
$processes[30].InJob = $false
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'browser outside the private job'
$processes = New-HealthyTree
$processes[30].ParentId = 10
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'job-owned sibling is not GUI-owned'
$processes = New-HealthyTree
$processes[20].ParentId = 5
Assert-Equal (Get-Readiness $processes) 'gui-process-not-job-owned' 'GUI is not a direct child of probe'
$processes = New-HealthyTree
$processes[20].InJob = $false
Assert-Equal (Get-Readiness $processes) 'gui-process-not-job-owned' 'GUI outside the private job'
$processes = New-HealthyTree
$processes[30].Profile = $otherProfile
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'outside profile'
$processes = New-HealthyTree
$processes[30].Profile = $profile + '-other'
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'prefix is not an exact profile'
$processes = New-HealthyTree
$processes[30].Profile = 'webview2'
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'relative profile not accepted'
$processes = New-HealthyTree
$processes[30].Profile = [IO.Path]::Combine($caseRoot, 'unverified', '..', 'webview2')
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'path alias is not exact profile evidence'
$processes = New-HealthyTree
$processes[40].Profile = $otherProfile
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'renderer explicit profile disagrees'
$processes = New-HealthyTree
$processes[30].StartedUtc = $epoch.AddMilliseconds(-1)
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'pre-existing browser'
$processes = New-HealthyTree
$processes[31].StartedUtc = $epoch.AddMilliseconds(4)
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'PID reuse changes ancestry chronology'
$processes = New-HealthyTree
$processes[31].ParentId = 40
$processes[31].StartedUtc = $processes[40].StartedUtc
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'same-timestamp cycle'
$processes = New-HealthyTree
$processes.Remove(31)
Assert-Equal (Get-Readiness $processes) 'isolated-webview2-renderer-not-observed' 'missing intermediate process'

$argsEvidence = Get-ClearraDesktopWebViewArgumentEvidence @('msedgewebview2.exe', '--embedded-browser-webview=1', "--user-data-dir=$profile")
Assert-Equal $argsEvidence.Role 'browser' 'browser is the no-type Chromium process'
Assert-Equal $argsEvidence.Profile $profile 'exact joined profile in one parsed argv element'
$argsEvidence = Get-ClearraDesktopWebViewArgumentEvidence @('msedgewebview2.exe', '--type=renderer', '--user-data-dir', $profile)
Assert-Equal $argsEvidence.Role 'renderer' 'renderer flag'
Assert-Equal $argsEvidence.Profile $profile 'exact spaced profile after separated flag'
Assert-Equal (Get-ClearraDesktopWebViewArgumentEvidence @('exe', '--type=renderer', '--type=utility')) $null 'conflicting renderer role'
Assert-Equal (Get-ClearraDesktopWebViewArgumentEvidence @('exe', "--user-data-dir=$profile", "--user-data-dir=$otherProfile")) $null 'conflicting profiles'
Assert-Equal (Get-ClearraDesktopWebViewArgumentEvidence @('exe', '--user-data-dir', '--embedded-browser-webview=1')) $null 'missing separated profile'
Assert-Equal (Get-ClearraDesktopWebViewArgumentEvidence @('exe', '--user-data-dir=')) $null 'empty joined profile'
Assert-Equal (Get-ClearraDesktopWebViewArgumentEvidence @('exe', '--type', 'renderer')) $null 'unknown type encoding'
$argsEvidence = Get-ClearraDesktopWebViewArgumentEvidence @('exe', '--', '--type=renderer', "--user-data-dir=$profile")
Assert-Equal $argsEvidence.Role 'browser' 'switch-like data after delimiter'
Assert-Equal $argsEvidence.Profile $null 'profile-like data after delimiter'

foreach ($name in @('windows-desktop-startup-proof.psm1', 'verify-windows-desktop-runtime.ps1', 'verify-windows-desktop-runtime.test.ps1')) {
    $tokens = $null
    $parseErrors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $PSScriptRoot $name), [ref]$tokens, [ref]$parseErrors
    )
    Assert-Equal $parseErrors.Count 0 "PowerShell parses $name"
}
"$script:passed focused Windows startup proof assertions passed (no GUI/native launch)"
