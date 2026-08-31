[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$wrapper = Join-Path $PSScriptRoot 'invoke-freeze-v080.ps1'
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'clearra-oracle-freeze-wrapper-test-' + [guid]::NewGuid().ToString('N')
)

function Invoke-ExtractedWrapperFunction {
    param(
        [Parameter(Mandatory = $true)][string] $Path,
        [Parameter(Mandatory = $true)][string] $FunctionName,
        [Parameter(Mandatory = $true)][object[]] $Arguments
    )
    $tokens = $null
    $parseErrors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile(
        $Path,
        [ref]$tokens,
        [ref]$parseErrors
    )
    if ($parseErrors.Count -ne 0) {
        throw "Wrapper parse failed while extracting $FunctionName."
    }
    $definitions = @($ast.FindAll({
        param($node)
        return $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -ceq $FunctionName
    }, $true))
    if ($definitions.Count -ne 1) {
        throw "Wrapper must define $FunctionName exactly once."
    }
    $invocation = [scriptblock]::Create(
        $definitions[0].Extent.Text + "`n& $FunctionName @args"
    )
    return & $invocation @Arguments
}

function Assert-ExactStringSequence {
    param(
        [Parameter(Mandatory = $true)][object[]] $Actual,
        [Parameter(Mandatory = $true)][string[]] $Expected,
        [Parameter(Mandatory = $true)][string] $Label
    )
    [string[]]$actualStrings = @($Actual | ForEach-Object { [string]$_ })
    if ($actualStrings.Count -ne $Expected.Count -or
        (Compare-Object -CaseSensitive -SyncWindow 0 $Expected $actualStrings)) {
        throw "$Label argument sequence drifted."
    }
}

$windowsTarget = 'C:\accepted\clearra-oracle-freeze-v080'
$windowsContract = Invoke-ExtractedWrapperFunction `
    -Path $wrapper `
    -FunctionName 'Get-OraclePosixSyntaxAuditContract' `
    -Arguments @('windows', $windowsTarget)
if ($windowsContract.ProjectionCommand -cne 'wsl.exe' -or
    $windowsContract.SyntaxCommand -cne 'wsl.exe') {
    throw 'Windows freeze syntax-audit command contract drifted.'
}
Assert-ExactStringSequence `
    -Actual @($windowsContract.ProjectionArguments) `
    -Expected @('-e', '/usr/bin/wslpath', '-a', '--', $windowsTarget) `
    -Label 'Windows freeze projection'
Assert-ExactStringSequence `
    -Actual @($windowsContract.SyntaxArguments) `
    -Expected @('-e', '/usr/bin/dash', '-n', '--') `
    -Label 'Windows freeze syntax audit'
$windowsSshConfig = Invoke-ExtractedWrapperFunction `
    -Path $wrapper -FunctionName 'Get-OracleSshConfigPath' -Arguments @('windows')
if ($windowsSshConfig -cne 'NUL') {
    throw 'Windows freeze SSH config path drifted.'
}

$linuxTarget = '/tmp/accepted/clearra-oracle-freeze-v080'
$linuxContract = Invoke-ExtractedWrapperFunction `
    -Path $wrapper `
    -FunctionName 'Get-OraclePosixSyntaxAuditContract' `
    -Arguments @('linux', $linuxTarget)
if ($null -ne $linuxContract.ProjectionCommand -or
    @($linuxContract.ProjectionArguments).Count -ne 0 -or
    $linuxContract.SyntaxCommand -cne '/usr/bin/dash') {
    throw 'Linux freeze syntax-audit command contract drifted.'
}
Assert-ExactStringSequence `
    -Actual @($linuxContract.SyntaxArguments) `
    -Expected @('-n', '--', $linuxTarget) `
    -Label 'Linux freeze syntax audit'
$linuxSshConfig = Invoke-ExtractedWrapperFunction `
    -Path $wrapper -FunctionName 'Get-OracleSshConfigPath' -Arguments @('linux')
if ($linuxSshConfig -cne '/dev/null') {
    throw 'Linux freeze SSH config path drifted.'
}
$wrapperSource = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $wrapper))
foreach ($requiredRemoteContract in @(
    "'--remote-overlay-archive', `$RemoteOverlayArchive",
    "'--remote-overlay-sha256', `$RemoteOverlaySha256"
)) {
    if ($wrapperSource.IndexOf($requiredRemoteContract, [StringComparison]::Ordinal) -lt 0) {
        throw "Freeze remote root-helper argument contract is missing: $requiredRemoteContract"
    }
}
foreach ($forbiddenRemoteRead in @(
    'function Copy-RemoteSealedOverlay',
    "'-o', '1001', '-g', '1001'",
    "'sudo', '-n', '/usr/bin/sha256sum', '--', `$RemoteOverlayArchive"
)) {
    if ($wrapperSource.IndexOf($forbiddenRemoteRead, [StringComparison]::Ordinal) -ge 0) {
        throw "Freeze wrapper regained private-overlay read authority: $forbiddenRemoteRead"
    }
}
$freezeHelperSource = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'clearra-oracle-freeze-v080'))
foreach ($sealedCopyMarker in @(
    'os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW',
    'metadata.st_nlink != 1 or metadata.st_size <= 0',
    'stat.S_IMODE(metadata.st_mode) & 0o022',
    'os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC | os.O_NOFOLLOW',
    'os.fsync(destination_fd)',
    'os.unlink(destination)',
    'expected_upload_count=6',
    'expected_upload_count=7'
)) {
    if ($freezeHelperSource.IndexOf($sealedCopyMarker, [StringComparison]::Ordinal) -lt 0) {
        throw "Freeze sealed-overlay fail-closed contract drifted: $sealedCopyMarker"
    }
}

try {
    [void][IO.Directory]::CreateDirectory($temporaryRoot)
    $source = Join-Path $temporaryRoot 'source.tar.gz'
    $overlay = Join-Path $temporaryRoot 'private-overlay-no-config.tar'
    $dist = Join-Path $temporaryRoot 'ctk3-dist.tar'
    $dependencies = Join-Path $temporaryRoot 'node_modules.tar'
    $manifest = Join-Path $temporaryRoot 'manifest.json'
    [IO.File]::WriteAllBytes($source, [byte[]](1, 2, 3, 4))
    [IO.File]::WriteAllBytes($overlay, [byte[]](5, 6, 7))
    [IO.File]::WriteAllBytes($dist, [byte[]](8, 9))
    [IO.File]::WriteAllBytes($dependencies, [byte[]](10, 11, 12, 13, 14))

    $arguments = @{
        SourceCommit = '0123456789abcdef0123456789abcdef01234567'
        SourceArchive = $source
        OverlayArchive = $overlay
        Ctk3DistArchive = $dist
        DependenciesArchive = $dependencies
        ManifestOutput = $manifest
        IdentityFile = Join-Path $temporaryRoot 'identity-must-not-be-read'
        AuditOnly = $true
    }
    $audit = @(& $wrapper @arguments)
    if ($audit.Count -ne 4 -or
        $audit[0] -cne 'oracle_freeze_invoker=audit-ok' -or
        $audit[1] -cne 'oracle_source_commit=0123456789abcdef0123456789abcdef01234567' -or
        $audit[2] -cne 'oracle_release_id=v0.8.0-0123456' -or
        $audit[3] -cnotmatch '^oracle_freeze_helper_sha256=[0-9a-f]{64}$') {
        throw 'AuditOnly attestation did not match.'
    }
    if (Test-Path -LiteralPath $manifest) {
        throw 'AuditOnly created a manifest output.'
    }

    $remoteOverlaySha256 = 'f' * 64
    $remoteOverlayArchive = "/opt/clearra/sealed-release-inputs/private-overlay-no-config-$remoteOverlaySha256.tar"
    $remoteArguments = [hashtable]$arguments.Clone()
    [void]$remoteArguments.Remove('OverlayArchive')
    $remoteArguments.RemoteOverlayArchive = $remoteOverlayArchive
    $remoteArguments.RemoteOverlaySha256 = $remoteOverlaySha256
    $remoteAudit = @(& $wrapper @remoteArguments)
    if ($remoteAudit.Count -ne 4 -or
        $remoteAudit[0] -cne 'oracle_freeze_invoker=audit-ok' -or
        (Test-Path -LiteralPath $manifest)) {
        throw 'Remote-overlay AuditOnly did not remain a typed local-only audit.'
    }

    $mismatchedRemoteArguments = [hashtable]$remoteArguments.Clone()
    $mismatchedRemoteArguments.RemoteOverlayArchive = "/opt/clearra/sealed-release-inputs/private-overlay-no-config-$('e' * 64).tar"
    $rejected = $false
    try {
        [void]@(& $wrapper @mismatchedRemoteArguments)
    } catch {
        $rejected = $_.Exception.Message -like '*Remote Oracle overlay authority is not canonical*'
    }
    if (-not $rejected) {
        throw 'Remote-overlay AuditOnly accepted a path/hash authority mismatch.'
    }

    $rejected = $false
    try {
        [void]@(& $wrapper @arguments `
            -RemoteOverlayArchive $remoteOverlayArchive `
            -RemoteOverlaySha256 $remoteOverlaySha256)
    } catch {
        $rejected = $true
    }
    if (-not $rejected) {
        throw 'Freeze wrapper accepted local and remote overlay inputs together.'
    }

    [IO.File]::WriteAllText($manifest, "occupied`n", [Text.UTF8Encoding]::new($false))
    $rejected = $false
    try {
        [void]@(& $wrapper @arguments)
    } catch {
        $rejected = $_.Exception.Message -like '*manifest output already exists*'
    }
    if (-not $rejected) {
        throw 'AuditOnly accepted an occupied manifest output path.'
    }

    'oracle_freeze_wrapper_test=pass'
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        $resolved = [IO.Path]::GetFullPath($temporaryRoot)
        $expectedParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if ($resolved.StartsWith($expectedParent, [StringComparison]::OrdinalIgnoreCase) -and
            [IO.Path]::GetFileName($resolved).StartsWith('clearra-oracle-freeze-wrapper-test-', [StringComparison]::Ordinal)) {
            Remove-Item -LiteralPath $resolved -Recurse -Force
        }
    }
}
