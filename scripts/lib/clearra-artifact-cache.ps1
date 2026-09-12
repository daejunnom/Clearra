# One physical build root; one replaceable experiment per source; five completed
# product generations. Never resets a root or cleans legacy paths.
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
        CARGO_INCREMENTAL = '0'
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
    $record = [pscustomobject][ordered]@{
        schema_version = 3; purpose = $Purpose; source_root = $source; source_id = $sourceId
        session_id = $session; transaction_root = $transaction; cargo_target_dir = $cargo
        owner_pid = $PID; status = 'active'; created_utc = [DateTime]::UtcNow.ToString('o'); completed_utc = $null
    }
    # All explicit path/environment checks above precede this first mutation.
    $lease = $null
    $catalogLease = $null
    try {
        if ($Purpose -eq 'product') {
            $catalogLease = Enter-ClearraBuildLease $record -ProductCatalog
            Assert-ClearraProductCatalogReady $source
            Invoke-ClearraBuildArtifactCacheRetention -RepositoryRoot $source | Out-Null
        }
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
            Remove-ClearraOwnedBuildTransaction $previous $lease
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
    $products = Assert-ClearraCanonicalBuildPath (Join-Path $root 'products') $RepositoryRoot
    $deleted = [Collections.Generic.List[string]]::new()
    if (-not (Test-Path -LiteralPath $products -PathType Container)) {
        return [pscustomobject]@{ action = 'absent'; retained_completed = 0; deleted = @() }
    }
    $completed = @()
    foreach ($directory in Get-ChildItem -LiteralPath $products -Directory -Force) {
        try {
            $record = Read-ClearraBuildTransactionRecord $directory.FullName
            if ($record.purpose -eq 'product' -and $record.status -eq 'complete') { $completed += $record }
        } catch { throw "Unverified product generation preserved; retention aborted: $($directory.FullName)" }
    }
    $ordered = @($completed | Sort-Object @{ Expression = { [DateTimeOffset]::Parse($_.completed_utc) }; Descending = $true },
        @{ Expression = { $_.session_id }; Descending = $true })
    foreach ($record in @($ordered | Select-Object -Skip 5)) {
        if (Test-Path -LiteralPath (Get-ClearraBuildLeasePath $record)) { continue }
        $lease = $null
        try {
            $lease = Enter-ClearraBuildLease $record
            $verified = Read-ClearraBuildTransactionRecord $record.transaction_root
            if ($verified.status -ne 'complete' -or $verified.session_id -ne $record.session_id) {
                throw 'Product generation changed during retention.'
            }
            Remove-ClearraOwnedBuildTransaction $verified $lease
            $deleted.Add($record.transaction_root)
        } catch {
            if ($null -ne $lease -or -not (Test-Path -LiteralPath (Get-ClearraBuildLeasePath $record))) { throw }
            # Another owner atomically claimed this generation after our scan.
        }
        finally { if ($null -ne $lease) { Exit-ClearraBuildLease $lease } }
    }
    return [pscustomobject]@{ action = 'product-generation-retention'; retained_completed = $completed.Count - $deleted.Count; deleted = $deleted.ToArray() }
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
            Invoke-ClearraBuildArtifactCacheRetention -RepositoryRoot $record.source_root | Out-Null
            Exit-ClearraBuildLease $script:ClearraProductCatalogLease
        }
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
        if (-not $released) { Write-Warning 'Build lease preserved because verified owner cleanup did not finish.' }
    }
}
