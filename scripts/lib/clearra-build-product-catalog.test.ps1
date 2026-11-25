# Metadata-only cross-platform fixtures; no WSL or compiler is launched.
if (Test-StartTestsWindows) {
    $wirePreviousLocal = $env:LOCALAPPDATA
    $wirePreviousXdg = $env:XDG_CACHE_HOME
    $env:LOCALAPPDATA = Join-Path $fixtureRoot 'wire-cache-home'
    $env:XDG_CACHE_HOME = $env:LOCALAPPDATA
    $wireSource = Join-Path $fixtureRoot 'wire-source'
    New-Item -ItemType Directory -Path $wireSource | Out-Null
    $wireRoot = Get-ClearraCanonicalBuildRoot
    $linuxWireSource = '/home/CaseSensitive/.local/share/Clearra/workspaces/0123456789abcdef/source'
    function New-ClearraForeignProductFixture([int]$Index, [string]$Status = 'complete') {
        $created = [DateTime]::UtcNow.AddMinutes(-100 + $Index)
        $session = [Guid]::NewGuid().ToString('N')
        $directory = Join-Path $wireRoot ('products/' + $created.ToString('yyyyMMddTHHmmssfffZ') + '-' + $session)
        $mounted = '/mnt/' + $directory.Substring(0,1).ToLowerInvariant() + '/' + $directory.Substring(3).Replace('\','/')
        $record = [pscustomobject][ordered]@{
            schema_version=3; purpose='product'; source_root=$linuxWireSource
            source_id=(Get-ClearraBuildMetadataSourceIdentity $linuxWireSource)
            session_id=$session; transaction_root=$mounted; cargo_target_dir="$mounted/cargo-target"
            owner_pid=$PID; status=$Status; created_utc=$created.ToString('o')
            completed_utc=$(if ($Status -eq 'complete') { $created.AddSeconds(1).ToString('o') } else { $null })
        }
        Assert-ClearraCanonicalBuildPath $directory $wireSource | Out-Null
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $directory '.clearra-build-transaction.json'), ($record | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
        return [pscustomobject]@{ path=$directory; record=$record }
    }
    try {
        Assert-ArtifactPathCondition ((Get-ClearraBuildMetadataSourceIdentity 'C:\Users\Fixture\Source') -eq
            (Get-ClearraBuildMetadataSourceIdentity '/mnt/c/Users/Fixture/Source')) 'windows_mounted_source_identity_matches'
        Assert-ArtifactPathCondition ((Get-ClearraBuildMetadataSourceIdentity $linuxWireSource) -ne
            (Get-ClearraBuildMetadataSourceIdentity $linuxWireSource.ToLowerInvariant())) 'foreign_posix_source_identity_keeps_case'
        $foreignProducts = @(0..2 | ForEach-Object { New-ClearraForeignProductFixture $_ })
        $decoded = Read-ClearraBuildTransactionRecord $foreignProducts[0].path
        Assert-ArtifactPathCondition ($decoded.source_root -ceq $linuxWireSource -and
            $decoded.transaction_root.Equals($foreignProducts[0].path, [StringComparison]::OrdinalIgnoreCase) -and
            $decoded.cargo_target_dir.Equals((Join-Path $foreignProducts[0].path 'cargo-target'), [StringComparison]::OrdinalIgnoreCase)) 'windows_reads_wsl_product_with_native_output_paths'
        Invoke-ClearraBuildArtifactCacheRetention -RepositoryRoot $wireSource | Out-Null
        Assert-ArtifactPathCondition ((Test-Path -LiteralPath $foreignProducts[0].path) -and
            @(Get-ChildItem -LiteralPath (Join-Path $wireRoot 'products') -Directory).Count -eq 3) 'windows_capacity_check_preserves_foreign_completed_products'
        $productOwner = Initialize-ClearraBuildArtifactCache -RepositoryRoot $wireSource -Purpose product
        $catalogPath = Join-Path $wireRoot '.leases/products-catalog.lock'
        $capacityPath = Get-ClearraBuildCapacityLeasePath $productOwner
        Assert-ArtifactPathCondition (Test-Path -LiteralPath $catalogPath) 'product_holds_catalog_for_entire_owner_lifetime'
        Assert-ArtifactPathCondition (Test-Path -LiteralPath $capacityPath) 'product_holds_shared_capacity_catalog_for_entire_owner_lifetime'
        $sameProduct = Initialize-ClearraBuildArtifactCache -RepositoryRoot $wireSource -Purpose product
        Assert-ArtifactPathCondition ($sameProduct.session_id -eq $productOwner.session_id) 'nested_product_reuses_same_catalog_owner'
        $policyLiteral = (Join-Path $script:ClearraPathPolicyRepositoryRoot 'scripts/lib/clearra-path-helpers.ps1').Replace("'","''")
        $wireSourceLiteral = $wireSource.Replace("'","''")
        $names = ($script:ClearraBuildTransactionEnvironmentNames | ForEach-Object { "'$_'" }) -join ','
        $probeCode = '$ErrorActionPreference="Stop"; foreach($n in @(' + $names + ')){[Environment]::SetEnvironmentVariable($n,$null,"Process")}; . ''' + $policyLiteral + '''; try { Ensure-ClearraBuildArtifactCache -RepositoryRoot ''' + $wireSourceLiteral + ''' -Purpose product; exit 9 } catch { Write-Output "catalog_owner=blocked" }'
        $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($probeCode))
        $probe = @(& (Get-Process -Id $PID).Path -NoProfile -NonInteractive -EncodedCommand $encoded 2>&1)
        Assert-ArtifactPathCondition ($LASTEXITCODE -eq 0 -and ($probe -join "`n").Contains('catalog_owner=blocked')) 'independent_product_cannot_overlap_active_catalog_owner'
        Complete-ClearraBuildTransaction
        Exit-ClearraBuildArtifactCacheUsage
        Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath $catalogPath) -and -not (Test-Path -LiteralPath $capacityPath) -and
            @(Get-ChildItem -LiteralPath (Join-Path $wireRoot 'products') -Directory).Count -eq 4) 'mixed_windows_wsl_products_share_capacity_and_release_catalog'

        $interrupted = New-ClearraForeignProductFixture 20 'active'
        $before = @(Get-ChildItem -LiteralPath (Join-Path $wireRoot 'products') -Directory).Count
        foreach ($attempt in 1..2) {
            Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $wireSource -Purpose product }) "interrupted_product_blocks_retry_$attempt"
        }
        Assert-ArtifactPathCondition ((Test-Path -LiteralPath $interrupted.path) -and
            @(Get-ChildItem -LiteralPath (Join-Path $wireRoot 'products') -Directory).Count -eq $before -and
            -not (Test-Path -LiteralPath $catalogPath)) 'interrupted_product_is_preserved_without_random_generation_growth'
        $savedWireBinding = @{}
        foreach ($name in $script:ClearraBuildTransactionEnvironmentNames) { $savedWireBinding[$name] = [Environment]::GetEnvironmentVariable($name,'Process') }
        try {
            # Historical schema3 records remain readable catalog evidence, not
            # live schema4 owners. Forge only the foreign environment binding
            # for this rejection probe; never install it through the live owner
            # setter or add incremental provenance to its persisted marker.
            $foreignBinding = @{
                CLEARRA_BUILD_ROOT = $wireRoot
                CLEARRA_BUILD_PURPOSE = 'product'
                CLEARRA_BUILD_SOURCE_ROOT = $interrupted.record.source_root
                CLEARRA_BUILD_SOURCE_ID = $interrupted.record.source_id
                CLEARRA_BUILD_SESSION_ID = $interrupted.record.session_id
                CLEARRA_BUILD_TRANSACTION_ROOT = $interrupted.record.transaction_root
                CLEARRA_BUILD_CACHE_OWNER_PID = [string]$interrupted.record.owner_pid
                CLEARRA_BUILD_CACHE_SESSION_KEY = $interrupted.record.session_id
                CARGO_TARGET_DIR = $interrupted.record.cargo_target_dir
                CARGO_INCREMENTAL = '0'
            }
            foreach ($name in $foreignBinding.Keys) { [Environment]::SetEnvironmentVariable($name,[string]$foreignBinding[$name],'Process') }
            $foreignOwnerError = $null
            try { Get-ClearraInheritedBuildTransaction $linuxWireSource 'product' | Out-Null }
            catch { $foreignOwnerError = $_.Exception.Message }
            Assert-ArtifactPathCondition ($foreignOwnerError -eq 'A foreign-platform build owner cannot be inherited by Windows.') 'windows_cannot_claim_foreign_owner_even_with_matching_local_pid'
        } finally {
            foreach ($name in $script:ClearraBuildTransactionEnvironmentNames) { [Environment]::SetEnvironmentVariable($name,$savedWireBinding[$name],'Process') }
        }
        # Test-owned explicit recovery of the simulated record, not a product
        # initializer recovery path. Keep it as a completed foreign fixture.
        $interrupted.record.status = 'complete'
        $interrupted.record.completed_utc = [DateTime]::UtcNow.ToString('o')
        [IO.File]::WriteAllText((Join-Path $interrupted.path '.clearra-build-transaction.json'), ($interrupted.record | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
        Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Invoke-ClearraBuildArtifactCacheRetention -RepositoryRoot $wireSource }) 'over_capacity_legacy_catalog_is_reported'
        Assert-ArtifactPathCondition ((Test-Path -LiteralPath $interrupted.path) -and (Test-Path -LiteralPath $foreignProducts[0].path)) 'over_capacity_legacy_catalog_is_not_automatically_deleted'
        $orphan = Enter-ClearraBuildLease $interrupted.record
        try {
            Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $wireSource -Purpose product }) 'complete_but_unreleased_product_blocks_new_generation'
        } finally { Exit-ClearraBuildLease $orphan }
        $staleCatalog = Enter-ClearraBuildLease $interrupted.record -ProductCatalog
        try {
            Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $wireSource -Purpose product }) 'stale_catalog_is_not_stolen'
            Assert-ArtifactPathCondition (Test-Path -LiteralPath $catalogPath) 'stale_catalog_is_preserved'
        } finally { Exit-ClearraBuildLease $staleCatalog }
        $staleCapacity = Enter-ClearraBuildLease $interrupted.record -CapacityCatalog
        try {
            Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $wireSource -Purpose experiment }) 'shared_stale_catalog_blocks_experiments'
            Assert-ArtifactPathCondition (Test-Path -LiteralPath $capacityPath) 'shared_stale_catalog_is_preserved'
        } finally { Exit-ClearraBuildLease $staleCapacity }
        $unknown = Join-Path $wireRoot 'products/unknown-fixture'
        New-Item -ItemType Directory -Path $unknown | Out-Null
        Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $wireSource -Purpose product }) 'unknown_product_record_blocks_new_generation'
        Assert-ArtifactPathCondition ((Test-Path -LiteralPath $unknown) -and -not (Test-Path -LiteralPath $catalogPath)) 'unknown_product_is_preserved_and_failed_claim_released'
    } finally {
        if ($null -ne $script:ClearraBuildTransaction) { Exit-ClearraBuildArtifactCacheUsage }
        $env:LOCALAPPDATA = $wirePreviousLocal
        $env:XDG_CACHE_HOME = $wirePreviousXdg
    }
}
