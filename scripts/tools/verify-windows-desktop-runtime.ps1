# Launch one ordinary Windows GUI artifact under clearra-manage, without a
# new browser mode, startup flag, process-global kill, or any publication.
# This proves startup only. No page hydration, IPC, or solver is asserted.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?$')][string]$Version,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$SourceCommit,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$ExpectedSha256,
    [ValidateRange(5, 45)][int]$TimeoutSeconds = 40
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'windows-desktop-startup-proof.psm1')
$report = [ordered]@{
    schema = 'clearra.windows-desktop-startup-readback.v1'
    source_commit = $SourceCommit
    artifact_version = $Version
    expected_artifact_sha256 = $ExpectedSha256
    artifact_sha256_before = $null
    artifact_sha256_after = $null
    artifact_bytes = $null
    runtime_run_id = $env:CLEARRA_RUNTIME_RUN_ID
    status = 'failed'
    reason = 'input-not-verified'
    private_supervisor_job_verified = $false
    gui_process_owned = $false
    visible_clearra_window = $false
    isolated_webview2_renderer_owned = $false
    gui_shutdown = 'not-started'
    owned_webview_tree_drained = $false
    ui_route_verified = $false
    release_authority = $false
}

function Stop-Readback([string]$Status, [string]$Reason) {
    $report.status = $Status
    $report.reason = $Reason
    throw [InvalidOperationException]::new($Reason)
}

function Get-NonReparseMember([string]$Path, [bool]$Directory) {
    $item = Get-Item -LiteralPath $Path -Force
    if (($Directory -and $item -isnot [IO.DirectoryInfo]) -or
        (-not $Directory -and $item -isnot [IO.FileInfo])) { throw 'unexpected managed path type' }
    $cursor = $item
    while ($null -ne $cursor) {
        if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'managed path must not traverse a reparse point'
        }
        $cursor = if ($cursor -is [IO.DirectoryInfo]) { $cursor.Parent } else { $cursor.Directory }
    }
    return $item
}

function Get-FileDigest([IO.Stream]$Stream) {
    $Stream.Position = 0
    return (Get-FileHash -InputStream $Stream -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-ProcessRows([int[]]$ProcessIds, [switch]$WithCommandLine) {
    if ($ProcessIds.Count -eq 0 -or $ProcessIds.Count -gt 64) { throw 'job snapshot outside process limit' }
    $filter = ($ProcessIds | ForEach-Object { 'ProcessId = ' + $_ }) -join ' OR '
    $properties = @('ProcessId', 'ParentProcessId', 'Name', 'CreationDate')
    if ($WithCommandLine) { $properties += 'CommandLine' }
    Get-CimInstance -Namespace root/cimv2 -ClassName Win32_Process -Filter $filter -OperationTimeoutSec 4 `
        -Property $properties
}

function Get-OwnedStartupProcesses([int]$GuiId, [datetime]$FreshSinceUtc) {
    $jobIds = [ClearraDesktopStartupNative]::SelfJobPids()
    if ($PID -notin $jobIds -or $GuiId -notin $jobIds) {
        Stop-Readback 'unavailable' 'gui-process-not-job-owned'
    }
    $processes = @{}
    foreach ($row in Get-ProcessRows $jobIds) {
        if ($null -eq $row.CreationDate) { continue }
        $identifier = [int]$row.ProcessId
        $processes[$identifier] = @{
            Id = $identifier
            ParentId = [int]$row.ParentProcessId
            Name = [string]$row.Name
            StartedUtc = ([datetime]$row.CreationDate).ToUniversalTime()
            InJob = $identifier -in $jobIds
            Role = $null
            Profile = $null
        }
    }
    # Read no command line except the job's fresh WebView2 descendants.
    $candidateIds = @($processes.Values | Where-Object {
        $_.Name -ieq 'msedgewebview2.exe' -and
        (Test-ClearraDesktopOwnedDescendant $processes $_.Id $GuiId $FreshSinceUtc $PID)
    } | ForEach-Object { $_.Id })
    if ($candidateIds.Count -eq 0) { return $processes }
    $currentJob = [ClearraDesktopStartupNative]::SelfJobPids()
    $idsStillInJob = @($candidateIds | Where-Object { $_ -in $currentJob })
    if ($idsStillInJob.Count -eq 0) { return $processes }
    foreach ($row in Get-ProcessRows $idsStillInJob -WithCommandLine) {
        $entry = $processes[[int]$row.ProcessId]
        if ($null -eq $entry -or $null -eq $row.CreationDate -or
            ([datetime]$row.CreationDate).ToUniversalTime() -ne $entry.StartedUtc -or
            [string]::IsNullOrEmpty($row.CommandLine)) { continue }
        $argv = [ClearraDesktopStartupNative]::SplitCommandLine($row.CommandLine)
        $flags = Get-ClearraDesktopWebViewArgumentEvidence $argv
        if ($null -ne $flags) {
            $entry.Role = $flags.Role
            $entry.Profile = $flags.Profile
        }
    }
    $finalJob = [ClearraDesktopStartupNative]::SelfJobPids()
    foreach ($entry in $processes.Values) { $entry.InJob = $entry.Id -in $finalJob }
    return $processes
}

$artifact = $null
$artifactStream = $null
$gui = $null
$jobProven = $false
$phase = 'preflight'
try {
    if (-not (Test-ClearraNativeX64DesktopHost $IsWindows `
        ([Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture) `
        ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture))) {
        Stop-Readback 'unavailable' 'windows-x64-host-required'
    }
    $runId = [string]$report.runtime_run_id
    $leaseIdentity = [regex]::Match($runId, '^[0-9]{1,24}-([1-9][0-9]{0,9})$')
    if ($env:CLEARRA_RUNTIME_SUPERVISED -cne '1' -or $env:CLEARRA_RUNTIME_PROFILE -cne 'local-service' -or
        -not $leaseIdentity.Success) {
        Stop-Readback 'failed' 'required-supervisor-lease-missing'
    }
    $supervisorId = [int]$leaseIdentity.Groups[1].Value
    if ($env:CLEARRA_SOURCE_COMMIT -cne $SourceCommit -or $env:CLEARRA_ENGINE_BUILD_ID -cne $SourceCommit) {
        Stop-Readback 'failed' 'source-identity-mismatch'
    }
    $repository = (Get-NonReparseMember ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))) $true).FullName
    $roots = ConvertFrom-Json -InputObject $env:CLEARRA_MANAGED_OUTPUT_ROOTS -AsHashtable
    $buildRoot = Join-Path $repository 'build'
    $artifactRoot = Join-Path $repository '_local/artifacts'
    if (-not $roots.Contains('build-publication') -or -not $roots.Contains('local-artifacts') -or
        [IO.Path]::GetFullPath($roots['build-publication']) -ine [IO.Path]::GetFullPath($buildRoot) -or
        [IO.Path]::GetFullPath($roots['local-artifacts']) -ine [IO.Path]::GetFullPath($artifactRoot)) {
        Stop-Readback 'failed' 'managed-root-contract-mismatch'
    }
    if (-not [IO.Path]::IsPathFullyQualified($Executable)) {
        Stop-Readback 'failed' 'artifact-path-outside-managed-workspace'
    }
    $absoluteArtifact = [IO.Path]::GetFullPath($Executable)
    $allowedPath = $false
    foreach ($root in @($buildRoot, $artifactRoot)) {
        $prefix = [IO.Path]::GetFullPath($root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
        if ($absoluteArtifact.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { $allowedPath = $true }
    }
    if (-not $allowedPath -or [IO.Path]::GetFileName($absoluteArtifact) -cne "Clearra-GUI-v$Version-windows-x86_64.exe") {
        Stop-Readback 'failed' 'artifact-path-outside-managed-workspace'
    }
    $artifact = Get-NonReparseMember $absoluteArtifact $false
    # Hold the file against a replacement or write for the complete launch/stop.
    $artifactStream = [IO.File]::Open($artifact.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $report.artifact_bytes = $artifactStream.Length
    $report.artifact_sha256_before = Get-FileDigest $artifactStream
    if ($artifactStream.Length -lt 2 -or $report.artifact_sha256_before -cne $ExpectedSha256) {
        Stop-Readback 'failed' 'artifact-digest-mismatch'
    }
    $artifactStream.Position = 0
    if ($artifactStream.ReadByte() -ne 0x4d -or $artifactStream.ReadByte() -ne 0x5a) {
        Stop-Readback 'failed' 'artifact-not-a-windows-image'
    }

    $phase = 'native-probes'
    if (-not [Environment]::UserInteractive -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0) {
        Stop-Readback 'unavailable' 'interactive-window-station-unavailable'
    }
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class ClearraDesktopStartupNative {
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool QueryInformationJobObject(IntPtr job, int kind, IntPtr buffer, uint bytes, out uint used);
    public static int[] SelfJobPids() {
        const int max = 64;
        int size = 8 + IntPtr.Size * max;
        IntPtr ptr = Marshal.AllocHGlobal(size);
        try {
            uint used;
            if (!QueryInformationJobObject(IntPtr.Zero, 3, ptr, (uint)size, out used))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            int assigned = Marshal.ReadInt32(ptr, 0);
            int count = Marshal.ReadInt32(ptr, 4);
            if (assigned != count || count < 1 || count > max)
                throw new InvalidOperationException("incomplete job process list");
            int[] ids = new int[count];
            for (int i = 0; i < count; ++i)
                ids[i] = checked((int)Marshal.ReadIntPtr(ptr, 8 + i * IntPtr.Size).ToInt64());
            return ids;
        } finally { Marshal.FreeHGlobal(ptr); }
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CommandLineToArgvW(string value, out int count);
    [DllImport("kernel32.dll")] private static extern IntPtr LocalFree(IntPtr allocation);
    public static string[] SplitCommandLine(string value) {
        int count;
        IntPtr argv = CommandLineToArgvW(value, out count);
        if (argv == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            string[] args = new string[count];
            for (int i = 0; i < count; ++i)
                args[i] = Marshal.PtrToStringUni(Marshal.ReadIntPtr(argv, i * IntPtr.Size));
            return args;
        } finally { LocalFree(argv); }
    }

    private delegate bool WindowEnum(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")] private static extern bool EnumWindows(WindowEnum visit, IntPtr data);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] private static extern IntPtr GetWindow(IntPtr hwnd, uint kind);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr hwnd, StringBuilder title, int max);
    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll")] private static extern bool CloseDesktop(IntPtr desktop);
    public static bool HasReadableDesktop() {
        IntPtr desktop = OpenInputDesktop(0, false, 1); // DESKTOP_READOBJECTS
        if (desktop == IntPtr.Zero) return false;
        return CloseDesktop(desktop);
    }
    public static IntPtr VisibleClearraWindow(int processId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hwnd, state) => {
            uint owner;
            GetWindowThreadProcessId(hwnd, out owner);
            if (owner != (uint)processId || !IsWindowVisible(hwnd) || GetWindow(hwnd, 4) != IntPtr.Zero) return true;
            StringBuilder title = new StringBuilder(128);
            GetWindowText(hwnd, title, title.Capacity);
            if (!String.Equals(title.ToString(), "Clearra", StringComparison.Ordinal)) return true;
            found = hwnd;
            return false;
        }, IntPtr.Zero);
        return found;
    }
}
'@
    if (-not [ClearraDesktopStartupNative]::HasReadableDesktop()) {
        Stop-Readback 'unavailable' 'interactive-window-station-unavailable'
    }
    $rows = @(Get-ProcessRows @($PID, $supervisorId))
    $launcherRow = $rows | Where-Object { [int]$_.ProcessId -eq $PID }
    $supervisorRow = $rows | Where-Object { [int]$_.ProcessId -eq $supervisorId }
    $initialJob = @([ClearraDesktopStartupNative]::SelfJobPids())
    if ($null -eq $launcherRow -or $null -eq $supervisorRow -or $supervisorRow.Name -ine 'clearra-manage.exe' -or
        $launcherRow.ParentProcessId -ne $supervisorId -or $initialJob.Count -ne 1 -or $initialJob[0] -ne $PID) {
        Stop-Readback 'unavailable' 'private-supervisor-job-unverified'
    }
    $jobProven = $true
    $report.private_supervisor_job_verified = $true

    $phase = 'profile'
    # Directories are below the verified artifact root. No existing run
    # profile is adopted or recursively removed, even on an interrupted test.
    foreach ($path in @((Join-Path $repository '_local'), $artifactRoot, (Join-Path $artifactRoot 'windows-desktop-runtime'))) {
        if (-not (Test-Path -LiteralPath $path)) { $null = New-Item -ItemType Directory -Path $path }
        $null = Get-NonReparseMember $path $true
    }
    $scratch = Join-Path $artifactRoot ("windows-desktop-runtime/$runId-" + [Guid]::NewGuid().ToString('N'))
    $null = New-Item -ItemType Directory -Path $scratch
    $null = Get-NonReparseMember $scratch $true
    $webviewData = Join-Path $scratch 'webview2'
    $localData = Join-Path $scratch 'local'
    $roamingData = Join-Path $scratch 'roaming'
    foreach ($path in @($webviewData, $localData, $roamingData)) {
        $null = New-Item -ItemType Directory -Path $path
        $null = Get-NonReparseMember $path $true
    }

    $phase = 'launch'
    $start = [Diagnostics.ProcessStartInfo]::new($artifact.FullName)
    $start.WorkingDirectory = $scratch
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    # Do not inherit diagnostic/alternate-channel overrides from this runner.
    foreach ($name in @($start.Environment.Keys | Where-Object { $_ -like 'WEBVIEW2_*' })) {
        $null = $start.Environment.Remove($name)
    }
    $start.Environment['WEBVIEW2_USER_DATA_FOLDER'] = $webviewData
    $start.Environment['LOCALAPPDATA'] = $localData
    $start.Environment['APPDATA'] = $roamingData
    $gui = [Diagnostics.Process]::Start($start)
    if ($null -eq $gui) { Stop-Readback 'failed' 'windows-gui-start-failed' }
    # Acquire/retain the original process handle, not a later PID lookup.
    $null = $gui.Handle
    if ($gui.HasExited) { Stop-Readback 'failed' 'gui-exited-before-readback' }
    $freshSince = $gui.StartTime.ToUniversalTime().AddMilliseconds(-1)
    $report.gui_shutdown = 'pending'

    $phase = 'observe'
    $elapsed = [Diagnostics.Stopwatch]::StartNew()
    $previousWindow = [IntPtr]::Zero
    $lastReason = 'gui-window-not-observed'
    while ($elapsed.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        if ($gui.HasExited) { Stop-Readback 'failed' 'gui-exited-before-readback' }
        $observations = Get-OwnedStartupProcesses $gui.Id $freshSince
        $window = [ClearraDesktopStartupNative]::VisibleClearraWindow($gui.Id)
        $observation = Get-ClearraDesktopStartupObservation $observations $gui.Id $PID $freshSince $webviewData ($window -ne [IntPtr]::Zero)
        foreach ($name in $observation.Keys) { $report[$name] = $observation[$name] }
        $lastReason = Get-ClearraDesktopStartupReadinessReason $observation
        if ($lastReason -ceq 'verified' -and $previousWindow -eq $window -and -not $gui.HasExited -and
            $elapsed.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
            # Resample the same owner window; a process start or one brief
            # rendering subprocess before a crash is not the pass condition.
            $report.status = 'verified'
            $report.reason = 'verified'
            break
        }
        $previousWindow = if ($lastReason -ceq 'verified') { $window } else { [IntPtr]::Zero }
        Start-Sleep -Milliseconds 150
    }
    if ($report.status -cne 'verified') {
        if ($lastReason -ceq 'verified') { $lastReason = 'startup-readiness-not-stable' }
        Stop-Readback 'unavailable' $lastReason
    }
} catch {
    if ($report.reason -ceq 'input-not-verified') {
        switch ($phase) {
            'preflight' { $report.reason = 'artifact-or-supervisor-input-unavailable' }
            'native-probes' { $report.status = 'unavailable'; $report.reason = 'native-startup-probe-unavailable' }
            'profile' { $report.reason = 'isolated-profile-creation-failed' }
            'launch' { $report.reason = 'windows-gui-start-failed' }
            'observe' { $report.status = 'unavailable'; $report.reason = 'owned-process-observation-unavailable' }
        }
    }
} finally {
    if ($null -ne $gui) {
        try {
            if ($gui.HasExited) {
                $report.gui_shutdown = 'already-exited'
                if ($report.status -ceq 'verified') {
                    $report.status = 'failed'
                    $report.reason = 'gui-exited-after-readback-before-close'
                }
            }
            elseif ($gui.CloseMainWindow() -and $gui.WaitForExit(2000)) { $report.gui_shutdown = 'graceful' }
            else {
                # Only the exact process handle we started. The Job-backed
                # manager, not an image-name lookup, owns remaining children.
                $gui.Kill()
                if (-not $gui.WaitForExit(2000)) { throw 'owned GUI did not stop' }
                $report.gui_shutdown = 'owned-root-forced'
            }
        } catch {
            $report.status = 'failed'
            $report.reason = 'owned-gui-shutdown-unverified'
        } finally { $gui.Dispose() }
    }
    if ($jobProven -and $report.status -ceq 'verified') {
        try {
            $drain = [Diagnostics.Stopwatch]::StartNew()
            do {
                $children = @([ClearraDesktopStartupNative]::SelfJobPids() | Where-Object { $_ -ne $PID })
                if ($children.Count -eq 0) { break }
                Start-Sleep -Milliseconds 100
            } while ($drain.ElapsedMilliseconds -lt 2000)
            if ($children.Count -ne 0) { throw 'supervisor must drain lingering WebView processes' }
            $report.owned_webview_tree_drained = $true
        } catch {
            $report.status = 'failed'
            $report.reason = 'owned-webview-tree-not-drained'
        }
    }
    if ($null -ne $artifactStream) {
        try {
            $report.artifact_sha256_after = Get-FileDigest $artifactStream
            if ($report.artifact_sha256_after -cne $report.artifact_sha256_before) {
                $report.status = 'failed'
                $report.reason = 'artifact-changed-during-probe'
            }
        } catch {
            $report.status = 'failed'
            $report.reason = 'artifact-rehash-failed'
        } finally { $artifactStream.Dispose() }
    }
}
$report | ConvertTo-Json -Compress
switch ($report.status) {
    'verified' { exit 0 }
    'unavailable' { exit 3 }
    default { exit 2 }
}
