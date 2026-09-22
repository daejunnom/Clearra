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

    [string]$SshPath = "$env:WINDIR\System32\OpenSSH\ssh.exe",
    [string]$SshKeyPath = "",
    [string]$SshDestination = "",
    [string]$ManagerPath = "",
    [string]$ConfigPath = "",
    [string]$EventLogPath = "$env:LOCALAPPDATA\Clearra\logs\local-services-v2.log",
    [switch]$DisableTunnel,
    [switch]$Once
)

$ErrorActionPreference = "Stop"

function Assert-ManagedStateInput {
    param([Parameter(Mandatory)][string]$Path)

    $stateRoot = [System.IO.Path]::GetFullPath(
        (Join-Path $env:LOCALAPPDATA "Clearra\state")
    ).TrimEnd('\', '/')
    $resolved = [System.IO.Path]::GetFullPath($Path)
    if (-not $resolved.StartsWith(
        $stateRoot + [System.IO.Path]::DirectorySeparatorChar,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw 'E_CLEARRA_STORAGE_PATH_NOT_ALLOWED: watchdog configuration must be under the managed Clearra state root.'
    }
    if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
        throw 'E_CLEARRA_STORAGE_PATH_NOT_ALLOWED: watchdog configuration is not a regular file.'
    }
    $inputItem = Get-Item -LiteralPath $resolved -Force
    if (($inputItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'E_CLEARRA_STORAGE_PATH_NOT_ALLOWED: watchdog configuration is a link or junction.'
    }
    $cursor = [System.IO.DirectoryInfo]::new((Split-Path -Parent $resolved))
    while ($null -ne $cursor -and $cursor.FullName.Length -ge $stateRoot.Length) {
        if (($cursor.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'E_CLEARRA_STORAGE_PATH_NOT_ALLOWED: watchdog configuration traverses a link or junction.'
        }
        if ($cursor.FullName.Equals($stateRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
            break
        }
        $cursor = $cursor.Parent
    }
    return $resolved
}

if ($ConfigPath) {
    $ConfigPath = Assert-ManagedStateInput -Path $ConfigPath
    $configuration = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
    $RepoRoot = [string]$configuration.repo_root
    $NodePath = [string]$configuration.node_path
    $SshPath = [string]$configuration.ssh_path
    $ManagerPath = [string]$configuration.manager_path
    $SshKeyPath = [string]$configuration.ssh_key_path
    $SshDestination = [string]$configuration.ssh_destination
}
if (-not $RepoRoot -or -not $NodePath) {
    throw "RepoRoot and NodePath are required."
}
$managerPath = if (-not [string]::IsNullOrWhiteSpace($ManagerPath)) {
    $ManagerPath
} else {
    Join-Path $env:LOCALAPPDATA 'Clearra\state\local-services-v2\clearra-manage.exe'
}
if (-not (Test-Path -LiteralPath $managerPath -PathType Leaf)) {
    throw 'The Clearra management entrypoint is unavailable.'
}
& $managerPath --root $RepoRoot storage verify --path $EventLogPath | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'E_CLEARRA_STORAGE_PATH_NOT_ALLOWED: watchdog log path failed management verification.'
}
if (-not $Once.IsPresent -and
    ($env:CLEARRA_RUNTIME_SUPERVISED -cne '1' -or
     $env:CLEARRA_RUNTIME_PROFILE -cne 'local-service')) {
    throw 'The persistent watchdog must run through the Clearra local-service runtime supervisor.'
}
# Keep the installed default owner identity stable. An isolated custom-port
# diagnostic must not silently exit because the real 4194 watchdog is alive.
$mutexSuffix = if ($GuiPort -eq 4194 -and $TunnelPort -eq 8790) {
    ""
} else {
    "-${GuiPort}-${TunnelPort}"
}
$mutex = [Threading.Mutex]::new($false, "Local\ClearraLocalServicesWatchdog-v2$mutexSuffix")
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
        $escapedVitePath = [regex]::Escape((Join-Path $RepoRoot "node_modules\vite\bin\vite.js"))
        $escapedFrontendPath = [regex]::Escape((Join-Path $RepoRoot "scripts\tools\build-clearra-frontend.mjs"))
        $process = Get-CimInstance Win32_Process -Filter "Name = 'node.exe'" `
            -ErrorAction SilentlyContinue |
            Where-Object {
                $_.ExecutablePath -match "^${escapedNodePath}$" -and
                ($_.CommandLine -match $escapedVitePath -or $_.CommandLine -match $escapedFrontendPath) -and
                $_.CommandLine -match "(?:^|\s)--port\s+${GuiPort}(?:\s|$)"
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
    $vitePath = Join-Path $RepoRoot "node_modules\vite\bin\vite.js"
    $frontendPath = Join-Path $RepoRoot "scripts\tools\build-clearra-frontend.mjs"
    $webRoot = Join-Path $RepoRoot "apps\clearra-web"
    if (-not (Test-Path -LiteralPath $webRoot -PathType Container) -or
        -not (Test-Path -LiteralPath $NodePath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $vitePath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $frontendPath -PathType Leaf)) {
        Write-WatchdogEvent "gui start skipped: workspace, node, or vite missing"
        return
    }

    try {
        # Future recovery holds the source experiment owner, but preserves the
        # existing WASM and never invokes its build or restarts an occupied port.
        $script:ownedGuiProcess = Start-HiddenProcess `
            -FilePath $NodePath `
            -ArgumentList @(('"{0}"' -f $frontendPath), "--app", "web", "--task", "dev", "--recovery", "--host", "127.0.0.1", "--port", [string]$GuiPort, "--strictPort", "--mode", "local-recovery") `
            -WorkingDirectory $webRoot
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
            if ($Once.IsPresent) {
                Write-WatchdogEvent "once gui-port-in-use=$(Test-PortInUse -Port $GuiPort)"
                Write-WatchdogEvent "once tunnel-port-in-use=$(Test-PortInUse -Port $TunnelPort)"
            } else {
                Ensure-DeveloperGui
                Ensure-AdminTunnel
            }
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
