# One independently owned product transaction at a time across Windows/WSL.
# Nested work may still run in parallel inside that transaction. No stale
# catalog/session lease or incomplete generation is automatically recovered.
function Assert-ClearraProductCatalogLeaseIdentity($Record) {
    $path = Assert-ClearraCanonicalBuildPath (Join-Path (Get-ClearraCanonicalBuildRoot) '.leases/products-catalog.lock') (Get-ClearraBuildRecordLocalSafetyRoot $Record)
    $lease = Read-ClearraBuildLease $path
    if ($lease.purpose -ne 'product' -or $lease.session_id -ne $Record.session_id -or
        $lease.source_id -ne $Record.source_id -or $lease.owner_pid -ne $Record.owner_pid) {
        throw 'Product catalog owner/session binding mismatch; explicit recovery is required.'
    }
}

function Assert-ClearraProductCatalogReady([string]$RepositoryRoot) {
    $root = Get-ClearraCanonicalBuildRoot
    $products = Assert-ClearraCanonicalBuildPath (Join-Path $root 'products') $RepositoryRoot
    if (Test-Path -LiteralPath $products) {
        if (-not (Test-Path -LiteralPath $products -PathType Container)) { throw 'Unverified product catalog path.' }
        foreach ($entry in Get-ChildItem -LiteralPath $products -Force) {
            if (-not $entry.PSIsContainer) { throw 'Unverified product catalog entry; explicit recovery is required.' }
            $record = Read-ClearraBuildTransactionRecord $entry.FullName
            if ($record.purpose -ne 'product' -or $record.status -ne 'complete') {
                throw 'An active, failed or interrupted product generation requires explicit recovery before another product build.'
            }
        }
    }
    $leases = Assert-ClearraCanonicalBuildPath (Join-Path $root '.leases') $RepositoryRoot
    if (Test-Path -LiteralPath $leases -PathType Container) {
        # Covers orphan claims and complete-then-crashed owners as well as
        # active generations. The products-catalog.lock itself is not matched.
        if (@(Get-ChildItem -LiteralPath $leases -Filter 'product-*.lock' -Force).Count -gt 0) {
            throw 'An unreleased product session lease requires explicit recovery before another product build.'
        }
    }
}
