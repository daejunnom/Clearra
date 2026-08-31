[CmdletBinding()]
param(
    [ValidateRange(1, 86400)]
    [int]$PollSeconds = 60,

    [ValidateRange(1, 65535)]
    [int]$GuiPort = 4194,

    [ValidateRange(1, 65535)]
    [int]$TunnelPort = 8790,

    [string]$RepoRoot = "",
    [string]$NodePath = "",
    [string]$NpmCliPath = "",

    [string]$SshPath = "$env:WINDIR\System32\OpenSSH\ssh.exe",
    [string]$SshKeyPath = "",
    [string]$SshDestination = "",
    [string]$ConfigPath = "",
    [string]$EventLogPath = "$env:LOCALAPPDATA\Clearra\startup\local-services-v2.log",
    [switch]$DisableTunnel,
    [switch]$Once
)

$ErrorActionPreference = "Stop"
if ($ConfigPath) {
    $configuration = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
    $RepoRoot = [string]$configuration.repo_root
    $NodePath = [string]$configuration.node_path
    $NpmCliPath = [string]$configuration.npm_cli_path
    $SshPath = [string]$configuration.ssh_path
    $SshKeyPath = [string]$configuration.ssh_key_path
    $SshDestination = [string]$configuration.ssh_destination
}
if (-not $RepoRoot -or -not $NodePath -or -not $NpmCliPath) {
    throw "RepoRoot, NodePath, and NpmCliPath are required."
}
$mutex = [Threading.Mutex]::new($false, "Local\ClearraLocalServicesWatchdog-v2")
$ownsMutex = $false
$ownedGuiProcess = $null
$ownedTunnelProcess = $null

function Write-WatchdogEvent {
    param([Parameter(Mandatory)][string]$Message)

    try {
        $parent = Split-Path -Parent $EventLogPath
        if ($parent -and -not (Test-Path -LiteralPath $parent -PathType Container)) {
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
        }
        if ((Test-Path -LiteralPath $EventLogPath -PathType Leaf) -and
            (Get-Item -LiteralPath $EventLogPath).Length -gt 1MB) {
            $previous = "$EventLogPath.1"
            if (Test-Path -LiteralPath $previous) {
                Remove-Item -LiteralPath $previous -Force
            }
            Move-Item -LiteralPath $EventLogPath -Destination $previous
        }
        Add-Content -LiteralPath $EventLogPath -Value (
            "{0} {1}" -f [datetime]::Now.ToString("o"), $Message
        )
    } catch {
        # A log failure must not change process ownership.
    }
}

function Test-PortInUse {
    param([Parameter(Mandatory)][int]$Port)

    $connection = Get-NetTCPConnection -State Listen -LocalPort $Port `
        -ErrorAction SilentlyContinue |
        Select-Object -First 1
    return $null -ne $connection
}

function Test-OwnedProcessRunning {
    param($Process)

    if ($null -eq $Process) {
        return $false
    }
    try {
        return -not $Process.HasExited
    } catch {
        return $false
    }
}

function Test-ExistingGuiStartup {
    try {
        $escapedNodePath = [regex]::Escape($NodePath)
        $escapedNpmCliPath = [regex]::Escape($NpmCliPath)
        $process = Get-CimInstance Win32_Process -Filter "Name = 'node.exe'" `
            -ErrorAction SilentlyContinue |
            Where-Object {
                $_.ExecutablePath -match "^${escapedNodePath}$" -and
                $_.CommandLine -match $escapedNpmCliPath -and
                $_.CommandLine -match '(?:^|\s)run(?:\s|$)' -and
                $_.CommandLine -match '(?:^|\s)dev(?:\s|$)' -and
                $_.CommandLine -match '@clearra/web'
            } |
            Select-Object -First 1
        return $null -ne $process
    } catch {
        return $false
    }
}

function Test-ExistingTunnelStartup {
    if (-not $SshDestination) {
        return $false
    }
    try {
        $escapedSshPath = [regex]::Escape($SshPath)
        $escapedDestination = [regex]::Escape($SshDestination)
        $escapedForward = [regex]::Escape(
            "127.0.0.1:${TunnelPort}:127.0.0.1:${TunnelPort}"
        )
        $process = Get-CimInstance Win32_Process -Filter "Name = 'ssh.exe'" `
            -ErrorAction SilentlyContinue |
            Where-Object {
                $_.ExecutablePath -match "^${escapedSshPath}$" -and
                $_.CommandLine -match $escapedForward -and
                $_.CommandLine -match $escapedDestination
            } |
            Select-Object -First 1
        return $null -ne $process
    } catch {
        return $false
    }
}

function Start-HiddenProcess {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(Mandatory)][string[]]$ArgumentList,
        [string]$WorkingDirectory = ""
    )

    $parameters = @{
        FilePath = $FilePath
        ArgumentList = $ArgumentList
        WindowStyle = "Hidden"
        PassThru = $true
    }
    if ($WorkingDirectory) {
        $parameters.WorkingDirectory = $WorkingDirectory
    }
    # The executable is node.exe or ssh.exe directly. A command-shell
    # intermediary is never allowed to flash a console window.
    return Start-Process @parameters
}

function Ensure-DeveloperGui {
    if (Test-PortInUse -Port $GuiPort) {
        Write-WatchdogEvent "gui preserved: port=$GuiPort already-in-use"
        return
    }
    if (Test-OwnedProcessRunning -Process $script:ownedGuiProcess) {
        Write-WatchdogEvent "gui preserved: owned startup still running"
        return
    }
    if (Test-ExistingGuiStartup) {
        Write-WatchdogEvent "gui preserved: matching startup process already running"
        return
    }
    if (-not (Test-Path -LiteralPath $RepoRoot -PathType Container) -or
        -not (Test-Path -LiteralPath $NodePath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $NpmCliPath -PathType Leaf)) {
        Write-WatchdogEvent "gui start skipped: workspace, node, or npm cli missing"
        return
    }

    try {
        $script:ownedGuiProcess = Start-HiddenProcess `
            -FilePath $NodePath `
            -ArgumentList @($NpmCliPath, "run", "dev", "-w", "@clearra/web") `
            -WorkingDirectory $RepoRoot
        Write-WatchdogEvent "gui start requested: pid=$($script:ownedGuiProcess.Id)"
    } catch {
        Write-WatchdogEvent "gui start failed"
    }
}

function Ensure-AdminTunnel {
    if ($DisableTunnel) {
        return
    }
    if (Test-PortInUse -Port $TunnelPort) {
        Write-WatchdogEvent "tunnel preserved: port=$TunnelPort already-in-use"
        return
    }
    if (Test-OwnedProcessRunning -Process $script:ownedTunnelProcess) {
        Write-WatchdogEvent "tunnel preserved: owned startup still running"
        return
    }
    if (Test-ExistingTunnelStartup) {
        Write-WatchdogEvent "tunnel preserved: matching startup process already running"
        return
    }
    if (-not (Test-Path -LiteralPath $SshPath -PathType Leaf) -or
        -not $SshKeyPath -or
        -not (Test-Path -LiteralPath $SshKeyPath -PathType Leaf) -or
        -not $SshDestination) {
        Write-WatchdogEvent "tunnel start skipped: ssh configuration unavailable"
        return
    }

    $arguments = @(
        "-i", $SshKeyPath,
        "-o", "BatchMode=yes",
        "-o", "IdentitiesOnly=yes",
        "-o", "ExitOnForwardFailure=yes",
        "-o", "StrictHostKeyChecking=yes",
        "-o", "ConnectTimeout=15",
        "-o", "ServerAliveInterval=30",
        "-o", "ServerAliveCountMax=3",
        "-N", "-T",
        "-L", "127.0.0.1:${TunnelPort}:127.0.0.1:${TunnelPort}",
        $SshDestination
    )
    try {
        $script:ownedTunnelProcess = Start-HiddenProcess `
            -FilePath $SshPath `
            -ArgumentList $arguments
        Write-WatchdogEvent "tunnel start requested: pid=$($script:ownedTunnelProcess.Id)"
    } catch {
        Write-WatchdogEvent "tunnel start failed"
    }
}

try {
    try {
        $ownsMutex = $mutex.WaitOne(0, $false)
    } catch [Threading.AbandonedMutexException] {
        $ownsMutex = $true
    }
    if (-not $ownsMutex) {
        exit 0
    }

    Write-WatchdogEvent "watchdog v2 started: poll-seconds=$PollSeconds"
    do {
        try {
            Ensure-DeveloperGui
            Ensure-AdminTunnel
        } catch {
            Write-WatchdogEvent "watchdog cycle failed"
        }
        if (-not $Once) {
            Start-Sleep -Seconds $PollSeconds
        }
    } while (-not $Once)
} finally {
    if ($ownsMutex) {
        $mutex.ReleaseMutex()
    }
    $mutex.Dispose()
}
