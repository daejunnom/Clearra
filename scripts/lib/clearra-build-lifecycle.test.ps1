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

$first = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource
$firstSession = $first.session_id
$experimentRoot = $first.transaction_root
$cargoRoot = Get-ClearraCargoTargetDir
Assert-ArtifactPathCondition (Test-ClearraBuildTransactionOwner) 'owner_public_predicate_matches_bound_session'
Assert-ArtifactPathCondition ($first.purpose -eq 'experiment' -and $cargoRoot -eq (Join-Path $experimentRoot 'cargo-target')) 'experiment_uses_one_bound_cargo_target'
Assert-ArtifactPathCondition ($env:RUSTC_WRAPPER -eq (Get-ClearraExpectedRustcWrapper) -and $env:CARGO_INCREMENTAL -eq '0') 'owner_installs_guard_and_incremental_policy'
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

$slot = New-TransientBuildDir 'probe'
[IO.File]::WriteAllText((Join-Path $slot 'old.txt'),'fixture')
Assert-ArtifactPathCondition (Test-ArtifactPathThrows { New-TransientBuildDir 'probe' }) 'active_transient_slot_cannot_be_replaced'
Remove-TransientBuildDir $slot
$slotAgain = New-TransientBuildDir 'probe'
Assert-ArtifactPathCondition ($slotAgain -eq $slot -and -not (Test-Path -LiteralPath (Join-Path $slot 'old.txt'))) 'transient_reuses_only_its_fixed_slot'
Remove-TransientBuildDir $slotAgain
Exit-ClearraBuildArtifactCacheUsage
Assert-ArtifactPathCondition (-not (Test-ClearraBuildTransactionOwner)) 'owner_public_predicate_false_after_exit'
Assert-ArtifactPathCondition ([string]::IsNullOrWhiteSpace($env:RUSTC_WRAPPER) -and [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) 'owner_restores_environment'
$second = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource
Assert-ArtifactPathCondition ($second.transaction_root -eq $experimentRoot -and $second.session_id -ne $firstSession -and -not (Test-Path -LiteralPath $oldDependency)) 'new_experiment_replaces_entire_dependency_graph'
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

# Seven product successes retain exactly the latest five, while the physical
# root sentinel and unrelated source experiment survive every retention pass.
$rootSentinel = Join-Path $canonicalFixtureRoot 'root-must-not-reset.txt'
[IO.File]::WriteAllText($rootSentinel,'not a generation')
$productPaths = [Collections.Generic.List[string]]::new()
for ($index=0; $index -lt 7; $index++) {
    $product = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource -Purpose product
    $productPaths.Add($product.transaction_root)
    [IO.File]::WriteAllText((Join-Path $product.transaction_root 'payload.bin'),"fixture $index")
    Complete-ClearraBuildTransaction
    Exit-ClearraBuildArtifactCacheUsage
}
Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath $productPaths[0]) -and -not (Test-Path -LiteralPath $productPaths[1])) 'old_completed_products_pruned'
Assert-ArtifactPathCondition (@(Get-ChildItem -LiteralPath (Join-Path $canonicalFixtureRoot 'products') -Directory).Count -eq 5) 'latest_five_completed_products_retained'
Assert-ArtifactPathCondition ((Test-Path -LiteralPath $rootSentinel) -and (Test-Path -LiteralPath $experimentRoot)) 'retention_never_resets_physical_root_or_experiments'
$failed = Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource -Purpose product
$failedPath = $failed.transaction_root
[IO.File]::WriteAllText((Join-Path $failedPath 'partial.bin'),'failed fixture')
Exit-ClearraBuildArtifactCacheUsage
Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath $failedPath)) 'failed_product_is_reclaimed_and_not_counted'

$protectedRecord = Read-ClearraBuildTransactionRecord $productPaths[2]
$protectedLease = Enter-ClearraBuildLease $protectedRecord
try {
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Initialize-ClearraBuildArtifactCache -RepositoryRoot $fixtureSource -Purpose product }) 'unreleased_product_session_blocks_independent_product'
    # Model one already completed generation arriving before this fixture's
    # retention pass, without starting a second independently owned product.
    $product = Read-ClearraBuildTransactionRecord $productPaths[6]
    $product.session_id = [Guid]::NewGuid().ToString('N')
    $product.transaction_root = Join-Path $canonicalFixtureRoot ('products/' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + $product.session_id)
    $product.cargo_target_dir = Join-Path $product.transaction_root 'cargo-target'
    $product.completed_utc = [DateTime]::UtcNow.ToString('o')
    New-Item -ItemType Directory -Path $product.transaction_root | Out-Null
    Write-ClearraBuildTransactionRecord $product
    Invoke-ClearraBuildArtifactCacheRetention -RepositoryRoot $fixtureSource | Out-Null
    Assert-ArtifactPathCondition (Test-Path -LiteralPath $protectedRecord.transaction_root) 'active_product_lease_preserved_during_retention'
} finally { Exit-ClearraBuildLease $protectedLease }
Invoke-ClearraBuildArtifactCacheRetention -RepositoryRoot $fixtureSource | Out-Null
Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath $protectedRecord.transaction_root) -and @(Get-ChildItem -LiteralPath (Join-Path $canonicalFixtureRoot 'products') -Directory).Count -eq 5) 'released_product_lease_allows_five_generation_recovery'
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
