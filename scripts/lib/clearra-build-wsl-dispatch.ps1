# A Windows transaction cannot be inherited by an ext4 source copy: both the
# canonical source identity and PID namespace differ. WSL builds start their
# own experiment owner, using the same physical mounted Windows build root.
function Assert-ClearraWslDispatchSource([string]$LinuxSourceRoot) {
    if ($LinuxSourceRoot -notmatch '^/home/[^/]+/\.local/share/Clearra/workspaces/[0-9a-f]{16}/source$' -or
        $LinuxSourceRoot -match '[\x00-\x1f]' -or $LinuxSourceRoot.Contains('/../') -or
        $LinuxSourceRoot.Contains('/./')) {
        throw 'An independent WSL build requires a validated ext4 source-copy path.'
    }
    return $LinuxSourceRoot
}

function ConvertTo-ClearraWslBuildPath([string]$WindowsPath, [string]$Distribution = 'Ubuntu') {
    if ($Distribution -notmatch '^[A-Za-z0-9._-]+$') { throw 'Unsafe WSL distribution name.' }
    $path = Assert-ClearraNoReparseBuildPath $WindowsPath
    $mapped = @(& wsl.exe -d $Distribution -- wslpath -a -u $path 2>$null)
    if ($LASTEXITCODE -ne 0 -or $mapped.Count -ne 1 -or
        ([string]$mapped[0]) -notmatch '^/mnt/[a-z]/' -or ([string]$mapped[0]) -match '[\x00-\x1f]') {
        throw 'Cannot map the Windows build authority into WSL.'
    }
    return ([string]$mapped[0]).TrimEnd('/')
}

function Get-ClearraIndependentWslTransactionRoot(
    [string]$LinuxSourceRoot,
    [string]$Distribution = 'Ubuntu'
) {
    $source = Assert-ClearraWslDispatchSource $LinuxSourceRoot
    $root = ConvertTo-ClearraWslBuildPath (Get-ClearraCanonicalBuildRoot) $Distribution
    # Linux source identity is case-sensitive and uses slash-separated paths.
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $digest = $sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($source)) }
    finally { $sha.Dispose() }
    $sourceId = ([BitConverter]::ToString($digest)).Replace('-', '').ToLowerInvariant().Substring(0, 24)
    return "$root/experiments/$sourceId/current"
}

function New-ClearraIndependentWslBuildArguments(
    [string]$LinuxSourceRoot,
    [string]$Distribution = 'Ubuntu',
    [ValidateSet('wsl-core-c-tests.sh', 'wsl-native-cargo.sh', 'wsl-pc-runtime-build-and-batch.sh')][string]$ScriptName,
    [string[]]$CommandArguments = @(),
    [hashtable]$AdditionalEnvironment = @{}
) {
    $source = Assert-ClearraWslDispatchSource $LinuxSourceRoot
    # Reject raw aliases before deliberately clearing the validated Windows
    # binding. Do not silently repair an externally requested output path.
    Assert-ClearraBuildEnvironmentBeforeMutation (Resolve-ClearraBuildSourceRoot)
    foreach ($name in $AdditionalEnvironment.Keys) {
        if ($name -ne 'CLEARRA_WSL_ENABLE_STAGE_PROFILING' -or
            [string]$AdditionalEnvironment[$name] -notin @('0', '1')) {
            throw "Unsupported independent WSL build environment setting: $name"
        }
    }
    $authority = ConvertTo-ClearraWslBuildPath $script:ClearraPathPolicyRepositoryRoot $Distribution
    $root = ConvertTo-ClearraWslBuildPath (Get-ClearraCanonicalBuildRoot) $Distribution
    $arguments = [Collections.Generic.List[string]]::new()
    $arguments.AddRange([string[]]@('-d', $Distribution, '--', 'env'))
    foreach ($name in @($script:ClearraBuildTransactionEnvironmentNames) + @(
        'CARGO_BUILD_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR', 'CARGO_BUILD_RUSTC_WRAPPER',
        'RUSTC_WORKSPACE_WRAPPER', 'CLEARRA_WSL_CARGO_TARGET_DIR', 'CLEARRA_RELEASE_BUILD_ROOT',
        'CLEARRA_WSL_NATIVE_BUILD_ROOT', 'CLEARRA_CORE_C_BUILD_DIR', 'CLEARRA_WSL_WORKSPACE'
    )) {
        $arguments.Add('-u')
        $arguments.Add($name)
    }
    $arguments.AddRange([string[]]@(
        "CLEARRA_BUILD_ROOT=$root", 'CLEARRA_BUILD_PURPOSE=experiment', "CLEARRA_WSL_WORKSPACE=$source"
    ))
    foreach ($name in $AdditionalEnvironment.Keys) { $arguments.Add("$name=$($AdditionalEnvironment[$name])") }
    # Use current policy code, not the potentially older source snapshot's
    # launcher. The shell establishes a Linux-only PATH before starting Node.
    $arguments.AddRange([string[]]@('bash', "$authority/scripts/tools/$ScriptName"))
    $arguments.AddRange([string[]]$CommandArguments)
    return $arguments.ToArray()
}
