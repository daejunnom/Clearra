# Wire-path identity is platform-independent. In particular, a POSIX source
# must never be interpreted as C:\home by a Windows metadata reader.
function Get-ClearraBuildMetadataPathIdentity([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path) -or $Path -match '[\x00-\x1f]') { throw 'Invalid build metadata path.' }
    $windows = $false
    if ($Path -match '^([A-Za-z]):[\\/](.*)$') {
        $prefix = $Matches[1].ToLowerInvariant() + ':/'
        $tail = $Matches[2].Replace('\','/')
        $windows = $true
    } elseif ($Path -cmatch '^/mnt/([a-z])/(.*)$') {
        $prefix = $Matches[1] + ':/'
        $tail = $Matches[2].Replace('\','/')
        $windows = $true
    } elseif ($Path.StartsWith('/') -and -not $Path.StartsWith('//')) {
        $prefix = '/'
        $tail = $Path.Substring(1)
    } else { throw 'Build metadata paths must be absolute local Windows or POSIX paths.' }
    if ($windows -and $tail.Contains(':')) { throw 'Alternate streams are forbidden in build metadata paths.' }
    $segments = [Collections.Generic.List[string]]::new()
    foreach ($segment in $tail.Split('/')) {
        if ($segment -in @('', '.')) { continue }
        if ($segment -eq '..') {
            if ($segments.Count -gt 0) { $segments.RemoveAt($segments.Count - 1) }
        } else { $segments.Add($segment) }
    }
    $identity = $prefix + ($segments -join '/')
    if ($windows) { return $identity.ToLowerInvariant() }
    return $identity
}

function Get-ClearraBuildMetadataSourceIdentity([string]$SourceRoot) {
    $identity = Get-ClearraBuildMetadataPathIdentity $SourceRoot
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $digest = $sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($identity)) }
    finally { $sha.Dispose() }
    return ([BitConverter]::ToString($digest)).Replace('-', '').ToLowerInvariant().Substring(0, 24)
}

function ConvertTo-ClearraNativeMetadataBuildPath([string]$Path) {
    $identity = Get-ClearraBuildMetadataPathIdentity $Path
    if ($identity -match '^([a-z]):/(.*)$') {
        if (Test-StartTestsWindows) { return $identity.Replace('/','\') }
        return ('/mnt/' + $Matches[1] + '/' + $Matches[2])
    }
    if (Test-StartTestsWindows) { throw 'A Windows build output must map to a local Windows drive.' }
    return $identity
}

function Get-ClearraBuildRecordLocalSafetyRoot($Record) {
    $identity = Get-ClearraBuildMetadataPathIdentity $Record.source_root
    if ((Test-StartTestsWindows) -and $identity.StartsWith('/')) {
        # A WSL ext4 source has no local Windows path. Its identity is verified
        # separately; use the policy tree solely for the local no-source-delete
        # boundary, never as the foreign source's identity or owner authority.
        return $script:ClearraPathPolicyRepositoryRoot
    }
    return (Get-ClearraCanonicalSourceRoot (ConvertTo-ClearraNativeMetadataBuildPath $Record.source_root) -AllowMissing)
}
