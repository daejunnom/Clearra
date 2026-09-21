# Ignored experiments are not product inputs. Inspect Git metadata and policy,
# never enumerate, read, execute, or clean the contents of _local.
function Assert-ClearraLocalToolDirectoryPolicy([string]$RepositoryRoot) {
    $repository = [System.IO.Path]::GetFullPath($RepositoryRoot).TrimEnd('\', '/')
    $localRoot = Join-Path $repository '_local'
    if (Test-Path -LiteralPath $localRoot) {
        $entry = Get-Item -LiteralPath $localRoot -Force
        if (-not $entry.PSIsContainer -or
            ($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'The ignored _local diagnostics boundary must be a real directory.'
        }
    }
    $ignoreFile = Join-Path $repository '.gitignore'
    $ignoreText = if (Test-Path -LiteralPath $ignoreFile -PathType Leaf) {
        Get-Content -LiteralPath $ignoreFile -Raw
    } else {
        ''
    }
    $fullyIgnored = $ignoreText -match '(?m)^/_local/\s*$'
    $managedSourcesOnly = $ignoreText -match '(?m)^/_local/\*\s*$' -and
        $ignoreText -match '(?m)^!/_local/clearra_manage\.py\s*$' -and
        $ignoreText -match '(?m)^!/_local/tests/\s*$' -and
        $ignoreText -match '(?m)^!/_local/tests/\*\*\s*$'
    if (-not $fullyIgnored -and -not $managedSourcesOnly) {
        throw 'Local diagnostics require the explicit /_local/ Git ignore boundary.'
    }
    $gitRoot = @(& git -C $repository rev-parse --show-toplevel 2>$null)
    if ($LASTEXITCODE -ne 0 -or $gitRoot.Count -ne 1 -or
        -not [System.IO.Path]::GetFullPath([string]$gitRoot[0]).TrimEnd('\', '/').Equals(
            $repository, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Local diagnostics policy requires the exact Git worktree root.'
    }
    $tracked = @(& git -C $repository -c core.quotepath=false ls-files -- _local)
    if ($LASTEXITCODE -ne 0) { throw 'Could not verify local diagnostics Git ownership.' }
    $deleted = @(& git -C $repository -c core.quotepath=false ls-files --deleted -- _local)
    if ($LASTEXITCODE -ne 0) { throw 'Could not verify intentionally removed local diagnostics.' }
    Assert-ClearraLocalGitOwnership $tracked $deleted -AllowManagementSources:$managedSourcesOnly
}

function Assert-ClearraLocalGitOwnership(
    [string[]]$TrackedPaths,
    [string[]]$DeletedPaths,
    [switch]$AllowManagementSources
) {
    $deleted = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($path in $DeletedPaths) { [void]$deleted.Add($path) }
    foreach ($path in $TrackedPaths) {
        # Compare the same Git-rendered names, including C-quoted unusual names.
        # Feeding a quoted name to Test-Path could misclassify a present file as
        # deleted. Intentional tracked deletion alone is allowed, never restored.
        $normalized = $path.Replace('\', '/')
        $isManagementSource = $AllowManagementSources.IsPresent -and
            ($normalized -ceq '_local/clearra_manage.py' -or
             $normalized.StartsWith('_local/tests/', [System.StringComparison]::Ordinal))
        if (-not $deleted.Contains($path) -and -not $isManagementSource) {
            throw "Local diagnostics must not be tracked product/release inputs: $path"
        }
    }
}

function Assert-ClearraProductExcludesLocalDiagnostics([object[]]$Files) {
    foreach ($file in $Files) {
        if ($file.Text -match '(?i)(?:^|[./\\"''\s])_local[/\\]') {
            throw "Product source or manifest references local diagnostics: $($file.RelativePath)"
        }
    }
}
