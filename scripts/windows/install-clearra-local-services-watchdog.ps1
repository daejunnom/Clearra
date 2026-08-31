[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$SshKeyPath,

    [Parameter(Mandatory)]
    [string]$SshDestination,

    [string]$TaskName = "Clearra Local Services Watchdog"
)

$ErrorActionPreference = "Stop"
$sourceDirectory = Split-Path -Parent $PSCommandPath
$repoRoot = Split-Path -Parent (Split-Path -Parent $sourceDirectory)
$runtimeDirectory = Join-Path $env:LOCALAPPDATA "Clearra\startup-v2"
$watcherSource = Join-Path $sourceDirectory "clearra-local-services-watchdog.ps1"
$launcherSource = Join-Path $sourceDirectory "launch-clearra-local-services-watchdog.vbs"
$watcherTarget = Join-Path $runtimeDirectory "clearra-local-services-watchdog.ps1"
$launcherTarget = Join-Path $runtimeDirectory "launch-clearra-local-services-watchdog.vbs"
$configurationTarget = Join-Path $runtimeDirectory "clearra-local-services-watchdog.json"
$node = Get-Command node.exe -ErrorAction Stop
$nodePath = $node.Source
$npmCliPath = Join-Path (Split-Path -Parent $nodePath) "node_modules\npm\bin\npm-cli.js"

foreach ($path in @($watcherSource, $launcherSource, $nodePath, $npmCliPath, $SshKeyPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required local-services file is unavailable: $path"
    }
}
if (-not $SshDestination.Trim()) {
    throw "SshDestination must not be empty."
}

New-Item -ItemType Directory -Path $runtimeDirectory -Force | Out-Null
Copy-Item -LiteralPath $watcherSource -Destination $watcherTarget -Force
Copy-Item -LiteralPath $launcherSource -Destination $launcherTarget -Force

$configuration = [ordered]@{
    repo_root = $repoRoot
    node_path = $nodePath
    npm_cli_path = $npmCliPath
    ssh_path = "$env:WINDIR\System32\OpenSSH\ssh.exe"
    ssh_key_path = $SshKeyPath
    ssh_destination = $SshDestination
}
$configuration | ConvertTo-Json | Set-Content `
    -LiteralPath $configurationTarget `
    -Encoding UTF8

$legacyTaskNames = @("Clearra Local Runtime", "Clearra Local Services Watchdog")
foreach ($legacyTaskName in $legacyTaskNames) {
    $legacy = Get-ScheduledTask -TaskName $legacyTaskName -ErrorAction SilentlyContinue
    if ($null -eq $legacy) {
        continue
    }
    Stop-ScheduledTask -TaskName $legacyTaskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $legacyTaskName -Confirm:$false
}

$action = New-ScheduledTaskAction `
    -Execute "$env:WINDIR\System32\wscript.exe" `
    -Argument ('"{0}"' -f $launcherTarget) `
    -WorkingDirectory $runtimeDirectory
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$principal = New-ScheduledTaskPrincipal `
    -UserId $env:USERNAME `
    -LogonType Interactive `
    -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet `
    -MultipleInstances IgnoreNew `
    -Hidden `
    -ExecutionTimeLimit ([timespan]::Zero) `
    -RestartCount 3 `
    -RestartInterval (New-TimeSpan -Minutes 1)

Register-ScheduledTask `
    -TaskName $TaskName `
    -Action $action `
    -Trigger $trigger `
    -Principal $principal `
    -Settings $settings `
    -Description "Single-owner hidden watchdog for Clearra ports 4194 and 8790." `
    -Force | Out-Null
Start-ScheduledTask -TaskName $TaskName

Write-Output "Installed one hidden Clearra local-services watchdog with a 60-second poll."
