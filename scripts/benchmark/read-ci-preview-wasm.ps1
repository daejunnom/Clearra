# Read-only CI artifact transport for the local WASM benchmark's --wasm-stdin.
# No archive extraction, build, publication, cleanup or credential-file access.
param(
    [Parameter(Mandatory)][long]$ArtifactId,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{40}$')][string]$SourceCommit
)
$ErrorActionPreference = 'Stop'
if ($ArtifactId -lt 1) { throw 'Invalid artifact ID' }
$metadataText = & gh api "repos/daejunnom/Clearra/actions/artifacts/$ArtifactId"
if ($LASTEXITCODE -ne 0) { throw 'Artifact metadata unavailable' }
$metadata = $metadataText | ConvertFrom-Json
if ($metadata.expired -or $metadata.size_in_bytes -gt 33554432 -or
    $metadata.workflow_run.head_sha -ne $SourceCommit -or
    $metadata.name -notmatch "^unqualified-integration-preview-wasm-$SourceCommit-run-[0-9]+-attempt-[0-9]+$") {
    throw 'Artifact is not the expected bounded integration preview'
}
$info = [Diagnostics.ProcessStartInfo]::new('gh')
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardOutput = $true
$info.ArgumentList.Add('api')
$info.ArgumentList.Add("repos/daejunnom/Clearra/actions/artifacts/$ArtifactId/zip")
$process = [Diagnostics.Process]::Start($info)
$compressed = [IO.MemoryStream]::new()
try {
    $buffer = [byte[]]::new(65536)
    while (($read = $process.StandardOutput.BaseStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
        if ($compressed.Length + $read -gt 33554432) { $process.Kill(); throw 'Archive exceeds bounded input' }
        $compressed.Write($buffer, 0, $read)
    }
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw 'Artifact download failed' }
    if ($metadata.digest -notmatch '^sha256:([0-9a-f]{64})$' -or
        [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($compressed.ToArray())).ToLowerInvariant() -ne $Matches[1]) {
        throw 'Artifact ZIP digest mismatch'
    }
    $compressed.Position = 0
    $archive = [IO.Compression.ZipArchive]::new($compressed, [IO.Compression.ZipArchiveMode]::Read, $true)
    try {
        if ($archive.Entries.Count -lt 5 -or $archive.Entries.Count -gt 6) { throw 'Unexpected preview entry count' }
        $manifestEntry = $archive.GetEntry('clearra_wasm.manifest.json')
        # JSON whitespace/descriptor lengths are not artifact identity. Bound
        # the input, then check its schema and exact source/hash descriptors.
        if ($null -eq $manifestEntry -or $manifestEntry.Length -lt 1 -or $manifestEntry.Length -gt 131072) { throw 'Invalid manifest entry' }
        $inputStream = [IO.StreamReader]::new($manifestEntry.Open())
        try { $manifest = $inputStream.ReadToEnd() | ConvertFrom-Json } finally { $inputStream.Dispose() }
        if ($manifest.schema_version -ne 1 -or
            $manifest.build.runtime_identity.source_commit -ne $SourceCommit -or
            $manifest.build.runtime_identity.engine_build_id -ne $SourceCommit) { throw 'Manifest source mismatch' }
        $allowed = @('clearra_wasm.manifest.json', 'clearra_wasm.retention-history.json',
            'clearra_wasm.js', 'clearra_wasm_bg.wasm', $manifest.bindings.path, $manifest.wasm.path)
        $names = @($archive.Entries.FullName)
        if (@($names | Select-Object -Unique).Count -ne $names.Count -or
            @($names | Where-Object { $_ -notin $allowed }).Count -gt 0) { throw 'Unexpected or duplicate preview entry' }
        # The producer may include its bounded retention receipt. It is not
        # read, executed, or used as artifact/release authority by this probe.
        $history = $archive.GetEntry('clearra_wasm.retention-history.json')
        if ($null -ne $history -and $history.Length -gt 65536) { throw 'Unexpected retention receipt size' }
        $files = @{}
        foreach ($artifact in @($manifest.bindings, $manifest.wasm)) {
            if ($artifact.path -notmatch '^clearra_wasm(?:_bg)?\.[0-9a-f]{24}\.(js|wasm)$' -or
                $artifact.bytes -lt 1 -or $artifact.bytes -gt 33554432) { throw 'Invalid preview descriptor' }
            $entry = $archive.GetEntry($artifact.path)
            if ($null -eq $entry -or $entry.Length -ne $artifact.bytes) { throw 'Invalid artifact entry' }
            $inputStream = $entry.Open()
            $bytes = [IO.MemoryStream]::new()
            try {
                $inputStream.CopyTo($bytes)
                if ($bytes.Length -ne $artifact.bytes -or
                    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes.ToArray())).ToLowerInvariant() -ne $artifact.sha256) {
                    throw 'Preview artifact hash mismatch'
                }
                $files[$artifact.path] = [Convert]::ToBase64String($bytes.ToArray())
            } finally { $inputStream.Dispose(); $bytes.Dispose() }
        }
        @{ manifest = $manifest; files = $files } | ConvertTo-Json -Depth 20 -Compress
    } finally { $archive.Dispose() }
} finally { $process.Dispose(); $compressed.Dispose() }
