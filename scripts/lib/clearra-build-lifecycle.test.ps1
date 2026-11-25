# Dot-sourced only by test_artifact_path_policy.ps1 inside its isolated fixture.
$canonicalFixtureRoot = Join-Path $fixtureBase 'Clearra/build'
$externalTarget = Join-Path $fixtureRoot 'outside-target'
$env:CARGO_TARGET_DIR = $externalTarget
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Ensure-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource }) 'outside_cargo_rejected_before_mutation'
Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath $canonicalFixtureRoot) -and -not (Test-Path -LiteralPath $externalTarget)) 'outside_cargo_created_nothing'
Remove-Item Env:\CARGO_TARGET_DIR
foreach ($alias in @('CARGO_BUILD_TARGET_DIR','CARGO_BUILD_BUILD_DIR','CARGO_BUILD_RUSTC_WRAPPER',
        'CLEARRA_WSL_CARGO_TARGET_DIR','CLEARRA_RELEASE_BUILD_ROOT','CLEARRA_WSL_NATIVE_BUILD_ROOT',
        'CLEARRA_CORE_C_BUILD_DIR','RUSTC_WORKSPACE_WRAPPER')) {
    [Environment]::SetEnvironmentVariable($alias,$externalTarget,'Process')
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Ensure-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource }) "unmanaged_alias_rejected_$alias"
    [Environment]::SetEnvironmentVariable($alias,$null,'Process')
}
$env:RUSTC_WRAPPER = Join-Path $fixtureRoot 'unmanaged-wrapper.cmd'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Ensure-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource }) 'unmanaged_compiler_wrapper_rejected'
Remove-Item Env:\RUSTC_WRAPPER
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource -ArtifactRoot $externalTarget }) 'alternate_artifact_root_rejected'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Resolve-ClearraArtifactPath $externalTarget $fixtureSource }) 'explicit_external_core_build_rejected'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Resolve-ClearraArtifactPath '../escape' $fixtureSource }) 'relative_transaction_escape_rejected'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-ClearraRequestedBuildPath $externalTarget $fixtureSource }) 'preflight_external_core_request_rejected'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-ClearraRequestedBuildPath (Join-Path $canonicalFixtureRoot 'products/unselected/core') $fixtureSource -Purpose product }) 'preflight_unselected_absolute_product_rejected'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-ClearraRequestedBuildPath '../escape' $fixtureSource -Purpose product }) 'preflight_relative_product_escape_rejected'
Assert-ArtifactPathCondition ((Assert-ClearraRequestedBuildPath 'core-c-cache' $fixtureSource -Purpose product) -eq 'core-c-cache') 'preflight_relative_product_name_needs_no_generation'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-ClearraRequestedBuildPath (Join-Path $canonicalFixtureRoot 'experiments/other/current/core') $fixtureSource }) 'preflight_other_experiment_path_rejected'
Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath $canonicalFixtureRoot)) 'all_path_validation_preceded_mkdir'
foreach ($invalidIncremental in @('2','false',' ')) {
    $env:CARGO_INCREMENTAL = $invalidIncremental
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource }) 'invalid_incremental_mode_rejected_before_mutation'
}
Remove-Item Env:\CARGO_INCREMENTAL
Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath $canonicalFixtureRoot)) 'invalid_incremental_mode_created_no_catalog'

$launcherPreparation = (Get-Command Initialize-ClearraNativeRustcLauncher).ScriptBlock
try {
    Set-Item Function:\Initialize-ClearraNativeRustcLauncher { throw 'injected launcher bootstrap failure' }
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Ensure-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource -Purpose product }) 'launcher_failure_rejects_build'
    Assert-ArtifactPathCondition (-not (Test-ClearraBuildTransactionOwner)) 'launcher_failure_releases_owner'
    Assert-ArtifactPathCondition (@(Get-ChildItem -LiteralPath (Join-Path $canonicalFixtureRoot '.leases') -Force).Count -eq 0) 'launcher_failure_releases_product_leases'
    Assert-ArtifactPathCondition (@(Get-ChildItem -LiteralPath (Join-Path $canonicalFixtureRoot 'products') -Force).Count -eq 0) 'launcher_failure_removes_failed_product'
} finally {
    Set-Item Function:\Initialize-ClearraNativeRustcLauncher $launcherPreparation
}

$first = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource
$firstSession = $first.session_id
$experimentRoot = $first.transaction_root
$firstMarker = Join-Path $experimentRoot '.clearra-build-transaction.json'
if (Test-StartTestsWindows) {
    [IO.File]::SetAttributes($firstMarker, ([IO.File]::GetAttributes($firstMarker) -bor [IO.FileAttributes]::Hidden))
}
# Unix dotfiles are already Hidden. Exercise the real read and size check on
# both platforms, not just an assertion that the source contains -Force.
$hiddenRecord = Read-ClearraBuildTransactionRecord $experimentRoot
Assert-ArtifactPathCondition ($hiddenRecord.session_id -eq $firstSession) 'hidden_transaction_marker_is_readable'
if (Test-StartTestsWindows) {
    [IO.File]::SetAttributes($firstMarker, ([IO.File]::GetAttributes($firstMarker) -band (-bnot [IO.FileAttributes]::Hidden)))
}
$cargoRoot = Get-ClearraCargoTargetDir
Assert-ArtifactPathCondition (Test-ClearraBuildTransactionOwner) 'owner_public_predicate_matches_bound_session'
$capacityPath = Get-ClearraBuildCapacityLeasePath $first
Assert-ArtifactPathCondition (Test-Path -LiteralPath $capacityPath) 'experiment_holds_shared_storage_catalog'
Assert-ArtifactPathCondition ($first.purpose -eq 'experiment' -and $cargoRoot -eq (Join-Path $experimentRoot 'cargo-target')) 'experiment_uses_one_bound_cargo_target'
Assert-ArtifactPathCondition ($env:RUSTC_WRAPPER -eq (Get-ClearraExpectedRustcWrapper) -and $env:CARGO_INCREMENTAL -eq '1') 'experiment_owner_installs_guard_and_incremental_policy'
$oldDependency = Join-Path $cargoRoot 'debug/deps/old-library.rlib'
New-Item -ItemType Directory -Path (Split-Path -Parent $oldDependency) -Force | Out-Null
[IO.File]::WriteAllText($oldDependency,'disposable fixture dependency')
$same = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource
Assert-ArtifactPathCondition ($same.session_id -eq $firstSession -and (Test-Path -LiteralPath $oldDependency)) 'same_owner_reuses_transaction'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Remove-ClearraOwnedBuildTransaction $first }) 'cleanup_requires_explicit_owner_lease'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Ensure-ClearraBuildArtifactCache -RepositoryRoot $otherSource }) 'nested_source_switch_rejected'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Ensure-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource -Purpose product }) 'nested_purpose_switch_rejected'
$env:CLEARRA_BUILD_SESSION_ID = [Guid]::NewGuid().ToString('N')
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Ensure-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource }) 'mutated_session_binding_rejected'
$env:CLEARRA_BUILD_SESSION_ID = $firstSession

$policyFileLiteral = (Join-Path $script:ClearraPathPolicyRepositoryRoot 'scripts/lib/clearra-path-helpers.ps1').Replace("'","''")
$sourceLiteral = $fixtureSource.Replace("'","''")
$hostExecutable = (Get-Process -Id $PID).Path
$nestedCode = '$ErrorActionPreference="Stop"; . ''' + $policyFileLiteral + '''; Ensure-ClearraBuildArtifactCache -RepositoryRoot ''' + $sourceLiteral + '''; Write-Output ("nested_session="+$env:CLEARRA_BUILD_SESSION_ID); try { Complete-ClearraBuildTransaction; exit 9 } catch {}; Exit-ClearraBuildArtifactCacheUsage'
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($nestedCode))
$nestedOutput = @(& $hostExecutable -NoProfile -NonInteractive -EncodedCommand $encoded 2>&1)
Assert-ArtifactPathCondition ($LASTEXITCODE -eq 0 -and ($nestedOutput -join "`n").Contains("nested_session=$firstSession")) 'nested_process_reuses_parent_without_completion_authority'
$namesLiteral = ($script:ClearraBuildTransactionEnvironmentNames | ForEach-Object { "'$_'" }) -join ','
$independentCode = '$ErrorActionPreference="Stop"; foreach($n in @(' + $namesLiteral + ')){[Environment]::SetEnvironmentVariable($n,$null,"Process")}; . ''' + $policyFileLiteral + '''; try { Ensure-ClearraBuildArtifactCache -RepositoryRoot ''' + $sourceLiteral + '''; exit 9 } catch { Write-Output "independent_owner=blocked" }'
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($independentCode))
$independentOutput = @(& $hostExecutable -NoProfile -NonInteractive -EncodedCommand $encoded 2>&1)
Assert-ArtifactPathCondition ($LASTEXITCODE -eq 0 -and ($independentOutput -join "`n").Contains('independent_owner=blocked') -and (Test-Path -LiteralPath $oldDependency)) 'independent_active_owner_cannot_replace_current'
$otherLiteral = $otherSource.Replace("'","''")
$independentProductCode = '$ErrorActionPreference="Stop"; foreach($n in @(' + $namesLiteral + ')){[Environment]::SetEnvironmentVariable($n,$null,"Process")}; . ''' + $policyFileLiteral + '''; try { Ensure-ClearraBuildArtifactCache -RepositoryRoot ''' + $otherLiteral + ''' -Purpose product; exit 9 } catch { Write-Output "independent_product=blocked" }'
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($independentProductCode))
$independentProductOutput = @(& $hostExecutable -NoProfile -NonInteractive -EncodedCommand $encoded 2>&1)
Assert-ArtifactPathCondition ($LASTEXITCODE -eq 0 -and ($independentProductOutput -join "`n").Contains('independent_product=blocked') -and
    (Test-Path -LiteralPath $capacityPath)) 'catalog_lease_blocks_different_source_and_purpose'

$slot = New-TransientBuildDir 'probe'
[IO.File]::WriteAllText((Join-Path $slot 'old.txt'),'fixture')
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { New-TransientBuildDir 'probe' }) 'active_transient_slot_cannot_be_replaced'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Complete-ClearraBuildTransaction }) 'active_powershell_scratch_blocks_outer_completion'
Assert-ArtifactPathCondition (Test-Path -LiteralPath (Join-Path $slot 'old.txt')) 'active_powershell_scratch_is_preserved'
Remove-TransientBuildDir $slot
$slotAgain = New-TransientBuildDir 'probe'
Assert-ArtifactPathCondition ($slotAgain -eq $slot -and -not (Test-Path -LiteralPath (Join-Path $slot 'old.txt'))) 'transient_reuses_only_its_fixed_slot'
Remove-TransientBuildDir $slotAgain
$legacyScratchLease = Join-Path $experimentRoot 'transient/.legacy.lock'
[IO.File]::WriteAllText($legacyScratchLease, '{}')
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { New-TransientBuildDir 'legacy' }) 'legacy_scratch_lease_is_not_stolen'
Assert-ArtifactPathCondition (Test-Path -LiteralPath $legacyScratchLease) 'legacy_scratch_lease_is_preserved'
# Unix dotfiles are hidden to PowerShell too. Only this test-created exact leaf
# is removed; this does not grant automatic recovery of real legacy leases.
Remove-Item -LiteralPath $legacyScratchLease -Force
$interruptedScratchLease = Join-Path $experimentRoot 'wasm-stage.lock'
[IO.File]::WriteAllText($interruptedScratchLease, '{}')
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Complete-ClearraBuildTransaction }) 'outer_completion_preserves_unresolved_inner_staging_lease'
Assert-ArtifactPathCondition ((Test-Path -LiteralPath $interruptedScratchLease) -and (Test-Path -LiteralPath $oldDependency)) 'inner_staging_failure_does_not_delete_compiler_cache'
# This fixture created the exact injected lease. No real stale-owner recovery.
Remove-Item -LiteralPath $interruptedScratchLease
Complete-ClearraBuildTransaction
Exit-ClearraBuildArtifactCacheUsage
Assert-ArtifactPathCondition (-not (Test-ClearraBuildTransactionOwner)) 'owner_public_predicate_false_after_exit'
Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath $capacityPath)) 'catalog_lease_released_after_owned_exit'
Assert-ArtifactPathCondition ([string]::IsNullOrWhiteSpace($env:RUSTC_WRAPPER) -and [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) 'owner_restores_environment'
$second = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource
Assert-ArtifactPathCondition ($second.transaction_root -eq $experimentRoot -and $second.session_id -ne $firstSession -and
    (Test-Path -LiteralPath $oldDependency) -and $second.incremental_seed_session_id -eq $firstSession -and
    $second.incremental_seed_snapshot_sha256 -eq $first.compiler_snapshot_sha256) 'new_experiment_reuses_only_complete_compiler_cache_with_provenance'
Complete-ClearraBuildTransaction
Exit-ClearraBuildArtifactCacheUsage

$env:CARGO_INCREMENTAL = '0'
$oneShot = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource
Assert-ArtifactPathCondition ($env:CARGO_INCREMENTAL -eq '0' -and $oneShot.incremental_cache_mode -eq 'disabled' -and
    $null -eq $oneShot.incremental_seed_session_id -and -not (Test-Path -LiteralPath $oldDependency)) 'one_shot_experiment_retires_prior_incremental_cache'
$sameOneShot = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource
Assert-ArtifactPathCondition ($sameOneShot.session_id -eq $oneShot.session_id -and $env:CARGO_INCREMENTAL -eq '0') 'nested_one_shot_preserves_disabled_incremental_mode'
$env:CARGO_INCREMENTAL = '1'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource }) 'active_incremental_mode_cannot_change'
$env:CARGO_INCREMENTAL = '0'
$oneShotOutput = Join-Path (Get-ClearraCargoTargetDir) 'one-shot-output'
New-Item -ItemType Directory -Path (Split-Path -Parent $oneShotOutput) -Force | Out-Null
[IO.File]::WriteAllText($oneShotOutput, 'completed fixture output')
Complete-ClearraBuildTransaction
Exit-ClearraBuildArtifactCacheUsage
Assert-ArtifactPathCondition ($env:CARGO_INCREMENTAL -eq '0') 'one_shot_owner_restores_explicit_policy'
Remove-Item Env:\CARGO_INCREMENTAL
$resumedLocal = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource
Assert-ArtifactPathCondition ($env:CARGO_INCREMENTAL -eq '1' -and $null -eq $resumedLocal.incremental_seed_session_id -and
    -not (Test-Path -LiteralPath $oneShotOutput)) 'local_default_resumes_without_one_shot_cache_seed'
Complete-ClearraBuildTransaction
Exit-ClearraBuildArtifactCacheUsage

if (Test-StartTestsWindows) {
    $outsideDirectory = Join-Path $fixtureRoot 'outside-preserved'
    New-Item -ItemType Directory -Path $outsideDirectory | Out-Null
    [IO.File]::WriteAllText((Join-Path $outsideDirectory 'keep.txt'),'must survive')
    $junction = Join-Path $experimentRoot 'unsafe-junction'
    New-Item -ItemType Junction -Path $junction -Target $outsideDirectory | Out-Null
    try {
        Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Resolve-ClearraArtifactPath (Join-Path $junction 'output') $fixtureSource }) 'reparse_explicit_path_rejected'
        Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Ensure-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource }) 'reparse_existing_tree_not_deleted'
        Assert-ArtifactPathCondition (Test-Path -LiteralPath (Join-Path $outsideDirectory 'keep.txt')) 'reparse_destination_preserved'
    } finally {
        # Windows PowerShell 5's Remove-Item can prompt for a nonempty junction.
        # Delete this exact link without recursion; never walk its destination.
        $link = Get-Item -LiteralPath $junction -Force
        if (($link.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) { throw 'Fixture link identity changed.' }
        [IO.Directory]::Delete($junction)
    }
}

# Reclaim an exact failed generation while the owning lease is still held.
$failed = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource -Purpose product
$failedPath = $failed.transaction_root
[IO.File]::WriteAllText((Join-Path $failedPath 'partial.bin'),'failed fixture')
Exit-ClearraBuildArtifactCacheUsage
Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath $failedPath)) 'failed_product_is_reclaimed_and_not_counted'

# Three product generations and one existing experiment consume the four
# completed slots. A different product is not admitted; a read-only catalog
# check neither deletes complete trees nor requires acquiring their leases.
$productPaths = [Collections.Generic.List[string]]::new()
for ($index=0; $index -lt 3; $index++) {
    $product = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource -Purpose product
    $productPaths.Add($product.transaction_root)
    [IO.File]::WriteAllText((Join-Path $product.transaction_root 'payload.bin'),"fixture $index")
    Complete-ClearraBuildTransaction
    Exit-ClearraBuildArtifactCacheUsage
}
$retainedProductDirectories = @(Get-ChildItem -LiteralPath (Join-Path $canonicalFixtureRoot 'products') -Directory)
Assert-ArtifactPathCondition ($retainedProductDirectories.Count -eq 3 -and (Test-Path -LiteralPath $experimentRoot)) 'capacity_counts_product_and_experiment_together'
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource -Purpose product }) 'catalog_rejects_fifth_completed_slot_before_write'
Assert-ArtifactPathCondition (@(Get-ChildItem -LiteralPath (Join-Path $canonicalFixtureRoot 'products') -Directory).Count -eq 3 -and
    -not (Test-Path -LiteralPath $capacityPath)) 'over_capacity_admission_creates_no_generation_and_releases_lease'
$protectedRecord = Read-ClearraBuildTransactionRecord $productPaths[0]
$protectedLease = Enter-ClearraBuildLease $protectedRecord
try {
    Invoke-ClearraBuildArtifactCacheRetention -RepositoryRoot $fixtureSource | Out-Null
    Assert-ArtifactPathCondition ((Test-Path -LiteralPath $protectedRecord.transaction_root) -and
        (Test-Path -LiteralPath $protectedLease.Path) -and
        @(Get-ChildItem -LiteralPath (Join-Path $canonicalFixtureRoot 'products') -Directory).Count -eq 3) 'read_only_inventory_preserves_completed_generation_and_lease'
} finally { Exit-ClearraBuildLease $protectedLease }
$unownedProduct = Join-Path $canonicalFixtureRoot 'products/unowned-fixture'
New-Item -ItemType Directory -Path $unownedProduct | Out-Null
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Invoke-ClearraBuildArtifactCacheRetention -RepositoryRoot $fixtureSource }) 'unowned_product_aborts_retention_without_deletion'
Assert-ArtifactPathCondition (Test-Path -LiteralPath $unownedProduct) 'unowned_product_is_preserved'
[IO.Directory]::Delete($unownedProduct)

$last = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource
$marker = Join-Path $last.transaction_root '.clearra-build-transaction.json'
$lastLeasePath = Get-ClearraBuildLeasePath $last
$wrongRecord = Read-ClearraBuildTransactionRecord $last.transaction_root
$wrongRecord.session_id = [Guid]::NewGuid().ToString('N')
Write-ClearraBuildTransactionRecord $wrongRecord
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Exit-ClearraBuildArtifactCacheUsage }) 'mismatched_exit_identity_is_rejected'
Assert-ArtifactPathCondition ((Test-Path -LiteralPath $marker) -and (Test-Path -LiteralPath $lastLeasePath)) 'mismatched_exit_preserves_transaction_and_lease'
