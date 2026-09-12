$script:ClearraBuildTransactionEnvironmentNames = @(
    'CLEARRA_BUILD_ROOT', 'CLEARRA_BUILD_PURPOSE', 'CLEARRA_BUILD_SOURCE_ROOT',
    'CLEARRA_BUILD_SOURCE_ID', 'CLEARRA_BUILD_SESSION_ID', 'CLEARRA_BUILD_TRANSACTION_ROOT',
    'CLEARRA_BUILD_CACHE_OWNER_PID', 'CLEARRA_BUILD_CACHE_SESSION_KEY',
    'CARGO_TARGET_DIR', 'CARGO_INCREMENTAL', 'RUSTC_WRAPPER'
)

function Resolve-ClearraBuildSourceRoot {
    if (-not [string]::IsNullOrWhiteSpace($env:CLEARRA_BUILD_SOURCE_ROOT)) {
        return $env:CLEARRA_BUILD_SOURCE_ROOT
    }
    return (Resolve-ClearraRoot)
}

function Get-ClearraBuildPurpose {
    $purpose = $env:CLEARRA_BUILD_PURPOSE
    if ([string]::IsNullOrWhiteSpace($purpose)) { return 'experiment' }
    if ($purpose -notin @('experiment', 'product')) { throw 'Build purpose must be experiment or product.' }
    return $purpose
}

function Get-ClearraExpectedRustcWrapper {
    $name = if (Test-StartTestsWindows) { 'clearra-rustc-guard.cmd' } else { 'clearra-rustc-guard.sh' }
    return [IO.Path]::GetFullPath((Join-Path $script:ClearraPathPolicyRepositoryRoot "scripts/tools/$name"))
}

function Assert-ClearraBuildEnvironmentBeforeMutation([string]$RepositoryRoot) {
    # Validate user-supplied environment BEFORE acquiring/creating any slot.
    foreach ($name in @('CARGO_BUILD_TARGET_DIR','CARGO_BUILD_BUILD_DIR','CARGO_BUILD_RUSTC_WRAPPER',
            'CLEARRA_WSL_CARGO_TARGET_DIR','CLEARRA_RELEASE_BUILD_ROOT','CLEARRA_WSL_NATIVE_BUILD_ROOT',
            'CLEARRA_CORE_C_BUILD_DIR','RUSTC_WORKSPACE_WRAPPER')) {
        if (-not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name, 'Process'))) {
            throw "Build path/wrapper alias $name is not supported; use the canonical managed transaction."
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        Assert-ClearraCanonicalBuildPath $env:CARGO_TARGET_DIR $RepositoryRoot | Out-Null
    }
    if (-not [string]::IsNullOrWhiteSpace($env:RUSTC_WRAPPER)) {
        $wrapper = [IO.Path]::GetFullPath($env:RUSTC_WRAPPER)
        if (-not $wrapper.Equals((Get-ClearraExpectedRustcWrapper), (Get-ClearraBuildPathComparison))) {
            throw 'Unmanaged RUSTC_WRAPPER chaining is not supported; use the official build owner.'
        }
    }
}

function Get-ClearraBuildLeasePath($Record) {
    $root = Get-ClearraCanonicalBuildRoot
    $key = if ($Record.purpose -eq 'experiment') { "experiment-$($Record.source_id)" }
           else { "product-$($Record.session_id)" }
    return (Assert-ClearraCanonicalBuildPath (Join-Path $root ".leases/$key.lock") (Get-ClearraBuildRecordLocalSafetyRoot $Record))
}

function Read-ClearraBuildTransactionRecord([string]$TransactionRoot) {
    $TransactionRoot = ConvertTo-ClearraNativeMetadataBuildPath $TransactionRoot
    $marker = Join-Path $TransactionRoot '.clearra-build-transaction.json'
    Assert-ClearraNoReparseBuildPath $marker | Out-Null
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
        throw "Managed build transaction metadata is missing: $marker"
    }
    if ((Get-Item -LiteralPath $marker).Length -gt 16384) { throw 'Build transaction metadata is too large.' }
    $record = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
    $expected = @('schema_version','purpose','source_root','source_id','session_id','transaction_root',
                  'cargo_target_dir','owner_pid','status','created_utc','completed_utc') | Sort-Object
    $actual = @($record.PSObject.Properties.Name) | Sort-Object
    if (($actual -join '|') -ne ($expected -join '|') -or $record.schema_version -ne 3 -or
        $record.purpose -notin @('experiment','product') -or
        $record.source_id -notmatch '^[0-9a-f]{24}$' -or $record.session_id -notmatch '^[0-9a-f]{32}$' -or
        $record.status -notin @('active','complete','failed') -or $record.owner_pid -le 0) {
        throw "Invalid managed build transaction metadata: $marker"
    }
    if ($record.source_id -ne (Get-ClearraBuildMetadataSourceIdentity $record.source_root)) { throw 'Build source identity mismatch.' }
    $source = Get-ClearraBuildRecordLocalSafetyRoot $record
    $wireRoot = ConvertTo-ClearraNativeMetadataBuildPath $record.transaction_root
    $wireCargo = ConvertTo-ClearraNativeMetadataBuildPath $record.cargo_target_dir
    $root = Get-ClearraCanonicalBuildRoot
    $expectedRoot = if ($record.purpose -eq 'experiment') {
        Join-Path $root "experiments/$($record.source_id)/current"
    } else {
        $generation = [IO.Path]::GetFileName($wireRoot)
        if ($generation -notmatch ('^[0-9]{8}T[0-9]{9}Z-' + $record.session_id + '$')) {
            throw 'Invalid product generation identity.'
        }
        Join-Path $root "products/$generation"
    }
    $comparison = Get-ClearraBuildPathComparison
    $actualRoot = Assert-ClearraCanonicalBuildPath $TransactionRoot $source
    if (-not $actualRoot.Equals([IO.Path]::GetFullPath($expectedRoot), $comparison) -or
        -not $actualRoot.Equals([IO.Path]::GetFullPath($wireRoot), $comparison) -or
        -not ([IO.Path]::GetFullPath($wireCargo)).Equals((Join-Path $actualRoot 'cargo-target'), $comparison)) {
        throw 'Build transaction path binding mismatch.'
    }
    $created = [DateTimeOffset]::MinValue
    if (-not [DateTimeOffset]::TryParse($record.created_utc, [ref]$created)) { throw 'Invalid transaction creation time.' }
    if ($record.status -eq 'complete') {
        $completed = [DateTimeOffset]::MinValue
        if (-not [DateTimeOffset]::TryParse($record.completed_utc, [ref]$completed) -or $completed -lt $created) {
            throw 'Invalid transaction completion time.'
        }
    } elseif ($null -ne $record.completed_utc) { throw 'An incomplete transaction cannot have a completion time.' }
    # Return native filesystem paths for retention, preserving the original
    # source-root wire spelling. This does not authorize foreign active owners.
    $record.transaction_root = $actualRoot
    $record.cargo_target_dir = Join-Path $actualRoot 'cargo-target'
    return $record
}

function Write-ClearraBuildTransactionRecord($Record) {
    $path = Join-Path $Record.transaction_root '.clearra-build-transaction.json'
    $temporary = "$path.tmp"
    Assert-ClearraPathInBuildTransaction $path $Record.transaction_root $Record.source_root | Out-Null
    Assert-ClearraNoReparseBuildPath $temporary | Out-Null
    $json = ($Record | ConvertTo-Json -Depth 4) + "`n"
    [IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination $path -Force
}

function Enter-ClearraBuildLease($Record, [switch]$ProductCatalog) {
    if ($ProductCatalog -and $Record.purpose -ne 'product') { throw 'Only product owners may hold the product catalog.' }
    $path = if ($ProductCatalog) {
        Assert-ClearraCanonicalBuildPath (Join-Path (Get-ClearraCanonicalBuildRoot) '.leases/products-catalog.lock') (Get-ClearraBuildRecordLocalSafetyRoot $Record)
    } else { Get-ClearraBuildLeasePath $Record }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    $lease = [pscustomobject][ordered]@{
        schema_version = 1; purpose = $Record.purpose; source_id = $Record.source_id
        session_id = $Record.session_id; owner_pid = $PID
    }
    try {
        $claim = [IO.File]::Open($path, 'CreateNew', 'Write', 'None')
    } catch [IO.IOException] { throw "Build transaction lease already exists; active or stale owners are preserved: $path" }
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes(($lease | ConvertTo-Json -Compress))
        $claim.Write($bytes, 0, $bytes.Length)
        $claim.Flush()
    } finally { $claim.Dispose() }
    return [pscustomobject]@{ Path = $path; Record = $lease; ProductCatalog = [bool]$ProductCatalog }
}

function Read-ClearraBuildLease([string]$Path) {
    Assert-ClearraNoReparseBuildPath $Path | Out-Null
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw 'The build lease is missing.' }
    $lease = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    if ($lease.schema_version -ne 1 -or $lease.session_id -notmatch '^[0-9a-f]{32}$' -or
        $lease.source_id -notmatch '^[0-9a-f]{24}$' -or $lease.purpose -notin @('experiment','product') -or
        $lease.owner_pid -le 0) { throw 'Invalid build lease metadata.' }
    return $lease
}

function Exit-ClearraBuildLease($Lease) {
    $key = if ($Lease.Record.purpose -eq 'experiment') { "experiment-$($Lease.Record.source_id)" }
           else { "product-$($Lease.Record.session_id)" }
    $catalog = $Lease.PSObject.Properties.Name -contains 'ProductCatalog' -and $Lease.ProductCatalog
    if ($catalog -and $Lease.Record.purpose -ne 'product') { throw 'Invalid product catalog lease.' }
    $expected = if ($catalog) { Join-Path (Get-ClearraCanonicalBuildRoot) '.leases/products-catalog.lock' }
                else { Join-Path (Get-ClearraCanonicalBuildRoot) ".leases/$key.lock" }
    if (-not ([IO.Path]::GetFullPath($Lease.Path)).Equals($expected, (Get-ClearraBuildPathComparison))) {
        throw 'Build lease release path binding mismatch.'
    }
    $actual = Read-ClearraBuildLease $Lease.Path
    if ($actual.session_id -ne $Lease.Record.session_id -or $actual.source_id -ne $Lease.Record.source_id -or
        $actual.purpose -ne $Lease.Record.purpose -or $actual.owner_pid -ne $PID) {
        throw 'Refusing to release a build lease owned by another session.'
    }
    Remove-Item -LiteralPath $Lease.Path -Force
}

function Get-ClearraInheritedBuildTransaction([string]$RepositoryRoot, [string]$Purpose) {
    $binding = @($env:CLEARRA_BUILD_SOURCE_ROOT, $env:CLEARRA_BUILD_SOURCE_ID,
        $env:CLEARRA_BUILD_SESSION_ID, $env:CLEARRA_BUILD_TRANSACTION_ROOT, $env:CLEARRA_BUILD_CACHE_OWNER_PID)
    $present = @($binding | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    if ($present -eq 0) { return $null }
    if ($present -ne $binding.Count -or [string]::IsNullOrWhiteSpace($env:CLEARRA_BUILD_ROOT)) {
        throw 'Incomplete inherited build transaction binding.'
    }
    if ((Test-StartTestsWindows) -and $env:CLEARRA_BUILD_SOURCE_ROOT -notmatch '^[A-Za-z]:[\\/]') {
        throw 'A foreign-platform build owner cannot be inherited by Windows.'
    }
    $transaction = Assert-ClearraCanonicalBuildPath $env:CLEARRA_BUILD_TRANSACTION_ROOT $RepositoryRoot
    $record = Read-ClearraBuildTransactionRecord $transaction
    $comparison = Get-ClearraBuildPathComparison
    if ($record.status -ne 'active' -or $record.purpose -ne $Purpose -or
        -not ([string]$record.source_root).Equals($RepositoryRoot, $comparison) -or
        -not ([string]$record.source_root).Equals($env:CLEARRA_BUILD_SOURCE_ROOT, $comparison) -or
        $record.source_id -ne $env:CLEARRA_BUILD_SOURCE_ID -or
        $record.session_id -ne $env:CLEARRA_BUILD_SESSION_ID -or
        $record.session_id -ne $env:CLEARRA_BUILD_CACHE_SESSION_KEY -or
        [string]$record.owner_pid -ne $env:CLEARRA_BUILD_CACHE_OWNER_PID -or
        [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR) -or
        -not ([IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)).Equals($record.cargo_target_dir, $comparison)) {
        throw 'Inherited build transaction purpose/source/session/path binding mismatch.'
    }
    try { $owner = [Diagnostics.Process]::GetProcessById([int]$record.owner_pid) }
    catch { throw 'Inherited build owner is no longer active.' }
    try { if ($owner.HasExited) { throw 'Inherited build owner is no longer active.' } }
    finally { $owner.Dispose() }
    $lease = Read-ClearraBuildLease (Get-ClearraBuildLeasePath $record)
    if ($lease.session_id -ne $record.session_id -or $lease.source_id -ne $record.source_id -or
        $lease.purpose -ne $record.purpose -or $lease.owner_pid -ne $record.owner_pid) {
        throw 'Inherited build owner does not hold its bound active lease.'
    }
    if ($record.purpose -eq 'product') { Assert-ClearraProductCatalogLeaseIdentity $record }
    return $record
}

function Assert-ClearraBuildTreeNoReparse([string]$Path) {
    Assert-ClearraNoReparseBuildPath $Path | Out-Null
    $pending = [Collections.Generic.Stack[IO.DirectoryInfo]]::new()
    $pending.Push([IO.DirectoryInfo]::new($Path))
    while ($pending.Count -gt 0) {
        foreach ($entry in $pending.Pop().EnumerateFileSystemInfos()) {
            if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Managed build cleanup refuses a nested reparse point: $($entry.FullName)"
            }
            if ($entry -is [IO.DirectoryInfo]) { $pending.Push($entry) }
        }
    }
}

function Remove-ClearraOwnedBuildTransaction($Record, $Lease) {
    if ($null -eq $Lease) { throw 'A verified active owner lease is required for transaction cleanup.' }
    $leaseIdentity = Read-ClearraBuildLease (Get-ClearraBuildLeasePath $Record)
    if ($leaseIdentity.owner_pid -ne $PID -or $leaseIdentity.session_id -ne $Lease.Record.session_id -or
        $leaseIdentity.source_id -ne $Record.source_id -or $leaseIdentity.purpose -ne $Record.purpose -or
        -not ([IO.Path]::GetFullPath($Lease.Path)).Equals((Get-ClearraBuildLeasePath $Record), (Get-ClearraBuildPathComparison))) {
        throw 'Build cleanup requires the current source/purpose owner lease.'
    }
    $path = Assert-ClearraCanonicalBuildPath $Record.transaction_root (Get-ClearraBuildRecordLocalSafetyRoot $Record)
    $verified = Read-ClearraBuildTransactionRecord $path
    if ($verified.session_id -ne $Record.session_id -or $verified.source_id -ne $Record.source_id) {
        throw 'Build cleanup ownership changed.'
    }
    Assert-ClearraBuildTreeNoReparse $path
    $stillOwned = Read-ClearraBuildLease $Lease.Path
    if ($stillOwned.session_id -ne $leaseIdentity.session_id -or $stillOwned.owner_pid -ne $PID) {
        throw 'Build cleanup lease changed during validation.'
    }
    # Only a validated experiment/current or product/generation is ever deleted.
    Remove-Item -LiteralPath $path -Recurse -Force
}
