param(
    [Parameter(Mandatory)]
    [string]$CargoTargetDirectory,
    [switch]$Apply
)

# Debug-only retention. A compile unit needs its shared rlib/rmeta dependencies;
# those are NOT independent build generations and must not be pruned by mtime.
# Keep one incremental variant per unit and one linked executable/PDB pair per
# package/target kind. Equal crate names in different packages are not build
# generations of one another. Release profiles, WASM publication and sources are out
# of scope. No compiler or test is launched by this command.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-NoReparseAncestor([string]$Path) {
    $entry = Get-Item -LiteralPath $Path -Force
    while ($null -ne $entry) {
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Debug retention refuses a reparse point: $($entry.FullName)"
        }
        $entry = if ($entry -is [IO.FileInfo]) { $entry.Directory } else { $entry.Parent }
    }
}

function Get-DebugTreeMetadata([IO.DirectoryInfo]$Directory) {
    [int64]$bytes = 0
    $latest = $Directory.LastWriteTimeUtc
    $pending = [Collections.Generic.Stack[IO.DirectoryInfo]]::new()
    $pending.Push($Directory)
    while ($pending.Count -gt 0) {
        foreach ($entry in $pending.Pop().EnumerateFileSystemInfos()) {
            if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Debug retention refuses a nested reparse point: $($entry.FullName)"
            }
            if ($entry.LastWriteTimeUtc -gt $latest) { $latest = $entry.LastWriteTimeUtc }
            if ($entry -is [IO.DirectoryInfo]) { $pending.Push($entry) }
            else { $bytes += $entry.Length }
        }
    }
    return [pscustomobject]@{ Bytes = $bytes; Latest = $latest }
}

function Assert-DebugChild([string]$Path, [string]$Parent) {
    $absolute = [IO.Path]::GetFullPath($Path)
    $prefix = $Parent.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $absolute.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Debug retention target escaped the explicitly selected debug directory.'
    }
    Assert-NoReparseAncestor $absolute
    return $absolute
}

$targetRoot = [IO.Path]::GetFullPath($CargoTargetDirectory).TrimEnd('\', '/')
if ([IO.Path]::GetFileName($targetRoot) -notmatch '^(?:cargo-)?target(?:-[A-Za-z0-9_-]+)?$') {
    throw 'An explicitly named Cargo target directory is required.'
}
$debugRoot = Join-Path $targetRoot 'debug'
if (-not (Test-Path -LiteralPath $debugRoot -PathType Container)) {
    return [pscustomobject]@{ Status = 'absent'; DeletedCount = 0; FreedBytes = 0 }
}
Assert-NoReparseAncestor $debugRoot
if ($env:OS -eq 'Windows_NT' -and @(Get-Process -Name cargo,rustc,link -ErrorAction SilentlyContinue).Count -gt 0) {
    return [pscustomobject]@{ Status = 'busy'; DeletedCount = 0; FreedBytes = 0 }
}
if ($env:OS -eq 'Windows_NT') {
    $debugPrefix = $debugRoot + [IO.Path]::DirectorySeparatorChar
    foreach ($process in Get-Process) {
        $processPath = $null
        try { $processPath = $process.Path } catch { continue }
        if ($processPath -and $processPath.StartsWith($debugPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            return [pscustomobject]@{ Status = 'busy'; DeletedCount = 0; FreedBytes = 0 }
        }
    }
}
if ($Apply -and $env:OS -ne 'Windows_NT') {
    throw 'Applying this native PowerShell retention requires Windows Cargo file sharing locks.'
}
$lockPath = Join-Path $debugRoot '.cargo-lock'
if (-not (Test-Path -LiteralPath $lockPath -PathType Leaf)) {
    throw 'Debug retention requires the existing Cargo debug lock marker.'
}
$lock = $null
try {
    # Cargo must not be able to open the profile lock during pruning. A compiler
    # already using it causes a fail-safe skip instead of racing live outputs.
    try { $lock = [IO.File]::Open($lockPath, 'Open', 'ReadWrite', 'None') }
    catch [IO.IOException] {
        return [pscustomobject]@{ Status = 'busy'; DeletedCount = 0; FreedBytes = 0 }
    }
    $stale = [Collections.Generic.List[object]]::new()
    $retained = [Collections.Generic.List[string]]::new()
    $incrementalRoot = Join-Path $debugRoot 'incremental'
    if (Test-Path -LiteralPath $incrementalRoot -PathType Container) {
        Assert-NoReparseAncestor $incrementalRoot
        $variants = @(Get-ChildItem -LiteralPath $incrementalRoot -Directory -Force |
            Where-Object Name -Match '^[A-Za-z0-9_]+-[a-z0-9]+$' |
            ForEach-Object {
                $metadata = Get-DebugTreeMetadata $_
                [pscustomobject]@{
                    Unit = $_.Name -replace '-[a-z0-9]+$', ''
                    Path = $_.FullName; Latest = $metadata.Latest; Bytes = $metadata.Bytes
                    Directory = $true
                }
            })
        foreach ($group in @($variants | Group-Object Unit)) {
            $ordered = @($group.Group | Sort-Object Latest,Path -Descending)
            $retained.Add($ordered[0].Path)
            foreach ($old in @($ordered | Select-Object -Skip 1)) { $stale.Add($old) }
        }
    }
    $depsRoot = Join-Path $debugRoot 'deps'
    $unresolvedExecutableCount = 0
    if (Test-Path -LiteralPath $depsRoot -PathType Container) {
        Assert-NoReparseAncestor $depsRoot
        # Cargo's fingerprint directory binds the rustc output hash to its
        # package. Read marker NAMES only, never compiler JSON or dependency
        # contents. Missing/ambiguous ownership is preserved, not stem-grouped.
        $fingerprintsByHash = @{}
        $fingerprintRoot = Join-Path $debugRoot '.fingerprint'
        if (Test-Path -LiteralPath $fingerprintRoot -PathType Container) {
            Assert-NoReparseAncestor $fingerprintRoot
            foreach ($fingerprint in Get-ChildItem -LiteralPath $fingerprintRoot -Directory -Force) {
                if ($fingerprint.Name -notmatch '^(.+)-([0-9a-f]{16})$') { continue }
                Assert-NoReparseAncestor $fingerprint.FullName
                $package = $Matches[1]
                $outputHash = $Matches[2]
                $markers = @(Get-ChildItem -LiteralPath $fingerprint.FullName -File -Filter '*.json' |
                    Where-Object Name -Match '^(?:test-)?(?:lib|bin|integration-test|example|bench)-.+\.json$')
                foreach ($marker in $markers) {
                    Assert-NoReparseAncestor $marker.FullName
                    $unit = [pscustomobject]@{ Package = $package; Marker = $marker.BaseName }
                    $fingerprintsByHash[$outputHash] = @($fingerprintsByHash[$outputHash]) + @($unit)
                }
            }
        }
        # A PDB alone can be an interrupted link, not a completed executable.
        # Never pick such a file as the generation to retain or remove libraries.
        $executables = @(Get-ChildItem -LiteralPath $depsRoot -File -Filter '*.exe' |
            Where-Object { $_.Length -gt 0 -and $_.Name -match '^[A-Za-z0-9_]+-[0-9a-f]{16}\.exe$' })
        $ownedExecutables = @($executables | ForEach-Object {
            $executable = $_
            [void]($executable.Name -match '^(.+)-([0-9a-f]{16})\.exe$')
            $stem = $Matches[1]
            $outputHash = $Matches[2]
            $owners = @($fingerprintsByHash[$outputHash] | Where-Object {
                $null -ne $_ -and
                $_.Marker -match '^(?:test-)?(?:lib|bin|integration-test|example|bench)-(?<target>.+)$' -and
                $Matches['target'].Replace('-', '_') -eq $stem.Replace('-', '_')
            })
            $unit = 'unresolved:' + $executable.FullName
            if ($owners.Count -eq 1) { $unit = $owners[0].Package + ':' + $owners[0].Marker }
            else { $unresolvedExecutableCount++ }
            [pscustomobject]@{ Unit = $unit; File = $executable }
        })
        foreach ($group in @($ownedExecutables | Group-Object Unit)) {
            $ordered = @($group.Group | Sort-Object { $_.File.LastWriteTimeUtc },{ $_.File.Name } -Descending)
            $retained.Add($ordered[0].File.FullName)
            foreach ($old in @($ordered | Select-Object -Skip 1)) {
                foreach ($extension in @('.exe', '.pdb', '.d', '.exp')) {
                    $sidecar = Join-Path $depsRoot ($old.File.BaseName + $extension)
                    if (-not (Test-Path -LiteralPath $sidecar -PathType Leaf)) { continue }
                    $item = Get-Item -LiteralPath $sidecar -Force
                    $stale.Add([pscustomobject]@{ Path = $item.FullName; Bytes = $item.Length; Directory = $false })
                }
            }
        }
    }
    [int64]$plannedBytes = 0
    foreach ($candidate in $stale) {
        [void](Assert-DebugChild $candidate.Path $debugRoot)
        $plannedBytes += $candidate.Bytes
    }
    [int64]$freedBytes = 0
    $deletedCount = 0
    if ($Apply) {
        foreach ($candidate in $stale) {
            $absolute = Assert-DebugChild $candidate.Path $debugRoot
            if ($candidate.Directory) {
                [void](Get-DebugTreeMetadata (Get-Item -LiteralPath $absolute))
                Remove-Item -LiteralPath $absolute -Recurse -Force
            } else { Remove-Item -LiteralPath $absolute -Force }
            $freedBytes += $candidate.Bytes
            $deletedCount++
        }
    }
    [pscustomobject]@{
        Status = $(if ($Apply) { 'pruned' } else { 'planned' })
        DebugRoot = $debugRoot; RetainPerTarget = 1
        PlannedCount = $stale.Count; PlannedBytes = $plannedBytes
        DeletedCount = $deletedCount; FreedBytes = $freedBytes
        RetainedCount = $retained.Count
        UnresolvedExecutableCount = $unresolvedExecutableCount
    }
} finally {
    if ($null -ne $lock) { $lock.Dispose() }
}
