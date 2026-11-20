# One physical build root and one shared Node/PowerShell capacity catalog.
# Never resets a root or adopts legacy paths.
. (Join-Path $PSScriptRoot 'clearra-build-transaction-record.ps1')
. (Join-Path $PSScriptRoot 'clearra-build-inputs.ps1')
. (Join-Path $PSScriptRoot 'clearra-build-product-catalog.ps1')
if ($null -eq (Get-Variable -Name ClearraBuildTransaction -Scope Script -ErrorAction SilentlyContinue)) {
    $script:ClearraBuildTransaction = $null
    $script:ClearraArtifactCacheUsageLock = $null
    $script:ClearraBuildCacheSessionKey = $null
    $script:ClearraBuildPreviousEnvironment = $null
    $script:ClearraBuildTransactionOwned = $false
    $script:ClearraProductCatalogLease = $null
    $script:ClearraBuildCapacityCatalogLease = $null
}

function Invoke-ClearraBuildCapacityCheck(
    [string]$Root,
    [ValidateSet('admit','live','complete')][string]$Phase,
    [string]$TransactionRoot
) {
    $arguments = @(
        (Join-Path $PSScriptRoot '../tools/clearra-build-capacity.mjs'),
        'check', '--root', $Root, '--phase', $Phase
    )
    if (-not [string]::IsNullOrWhiteSpace($TransactionRoot)) {
        $arguments += @('--slot', $TransactionRoot)
    }
    $node = (Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    # Windows PowerShell 5 treats redirected native stderr as an ErrorRecord.
    # Preserve the same capacity diagnostic and child status on PS 5 and 7.
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $PSNativeCommandUseErrorActionPreference = $false
        $output = @(& $node @arguments 2>&1)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($exitCode -ne 0) {
        throw "Clearra build storage check failed during ${Phase}: $($output -join [Environment]::NewLine)"
    }
    return (($output -join [Environment]::NewLine) | ConvertFrom-Json)
}

function Get-ClearraBuildTransactionRoot(
    [string]$RepositoryRoot = (Resolve-ClearraBuildSourceRoot),
    [ValidateSet('experiment','product')][string]$Purpose = (Get-ClearraBuildPurpose)
) {
    $source = Get-ClearraCanonicalSourceRoot $RepositoryRoot
    Assert-ClearraBuildEnvironmentBeforeMutation $source
    if ($null -ne $script:ClearraBuildTransaction) {
        $record = $script:ClearraBuildTransaction
        if ($record.purpose -ne $Purpose -or
            -not ([string]$record.source_root).Equals($source, (Get-ClearraBuildPathComparison))) {
            throw 'A process cannot switch the purpose or source of its active build transaction.'
        }
        $verified = Get-ClearraInheritedBuildTransaction $source $Purpose
        if ($null -eq $verified -or $verified.session_id -ne $record.session_id) {
            throw 'The active process lost its build transaction environment binding.'
        }
        return (Assert-ClearraCanonicalBuildPath $record.transaction_root $source)
    }
    $inherited = Get-ClearraInheritedBuildTransaction $source $Purpose
    if ($null -ne $inherited) { return $inherited.transaction_root }
    if ($Purpose -eq 'product') { throw 'Select a product generation with Ensure-ClearraBuildArtifactCache first.' }
    $root = Get-ClearraCanonicalBuildRoot
    $sourceId = Get-ClearraBuildSourceIdentity $source
    return (Assert-ClearraCanonicalBuildPath (Join-Path $root "experiments/$sourceId/current") $source)
}

function Set-ClearraBuildTransactionEnvironment($Record) {
    $values = @{
        CLEARRA_BUILD_ROOT = (Get-ClearraCanonicalBuildRoot)
        CLEARRA_BUILD_PURPOSE = $Record.purpose
        CLEARRA_BUILD_SOURCE_ROOT = $Record.source_root
        CLEARRA_BUILD_SOURCE_ID = $Record.source_id
        CLEARRA_BUILD_SESSION_ID = $Record.session_id
        CLEARRA_BUILD_TRANSACTION_ROOT = $Record.transaction_root
        CLEARRA_BUILD_CACHE_OWNER_PID = [string]$Record.owner_pid
        CLEARRA_BUILD_CACHE_SESSION_KEY = $Record.session_id
        CARGO_TARGET_DIR = $Record.cargo_target_dir
        CARGO_INCREMENTAL = if ($Record.purpose -eq 'experiment') { '1' } else { '0' }
        RUSTC_WRAPPER = (Get-ClearraExpectedRustcWrapper)
    }
    foreach ($name in $values.Keys) { [Environment]::SetEnvironmentVariable($name, [string]$values[$name], 'Process') }
    # Resolve the guard after installing the new transaction root, including
    # bootstrap failure cleanup before the native executable exists.
    $env:RUSTC_WRAPPER = Get-ClearraExpectedRustcWrapper
}

function Initialize-ClearraNativeRustcLauncher {
    if (Test-StartTestsWindows) {
        & node (Join-Path $PSScriptRoot '../tools/prepare-clearra-rustc-launcher.mjs')
        if ($LASTEXITCODE -ne 0) { throw 'Native Windows compiler guard preparation failed.' }
    }
}

function Preserve-ClearraExperimentCompilerCache($Record, $Lease) {
    if ($Record.schema_version -ne 4 -or $Record.purpose -ne 'experiment' -or $Record.status -ne 'complete') {
        return $false
    }
    $verified = Read-ClearraBuildTransactionRecord $Record.transaction_root
    $leaseIdentity = Read-ClearraBuildLease $Lease.Path
    if ($verified.session_id -ne $Record.session_id -or $verified.source_id -ne $Record.source_id -or
        $leaseIdentity.owner_pid -ne $PID -or $leaseIdentity.source_id -ne $Record.source_id -or
        $leaseIdentity.purpose -ne 'experiment') {
        throw 'Incremental cache reuse requires the verified experiment owner lease.'
    }
    $cargo = Join-Path $verified.transaction_root 'cargo-target'
    if (-not (Test-Path -LiteralPath $cargo -PathType Container)) { return $false }
    Assert-ClearraBuildTreeNoReparse $verified.transaction_root
    Assert-ClearraBuildScratchRetired $verified.transaction_root
    foreach ($entry in Get-ChildItem -LiteralPath $verified.transaction_root -Force) {
        if ($entry.Name -in @('.clearra-build-transaction.json','cargo-target')) { continue }
        Remove-Item -LiteralPath $entry.FullName -Recurse -Force
    }
    return $true
}

function Initialize-ClearraBuildArtifactCache(
    [string]$RepositoryRoot = (Resolve-ClearraBuildSourceRoot),
    [string]$ArtifactRoot = (Get-ClearraArtifactRoot),
    [ValidateSet('experiment','product')][string]$Purpose = (Get-ClearraBuildPurpose)
) {
    $source = Get-ClearraCanonicalSourceRoot $RepositoryRoot
    $root = Get-ClearraCanonicalBuildRoot
    $comparison = Get-ClearraBuildPathComparison
    if (-not ([IO.Path]::GetFullPath($ArtifactRoot).TrimEnd('\','/')).Equals($root, $comparison)) {
        throw "ArtifactRoot must be the one canonical physical root: $root"
    }
    Assert-ClearraCanonicalBuildPath $root $source -AllowRoot | Out-Null
    Assert-ClearraBuildEnvironmentBeforeMutation $source
    if ($null -ne $script:ClearraBuildTransaction) {
        Get-ClearraBuildTransactionRoot $source $Purpose | Out-Null
        return $script:ClearraBuildTransaction
    }
    $inherited = Get-ClearraInheritedBuildTransaction $source $Purpose
    if ($null -ne $inherited) {
        $script:ClearraBuildTransaction = $inherited
        $script:ClearraBuildCacheSessionKey = $inherited.session_id
        $script:ClearraBuildTransactionOwned = $false
        Set-ClearraBuildTransactionEnvironment $inherited
        Initialize-ClearraNativeRustcLauncher
        return $inherited
    }
    $sourceId = Get-ClearraBuildSourceIdentity $source
    $session = [Guid]::NewGuid().ToString('N')
    $generation = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + $session
    $transaction = if ($Purpose -eq 'experiment') { Join-Path $root "experiments/$sourceId/current" }
                   else { Join-Path $root "products/$generation" }
    $transaction = Assert-ClearraCanonicalBuildPath $transaction $source
    $cargo = Join-Path $transaction 'cargo-target'
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR) -and
        -not ([IO.Path]::GetFullPath($env:CARGO_TARGET_DIR).TrimEnd('\','/')).Equals($cargo, $comparison)) {
        throw "CARGO_TARGET_DIR must be the selected transaction's exact cargo-target: $cargo"
    }
    $compilerSnapshot = Get-ClearraWorkspaceBuildSignature $source
    $record = [pscustomobject][ordered]@{
        schema_version = 4; purpose = $Purpose; source_root = $source; source_id = $sourceId
        session_id = $session; transaction_root = $transaction; cargo_target_dir = $cargo
        owner_pid = $PID; status = 'active'; created_utc = [DateTime]::UtcNow.ToString('o'); completed_utc = $null
        compiler_snapshot_sha256 = $compilerSnapshot.source_snapshot_sha256
        compiler_input_file_count = $compilerSnapshot.input_file_count
        incremental_cache_mode = if ($Purpose -eq 'experiment') { 'enabled' } else { 'disabled' }
        incremental_context_sha256 = $compilerSnapshot.incremental_context_sha256
        incremental_seed_session_id = $null
        incremental_seed_snapshot_sha256 = $null
    }
    # All explicit path/environment checks above precede this first mutation.
    $lease = $null
    $catalogLease = $null
    $capacityLease = $null
    try {
        # The Node and PowerShell owners take this same catalog lock before
        # either may change a transaction. An interrupted lock is never stolen.
        $capacityLease = Enter-ClearraBuildLease $record -CapacityCatalog
        if ($Purpose -eq 'product') {
            $catalogLease = Enter-ClearraBuildLease $record -ProductCatalog
            Assert-ClearraProductCatalogReady $source
        }
        Invoke-ClearraBuildCapacityCheck -Root $root -Phase admit -TransactionRoot $transaction | Out-Null
        $lease = Enter-ClearraBuildLease $record
        if (Test-Path -LiteralPath $transaction) {
            if ($Purpose -ne 'experiment') { throw 'A product generation must be new.' }
            $previous = Read-ClearraBuildTransactionRecord $transaction
            if ($previous.source_id -ne $sourceId -or $previous.purpose -ne 'experiment') {
                throw 'The existing experiment slot belongs to another source or purpose.'
            }
            if ($previous.status -eq 'active') {
                throw 'An active or interrupted experiment requires explicit owner-aware recovery.'
            }
            $reusable = $previous.schema_version -eq 4 -and $previous.status -eq 'complete' -and
                $previous.incremental_cache_mode -eq 'enabled' -and
                $previous.incremental_context_sha256 -eq $compilerSnapshot.incremental_context_sha256 -and
                (Preserve-ClearraExperimentCompilerCache $previous $lease)
            if ($reusable) {
                $record.incremental_seed_session_id = $previous.session_id
                $record.incremental_seed_snapshot_sha256 = $previous.compiler_snapshot_sha256
            } else {
                Remove-ClearraOwnedBuildTransaction $previous $lease
            }
        }
        New-Item -ItemType Directory -Path $transaction -Force | Out-Null
        Write-ClearraBuildTransactionRecord $record
        $saved = @{}
        foreach ($name in $script:ClearraBuildTransactionEnvironmentNames) {
            $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        }
        $script:ClearraBuildPreviousEnvironment = $saved
        $script:ClearraBuildTransaction = $record
        $script:ClearraArtifactCacheUsageLock = $lease
        $script:ClearraBuildCacheSessionKey = $session
        $script:ClearraBuildTransactionOwned = $true
        $script:ClearraProductCatalogLease = $catalogLease
        $script:ClearraBuildCapacityCatalogLease = $capacityLease
        Set-ClearraBuildTransactionEnvironment $record
        Initialize-ClearraNativeRustcLauncher
        return $record
    } catch {
        if ($script:ClearraBuildTransactionOwned -and $null -ne $script:ClearraBuildTransaction -and
            $script:ClearraBuildTransaction.session_id -eq $session) {
            # Bootstrap failures occur after ownership is installed. Mark the
            # generation failed before releasing its lease, using normal exit.
            Exit-ClearraBuildArtifactCacheUsage
        } else {
            if ($null -ne $lease) { Exit-ClearraBuildLease $lease }
            if ($null -ne $catalogLease) { Exit-ClearraBuildLease $catalogLease }
            if ($null -ne $capacityLease) { Exit-ClearraBuildLease $capacityLease }
        }
        throw
    }
}

function Ensure-ClearraBuildArtifactCache(
    [string]$RepositoryRoot = (Resolve-ClearraBuildSourceRoot),
    [ValidateSet('experiment','product')][string]$Purpose = (Get-ClearraBuildPurpose)
) {
    Initialize-ClearraBuildArtifactCache -RepositoryRoot $RepositoryRoot -Purpose $Purpose | Out-Null
}

function Test-ClearraBuildTransactionOwner {
    if ($null -eq $script:ClearraBuildTransaction -or -not $script:ClearraBuildTransactionOwned) { return $false }
    try {
        $record = Read-ClearraBuildTransactionRecord $script:ClearraBuildTransaction.transaction_root
        $lease = Read-ClearraBuildLease $script:ClearraArtifactCacheUsageLock.Path
        Assert-ClearraBuildCapacityLeaseIdentity $record
        if ($record.purpose -eq 'product') { Assert-ClearraProductCatalogLeaseIdentity $record }
        return $record.owner_pid -eq $PID -and $lease.owner_pid -eq $PID -and
            $record.session_id -eq $script:ClearraBuildTransaction.session_id -and
            $record.source_id -eq $script:ClearraBuildTransaction.source_id -and
            $record.purpose -eq $script:ClearraBuildTransaction.purpose -and
            $lease.session_id -eq $record.session_id -and $lease.source_id -eq $record.source_id -and
            $lease.purpose -eq $record.purpose
    } catch { return $false }
}

function Complete-ClearraBuildTransaction {
    if ($null -eq $script:ClearraBuildTransaction -or -not $script:ClearraBuildTransactionOwned) {
        throw 'Only the owning runner may complete a build transaction.'
    }
    $record = Read-ClearraBuildTransactionRecord $script:ClearraBuildTransaction.transaction_root
    $lease = Read-ClearraBuildLease $script:ClearraArtifactCacheUsageLock.Path
    Assert-ClearraBuildCapacityLeaseIdentity $record
    if ($record.purpose -eq 'product') { Assert-ClearraProductCatalogLeaseIdentity $record }
    if ($record.session_id -ne $script:ClearraBuildTransaction.session_id -or
        $record.source_id -ne $script:ClearraBuildTransaction.source_id -or
        $record.purpose -ne $script:ClearraBuildTransaction.purpose -or
        $lease.session_id -ne $record.session_id -or $lease.source_id -ne $record.source_id -or
        $lease.purpose -ne $record.purpose -or $lease.owner_pid -ne $PID -or $record.owner_pid -ne $PID) {
        throw 'Build completion owner/session binding mismatch.'
    }
    if ($record.status -eq 'complete') { return }
    if ($record.status -ne 'active') { throw 'Only an active transaction may be completed.' }
    Assert-ClearraBuildScratchRetired $record.transaction_root
    $finalSnapshot = Get-ClearraWorkspaceBuildSignature $record.source_root
    if ($finalSnapshot.source_snapshot_sha256 -ne $record.compiler_snapshot_sha256 -or
        $finalSnapshot.input_file_count -ne $record.compiler_input_file_count -or
        $finalSnapshot.incremental_context_sha256 -ne $record.incremental_context_sha256) {
        $sourceChanged = $finalSnapshot.source_snapshot_sha256 -ne $record.compiler_snapshot_sha256
        $countChanged = $finalSnapshot.input_file_count -ne $record.compiler_input_file_count
        $contextChanged = $finalSnapshot.incremental_context_sha256 -ne $record.incremental_context_sha256
        $record.status = 'failed'
        Write-ClearraBuildTransactionRecord $record
        $script:ClearraBuildTransaction = $record
        throw "Clearra build inputs changed while the transaction was active; the incremental seed was not sealed. source_changed=$sourceChanged input_count_changed=$countChanged context_changed=$contextChanged"
    }
    Invoke-ClearraBuildCapacityCheck -Root (Get-ClearraCanonicalBuildRoot) -Phase complete -TransactionRoot $record.transaction_root | Out-Null
    $record.status = 'complete'
    $record.completed_utc = [DateTime]::UtcNow.ToString('o')
    Write-ClearraBuildTransactionRecord $record
    $script:ClearraBuildTransaction = $record
}

function Invoke-ClearraBuildArtifactCacheRetention(
    [string]$RepositoryRoot = (Resolve-ClearraBuildSourceRoot),
    [string]$ArtifactRoot = (Get-ClearraArtifactRoot)
) {
    $root = Get-ClearraCanonicalBuildRoot
    if (-not ([IO.Path]::GetFullPath($ArtifactRoot)).Equals($root, (Get-ClearraBuildPathComparison))) {
        throw 'Retention cannot select a noncanonical artifact root.'
    }
    Assert-ClearraBuildEnvironmentBeforeMutation $RepositoryRoot
    Assert-ClearraCanonicalBuildPath $root $RepositoryRoot -AllowRoot | Out-Null
    # Compatibility name only. A read-only inventory cannot establish reader
    # lifetimes for deleting completed trees; fail closed instead of pruning.
    return Invoke-ClearraBuildCapacityCheck -Root $root -Phase live
}

function Exit-ClearraBuildArtifactCacheUsage {
    if ($null -eq $script:ClearraBuildTransaction) { return }
    if (-not $script:ClearraBuildTransactionOwned) {
        $script:ClearraBuildTransaction = $null
        $script:ClearraBuildCacheSessionKey = $null
        return
    }
    $record = $script:ClearraBuildTransaction
    $released = $false
    try {
        $verified = Read-ClearraBuildTransactionRecord $record.transaction_root
        $lease = Read-ClearraBuildLease $script:ClearraArtifactCacheUsageLock.Path
        Assert-ClearraBuildCapacityLeaseIdentity $record
        if ($record.purpose -eq 'product') { Assert-ClearraProductCatalogLeaseIdentity $record }
        if ($verified.session_id -ne $record.session_id -or $lease.session_id -ne $record.session_id -or
            $verified.source_id -ne $record.source_id -or $verified.purpose -ne $record.purpose -or
            $lease.source_id -ne $record.source_id -or $lease.purpose -ne $record.purpose -or
            $lease.owner_pid -ne $PID -or $verified.owner_pid -ne $PID) {
            throw 'Build exit ownership mismatch; the transaction and lease are preserved.'
        }
        if ($verified.status -eq 'active') {
            $verified.status = 'failed'
            Write-ClearraBuildTransactionRecord $verified
        }
        if ($verified.purpose -eq 'product' -and $verified.status -ne 'complete') {
            Remove-ClearraOwnedBuildTransaction $verified $script:ClearraArtifactCacheUsageLock
        }
        Exit-ClearraBuildLease $script:ClearraArtifactCacheUsageLock
        if ($record.purpose -eq 'product') {
            Exit-ClearraBuildLease $script:ClearraProductCatalogLease
        }
        Exit-ClearraBuildLease $script:ClearraBuildCapacityCatalogLease
        $released = $true
    } finally {
        foreach ($name in $script:ClearraBuildTransactionEnvironmentNames) {
            [Environment]::SetEnvironmentVariable($name, $script:ClearraBuildPreviousEnvironment[$name], 'Process')
        }
        $script:ClearraBuildTransaction = $null
        $script:ClearraBuildCacheSessionKey = $null
        $script:ClearraArtifactCacheUsageLock = $null
        $script:ClearraBuildPreviousEnvironment = $null
        $script:ClearraBuildTransactionOwned = $false
        $script:ClearraProductCatalogLease = $null
        $script:ClearraBuildCapacityCatalogLease = $null
        if (-not $released) { Write-Warning 'Build lease preserved because verified owner cleanup did not finish.' }
    }
}
