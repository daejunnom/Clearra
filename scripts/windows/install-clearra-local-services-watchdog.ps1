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

function Get-ListenerOwner {
    param([Parameter(Mandatory)][int]$Port)

    $listener = Get-NetTCPConnection -State Listen -LocalPort $Port `
        -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($null -eq $listener) {
        return $null
    }
    return [int]$listener.OwningProcess
}

foreach ($path in @($watcherSource, $launcherSource, $nodePath, $npmCliPath, $SshKeyPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required local-services file is unavailable: $path"
    }
}
if (-not $SshDestination.Trim()) {
    throw "SshDestination must not be empty."
}

$configuration = [ordered]@{
    repo_root = $repoRoot
    node_path = $nodePath
    npm_cli_path = $npmCliPath
    ssh_path = "$env:WINDIR\System32\OpenSSH\ssh.exe"
    ssh_key_path = $SshKeyPath
    ssh_destination = $SshDestination
}

# Stage each next-start artifact under a unique sibling name before replacing
# its destination. The currently running watchdog has already loaded its
# script/configuration, so this does not disturb its process or its children.
New-Item -ItemType Directory -Path $runtimeDirectory -Force | Out-Null
$stageId = [guid]::NewGuid().ToString("N")
$watcherStaged = Join-Path $runtimeDirectory ".$stageId.watchdog.tmp"
$launcherStaged = Join-Path $runtimeDirectory ".$stageId.launcher.tmp"
$configurationStaged = Join-Path $runtimeDirectory ".$stageId.configuration.tmp"
try {
    Copy-Item -LiteralPath $watcherSource -Destination $watcherStaged
    Copy-Item -LiteralPath $launcherSource -Destination $launcherStaged
    $configuration | ConvertTo-Json | Set-Content `
        -LiteralPath $configurationStaged `
        -Encoding UTF8
    Move-Item -LiteralPath $watcherStaged -Destination $watcherTarget -Force
    Move-Item -LiteralPath $launcherStaged -Destination $launcherTarget -Force
    Move-Item -LiteralPath $configurationStaged -Destination $configurationTarget -Force
} finally {
    foreach ($stagedPath in @($watcherStaged, $launcherStaged, $configurationStaged)) {
        if (Test-Path -LiteralPath $stagedPath) {
            Remove-Item -LiteralPath $stagedPath -Force
        }
    }
}

$listenerOwnersBefore = @{
    4194 = Get-ListenerOwner -Port 4194
    8790 = Get-ListenerOwner -Port 8790
}
$existingTask = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
$existingTaskWasRunning = (
    $null -ne $existingTask -and [string]$existingTask.State -eq "Running"
)

# A differently named legacy registration may be removed while idle. If it is
# active, disable only its future triggers: stopping/unregistering it can tear
# down a child process that currently owns a local service.
$legacyTaskNames = @("Clearra Local Runtime")
foreach ($legacyTaskName in $legacyTaskNames) {
    if ($legacyTaskName -eq $TaskName) {
        continue
    }
    $legacy = Get-ScheduledTask -TaskName $legacyTaskName -ErrorAction SilentlyContinue
    if ($null -eq $legacy) {
        continue
    }
    if ([string]$legacy.State -eq "Running") {
        Disable-ScheduledTask -TaskName $legacyTaskName | Out-Null
        Write-Output "Disabled the running legacy task without stopping its current instance."
    } else {
        Unregister-ScheduledTask -TaskName $legacyTaskName -Confirm:$false
    }
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

foreach ($port in @(4194, 8790)) {
    $ownerBefore = $listenerOwnersBefore[$port]
    if ($null -eq $ownerBefore) {
        continue
    }
    $ownerAfter = Get-ListenerOwner -Port $port
    if ($ownerAfter -ne $ownerBefore -or
        $null -eq (Get-Process -Id $ownerBefore -ErrorAction SilentlyContinue)) {
        throw "Port $port listener changed during the non-disruptive task migration."
    }
}

if ($existingTaskWasRunning) {
    # Register-ScheduledTask -Force updates the next-start definition without
    # terminating the running instance. IgnoreNew then keeps it authoritative
    # until a naturally safe restart (for example, the next logon).
    Write-Output "Updated the watchdog definition for its next safe start; preserved the running instance."
} else {
    Start-ScheduledTask -TaskName $TaskName
}

Write-Output "Installed one hidden Clearra local-services watchdog with a 60-second poll."
