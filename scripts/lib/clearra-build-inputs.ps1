# Source/identity compatibility APIs used by source transport. No lifecycle or deletion.
$script:ClearraArtifactCacheSchemaVersion = 4

function Test-ClearraSecretOrGeneratedInput([System.IO.FileInfo]$File) {
    $name = $File.Name
    if ($name -eq 'pnpm-lock.yaml' -or
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

function Test-ClearraGeneratedInputDirectory([System.IO.DirectoryInfo]$Directory) {
    if ($Directory.Name -eq 'schemas' -and
        $Directory.Parent.Name -eq 'gen' -and
        $Directory.Parent.Parent.Name -eq 'src-tauri') {
        # Tauri generates and rewrites these ignored schemas during a GUI build.
        return $true
    }
    if ($Directory.Name -eq 'wasm' -and
        $Directory.Parent.Name -eq 'static' -and
        $Directory.Parent.Parent.Name -eq 'clearra-web' -and
        $Directory.Parent.Parent.Parent.Name -eq 'apps') {
        # The Pages build stages accepted WASM into this ignored web output.
        return $true
    }
    if ($Directory.Name -in @(
            '.git', '.cache', '.svelte-kit', '.vite-temp', '_local', 'dist', 'dist-server',
            'node_modules', 'build', 'models', 'checkpoints'
        )) {
        return $true
    }
    if ($Directory.Name -in @('target', 'coverage')) {
        # `src/target` and `src/coverage` are domain modules, while Cargo target
        # trees and report coverage trees are generated. Fixture/golden
        # directories likewise carry tracked build inputs.
        return $Directory.Parent.Name -notin @('src', 'fixtures', 'golden')
    }
    return $false
}

function Get-ClearraBuildInputFiles([string]$RepositoryRoot) {
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $files = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
    foreach ($name in @('Cargo.toml', 'Cargo.lock', 'CMakeLists.txt', 'package.json', 'rust-toolchain.toml', '.cargo/config.toml')) {
        $path = Join-Path $repository $name
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $file = [System.IO.FileInfo]::new($path)
            if (($file.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Clearra compiler snapshot refuses a linked input file: $($file.FullName)"
            }
            $files.Add($file)
        }
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
                    if (($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                        throw "Clearra compiler snapshot refuses a linked input directory: $($entry.FullName)"
                    }
                    if (-not (Test-ClearraGeneratedInputDirectory $entry)) {
                        $pending.Push($entry)
                    }
                    continue
                }
                if ($entry -is [System.IO.FileInfo] -and
                    -not (Test-ClearraSecretOrGeneratedInput $entry)) {
                    if (($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                        throw "Clearra compiler snapshot refuses a linked input file: $($entry.FullName)"
                    }
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
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        # Windows PowerShell can promote redirected native stderr to an error
        # under Stop. rustup's first-use diagnostics are not compiler identity.
        $ErrorActionPreference = 'Continue'
        $output = @(& $command.Source @Arguments 2>$null)
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            return "$Name-version=error-$exitCode"
        }
        return "$Name-version=$(($output -join '|').Trim())"
    } catch {
        return "$Name-version=error"
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
}

function Get-ClearraWorkspaceBuildSignature([string]$RepositoryRoot) {
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot)
    $sourceLines = [System.Collections.Generic.List[string]]::new()
    $sourceLines.Add('clearra.compiler-input-snapshot.v1')
    $inputFiles = @(Get-ClearraBuildInputFiles $repository)
    foreach ($file in $inputFiles) {
        $relative = $file.FullName.Substring($repository.Length).TrimStart('\', '/').Replace('\', '/')
        $fileSha = [System.Security.Cryptography.SHA256]::Create()
        $stream = $null
        try {
            $stream = [System.IO.File]::Open($file.FullName, 'Open', 'Read', 'Read')
            $digest = $fileSha.ComputeHash($stream)
        } finally {
            if ($null -ne $stream) { $stream.Dispose() }
            $fileSha.Dispose()
        }
        $fileDigest = ([System.BitConverter]::ToString($digest)).Replace('-', '').ToLowerInvariant()
        $sourceLines.Add("$relative`0$($file.Length)`0$fileDigest")
    }
    $contextLines = [System.Collections.Generic.List[string]]::new()
    $contextLines.Add('schema=clearra.incremental-context.v1')
    $contextLines.Add('owner=powershell-v1')
    $contextLines.Add("os=$([System.Environment]::OSVersion.VersionString)")
    $contextLines.Add("architecture=$([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture)")
    $contextLines.Add("execution_surface=$($env:CLEARRA_EXECUTION_SURFACE)")
    $contextLines.Add("rustflags=$($env:RUSTFLAGS)")
    $contextLines.Add("windows_rustflags=$($env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS)")
    $contextLines.Add("cargo_build_target=$($env:CARGO_BUILD_TARGET)")
    $contextLines.Add((Get-ClearraCommandMetadata 'cargo'))
    $contextLines.Add((Get-ClearraCommandMetadata 'rustc'))
    $contextLines.Add((Get-ClearraCommandMetadata 'cmake'))
    $contextLines.Add((Get-ClearraCommandVersionMetadata 'cargo' @('--version', '--verbose')))
    $contextLines.Add((Get-ClearraCommandVersionMetadata 'rustc' @('--version', '--verbose')))
    $contextLines.Add((Get-ClearraCommandVersionMetadata 'cmake' @('--version')))

    $sourceBytes = [System.Text.Encoding]::UTF8.GetBytes(($sourceLines -join "`n"))
    $contextBytes = [System.Text.Encoding]::UTF8.GetBytes(($contextLines -join "`n"))
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $sourceDigest = ([System.BitConverter]::ToString($sha.ComputeHash($sourceBytes))).Replace('-', '').ToLowerInvariant()
        $contextDigest = ([System.BitConverter]::ToString($sha.ComputeHash($contextBytes))).Replace('-', '').ToLowerInvariant()
        $signatureBytes = [System.Text.Encoding]::UTF8.GetBytes("source=$sourceDigest`ncontext=$contextDigest")
        $signatureDigest = $sha.ComputeHash($signatureBytes)
    } finally {
        $sha.Dispose()
    }
    return [pscustomobject]@{
        signature = ([System.BitConverter]::ToString($signatureDigest)).Replace('-', '').ToLowerInvariant()
        source_snapshot_sha256 = $sourceDigest
        incremental_context_sha256 = $contextDigest
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
