# Bounded lifecycle for generated build/test artifacts outside the repository.
$script:ClearraArtifactCacheSchemaVersion = 2
$script:ClearraDefaultBuildCacheMaxBytes = [int64](8GB)
$script:ClearraBuildCacheSessionKey = $null
$script:ClearraArtifactCacheUsageLock = $null

function Get-ClearraBuildCacheMaxBytes {
    $configured = $env:CLEARRA_MAX_BUILD_CACHE_GIB
    if ([string]::IsNullOrWhiteSpace($configured)) {
        return $script:ClearraDefaultBuildCacheMaxBytes
    }

    $gib = 0.0
    if (-not [double]::TryParse(
            $configured,
            [System.Globalization.NumberStyles]::Float,
            [System.Globalization.CultureInfo]::InvariantCulture,
            [ref]$gib
        ) -or $gib -lt 1.0 -or $gib -gt 128.0) {
        throw 'CLEARRA_MAX_BUILD_CACHE_GIB must be a number from 1 through 128.'
    }
    return [int64]($gib * 1GB)
}

function Test-ClearraSecretOrGeneratedInput([System.IO.FileInfo]$File) {
    $name = $File.Name
    if ($name -eq 'package-lock.json' -or
        $name -eq '.env' -or
        $name.StartsWith('.env.', [System.StringComparison]::OrdinalIgnoreCase)) {
        return $true
    }
    if ($name -match '(?i)(credential|service[-_]?account|api[-_]?key)' -or
        $File.Extension -match '(?i)^\.(pem|key|pfx|p12)$') {
        return $true
    }
    return $false
}

function Get-ClearraBuildInputFiles([string]$RepositoryRoot) {
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $files = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
    foreach ($name in @('Cargo.toml', 'Cargo.lock', 'CMakeLists.txt', 'package.json')) {
        $path = Join-Path $repository $name
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $files.Add([System.IO.FileInfo]::new($path))
        }
    }

    $excludedDirectories = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($name in @(
            '.git', '.cache', '_local', 'dist', 'dist-server', 'node_modules'
        )) {
        [void]$excludedDirectories.Add($name)
    }

    foreach ($relativeRoot in @('apps', 'assets', 'core-c', 'crates', 'packages', 'scripts', 'tests', 'tools')) {
        $root = Join-Path $repository $relativeRoot
        if (-not (Test-Path -LiteralPath $root -PathType Container)) {
            continue
        }
        $pending = [System.Collections.Generic.Stack[System.IO.DirectoryInfo]]::new()
        $pending.Push([System.IO.DirectoryInfo]::new($root))
        while ($pending.Count -gt 0) {
            $directory = $pending.Pop()
            foreach ($entry in $directory.EnumerateFileSystemInfos()) {
                if ($entry -is [System.IO.DirectoryInfo]) {
                    if (-not $excludedDirectories.Contains($entry.Name) -and
                        -not (($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)) {
                        $pending.Push($entry)
                    }
                    continue
                }
                if ($entry -is [System.IO.FileInfo] -and
                    -not (Test-ClearraSecretOrGeneratedInput $entry)) {
                    $files.Add($entry)
                }
            }
        }
    }
    return @($files | Sort-Object FullName -Unique)
}

function Get-ClearraCommandMetadata([string]$Name) {
    $command = Get-Command $Name -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $command -or [string]::IsNullOrWhiteSpace($command.Source)) {
        return "$Name=unavailable"
    }
    try {
        $file = [System.IO.FileInfo]::new($command.Source)
        return "$Name=$($file.FullName)|$($file.Length)|$($file.LastWriteTimeUtc.Ticks)"
    } catch {
        return "$Name=$($command.Source)"
    }
}

function Get-ClearraCommandVersionMetadata([string]$Name, [string[]]$Arguments) {
    $command = Get-Command $Name -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $command -or [string]::IsNullOrWhiteSpace($command.Source)) {
        return "$Name-version=unavailable"
    }
    try {
        $output = @(& $command.Source @Arguments 2>&1)
        if ($LASTEXITCODE -ne 0) {
            return "$Name-version=error-$LASTEXITCODE"
        }
        return "$Name-version=$(($output -join '|').Trim())"
    } catch {
        return "$Name-version=error"
    }
}

function Get-ClearraWorkspaceBuildSignature([string]$RepositoryRoot) {
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add("schema=$script:ClearraArtifactCacheSchemaVersion")
    $lines.Add("repository=$repository")
    $lines.Add("os=$([System.Environment]::OSVersion.VersionString)")
    $lines.Add("architecture=$([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture)")
    $lines.Add("execution_surface=$($env:CLEARRA_EXECUTION_SURFACE)")
    $lines.Add("rustflags=$($env:RUSTFLAGS)")
    $lines.Add("windows_rustflags=$($env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS)")
    $lines.Add((Get-ClearraCommandMetadata 'cargo'))
    $lines.Add((Get-ClearraCommandMetadata 'rustc'))
    $lines.Add((Get-ClearraCommandMetadata 'cmake'))
    $lines.Add((Get-ClearraCommandVersionMetadata 'cargo' @('--version', '--verbose')))
    $lines.Add((Get-ClearraCommandVersionMetadata 'rustc' @('--version', '--verbose')))
    $lines.Add((Get-ClearraCommandVersionMetadata 'cmake' @('--version')))

    $inputFiles = @(Get-ClearraBuildInputFiles $repository)
    foreach ($file in $inputFiles) {
        $relative = $file.FullName.Substring($repository.Length).TrimStart('\', '/').Replace('\', '/')
        $lines.Add("$relative|$($file.Length)|$($file.LastWriteTimeUtc.Ticks)")
    }

    $bytes = [System.Text.Encoding]::UTF8.GetBytes(($lines -join "`n"))
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $digest = $sha.ComputeHash($bytes)
    } finally {
        $sha.Dispose()
    }
    return [pscustomobject]@{
        signature = ([System.BitConverter]::ToString($digest)).Replace('-', '').ToLowerInvariant()
        input_file_count = $inputFiles.Count
    }
}

function Get-ClearraDirectorySizeBytes([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        return [int64]0
    }
    $bytes = [int64]0
    foreach ($file in [System.IO.Directory]::EnumerateFiles(
            $Path,
            '*',
            [System.IO.SearchOption]::AllDirectories
        )) {
        try {
            $bytes += [System.IO.FileInfo]::new($file).Length
        } catch {}
    }
    return $bytes
}

function Remove-ClearraDirectorySafely(
    [string]$Path,
    [string]$AllowedPath,
    [string]$RepositoryRoot
) {
    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }
    $candidate = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    $allowed = [System.IO.Path]::GetFullPath($AllowedPath).TrimEnd('\', '/')
    $comparison = if (Test-StartTestsWindows) {
        [System.StringComparison]::OrdinalIgnoreCase
    } else {
        [System.StringComparison]::Ordinal
    }
    if (-not $candidate.Equals($allowed, $comparison)) {
        throw "Refusing to remove an artifact path outside the verified root: $candidate"
    }
    Assert-ClearraPathOutsideRepository $candidate $RepositoryRoot | Out-Null
    Remove-Item -LiteralPath $candidate -Recurse -Force
}

function Enter-ClearraArtifactCacheLock([string]$ArtifactRoot) {
    $parent = Split-Path -Parent ([System.IO.Path]::GetFullPath($ArtifactRoot))
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $lockPath = Join-Path $parent '.artifact-cache.lock'
    $deadline = [DateTime]::UtcNow.AddMinutes(2)
    do {
        try {
            return [System.IO.File]::Open(
                $lockPath,
                [System.IO.FileMode]::OpenOrCreate,
                [System.IO.FileAccess]::ReadWrite,
                [System.IO.FileShare]::None
            )
        } catch [System.IO.IOException] {
            if ([DateTime]::UtcNow -ge $deadline) {
                throw "Timed out waiting for the Clearra artifact cache lock: $lockPath"
            }
            Start-Sleep -Milliseconds 200
        }
    } while ($true)
}

function Enter-ClearraArtifactCacheUsageLock([string]$ArtifactRoot) {
    $parent = Split-Path -Parent ([System.IO.Path]::GetFullPath($ArtifactRoot))
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $lockPath = Join-Path $parent '.artifact-cache-use.lock'
    $deadline = [DateTime]::UtcNow.AddMinutes(30)
    do {
        try {
            return [System.IO.File]::Open(
                $lockPath,
                [System.IO.FileMode]::OpenOrCreate,
                [System.IO.FileAccess]::ReadWrite,
                [System.IO.FileShare]::None
            )
        } catch [System.IO.IOException] {
            if ([DateTime]::UtcNow -ge $deadline) {
                throw "Timed out waiting for the active Clearra build cache: $lockPath"
            }
            Start-Sleep -Milliseconds 250
        }
    } while ($true)
}

function Exit-ClearraBuildArtifactCacheUsage {
    if ($null -ne $script:ClearraArtifactCacheUsageLock) {
        try {
            Invoke-ClearraBuildArtifactCacheRetention | Out-Null
        } catch {
            Write-Warning "Clearra build-cache retention failed: $($_.Exception.Message)"
        } finally {
            $script:ClearraArtifactCacheUsageLock.Dispose()
            $script:ClearraArtifactCacheUsageLock = $null
        }
    }
}

function Test-ClearraInheritedArtifactCacheOwner {
    $ownerPid = 0
    if (-not [int]::TryParse($env:CLEARRA_BUILD_CACHE_OWNER_PID, [ref]$ownerPid) -or
        $ownerPid -le 0) {
        return $false
    }
    if ($ownerPid -eq $PID) {
        return $true
    }
    try {
        $owner = [System.Diagnostics.Process]::GetProcessById($ownerPid)
        return -not $owner.HasExited
    } catch {
        return $false
    }
}

function Remove-ClearraStaleTransientArtifacts([string]$ArtifactRoot) {
    $artifact = [System.IO.Path]::GetFullPath($ArtifactRoot).TrimEnd('\', '/')
    $comparison = if (Test-StartTestsWindows) {
        [System.StringComparison]::OrdinalIgnoreCase
    } else {
        [System.StringComparison]::Ordinal
    }
    if (Test-Path -LiteralPath $ArtifactRoot -PathType Container) {
        foreach ($entry in Get-ChildItem -LiteralPath $ArtifactRoot -Force) {
            if ($entry.Name -like 'clearra-*' -or $entry.Name -like 'wsl-sync-*') {
                Remove-Item -LiteralPath $entry.FullName -Recurse -Force -ErrorAction SilentlyContinue
            }
        }
        $transientRoot = Join-Path $ArtifactRoot 'transient'
        if (Test-Path -LiteralPath $transientRoot -PathType Container) {
            foreach ($entry in Get-ChildItem -LiteralPath $transientRoot -Force) {
                Remove-Item -LiteralPath $entry.FullName -Recurse -Force -ErrorAction SilentlyContinue
            }
        }
    }

    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\', '/')
    foreach ($entry in Get-ChildItem -LiteralPath $tempRoot -Force -Filter 'clearra-*' -ErrorAction SilentlyContinue) {
        $candidate = [System.IO.Path]::GetFullPath($entry.FullName).TrimEnd('\', '/')
        $candidatePrefix = $candidate + [System.IO.Path]::DirectorySeparatorChar
        if ($artifact.Equals($candidate, $comparison) -or
            $artifact.StartsWith($candidatePrefix, $comparison)) {
            continue
        }
        Remove-Item -LiteralPath $candidate -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-ClearraBuildArtifactCacheRetention(
    [string]$RepositoryRoot = (Resolve-ClearraRoot),
    [string]$ArtifactRoot = (Get-ClearraArtifactRoot),
    [int64]$MaxBytes = (Get-ClearraBuildCacheMaxBytes)
) {
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $artifact = Assert-ClearraPathOutsideRepository $ArtifactRoot $repository
    if (-not (Test-Path -LiteralPath $artifact -PathType Container)) {
        return [pscustomobject]@{ action = 'absent'; cache_size_bytes = [int64]0 }
    }

    $size = Get-ClearraDirectorySizeBytes $artifact
    if ($size -le $MaxBytes) {
        return [pscustomobject]@{ action = 'reuse'; cache_size_bytes = $size }
    }

    $lock = Enter-ClearraArtifactCacheLock $artifact
    try {
        $size = Get-ClearraDirectorySizeBytes $artifact
        if ($size -gt $MaxBytes) {
            Remove-ClearraDirectorySafely $artifact $artifact $repository
            return [pscustomobject]@{ action = 'post-run-budget-reset'; cache_size_bytes = $size }
        }
        return [pscustomobject]@{ action = 'reuse'; cache_size_bytes = $size }
    } finally {
        $lock.Dispose()
    }
}

function Remove-ClearraLegacyArtifactSurfaces([string]$ArtifactRoot) {
    $clearraRoot = Split-Path -Parent ([System.IO.Path]::GetFullPath($ArtifactRoot))
    foreach ($name in @('research', 'verification', 'srp-tool-sample', 'srp-tool-sample-2', 'srp-tool-sample-3')) {
        $path = Join-Path $clearraRoot $name
        if (Test-Path -LiteralPath $path -PathType Container) {
            Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Invoke-ClearraReportRetention([string]$ArtifactRoot) {
    $reportRoot = Join-Path (Split-Path -Parent ([System.IO.Path]::GetFullPath($ArtifactRoot))) 'reports'
    if (-not (Test-Path -LiteralPath $reportRoot -PathType Container)) {
        return
    }
    $files = @(Get-ChildItem -LiteralPath $reportRoot -Recurse -Force -File |
        Sort-Object LastWriteTimeUtc -Descending)
    $retainedBytes = [int64]0
    $retainedCount = 0
    foreach ($file in $files) {
        $retainedCount += 1
        $retainedBytes += $file.Length
        if ($file.LastWriteTimeUtc -lt [DateTime]::UtcNow.AddDays(-14) -or
            $retainedCount -gt 200 -or
            $retainedBytes -gt 256MB) {
            Remove-Item -LiteralPath $file.FullName -Force -ErrorAction SilentlyContinue
        }
    }
}

function Initialize-ClearraBuildArtifactCache(
    [string]$RepositoryRoot = (Resolve-ClearraRoot),
    [string]$ArtifactRoot = (Get-ClearraArtifactRoot),
    [int64]$MaxBytes = (Get-ClearraBuildCacheMaxBytes)
) {
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $artifact = Assert-ClearraPathOutsideRepository $ArtifactRoot $repository
    Remove-ClearraRepositoryLocalBuildArtifacts $repository
    $statePath = Join-Path $artifact '.clearra-cache-state.json'
    $signature = Get-ClearraWorkspaceBuildSignature $repository
    $lock = Enter-ClearraArtifactCacheLock $artifact
    try {
        Remove-ClearraStaleTransientArtifacts $artifact
        $state = $null
        if (Test-Path -LiteralPath $statePath -PathType Leaf) {
            try {
                $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
            } catch {
                $state = $null
            }
        }

        $sameWorkspace = $null -ne $state -and
            $state.schema_version -eq $script:ClearraArtifactCacheSchemaVersion -and
            $state.workspace_root -eq $repository
        $sameInputs = $sameWorkspace -and
            $state.session_key -eq $signature.signature
        $action = 'reuse'
        $sizeBefore = [int64]0
        if ($sameWorkspace) {
            $sizeBefore = Get-ClearraDirectorySizeBytes $artifact
            if ($sizeBefore -gt $MaxBytes) {
                Remove-ClearraDirectorySafely $artifact $artifact $repository
                $action = 'budget-reset'
            } elseif (-not $sameInputs) {
                $action = 'input-change-reuse'
            }
        } else {
            if (Test-Path -LiteralPath $artifact) {
                Remove-ClearraDirectorySafely $artifact $artifact $repository
            }
            $action = if ($null -eq $state) { 'fresh' } else { 'workspace-or-schema-reset' }
        }

        New-Item -ItemType Directory -Force -Path $artifact | Out-Null
        $cacheState = [ordered]@{
            schema_version = $script:ClearraArtifactCacheSchemaVersion
            workspace_root = $repository
            session_key = $signature.signature
            input_file_count = $signature.input_file_count
            cargo_incremental = 0
            max_bytes = $MaxBytes
            action = $action
            cache_size_before_bytes = $sizeBefore
            initialized_utc = [DateTime]::UtcNow.ToString('o')
        }
        $temporaryState = "$statePath.$PID.tmp"
        $cacheState | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $temporaryState -Encoding UTF8
        Move-Item -LiteralPath $temporaryState -Destination $statePath -Force

        Remove-ClearraLegacyArtifactSurfaces $artifact
        Invoke-ClearraReportRetention $artifact
        return [pscustomobject]@{
            artifact_root = $artifact
            session_key = $signature.signature
            action = $action
            cache_size_before_bytes = $sizeBefore
            max_bytes = $MaxBytes
        }
    } finally {
        $lock.Dispose()
    }
}

function Ensure-ClearraBuildArtifactCache {
    $artifact = [System.IO.Path]::GetFullPath((Get-ClearraArtifactRoot))
    $statePath = Join-Path $artifact '.clearra-cache-state.json'
    $inheritedKey = $env:CLEARRA_BUILD_CACHE_SESSION_KEY
    if (-not [string]::IsNullOrWhiteSpace($inheritedKey) -and
        (Test-ClearraInheritedArtifactCacheOwner) -and
        (Test-Path -LiteralPath $statePath -PathType Leaf)) {
        try {
            $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
            if ($state.session_key -eq $inheritedKey) {
                $script:ClearraBuildCacheSessionKey = $inheritedKey
                $env:CARGO_INCREMENTAL = '0'
                return
            }
        } catch {}
    }

    $acquiredUsageLock = $false
    try {
        if ($null -eq $script:ClearraArtifactCacheUsageLock) {
            $script:ClearraArtifactCacheUsageLock = Enter-ClearraArtifactCacheUsageLock $artifact
            $env:CLEARRA_BUILD_CACHE_OWNER_PID = [string]$PID
            $acquiredUsageLock = $true
        }
        if (-not [string]::IsNullOrWhiteSpace($script:ClearraBuildCacheSessionKey) -and
            (Test-Path -LiteralPath $statePath -PathType Leaf)) {
            $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
            if ($state.session_key -eq $script:ClearraBuildCacheSessionKey) {
                $env:CARGO_INCREMENTAL = '0'
                return
            }
        }

        $result = Initialize-ClearraBuildArtifactCache
        $script:ClearraBuildCacheSessionKey = $result.session_key
        $env:CLEARRA_BUILD_CACHE_SESSION_KEY = $result.session_key
        $env:CARGO_INCREMENTAL = '0'
    } catch {
        if ($acquiredUsageLock) {
            Exit-ClearraBuildArtifactCacheUsage
        }
        throw
    }
}
