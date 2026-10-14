# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Get-SrpGovernedFiles() {
    $cacheVariable = Get-Variable -Name SrpGovernedFilesCache -Scope Script -ErrorAction SilentlyContinue
    if ($null -ne $cacheVariable -and $null -ne $cacheVariable.Value) {
        return $cacheVariable.Value
    }

    $extensions = @(
        '.rs', '.c', '.h', '.ps1', '.psm1', '.psd1', '.py',
        '.js', '.jsx', '.ts', '.tsx', '.svelte', '.sh', '.cmake'
    )
    $excludedSegments = @(
        'node_modules', 'dist', 'dist-server', 'build', 'coverage',
        'models', 'checkpoints', '.cache', '.svelte-kit', 'target', 'vendor', 'history'
    )
    $files = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
    foreach ($relativeRoot in @('crates', 'core-c', 'scripts', 'tools', 'apps', 'packages', 'gui')) {
        $searchRoot = Join-Path $Root $relativeRoot
        if (-not (Test-Path -LiteralPath $searchRoot)) { continue }
        Get-ChildItem -LiteralPath $searchRoot -Recurse -File | Where-Object {
            $extensionIncluded = $extensions -contains $_.Extension.ToLowerInvariant() -or
                $_.Name -eq 'CMakeLists.txt'
            $pathSegments = $_.FullName.Substring($searchRoot.Length).Split(
                [System.IO.Path]::DirectorySeparatorChar,
                [System.StringSplitOptions]::RemoveEmptyEntries
            )
            $excluded = @($pathSegments | Where-Object { $excludedSegments -contains $_ }).Count -gt 0
            $extensionIncluded -and -not $excluded
        } | ForEach-Object { $files.Add($_) }
    }
    $script:SrpGovernedFilesCache = @($files)
    return $script:SrpGovernedFilesCache
}
