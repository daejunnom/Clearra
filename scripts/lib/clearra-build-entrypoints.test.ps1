# Dot-sourced by test_artifact_path_policy.ps1 inside its disposable fixture.
# No compiler, WSL process, actual cache or published artifact is exercised.
$entryCacheHome = Join-Path $fixtureRoot 'entrypoint-cache-home'
$entrySource = Join-Path $fixtureRoot 'entrypoint-source'
$entryOutside = Join-Path $fixtureRoot 'entrypoint-outside-request'
$entrySavedLocal = $env:LOCALAPPDATA
$entrySavedXdg = $env:XDG_CACHE_HOME
$entryHost = (Get-Process -Id $PID).Path
$entryAuthority = $script:ClearraPathPolicyRepositoryRoot
function Invoke-ClearraEntryFailureProbe([string[]]$Arguments) {
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = @(& $entryHost @Arguments 2>&1)
        return [pscustomobject]@{ code=$LASTEXITCODE; output=($output -join "`n") }
    } finally { $ErrorActionPreference = $previousPreference }
}
    New-Item -ItemType Directory -Path $entrySource | Out-Null
    foreach ($relative in @(
        'core-c/src/coverage/fixture.c',
        'crates/fixture/src/target/mod.rs',
        'apps/fixture/build/generated.js',
        'crates/fixture/target/generated.bin'
    )) {
        $fixturePath = Join-Path $entrySource $relative
        [void][IO.Directory]::CreateDirectory((Split-Path -Parent $fixturePath))
        [IO.File]::WriteAllText($fixturePath, 'fixture')
    }
$env:LOCALAPPDATA = $entryCacheHome
$env:XDG_CACHE_HOME = $entryCacheHome
try {
    foreach ($case in @(
        @{ file='clearra.ps1'; arguments=@('-Task','Validate','-CoreCBuildDir',$entryOutside) },
        @{ file='verify.ps1'; arguments=@('-CoreCBuildDir',$entryOutside) },
        @{ file='build-core-c.ps1'; arguments=@('-BuildDir',$entryOutside) },
        @{ file='run-c-core-tests.ps1'; arguments=@('-BuildDir',$entryOutside) },
        @{ file='compare-pc-runtime-environments.ps1'; arguments=@('-Environment','wasm','-Prepare','-WasmModuleDirectory',$entryOutside) }
    )) {
        $entryArguments = @('-NoProfile','-NonInteractive','-File',(Join-Path $entryAuthority "scripts/$($case.file)")) + $case.arguments
        $entryProbe = Invoke-ClearraEntryFailureProbe $entryArguments
        Assert-ArtifactPathCondition ($entryProbe.code -ne 0 -and $entryProbe.output -match 'outside the canonical Clearra/build root') "entrypoint_external_request_rejected_$($case.file)"
        Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath (Join-Path $entryCacheHome 'Clearra')) -and
            -not (Test-Path -LiteralPath $entryOutside)) "entrypoint_preflight_precedes_mutation_$($case.file)"
    }

    $env:CARGO_TARGET_DIR = $entryOutside
    foreach ($case in @(
        @{ file='desktop-host-check.ps1'; arguments=@('-ExecutionSurface','Trusted') },
        @{ file='run-rust-test.ps1'; arguments=@('-ExecutionSurface','Trusted','-Package','clearra-wasm','-Lib') },
        @{ file='wasm-command-runtime-check.ps1'; arguments=@('-ExecutionSurface','Trusted') },
        @{ file='export-pc-artifact.ps1'; arguments=@('-ExecutionSurface','Trusted') }
    )) {
        $entryArguments = @('-NoProfile','-NonInteractive','-File',(Join-Path $entryAuthority "scripts/$($case.file)")) + $case.arguments
        $entryProbe = Invoke-ClearraEntryFailureProbe $entryArguments
        Assert-ArtifactPathCondition ($entryProbe.code -ne 0 -and $entryProbe.output -match 'outside the canonical Clearra/build root') "standalone_external_cargo_preflight_$($case.file)"
    }
    Remove-Item Env:\CARGO_TARGET_DIR
    Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath (Join-Path $entryCacheHome 'Clearra'))) 'standalone_external_cargo_created_no_cache'

    foreach ($file in @('clearra.ps1','verify.ps1','build-core-c.ps1','run-c-core-tests.ps1',
            'desktop-host-check.ps1','run-rust-test.ps1','wasm-command-runtime-check.ps1')) {
        $tokens = $null
        $parseErrors = $null
        $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $entryAuthority "scripts/$file"), [ref]$tokens, [ref]$parseErrors)
        $ownedTry = @($ast.FindAll({ param($node)
            $node -is [Management.Automation.Language.TryStatementAst] -and
            $null -ne $node.Finally -and $node.Finally.Extent.Text.Contains('Exit-ClearraBuildArtifactCacheUsage') -and
            $node.Body.Extent.Text.Contains('Ensure-ClearraBuildArtifactCache') -and
            $node.Body.Extent.Text.Contains('Test-ClearraBuildTransactionOwner') -and
            $node.Body.Extent.Text.Contains('Complete-ClearraBuildTransaction')
        }, $true))
        Assert-ArtifactPathCondition ($parseErrors.Count -eq 0 -and $ownedTry.Count -eq 1) "standalone_has_bound_try_finally_owner_$file"
    }
    $syncTokens = $null
    $syncParseErrors = $null
    $syncAst = [Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $entryAuthority 'scripts/sync-wsl-workspace.ps1'),
        [ref]$syncTokens,
        [ref]$syncParseErrors
    )
    $syncOwnedTry = @($syncAst.FindAll({ param($node)
        $node -is [Management.Automation.Language.TryStatementAst] -and
        $null -ne $node.Finally -and
        $node.Finally.Extent.Text.Contains('Exit-ClearraBuildArtifactCacheUsage') -and
        $node.Body.Extent.Text.Contains('Sync-ClearraWslExt4Workspace') -and
        $node.Body.Extent.Text.Contains('Test-ClearraBuildTransactionOwner') -and
        $node.Body.Extent.Text.Contains('Complete-ClearraBuildTransaction')
    }, $true))
    Assert-ArtifactPathCondition `
        ($syncParseErrors.Count -eq 0 -and $syncOwnedTry.Count -eq 1) `
        'wsl_source_sync_releases_its_bound_build_owner'
    $readOnlyExport = Get-Content -LiteralPath (Join-Path $entryAuthority 'scripts/export-pc-artifact.ps1') -Raw
    Assert-ArtifactPathCondition (-not $readOnlyExport.Contains('Get-ClearraCargoTargetDir') -and
        $readOnlyExport.Contains('Get-ClearraBuildTransactionRoot')) 'prebuilt_export_does_not_create_or_replace_build'
    $wslBatchOwner = Get-Content -LiteralPath (Join-Path $entryAuthority 'scripts/tools/wsl-pc-runtime-build-and-batch.sh') -Raw
    $ownerPosition = $wslBatchOwner.IndexOf('exec node')
    $cargoPosition = $wslBatchOwner.IndexOf('bash "$AUTHORITY_ROOT/scripts/tools/wsl-native-cargo.sh"')
    $batchPosition = $wslBatchOwner.IndexOf('bash "$AUTHORITY_ROOT/scripts/tools/wsl-pc-runtime-batch.sh"')
    Assert-ArtifactPathCondition ($ownerPosition -ge 0 -and $cargoPosition -gt $ownerPosition -and
        $batchPosition -gt $cargoPosition -and $wslBatchOwner.Contains('clearra-build-paths.mjs')) 'wsl_prepare_and_runtime_share_one_owner'

    . (Join-Path $entryAuthority 'scripts/lib/core-c-build.ps1')
    . (Join-Path $entryAuthority 'scripts/lib/clearra-build-wsl-dispatch.ps1')
    . (Join-Path $entryAuthority 'scripts/lib/clearra-runtime-environment.ps1')
    $digestFixture = Join-Path $entrySource 'digest-fixture.txt'
    [IO.File]::WriteAllText($digestFixture, 'fixture', [Text.UTF8Encoding]::new($false))
    Assert-ArtifactPathCondition `
        ((Get-ClearraFileDigest $digestFixture) -ceq 'f16d05ec6b29248d2c61adb1e9263f78e4f7bace1b955014a2d17872cfe4064d') `
        'wsl_source_manifest_hash_is_independent_of_powershell_module_discovery'
    $runtimeEnvironmentSource = Get-Content -LiteralPath `
        (Join-Path $entryAuthority 'scripts/lib/clearra-runtime-environment.ps1') -Raw
    Assert-ArtifactPathCondition `
        ($runtimeEnvironmentSource.Contains('--checksum --delay-updates') -and
         $runtimeEnvironmentSource.Contains('--exclude=.clearra-source-digest') -and
         $runtimeEnvironmentSource.Contains('.clearra-source-digest.next') -and
         -not $runtimeEnvironmentSource.Contains('rm -rf -- $linuxWorkspace')) `
        'wsl_source_sync_preserves_unchanged_cargo_inputs_and_commits_digest_last'
    $enumeratedBuildInputs = @(Get-ClearraBuildInputFiles $entrySource | ForEach-Object {
        $_.FullName.Substring($entrySource.Length).TrimStart('\', '/').Replace('\', '/')
    })
    Assert-ArtifactPathCondition `
        ($enumeratedBuildInputs -contains 'core-c/src/coverage/fixture.c' -and
         $enumeratedBuildInputs -contains 'crates/fixture/src/target/mod.rs') `
        'source_coverage_and_target_modules_are_build_inputs'
    Assert-ArtifactPathCondition `
        ($enumeratedBuildInputs -notcontains 'apps/fixture/build/generated.js' -and
         $enumeratedBuildInputs -notcontains 'crates/fixture/target/generated.bin') `
        'generated_build_and_target_directories_are_excluded'
    # Windows PowerShell 5.1 reads BOM-less UTF-8 scripts through the active
    # ANSI code page. Keep the source ASCII while still exercising a Unicode
    # Windows path so hosted runners parse the test before reaching this case.
    $unicodeUser = -join @([char]0xD55C, [char]0xAE00, [char]0x20, [char]0xC0AC, [char]0xC6A9, [char]0xC790)
    $unicodeWindowsPath = "C:\Users\$unicodeUser\Clearra\build"
    $escapedWslArgument = ConvertTo-ClearraWslpathArgument $unicodeWindowsPath
    $expectedEscapedWslArgument = $unicodeWindowsPath.Replace('\', '\\')
    Assert-ArtifactPathCondition `
        ($escapedWslArgument -ceq $expectedEscapedWslArgument) `
        'wslpath_argument_preserves_windows_separators_and_unicode'
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Resolve-CoreCBuildDir 'unowned-core' }) 'core_library_cannot_create_without_owner'
    Assert-ArtifactPathCondition (-not (Test-Path -LiteralPath (Join-Path $entryCacheHome 'Clearra'))) 'unowned_core_library_created_no_cache'
    $entryRecord = Initialize-ClearraBuildArtifactCache -RepositoryRoot $entrySource
    $entryCore = Resolve-CoreCBuildDir 'core-c-fixture'
    Assert-ArtifactPathCondition ($entryCore -eq (Join-Path $entryRecord.transaction_root 'core-c-fixture')) 'core_library_uses_bound_source_not_policy_root'
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-CoreCManagedConfigureArgs @('-B',$entryOutside) }) 'cmake_output_override_rejected'
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-CoreCManagedConfigureArgs @('-DCMAKE_LIBRARY_OUTPUT_DIRECTORY=' + $entryOutside) }) 'cmake_library_output_override_rejected'
    Assert-CoreCManagedConfigureArgs @('-DBUILD_TESTING=OFF','-DCLEARRA_BUILD_TEST_ORACLE=ON','-DCLEARRA_CORE_SPLIT_TESTS=ON','-DCMAKE_BUILD_TYPE=Release')
    Assert-ArtifactPathCondition $true 'cmake_managed_options_preserved'
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-CoreCManagedConfigureArgs @('-DCLEARRA_BUILD_TEST_ORACLE=MAYBE') }) 'cmake_test_oracle_invalid_value_rejected'
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-CoreCManagedConfigureArgs @('-DCLEARRA_UNKNOWN_TEST_ORACLE=ON') }) 'cmake_unknown_test_oracle_rejected'

    # Stub path conversion only; construction and identity checks are real.
    $entryOriginalMapping = ${function:ConvertTo-ClearraWslBuildPath}
    function ConvertTo-ClearraWslBuildPath([string]$WindowsPath, [string]$Distribution) {
        if ($WindowsPath -eq (Get-ClearraCanonicalBuildRoot)) { return '/mnt/c/fixture-cache/Clearra/build' }
        return '/mnt/c/current-policy'
    }
    try {
        $linuxSource = '/home/fixture/.local/share/Clearra/workspaces/0123456789abcdef/source'
        $dispatch = @(New-ClearraIndependentWslBuildArguments -LinuxSourceRoot $linuxSource `
            -ScriptName 'wsl-core-c-tests.sh' -CommandArguments @('--workers','1'))
        foreach ($name in @($script:ClearraBuildTransactionEnvironmentNames) + @('RUSTC_WORKSPACE_WRAPPER','CLEARRA_CORE_C_BUILD_DIR')) {
            $position = [Array]::IndexOf($dispatch, $name)
            Assert-ArtifactPathCondition ($position -gt 0 -and $dispatch[$position - 1] -eq '-u') "wsl_detaches_windows_binding_$name"
        }
        Assert-ArtifactPathCondition ($dispatch -contains 'CLEARRA_BUILD_PURPOSE=experiment' -and
            $dispatch -contains 'CLEARRA_BUILD_ROOT=/mnt/c/fixture-cache/Clearra/build' -and
            $dispatch -contains 'CLEARRA_WSL_WORKSPACE=/home/fixture/.local/share/Clearra/workspaces/0123456789abcdef/source' -and
            $dispatch -contains '/mnt/c/current-policy/scripts/tools/wsl-core-c-tests.sh') 'wsl_uses_shared_physical_root_and_current_authority'
        $linuxTransaction = Get-ClearraIndependentWslTransactionRoot $linuxSource
        $linuxOtherCase = Get-ClearraIndependentWslTransactionRoot $linuxSource.Replace('/fixture/','/Fixture/')
        Assert-ArtifactPathCondition ($linuxTransaction -match '^/mnt/c/fixture-cache/Clearra/build/experiments/[0-9a-f]{24}/current$' -and
            $linuxTransaction -ne $linuxOtherCase) 'wsl_source_identity_is_independent_and_case_sensitive'
        $stackDispatch = @(New-ClearraIndependentWslBuildArguments -LinuxSourceRoot $linuxSource `
            -ScriptName 'wsl-native-cargo.sh' -AdditionalEnvironment @{ RUST_MIN_STACK='16777216' })
        Assert-ArtifactPathCondition ($stackDispatch -contains 'RUST_MIN_STACK=16777216') 'wsl_allows_bounded_rust_test_stack'
        Assert-ArtifactPathCondition (Test-ArtifactPathThrows { New-ClearraIndependentWslBuildArguments -LinuxSourceRoot '/mnt/c/source' -ScriptName 'wsl-core-c-tests.sh' }) 'wsl_dispatch_refuses_unvalidated_source_copy'
        Assert-ArtifactPathCondition (Test-ArtifactPathThrows { New-ClearraIndependentWslBuildArguments -LinuxSourceRoot $linuxSource -ScriptName 'wsl-core-c-tests.sh' -AdditionalEnvironment @{ CARGO_TARGET_DIR=$entryOutside } }) 'wsl_dispatch_cannot_reintroduce_output_override'
        Assert-ArtifactPathCondition (Test-ArtifactPathThrows { New-ClearraIndependentWslBuildArguments -LinuxSourceRoot $linuxSource -ScriptName 'wsl-native-cargo.sh' -AdditionalEnvironment @{ RUST_MIN_STACK='67108865' } }) 'wsl_rejects_unbounded_rust_test_stack'
    } finally { ${function:ConvertTo-ClearraWslBuildPath} = $entryOriginalMapping }
    Complete-ClearraBuildTransaction
    Exit-ClearraBuildArtifactCacheUsage
} finally {
    if ($null -ne $script:ClearraBuildTransaction) { Exit-ClearraBuildArtifactCacheUsage }
    $env:LOCALAPPDATA = $entrySavedLocal
    $env:XDG_CACHE_HOME = $entrySavedXdg
}
