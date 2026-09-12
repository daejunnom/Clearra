# Source/identity compatibility APIs used by source transport. No lifecycle or deletion.
$script:ClearraArtifactCacheSchemaVersion = 3

function Test-ClearraSecretOrGeneratedInput([System.IO.FileInfo]$File) {
    $name = $File.Name
    if ($name -eq 'package-lock.json' -or
        $name -eq '.env' -or
        $name.StartsWith('.env.', [System.StringComparison]::OrdinalIgnoreCase)) {
        return $true
    }
    if ($name -match '(?i)(credential|service[-_]?account|api[-_]?key|^id_(rsa|dsa|ecdsa|ed25519)(\.|$)|^authorized_keys$)' -or
        $File.Extension -match '(?i)^\.(pem|key|pfx|p12)$') {
        return $true
    }
    return $false
}

function Get-ClearraBuildInputFiles([string]$RepositoryRoot) {
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $files = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
    foreach ($name in @('Cargo.toml', 'Cargo.lock', 'CMakeLists.txt', 'package.json', '.cargo/config.toml')) {
        $path = Join-Path $repository $name
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $files.Add([System.IO.FileInfo]::new($path))
        }
    }

    $excludedDirectories = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($name in @(
            '.git', '.cache', '.svelte-kit', '.vite-temp', '_local', 'dist', 'dist-server', 'node_modules',
            'target', 'build', 'coverage', 'models', 'checkpoints'
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
