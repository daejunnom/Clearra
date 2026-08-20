# Release-blocking static validation is intentionally limited to boundaries,
# forbidden APIs, public ABI shape, unsafe isolation, and unsupported capability
# disclosure. Runtime correctness belongs to executed adversarial tests.
# SRP rationale: this validator has one change reason: the accepted release and
# deployment authority contract changes at a publication or cutover boundary.

function Assert-ReleasePublicContractContains {
    param(
        [string]$Path,
        [string[]]$Fields,
        [string]$Contract
    )

    $text = Read-Text $Path
    foreach ($field in $Fields) {
        if ($text.IndexOf($field, [System.StringComparison]::OrdinalIgnoreCase) -lt 0) {
            Add-ArchitectureError "$Contract public contract is missing '$field' in $Path"
        }
    }
}

function Get-ReleaseSourceSurface {
    param([string[]]$Paths)

    $parts = foreach ($relativePath in $Paths) {
        $fullPath = Join-Path $Root $relativePath
        if (-not (Test-Path -LiteralPath $fullPath)) { continue }
        Get-ChildItem -LiteralPath $fullPath -Recurse -File |
            Where-Object {
                $_.Extension -in @('.rs', '.ts', '.tsx', '.js', '.svelte', '.c', '.h') -and
                $_.Name -notmatch '(_tests|\.test|\.spec)\.' -and
                $_.FullName -notmatch '[\\/](node_modules|dist|dist-server|build|target|coverage|tests|\.cache|\.svelte-kit)[\\/]'
            } |
            ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }
    }
    return $parts -join "`n"
}

function Assert-ReleaseYamlKeyAllowlist {
    param(
        [string]$Text,
        [int]$Indentation,
        [string[]]$AllowedKeys,
        [string]$Contract
    )

    $prefix = ' ' * $Indentation
    foreach ($line in ($Text -split '\r?\n')) {
        if (-not $line.StartsWith($prefix, [System.StringComparison]::Ordinal) -or
            $line.Length -le $Indentation -or
            $line[$Indentation] -eq ' ') {
            continue
        }
        $content = $line.Substring($Indentation)
        if ($content.StartsWith('#', [System.StringComparison]::Ordinal)) {
            continue
        }
        $match = [regex]::Match($content, '^([A-Za-z0-9_-]+):')
        if (-not $match.Success -or $AllowedKeys -notcontains $match.Groups[1].Value) {
            Add-ArchitectureError "$Contract contains a noncanonical YAML key: '$content'"
        }
    }
}

function Assert-ReleaseYamlExactKeySet {
    param(
        [string]$Text,
        [int]$Indentation,
        [string[]]$ExpectedKeys,
        [string]$Contract
    )

    $prefix = ' ' * $Indentation
    $actualKeys = [System.Collections.Generic.List[string]]::new()
    foreach ($line in ($Text -split '\r?\n')) {
        if (-not $line.StartsWith($prefix, [System.StringComparison]::Ordinal) -or
            $line.Length -le $Indentation -or
            $line[$Indentation] -eq ' ') {
            continue
        }
        $content = $line.Substring($Indentation)
        if ($content.StartsWith('#', [System.StringComparison]::Ordinal)) {
            continue
        }
        $match = [regex]::Match($content, '^([A-Za-z0-9_-]+):')
        if (-not $match.Success) {
            Add-ArchitectureError "$Contract contains a noncanonical YAML key: '$content'"
            continue
        }
        $actualKeys.Add($match.Groups[1].Value)
    }

    $actualUnique = @($actualKeys | Sort-Object -Unique)
    $expectedUnique = @($ExpectedKeys | Sort-Object -Unique)
    $missing = @($ExpectedKeys | Where-Object { $actualKeys -notcontains $_ })
    if ($actualKeys.Count -ne $ExpectedKeys.Count -or
        $actualUnique.Count -ne $ExpectedKeys.Count -or
        $expectedUnique.Count -ne $ExpectedKeys.Count -or
        $missing.Count -ne 0) {
        Add-ArchitectureError "$Contract keys must be exactly [$($ExpectedKeys -join ', ')], got [$($actualKeys -join ', ')]"
    }
}

function Assert-ReleaseYamlExactScalar {
    param(
        [string]$Text,
        [int]$Indentation,
        [string]$Key,
        [string]$ExpectedValue,
        [string]$Contract
    )

    $prefix = ' ' * $Indentation
    $pattern = '(?m)^' + [regex]::Escape($prefix + $Key + ':') +
        '\s*' + [regex]::Escape($ExpectedValue) + '\s*$'
    if ([regex]::Matches($Text, $pattern).Count -ne 1) {
        Add-ArchitectureError "$Contract must be exactly '${Key}: $ExpectedValue'"
    }
}

function Assert-ReleaseYamlExactLiteralScript {
    param(
        [string]$Text,
        [string[]]$ExpectedLines,
        [string]$Contract
    )

    $normalized = $Text.Replace("`r`n", "`n")
    $lines = [System.Collections.Generic.List[string]]::new()
    foreach ($line in ($normalized -split "`n")) {
        $lines.Add($line)
    }
    $runIndexes = @()
    for ($index = 0; $index -lt $lines.Count; $index += 1) {
        if ($lines[$index] -match '^ {8}run: \|\s*$') {
            $runIndexes += $index
        }
    }
    if ($runIndexes.Count -ne 1) {
        Add-ArchitectureError "$Contract must contain exactly one literal run block"
        return
    }

    $actualLines = [System.Collections.Generic.List[string]]::new()
    for ($index = $runIndexes[0] + 1; $index -lt $lines.Count; $index += 1) {
        $line = $lines[$index]
        if (-not $line.StartsWith('          ', [System.StringComparison]::Ordinal)) {
            Add-ArchitectureError "$Contract contains a noncanonical script line: '$line'"
            return
        }
        $actualLines.Add($line.Substring(10))
    }
    while ($actualLines.Count -gt 0 -and $actualLines[$actualLines.Count - 1] -eq '') {
        $actualLines.RemoveAt($actualLines.Count - 1)
    }
    if ($actualLines.Count -ne $ExpectedLines.Count) {
        Add-ArchitectureError "$Contract must match the canonical fail-closed script exactly"
        return
    }
    for ($index = 0; $index -lt $ExpectedLines.Count; $index += 1) {
        if (-not [string]::Equals(
                $actualLines[$index],
                $ExpectedLines[$index],
                [System.StringComparison]::Ordinal
            )) {
            Add-ArchitectureError "$Contract must match the canonical fail-closed script exactly"
            return
        }
    }
}

function Assert-ReleaseExactStepSkeleton {
    param(
        [string]$Text,
        [string[]]$ExpectedSteps,
        [string]$Contract
    )

    $actualSteps = [System.Collections.Generic.List[string]]::new()
    foreach ($line in ($Text.Replace("`r`n", "`n") -split "`n")) {
        if ($line.StartsWith('      -', [System.StringComparison]::Ordinal)) {
            $actualSteps.Add($line.Substring(6))
        }
    }
    if ($actualSteps.Count -ne $ExpectedSteps.Count) {
        Add-ArchitectureError "$Contract steps must match the canonical protected prelude exactly"
        return
    }
    for ($index = 0; $index -lt $ExpectedSteps.Count; $index += 1) {
        if (-not [string]::Equals(
                $actualSteps[$index],
                $ExpectedSteps[$index],
                [System.StringComparison]::Ordinal
            )) {
            Add-ArchitectureError "$Contract steps must match the canonical protected prelude exactly"
            return
        }
    }
}

function Assert-ReleaseExactText {
    param(
        [string]$Text,
        [string]$Expected,
        [string]$Contract
    )

    $actual = $Text.Replace("`r`n", "`n")
    if (-not [string]::Equals($actual, $Expected, [System.StringComparison]::Ordinal)) {
        Add-ArchitectureError "$Contract must match the canonical protected step exactly"
    }
}

function Invoke-ReleaseForbiddenApiValidation {
    Assert-ForbiddenAlgorithmMarkersAbsent @(
        'MeetInTheMiddlePacking', 'mitm_pc_backend', 'half_join_pc',
        'front_half_packing', 'back_half_packing', 'complement_join_pc',
        'mitm_static_tiling_in_search_path'
    ) 'architecture_validation_rejects_mitm_pc_backend'

    Assert-ForbiddenAlgorithmMarkersAbsent @(
        'mcts_low_score', 'MctsLowScore', 'rare_piece_heuristic',
        'RarePieceHeuristic', 'bad_shape_heuristic', 'BadShapeHeuristic',
        'probably_impossible', 'ProbablyImpossible', 'no_immediate_placement',
        'NoImmediatePlacement', 'target_frame_floating', 'TargetFrameFloating',
        'spin_classifier_unknown', 'SpinClassifierUnknown', 'score_below_threshold',
        'ScoreBelowThreshold', 'first_witness_missing', 'FirstWitnessMissing',
        'representative_order_failed', 'RepresentativeOrderFailed',
        'bloom_filter_false_positive', 'BloomFilterFalsePositive',
        'resource_cap_reached', 'ResourceCapReached'
    ) 'architecture_validation_rejects_heuristic_prune_reason' @(
        'crates/clearra-core-domain/src/pruning/prune_reason.rs',
        'core-c/src/pruning/prune_reason.c'
    )

    Assert-ForbiddenAlgorithmMarkersAbsent @(
        'representative_order_only_coverage', 'RepresentativeOrderOnlyCoverage',
        'first_witness_coverage', 'FirstWitnessCoverage'
    ) 'architecture_validation_rejects_representative_or_first_witness_coverage'

    $guiSurface = Get-ReleaseSourceSurface @(
        'crates/clearra-gui-host/src', 'gui/clearra-gui/src',
        'apps/clearra-desktop', 'packages/clearra-ui/src'
    )
    foreach ($forbidden in @(
        'std::process::Command', 'process::Command', 'CARGO_BIN_EXE_clearra',
        'clearra.exe', 'clearra_core_ffi', 'clearra_board64_', '#include "clr_'
    )) {
        if ($guiSurface -like "*$forbidden*") {
            Add-ArchitectureError "GUI product surface contains forbidden API '$forbidden'"
        }
    }

    $wasmSurface = Get-ReleaseSourceSurface @(
        'crates/clearra-wasm/src', 'crates/clearra-web-command/src',
        'apps/clearra-web', 'packages/clearra-ui/src/wasm'
    )
    foreach ($forbidden in @(
        'std::process::Command', 'process::Command', 'clearra.exe',
        'CARGO_BIN_EXE_clearra', 'clearra_core_ffi', 'clearra_packing_',
        'clr_buildup_', 'localized_output_keys'
    )) {
        if ($wasmSurface -like "*$forbidden*") {
            Add-ArchitectureError "WASM product surface contains forbidden API '$forbidden'"
        }
    }

    $webGpuSurface = Get-ReleaseSourceSurface @(
        'crates/clearra-webgpu/src', 'apps/clearra-web',
        'packages/clearra-ui/src'
    )
    foreach ($forbidden in @(
        'userProvidedWgsl', 'userProvidedWGSL', 'runtimeShaderInjection',
        'shaderTextFromUser', 'createShaderModule({ code: data',
        'new Function', 'eval(', 'can_source_exact_probability: true'
    )) {
        if ($webGpuSurface -like "*$forbidden*") {
            Add-ArchitectureError "WebGPU product surface contains forbidden API '$forbidden'"
        }
    }

    $productExecutionSurface = Get-ReleaseSourceSurface @(
        'crates/clearra-app/src', 'crates/clearra-core-executor/src',
        'crates/clearra-gui-host/src', 'crates/clearra-wasm/src',
        'crates/clearra-postprocess-gpu/src', 'crates/clearra-render/src',
        'apps/clearra-web/src', 'core-c/src', 'core-c/include'
    )
    foreach ($forbidden in @(
        'GuiAppResponsePreview', 'RenderPlaceholderPreview',
        'not_executed_preview', 'preview_only', 'shell-preview',
        'portable_reference_packing_fallback_allowed',
        'portable_reference_buildup_fallback_allowed',
        'fallback_build_variant_from_candidate',
        'observed-preview-first-pattern-only',
        'FixtureFallback', 'fixture_fallback', 'ExampleResult', 'example_result',
        'WillBeConnectedLater', 'will_be_connected_later',
        'explicit_order_scaffold', 'placeholder execution', 'scaffold execution'
    )) {
        if ($productExecutionSurface.IndexOf($forbidden, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
            Add-ArchitectureError "Finish-or-Remove product surface contains forbidden execution path '$forbidden'"
        }
    }
}

function Invoke-PublicAbiContractValidation {
    Assert-ReleasePublicContractContains 'crates/clearra-app/src/app_request.rs' @(
        'pub struct AppRequest', 'command:', 'query:', 'backend_policy:',
        'output_policy:', 'diagnostics_policy:', 'locale_policy:',
        'resource_budget:'
    ) 'AppRequest'
    Assert-ReleasePublicContractContains 'crates/clearra-app/src/app_response.rs' @(
        'pub struct AppResponse', 'status:', 'result:', 'diagnostics:',
        'backend_report:', 'resource_report:', 'capability_report:', 'continuation:'
    ) 'AppResponse'
    Assert-ReleasePublicContractContains 'core-c/include/clr_problem.h' @(
        'typedef struct clr_packing_problem', 'clr_piece_multiset_window',
        'clr_piece_source_descriptor piece_source', 'typedef struct clr_buildup_problem',
        'clr_hold_automaton_state initial_hold_automaton', 'uint64_t candidate_id',
        'uint64_t canonical_operation_set_id'
    ) 'C problem descriptor'
    Assert-ReleasePublicContractContains 'core-c/include/clr_piece_source.h' @(
        'typedef struct clr_piece_source_descriptor', 'piece_source_id',
        'pattern_universe_id', 'pattern_weight_model_id', 'complete',
        'truncation_reason', 'typedef struct clr_piece_source_pattern_reader'
    ) 'PieceSource'
    Assert-ReleasePublicContractContains 'core-c/include/clr_hold_automaton.h' @(
        'typedef struct clr_hold_automaton_state', 'piece_source_id', 'cursor',
        'bag_epoch', 'bag_remainder_key', 'provenance_id', 'hold_piece', 'hold_empty'
    ) 'HoldAutomaton'
    Assert-ReleasePublicContractContains 'core-c/include/clr_memory.h' @(
        'ClrMemStatus clr_mem_context_release(ClrMemContext **context)'
    ) 'C memory release'
    Assert-ReleasePublicContractContains 'core-c/include/clr_gpu_worker.h' @(
        'memory_ticket_id', 'fence_epoch', 'scope_epoch', 'byte_budget'
    ) 'GPU worker lifetime'
    Assert-ReleasePublicContractContains 'core-c/include/clr_gpu.h' @(
        'ClearraGpuPackingBatchDescriptor', 'piece_source_id',
        'piece_multiset_window', 'pattern_universe_id', 'pattern_weight_model_id'
    ) 'GPU packing descriptor'
    Assert-ReleasePublicContractContains 'core-c/include/clr_pruning.h' @(
        'clr_pruning_evidence_policy', 'CLR_PRUNING_EVIDENCE_BEST_EFFORT',
        'CLR_PRUNING_EVIDENCE_COMPLETE_REQUIRED', 'evidence_truncated',
        'complete_required_capacity_hit'
    ) 'Pruning evidence policy'
    Assert-ReleasePublicContractContains 'core-c/include/clr_coverage.h' @(
        'piece_source_id', 'pattern_universe_id', 'pattern_weight_model_id',
        'pattern_count', 'uint64_t words[CLR_COVERAGE_MAX_WORDS]',
        'clr_pattern_bitset_c patterns'
    ) 'Coverage row identity'
}

function Invoke-UnsupportedCapabilityStaticValidation {
    Assert-ReleasePublicContractContains 'crates/clearra-ui-schema/src/capability/capability_state.rs' @(
        'pub enum CapabilityState', 'Unsupported', 'ConnectedApproximate', 'ConnectedExact',
        'runtime_execution_allowed', 'exact_claim_allowed'
    ) 'Capability state'
    Assert-ReleasePublicContractContains 'crates/clearra-render/src/capability/render_capability.rs' @(
        'RenderCapabilityReport', 'pub fn current()', 'connected_exact',
        'supported: true', 'render_exact: true'
    ) 'Renderer connected exact capability'
    Assert-ReleasePublicContractContains 'crates/clearra-postprocess-gpu/src/post_gpu_capability.rs' @(
        'PostGpuCapabilityState', 'Connected', 'Unavailable', 'RejectedMismatch',
        'connected_exact', 'exact_supported: true', 'exact_supported: false'
    ) 'PostProcess GPU stable capability outcomes'

    $postGpuResult = Read-PhysicalText 'crates/clearra-postprocess-gpu/src/post_gpu_result.rs'
    if ($postGpuResult -match '(?m)^\s*pub\s+fn\s+(new|trusted)\s*\(') {
        Add-ArchitectureError 'PostProcess GPU trusted results must be constructed only by the connected backend'
    }

    foreach ($capabilityPath in @(
        'crates/clearra-ui-schema/src/capability/capability_state.rs',
        'crates/clearra-validation/src/capability/mvp2_capability_registry.rs',
        'crates/clearra-validation/src/capability/mvp3_capability_registry.rs',
        'crates/clearra-postprocess-gpu/src/post_gpu_capability.rs'
    )) {
        $capability = Read-PhysicalText $capabilityPath
        foreach ($forbiddenState in @(
            'SchemaOnly', 'ValidationGuard', 'Preview', 'Scaffold', 'Placeholder',
            'ExampleResult', 'FixtureFallback', 'WillBeConnectedLater',
            'BasicApproximation', 'RuntimeConnected', 'ExactSupported'
        )) {
            if ($capability -match "(?m)^\s*$forbiddenState\s*,?\s*$") {
                Add-ArchitectureError "Stable capability state '$forbiddenState' is forbidden in $capabilityPath"
            }
        }
    }
}

function Invoke-AdversarialReleaseGateWiringValidation {
    $clearra = @(
        Read-Text 'scripts/clearra.ps1'
        Read-Text 'scripts/lib/clearra-task-ui-helpers.ps1'
    ) -join "`n"
    $adversarial = Read-PhysicalText 'scripts/lib/adversarial-correctness.ps1'
    $cMake = Read-Text 'core-c/CMakeLists.txt'

    foreach ($required in @(
        '$expanded.Add("AdversarialCorrectness")',
        'Invoke-AdversarialCorrectnessGate'
    )) {
        if ($clearra -notlike "*$required*") {
            Add-ArchitectureError "ReleaseAcceptance must execute adversarial correctness gate '$required'"
        }
    }
    foreach ($required in @(
        'CLEARRA_CORE_ADVERSARIAL_TESTS=ON', 'clearra_adversarial_tests',
        'RequireExecution',
        'operation_set_hash_collision_does_not_merge_candidates',
        'alternate_success_order_replay_is_legal',
        'objective_uses_nonuniform_pattern_weights',
        'minimum_cover_requires_all_requested_patterns',
        'complete_required_capacity_keeps_candidate',
        'execution_variant_set_preserves_successes_from_multiple_patterns',
        'clearra-core-domain', 'clearra-core-executor',
        'Sort-Object -Unique',
        'did not execute any tests'
    )) {
        if ($adversarial -notlike "*$required*") {
            Add-ArchitectureError "Adversarial correctness runner is missing execution contract '$required'"
        }
    }
    foreach ($removedExecutionSurface in @(
        'ledger_complete_required_executor',
        "'clearra-core-ffi',`n            'clearra-coverage',`n            'clearra-postprocess-gpu'"
    )) {
        if ($adversarial -like "*$removedExecutionSurface*") {
            Add-ArchitectureError "Adversarial correctness runner retains removed or empty execution surface '$removedExecutionSurface'"
        }
    }
    foreach ($required in @(
        'CLEARRA_CORE_ADVERSARIAL_TESTS', 'tests/adversarial_tests.c',
        'add_test(NAME clearra_adversarial_tests'
    )) {
        if ($cMake -notlike "*$required*") {
            Add-ArchitectureError "C adversarial correctness target is missing '$required'"
        }
    }
}

function Invoke-ArchitectureValidationAuthorityPolicy {
    Assert-ReleasePublicContractContains 'docs/architecture-validation.md' @(
        'dependency boundary', 'forbidden API', 'public ABI field',
        'unsafe boundary', 'unsupported capability contract',
        'runtime correctness is not inferred from marker presence',
        'AdversarialCorrectness', 'NoProductDebt'
    ) 'Architecture validation authority'
}

function Invoke-ReleaseIdentityGateValidation {
    $release = Read-Text '.github/workflows/release-cli.yml'
    $pages = Read-Text '.github/workflows/pages.yml'
    $cloudBuild = Read-Text 'apps/clearra-discord-bot/cloudbuild-current-job-service.yaml'
    $currentDocker = Read-Text 'apps/clearra-discord-bot/Dockerfile.current-job-service'
    $legacyCloudBuild = Read-Text 'apps/clearra-discord-bot/cloudbuild-job-service.yaml'
    $legacyDocker = Read-Text 'apps/clearra-discord-bot/Dockerfile.job-service'
    $runtimeIdentity = Read-Text 'apps/clearra-discord-bot/src/job-service/runtime-identity.mjs'
    $cloudDeploy = Read-Text 'apps/clearra-discord-bot/CLOUD_RUN_JOB_SERVICE.md'
    $cloudReadme = Read-Text 'apps/clearra-discord-bot/README.md'
    $acceptedSourcePreflight = Read-Text 'apps/clearra-discord-bot/scripts/verify-accepted-source.mjs'
    $acceptedSourcePreflightTest = Read-Text 'apps/clearra-discord-bot/test/accepted-source-preflight.test.mjs'
    $runtimeServiceAccountBootstrap = Read-Text 'apps/clearra-discord-bot/scripts/prepare-cloud-runtime-service-account.mjs'
    $runtimeServiceAccountBootstrapTest = Read-Text 'apps/clearra-discord-bot/test/cloud-runtime-service-account.test.mjs'
    $candidateSmoke = Read-Text 'apps/clearra-discord-bot/scripts/verify-cloud-run-candidate.mjs'
    $candidateSmokeTest = Read-Text 'apps/clearra-discord-bot/test/cloud-run-candidate-smoke.test.mjs'
    $oracleProofProducer = Read-Text 'apps/clearra-discord-bot/scripts/produce-oracle-deployment-proof.mjs'
    $oracleCandidateProof = Read-Text 'apps/clearra-discord-bot/scripts/verify-oracle-candidate-proof.mjs'
    $oracleRollbackProof = Read-Text 'apps/clearra-discord-bot/scripts/verify-oracle-rollback-proof.mjs'
    $oracleRestore = Read-Text 'apps/clearra-discord-bot/scripts/restore-oracle-release'
    $oracleRollbackCapture = Read-Text 'apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs'
    $oracleReleaseDigest = Read-Text 'apps/clearra-discord-bot/scripts/release-tree-digest.mjs'
    $oracleProofProducerTest = Read-Text 'apps/clearra-discord-bot/test/oracle-deployment-proof-producer.test.mjs'
    $oracleCandidateProofTest = Read-Text 'apps/clearra-discord-bot/test/oracle-candidate-proof.test.mjs'
    $oracleRollbackProofTest = Read-Text 'apps/clearra-discord-bot/test/oracle-rollback-proof.test.mjs'
    $oracleRestoreTest = Read-Text 'apps/clearra-discord-bot/test/oracle-rollback-contract.test.mjs'
    $oracleRollbackCaptureTest = Read-Text 'apps/clearra-discord-bot/test/oracle-rollback-capture.test.mjs'
    $oracleReleaseDigestTest = Read-Text 'apps/clearra-discord-bot/test/release-tree-digest.test.mjs'
    $productionJobExecutor = Read-Text 'apps/clearra-discord-bot/src/clearra/command.mjs'
    $gatewayConfig = Read-Text 'apps/clearra-discord-bot/src/config.mjs'
    $gatewayCommandTests = Read-Text 'apps/clearra-discord-bot/test/command.test.mjs'
    $jobRunner = Read-Text 'apps/clearra-discord-bot/src/job-service/runner.mjs'
    $wasmReleaseGate = Read-Text 'scripts/lib/wasm-release-gate.ps1'
    $wasmBuildContract = Read-Text 'scripts/tools/clearra-wasm-build-contract.mjs'
    $wasmBuild = Read-Text 'scripts/tools/build-clearra-wasm.mjs'
    $wasmProductProbe = Read-Text 'scripts/tools/wasm-pc-environment-probe.mjs'
    $wasmProductTerminalContract = Read-Text 'scripts/tools/wasm-product-terminal-contract.mjs'
    $wasmProductTerminalContractTest = Read-Text 'scripts/tools/wasm-product-terminal-contract.test.mjs'
    $webWasmRuntime = Read-Text 'apps/clearra-web/src/workers/clearraWasmRuntime.ts'
    $releasePackage = Read-Text 'scripts/tools/package-release-cli.sh'
    $uiPackage = Read-Text 'packages/clearra-ui/package.json'
    $webPackage = Read-Text 'apps/clearra-web/package.json'
    $uiContractTypecheck = Read-Text 'packages/clearra-ui/tsconfig.contract.json'
    $webContractTypecheck = Read-Text 'apps/clearra-web/tsconfig.contract.json'
    $productProcessSurface = Read-Text 'scripts/lib/product-process-surface.ps1'
    $remoteTagVerifier = Read-Text 'scripts/release/verify-remote-annotated-tag.mjs'
    $remoteTagVerifierTest = Read-Text 'scripts/release/verify-remote-annotated-tag.test.mjs'
    $gitAttributes = Read-Text '.gitattributes'
    $exactSourceArchive = Read-Text 'scripts/release/create-exact-source-archive.mjs'
    $exactSourceTarContract = Read-Text 'scripts/release/exact-source-tar-contract.mjs'
    $exactSourceArchiveTest = Read-Text 'scripts/release/create-exact-source-archive.test.mjs'
    $releaseCliSmokeTest = Read-Text 'scripts/tools/validate-release-cli-smokes.test.mjs'

    foreach ($required in @(
        'validate-release-metadata.mjs',
        'node --test scripts/release/validate-release-metadata.test.mjs',
        'node --test scripts/release/verify-remote-annotated-tag.test.mjs',
        'node --test scripts/release/create-exact-source-archive.test.mjs',
        'validate-release-cli-smokes.mjs',
        'release tag must point at the exact current main commit',
        'release tag is no longer the exact current main commit',
        '-f event=workflow_dispatch',
        'published versions are immutable',
        'node scripts/release/verify-remote-annotated-tag.mjs',
        '--tag "$GITHUB_REF_NAME"',
        '--expected-commit "$GITHUB_SHA"',
        'CLEARRA_IMMUTABLE_RELEASES_ENABLED',
        '--draft',
        'gh release edit "$GITHUB_REF_NAME" --draft=false',
        'X-GitHub-Api-Version: 2026-03-10',
        "--jq '.immutable'",
        'published release is not immutable',
        'github.ref_type == ''tag'''
    )) {
        if ($release -notlike "*$required*") {
            Add-ArchitectureError "Product release workflow is missing exact release identity gate '$required'"
        }
    }
    foreach ($required in @(
        'rejects workflow defaults that can replace protected shells',
        'rejects a preceding step that can poison protected executable resolution',
        'rejects a skipped dependency injected into the metadata root job',
        'rejects a wrong runner whose comment contains the expected runner',
        'rejects a custom shell that only echoes the protected script path',
        'rejects a parent-commit archive hidden behind the expected SHA comment',
        'rejects publication dependencies spoofed in a later job',
        'rejects ownership transfer moved before the archive helper succeeds'
    )) {
        if ($releaseCliSmokeTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Release CLI smoke gate regression coverage is missing '$required'"
        }
    }
    foreach ($workflow in @(
        @{
            Name = 'Product release'
            Text = $release
            HeadShaMarkers = @(
                '-f head_sha="$GITHUB_SHA"',
                '-f head_sha="$GITHUB_SHA"'
            )
        },
        @{
            Name = 'Pages'
            Text = $pages
            HeadShaMarkers = @(
                '-f head_sha="$checked_sha"',
                '-f head_sha="$EXPECTED_SHA"'
            )
        }
    )) {
        $acceptedAtLeastOne = [regex]::Matches(
            $workflow.Text,
            '(?m)^\s*if \[\[ "\$accepted" -lt 1 \]\]; then\s*$'
        ).Count
        if ($acceptedAtLeastOne -lt 2) {
            Add-ArchitectureError "$($workflow.Name) must accept one or more successful exact-SHA acceptance runs at both identity gates"
        }
        if ($workflow.Text -match '(?m)^\s*if \[\[ "\$accepted" -(?:ne|eq) 1 \]\]; then\s*$') {
            Add-ArchitectureError "$($workflow.Name) must not deadlock re-acceptance by requiring exactly one successful acceptance run"
        }
        if ([regex]::Matches($workflow.Text, '(?m)^\s*accepted="\$\(gh api --method GET \\$').Count -lt 2) {
            Add-ArchitectureError "$($workflow.Name) acceptance lookup must remain a fail-closed GET query at both identity gates"
        }
        if ([regex]::Matches($workflow.Text, '(?m)^\s*-f per_page=1 \\$').Count -lt 2) {
            Add-ArchitectureError "$($workflow.Name) acceptance lookup must request only one server-filtered run at both identity gates"
        }
        if ([regex]::Matches($workflow.Text, '(?m)^\s*-f event=workflow_dispatch \\$').Count -lt 2 -or
            [regex]::Matches($workflow.Text, '(?m)^\s*-f status=success \\$').Count -lt 2 -or
            [regex]::Matches($workflow.Text, '(?m)^\s*--jq ''\.workflow_runs \| length''\)"\s*$').Count -lt 2) {
            Add-ArchitectureError "$($workflow.Name) acceptance lookup must server-filter successful canonical dispatch runs at both identity gates"
        }
        $missingHeadSha = $false
        foreach ($headShaMarker in @($workflow.HeadShaMarkers)) {
            $requiredCount = @($workflow.HeadShaMarkers | Where-Object { $_ -eq $headShaMarker }).Count
            $actualCount = [regex]::Matches($workflow.Text, [regex]::Escape($headShaMarker)).Count
            if ($actualCount -lt $requiredCount) {
                $missingHeadSha = $true
                break
            }
        }
        if ($missingHeadSha -or [regex]::Matches($workflow.Text, '(?m)^\s*-f head_sha=').Count -lt 2) {
            Add-ArchitectureError "$($workflow.Name) acceptance lookup must filter the accepted head SHA on the server at both identity gates"
        }
        if ($workflow.Text -match 'per_page=100|select\(\.head_sha') {
            Add-ArchitectureError "$($workflow.Name) must not recover exact-SHA acceptance by client-filtering a bounded run history"
        }
    }
    foreach ($required in @(
        'CLEARRA_CLI_SHA256=${_CLEARRA_CLI_SHA256}',
        '_CLEARRA_CLI_SHA256: required',
        'clearra.search.contract.legacy-v1'
    )) {
        if ($legacyCloudBuild -notlike "*$required*") {
            Add-ArchitectureError "Legacy job build is missing fail-closed engine identity '$required'"
        }
    }
    foreach ($required in @(
        'sha256sum --check --strict',
        'clearra.search.contract.legacy-v1'
    )) {
        if ($legacyDocker -notlike "*$required*") {
            Add-ArchitectureError "Legacy job image is missing fail-closed engine identity '$required'"
        }
    }
    if ($legacyCloudBuild -like '*CLEARRA_SEARCH_CONTRACT_REVISION=clearra.search.contract.v2*') {
        Add-ArchitectureError 'A downloaded legacy CLI must not claim the current search contract revision'
    }
    if ($release -match '(?m)\bgh\s+release\s+upload\b.*--clobber') {
        Add-ArchitectureError 'Published GitHub Release assets must not be overwritten'
    }
    if ($release -match '(?m)^\s*git ls-remote\b') {
        Add-ArchitectureError 'Remote annotated-tag validation must stay encapsulated in its tested release helper'
    }
    foreach ($required in @(
        '["ls-remote", "origin", tagRef, `${tagRef}^{}`]',
        'remote release tag ${tag} is lightweight, not annotated',
        'remote tag response is ambiguous: duplicate ref',
        'remote annotated release tag ${tag} moved or resolves to a different commit'
    )) {
        if ($remoteTagVerifier.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Remote annotated-tag verifier is missing fail-closed marker '$required'"
        }
    }
    foreach ($required in @(
        'exact annotated remote tag succeeds',
        'moved annotated remote tag fails closed',
        'lightweight remote tag fails closed',
        'missing remote tag fails closed',
        'malformed remote tag response fails closed',
        'ambiguous remote tag response fails closed'
    )) {
        if ($remoteTagVerifierTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Remote annotated-tag regression coverage is missing '$required'"
        }
    }
    foreach ($required in @(
        'core.autocrlf=false',
        'core.eol=lf',
        'tar.umask=0022',
        '["--no-replace-objects", ...args]',
        'get-tar-commit-id',
        '["hash-object", "--no-filters", "--", helperPath]',
        'exact-source-tar-contract.mjs',
        '["ls-tree", "-r", "-t", "-z", "--full-tree", sourceCommit]',
        'gzipSync(rawTar, { level: 9 })',
        'gunzipSync(readFileSync(outputPath)).equals(rawTar)',
        'archive output already exists',
        'canonical Git archive contains a different commit',
        'unlinkSync(outputPath)'
    )) {
        if ($exactSourceArchive.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Exact source archive helper is missing fail-closed marker '$required'"
        }
    }
    foreach ($required in @(
        'expectedEntryKind',
        'mode === "100644"',
        'mode === "100755"',
        'mode === "120000"',
        'gitBlobOid(objectFormat, content)',
        'tar header checksum mismatch',
        'duplicate source tar path',
        'source tar is missing Git tree path',
        'unsupported source tar member type',
        'symlink target is dangling or outside the source tree',
        'key !== "path" && key !== "linkpath"'
    )) {
        if ($exactSourceTarContract.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Exact source tar payload verifier is missing fail-closed marker '$required'"
        }
    }
    foreach ($required in @(
        'exports every exact commit byte, 0644/0755 mode, safe symlink, and embedded identity with autocrlf enabled',
        'rejects committed eol=crlf archive conversion and deletes output',
        'rejects committed export-ignore archive omission and deletes output',
        'rejects committed export-subst archive mutation and deletes output',
        'rejects uncommitted info attributes archive omission and deletes output',
        'rejects helper-module drift from the accepted commit before creating output',
        'rejects raw helper drift hidden by an assume-unchanged index flag',
        'rejects helper modules absent from the accepted commit before creating output',
        'refuses to overwrite an existing archive path',
        'rejects a noncanonical source commit before creating output',
        'ignores local Git replacement refs and archives the canonical accepted object',
        'long regular path must exercise a PAX path record',
        'long symlink target must exercise a PAX linkpath record',
        'production tar verifier rejects duplicate paths, unsupported types, bad checksums, and truncation'
    )) {
        if ($exactSourceArchiveTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Exact source archive regression coverage is missing '$required'"
        }
    }
    Assert-ReleaseYamlExactKeySet `
        -Text $release `
        -Indentation 0 `
        -ExpectedKeys @('name', 'on', 'permissions', 'env', 'jobs') `
        -Contract 'Release workflow top level'
    $releaseEnvironmentStart = $release.IndexOf("`nenv:", [System.StringComparison]::Ordinal)
    $releaseJobsStart = $release.IndexOf("`njobs:", [System.StringComparison]::Ordinal)
    if ($releaseEnvironmentStart -lt 0 -or $releaseJobsStart -le $releaseEnvironmentStart) {
        Add-ArchitectureError 'Release workflow environment boundary is missing'
    }
    else {
        $releaseEnvironment = $release.Substring(
            $releaseEnvironmentStart,
            $releaseJobsStart - $releaseEnvironmentStart
        )
        Assert-ReleaseYamlExactKeySet `
            -Text $releaseEnvironment `
            -Indentation 2 `
            -ExpectedKeys @('CLEARRA_SOURCE_COMMIT', 'CLEARRA_ENGINE_BUILD_ID') `
            -Contract 'Release workflow environment'
        foreach ($identityVariable in @('CLEARRA_SOURCE_COMMIT', 'CLEARRA_ENGINE_BUILD_ID')) {
            Assert-ReleaseYamlExactScalar `
                -Text $releaseEnvironment `
                -Indentation 2 `
                -Key $identityVariable `
                -ExpectedValue '${{ github.sha }}' `
                -Contract "Release workflow $identityVariable"
        }
    }
    $metadataJobStart = $release.IndexOf("`n  metadata:", [System.StringComparison]::Ordinal)
    $linuxJobStart = $release.IndexOf("`n  linux-cli:", [System.StringComparison]::Ordinal)
    $discordJobStart = $release.IndexOf("`n  discord-bot:", [System.StringComparison]::Ordinal)
    $releaseAcceptanceJobStart = $release.IndexOf("`n  release-acceptance:", [System.StringComparison]::Ordinal)
    $windowsProductsJobStart = $release.IndexOf("`n  windows-products:", [System.StringComparison]::Ordinal)
    $publishBoundaryStart = $release.IndexOf("`n  publish:", [System.StringComparison]::Ordinal)
    if ($metadataJobStart -lt 0 -or
        $linuxJobStart -le $metadataJobStart -or
        $discordJobStart -le $linuxJobStart -or
        $releaseAcceptanceJobStart -le $discordJobStart -or
        $windowsProductsJobStart -le $releaseAcceptanceJobStart -or
        $publishBoundaryStart -le $windowsProductsJobStart) {
        Add-ArchitectureError 'Exact source archive workflow job boundaries are missing'
    }
    else {
        $metadataJob = $release.Substring($metadataJobStart, $linuxJobStart - $metadataJobStart)
        $linuxJob = $release.Substring($linuxJobStart, $discordJobStart - $linuxJobStart)
        $discordJob = $release.Substring($discordJobStart, $releaseAcceptanceJobStart - $discordJobStart)
        $releaseAcceptanceJob = $release.Substring(
            $releaseAcceptanceJobStart,
            $windowsProductsJobStart - $releaseAcceptanceJobStart
        )
        $windowsProductsJob = $release.Substring(
            $windowsProductsJobStart,
            $publishBoundaryStart - $windowsProductsJobStart
        )
        Assert-ReleaseYamlExactKeySet `
            -Text $metadataJob `
            -Indentation 4 `
            -ExpectedKeys @('outputs', 'runs-on', 'steps') `
            -Contract 'Linux metadata job'
        Assert-ReleaseYamlExactKeySet `
            -Text $releaseAcceptanceJob `
            -Indentation 4 `
            -ExpectedKeys @('needs', 'runs-on', 'timeout-minutes', 'steps') `
            -Contract 'Windows canonical acceptance job'
        Assert-ReleaseYamlExactScalar `
            -Text $metadataJob `
            -Indentation 4 `
            -Key 'runs-on' `
            -ExpectedValue 'ubuntu-latest' `
            -Contract 'Linux metadata runner'
        Assert-ReleaseYamlExactScalar `
            -Text $releaseAcceptanceJob `
            -Indentation 4 `
            -Key 'needs' `
            -ExpectedValue 'metadata' `
            -Contract 'Windows canonical acceptance dependency'
        Assert-ReleaseYamlExactScalar `
            -Text $releaseAcceptanceJob `
            -Indentation 4 `
            -Key 'runs-on' `
            -ExpectedValue 'windows-latest' `
            -Contract 'Windows canonical acceptance runner'
        foreach ($job in @(
            @{ Name = 'Linux CLI'; Text = $linuxJob; Runner = 'ubuntu-latest' },
            @{ Name = 'Discord'; Text = $discordJob; Runner = 'ubuntu-latest' },
            @{ Name = 'Windows products'; Text = $windowsProductsJob; Runner = 'windows-latest' }
        )) {
            Assert-ReleaseYamlExactKeySet `
                -Text $job.Text `
                -Indentation 4 `
                -ExpectedKeys @('needs', 'runs-on', 'steps') `
                -Contract "$($job.Name) job"
            Assert-ReleaseYamlExactScalar `
                -Text $job.Text `
                -Indentation 4 `
                -Key 'needs' `
                -ExpectedValue 'metadata' `
                -Contract "$($job.Name) dependency"
            Assert-ReleaseYamlExactScalar `
                -Text $job.Text `
                -Indentation 4 `
                -Key 'runs-on' `
                -ExpectedValue $job.Runner `
                -Contract "$($job.Name) runner"
        }

        $linuxRegressionStart = $metadataJob.IndexOf(
            "`n      - name: Validate exact source archive regression coverage",
            [System.StringComparison]::Ordinal
        )
        $linuxArchiveStart = $metadataJob.IndexOf(
            "`n      - name: Archive the exact accepted source on Linux",
            [System.StringComparison]::Ordinal
        )
        $linuxArchiveEnd = $metadataJob.IndexOf(
            "`n      - name: Resolve release version",
            [System.StringComparison]::Ordinal
        )
        $windowsRegressionStart = $releaseAcceptanceJob.IndexOf(
            "`n      - name: Validate Windows exact source archive regression coverage",
            [System.StringComparison]::Ordinal
        )
        $windowsArchiveStart = $releaseAcceptanceJob.IndexOf(
            "`n      - name: Archive the exact accepted source on Windows",
            [System.StringComparison]::Ordinal
        )
        $windowsArchiveEnd = $releaseAcceptanceJob.IndexOf(
            "`n      - uses: actions/cache@v4",
            [System.StringComparison]::Ordinal
        )
        if ($linuxRegressionStart -lt 0 -or
            $linuxArchiveStart -le $linuxRegressionStart -or
            $linuxArchiveEnd -le $linuxArchiveStart -or
            $windowsRegressionStart -lt 0 -or
            $windowsArchiveStart -le $windowsRegressionStart -or
            $windowsArchiveEnd -le $windowsArchiveStart) {
            Add-ArchitectureError 'Exact source archive workflow step boundaries are missing or out of order'
        }
        else {
            $linuxRegressionStep = $metadataJob.Substring(
                $linuxRegressionStart,
                $linuxArchiveStart - $linuxRegressionStart
            )
            $linuxArchiveStep = $metadataJob.Substring(
                $linuxArchiveStart,
                $linuxArchiveEnd - $linuxArchiveStart
            )
            $windowsRegressionStep = $releaseAcceptanceJob.Substring(
                $windowsRegressionStart,
                $windowsArchiveStart - $windowsRegressionStart
            )
            $windowsArchiveStep = $releaseAcceptanceJob.Substring(
                $windowsArchiveStart,
                $windowsArchiveEnd - $windowsArchiveStart
            )
            $linuxStepsStart = $metadataJob.IndexOf("`n    steps:", [System.StringComparison]::Ordinal)
            $windowsStepsStart = $releaseAcceptanceJob.IndexOf("`n    steps:", [System.StringComparison]::Ordinal)
            if ($linuxStepsStart -lt 0 -or
                $linuxStepsStart -ge $linuxRegressionStart -or
                $windowsStepsStart -lt 0 -or
                $windowsStepsStart -ge $windowsRegressionStart) {
                Add-ArchitectureError 'Exact source archive protected step preludes are missing'
            }
            else {
                $linuxProtectedPrelude = $metadataJob.Substring(
                    $linuxStepsStart,
                    $linuxArchiveEnd - $linuxStepsStart
                )
                $windowsProtectedPrelude = $releaseAcceptanceJob.Substring(
                    $windowsStepsStart,
                    $windowsArchiveEnd - $windowsStepsStart
                )
                Assert-ReleaseExactStepSkeleton `
                    -Text $linuxProtectedPrelude `
                    -ExpectedSteps @(
                        '- uses: actions/checkout@v4',
                        '- uses: actions/setup-node@v4',
                        '- name: Validate exact source archive regression coverage',
                        '- name: Archive the exact accepted source on Linux'
                    ) `
                    -Contract 'Linux exact source archive protected prelude'
                Assert-ReleaseExactStepSkeleton `
                    -Text $windowsProtectedPrelude `
                    -ExpectedSteps @(
                        '- uses: actions/checkout@v4',
                        '- uses: actions/setup-node@v4',
                        '- name: Validate Windows exact source archive regression coverage',
                        '- name: Archive the exact accepted source on Windows'
                    ) `
                    -Contract 'Windows exact source archive protected prelude'
                Assert-ReleaseExactText `
                    -Text $metadataJob.Substring(
                        $linuxStepsStart,
                        $linuxRegressionStart - $linuxStepsStart
                    ) `
                    -Expected "`n    steps:`n      - uses: actions/checkout@v4`n      - uses: actions/setup-node@v4`n        with:`n          node-version: 22" `
                    -Contract 'Linux protected checkout and Node setup'
                Assert-ReleaseExactText `
                    -Text $releaseAcceptanceJob.Substring(
                        $windowsStepsStart,
                        $windowsRegressionStart - $windowsStepsStart
                    ) `
                    -Expected "`n    steps:`n      - uses: actions/checkout@v4`n      - uses: actions/setup-node@v4`n        with:`n          node-version: 22`n          cache: npm`n          cache-dependency-path: package-lock.json" `
                    -Contract 'Windows protected checkout and Node setup'
            }
            foreach ($step in @(
                @{ Name = 'Linux archive regression'; Text = $linuxRegressionStep; Shell = 'bash' },
                @{ Name = 'Linux accepted source archive'; Text = $linuxArchiveStep; Shell = 'bash' },
                @{ Name = 'Windows archive regression'; Text = $windowsRegressionStep; Shell = 'pwsh' },
                @{ Name = 'Windows accepted source archive'; Text = $windowsArchiveStep; Shell = 'pwsh' }
            )) {
                Assert-ReleaseYamlExactKeySet `
                    -Text $step.Text `
                    -Indentation 8 `
                    -ExpectedKeys @('shell', 'run') `
                    -Contract "$($step.Name) step"
                Assert-ReleaseYamlExactScalar `
                    -Text $step.Text `
                    -Indentation 8 `
                    -Key 'shell' `
                    -ExpectedValue $step.Shell `
                    -Contract "$($step.Name) shell"
            }
            if ($linuxRegressionStep -notmatch '(?m)^        run: node --test scripts/release/create-exact-source-archive\.test\.mjs scripts/tools/validate-release-cli-smokes\.test\.mjs\s*$' -or
                $windowsRegressionStep -notmatch '(?m)^        run: node --test scripts/release/create-exact-source-archive\.test\.mjs scripts/tools/validate-release-cli-smokes\.test\.mjs\s*$') {
                Add-ArchitectureError 'Exact source archive regression steps must execute the real test command'
            }
            Assert-ReleaseYamlExactLiteralScript `
                -Text $linuxArchiveStep `
                -ExpectedLines @(
                    'archive_path="$RUNNER_TEMP/clearra-exact-source-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT.tar.gz"',
                    'archive_owned=false',
                    'if [[ -e "$archive_path" ]]; then',
                    '  echo ''exact source archive output already exists'' >&2',
                    '  exit 2',
                    'fi',
                    'trap ''if [[ "$archive_owned" == true ]]; then rm -f -- "$archive_path"; fi'' EXIT',
                    'node scripts/release/create-exact-source-archive.mjs \',
                    '  --source-commit "$GITHUB_SHA" \',
                    '  --output "$archive_path"',
                    'archive_owned=true',
                    'test -s "$archive_path"'
                ) `
                -Contract 'Linux accepted source archive script'
            Assert-ReleaseYamlExactLiteralScript `
                -Text $windowsArchiveStep `
                -ExpectedLines @(
                    '$archivePath = Join-Path $env:RUNNER_TEMP ("clearra-exact-source-" + [Guid]::NewGuid().ToString("N") + ".tar.gz")',
                    '$archiveOwned = $false',
                    'try {',
                    '  node scripts/release/create-exact-source-archive.mjs `',
                    '    --source-commit $env:GITHUB_SHA `',
                    '    --output $archivePath',
                    '  if ($LASTEXITCODE -ne 0) { throw "exact source archive failed" }',
                    '  $archiveOwned = $true',
                    '  $archive = Get-Item -LiteralPath $archivePath',
                    '  if (-not $archive.Length) { throw "exact source archive is empty" }',
                    '} finally {',
                    '  if ($archiveOwned -and (Test-Path -LiteralPath $archivePath)) {',
                    '    Remove-Item -LiteralPath $archivePath -Force',
                    '  }',
                    '}'
                ) `
                -Contract 'Windows accepted source archive script'
        }
    }

    $publishJobStart = $release.IndexOf("`n  publish:", [System.StringComparison]::Ordinal)
    if ($publishJobStart -lt 0) {
        Add-ArchitectureError 'Release publish job is missing'
    }
    else {
        $publishJob = $release.Substring($publishJobStart)
        $publishRemainder = $publishJob.Substring("`n  publish:".Length)
        $publishStepsIndex = $publishJob.IndexOf("`n    steps:", [System.StringComparison]::Ordinal)
        $laterPublishJob = $false
        foreach ($line in ($publishRemainder -split '\r?\n')) {
            if ($line -match '^ {2}\S' -and $line -notmatch '^ {2}#') {
                $laterPublishJob = $true
                break
            }
        }
        if ($laterPublishJob -or $publishStepsIndex -lt 0) {
            Add-ArchitectureError 'Release publish must remain the final workflow job'
        }
        else {
            $publishHeader = $publishJob.Substring(0, $publishStepsIndex)
            Assert-ReleaseYamlExactKeySet `
                -Text $publishHeader `
                -Indentation 4 `
                -ExpectedKeys @('if', 'needs', 'runs-on') `
                -Contract 'Release publish header'
            Assert-ReleaseYamlExactScalar `
                -Text $publishHeader `
                -Indentation 4 `
                -Key 'runs-on' `
                -ExpectedValue 'ubuntu-latest' `
                -Contract 'Release publish runner'
            if ([regex]::Matches($publishHeader, '(?m)^    needs\s*:').Count -ne 1 -or
                $publishHeader -notmatch '(?m)^    needs:\s*\r?\n      \[metadata, release-acceptance, linux-cli, windows-products, discord-bot\]\s*$' -or
                [regex]::Matches($publishHeader, '(?m)^    if\s*:').Count -ne 1 -or
                $publishHeader -notmatch '(?m)^    if: github\.ref_type == ''tag''\s*$') {
                Add-ArchitectureError 'Release publish must depend on every exact acceptance job and run only for tags'
            }
        }
    }
    foreach ($required in @(
        'apps/clearra-discord-bot/scripts/restore-oracle-release text eol=lf',
        'scripts/release/create-exact-source-archive.mjs text eol=lf',
        'scripts/release/exact-source-tar-contract.mjs text eol=lf'
    )) {
        if ($gitAttributes.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Executable release helper LF checkout contract is missing '$required'"
        }
    }
    $lateMainIndex = $release.LastIndexOf(
        'release tag is no longer the exact current main commit',
        [System.StringComparison]::Ordinal
    )
    $lateAcceptanceIndex = $release.LastIndexOf(
        'release tag no longer has a successful exact-SHA canonical acceptance run',
        [System.StringComparison]::Ordinal
    )
    $remoteTagIndex = $release.LastIndexOf(
        'node scripts/release/verify-remote-annotated-tag.mjs',
        [System.StringComparison]::Ordinal
    )
    $immutabilityPreconditionIndex = $release.LastIndexOf(
        'repository release immutability has not been confirmed by the approved administrator',
        [System.StringComparison]::Ordinal
    )
    $draftCreateIndex = $release.LastIndexOf(
        'gh release create "$GITHUB_REF_NAME"',
        [System.StringComparison]::Ordinal
    )
    $publishDraftIndex = $release.LastIndexOf(
        'gh release edit "$GITHUB_REF_NAME" --draft=false',
        [System.StringComparison]::Ordinal
    )
    $immutableCheckIndex = $release.LastIndexOf(
        "--jq '.immutable'",
        [System.StringComparison]::Ordinal
    )
    if ($lateMainIndex -lt 0 -or
        $lateAcceptanceIndex -le $lateMainIndex -or
        $immutabilityPreconditionIndex -le $lateAcceptanceIndex -or
        $remoteTagIndex -le $immutabilityPreconditionIndex -or
        $draftCreateIndex -le $remoteTagIndex -or
        $publishDraftIndex -le $draftCreateIndex -or
        $immutableCheckIndex -le $publishDraftIndex) {
        Add-ArchitectureError 'Release publication must rebind the remote annotated tag, build an asset-complete draft, publish it, and verify immutable state in order'
    }
    if ($release -notmatch '(?m)^\s*fi\r?\n\s*node scripts/release/verify-remote-annotated-tag\.mjs \\\s*$') {
        Add-ArchitectureError 'Release publication must run the remote annotated-tag verifier immediately after its late preconditions'
    }
    foreach ($required in @(
        'accepted_sha',
        'clearra-build-identity.json',
        'clearra.pages.identity.v2',
        'Pages source has no successful canonical workflow_dispatch acceptance run',
        'Pages source is no longer the exact current main commit'
    )) {
        if ($pages -notlike "*$required*") {
            Add-ArchitectureError "Pages workflow is missing accepted-source identity gate '$required'"
        }
    }
    if ($pages -match '(?ms)^\s*push:\s*\r?\n\s*branches:\s*\[main\]') {
        Add-ArchitectureError 'Pages must not deploy an unaccepted main push'
    }
    if ($pages -match '(?m)^\s*(?!ref:|[A-Z_]+:)\S.*\$\{\{\s*inputs\.') {
        Add-ArchitectureError 'Pages shell steps must receive workflow inputs through environment variables, never direct expression interpolation'
    }
    foreach ($required in @(
        'EXPECTED_SHA: ${{ inputs.accepted_sha }}',
        '[[ ! "$EXPECTED_SHA" =~ ^[0-9a-f]{40}$ ]]',
        'if [[ "$checked_sha" != "$EXPECTED_SHA" ]]'
    )) {
        if ($pages.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages accepted-source validation is missing untrusted-input guard '$required'"
        }
    }
    foreach ($required in @(
        'CLEARRA_SOURCE_COMMIT: ${{ github.sha }}',
        'CLEARRA_ENGINE_BUILD_ID: ${{ github.sha }}'
    )) {
        if ($release.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Product release workflow is missing compile-time build identity '$required'"
        }
    }
    foreach ($required in @(
        'CLEARRA_SOURCE_COMMIT: ${{ inputs.accepted_sha }}',
        'CLEARRA_ENGINE_BUILD_ID: ${{ inputs.accepted_sha }}',
        'apps/clearra-web/build/wasm/clearra_wasm.manifest.json',
        '${PAGE_URL%/}/wasm/clearra_wasm.manifest.json?source=${EXPECTED_SHA}',
        '.build.runtime_identity.source_commit == $sha',
        '.build.runtime_identity.engine_build_id == $sha',
        'clearra.supply.projected-terminal-lookahead.v1',
        'clearra.solution-data.v1'
    )) {
        if ($pages.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages workflow is missing exact product build identity '$required'"
        }
    }
    foreach ($package in @(
        @{ Name = 'UI'; Text = $uiPackage; Config = $uiContractTypecheck },
        @{ Name = 'Web'; Text = $webPackage; Config = $webContractTypecheck }
    )) {
        foreach ($required in @(
            'npm exec tsc -- --noEmit -p tsconfig.contract.json',
            'run-typescript-contracts.mjs'
        )) {
            if ($package.Text -notlike "*$required*") {
                Add-ArchitectureError "$($package.Name) test script is missing TypeScript contract gate '$required'"
            }
        }
        if ($package.Config.IndexOf('"noEmit": true', [System.StringComparison]::Ordinal) -lt 0 -or
            $package.Config.IndexOf('"include": ["test/*.contract.ts"]', [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "$($package.Name) TypeScript contract typecheck must compile every tracked .contract.ts without emitting artifacts"
        }
    }
    if ($webPackage.IndexOf('"pretest": "npm run sync"', [System.StringComparison]::Ordinal) -lt 0) {
        Add-ArchitectureError 'Web TypeScript contracts must generate the SvelteKit tsconfig before typechecking on a clean checkout'
    }
    foreach ($required in @(
        'apps/clearra-discord-bot/scripts/verify-terminal-supply-product.mjs',
        'packages/clearra-ui/scripts/verify-terminal-supply-product.mjs',
        '$probePath "--clearra" $builtExePath',
        '$uiProbePath "--clearra" $builtExePath'
    )) {
        if ($productProcessSurface.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Built product acceptance is missing exact terminal-supply artifact probe '$required'"
        }
    }
    foreach ($required in @(
        "'test', '--workspace', '@clearra/ui'",
        "'test', '--workspace', '@clearra/web'",
        "'--test', 'terminal_supply_public_contract'",
        'wasm-product-terminal-contract.test.mjs',
        "'--manifest'",
        "'--expected-source-commit'"
    )) {
        if ($wasmReleaseGate -notlike "*$required*") {
            Add-ArchitectureError "WASM release acceptance is missing product contract gate '$required'"
        }
    }
    foreach ($required in @(
        'validateWasmProbeTerminal',
        'expectedWasmProbeIdentity',
        'runtime_identity',
        'WASM probe final response identity does not match its manifest'
    )) {
        if ($wasmProductProbe.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0 -and
            $wasmProductTerminalContract.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Actual WASM product probe is missing terminal identity assertion '$required'"
        }
    }
    foreach ($required in @(
        'rejects non-success terminal states and events',
        'rejects missing, duplicate, or unsuccessful final responses',
        'rejects manifest, source, and final identity drift'
    )) {
        if ($wasmProductTerminalContractTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Actual WASM product probe regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'CLEARRA_SOURCE_COMMIT=${_SOURCE_COMMIT}',
        'CLEARRA_ENGINE_BUILD_ID=${_SOURCE_COMMIT}',
        'clearra.search.contract.v2',
        'clearra.supply.projected-terminal-lookahead.v1',
        'clearra.solution-data.v1'
    )) {
        if ($cloudBuild -notlike "*$required*") {
            Add-ArchitectureError "Cloud Run build is missing immutable runtime identity '$required'"
        }
    }
    if ($cloudBuild -match '(?m)^\s*_TAG:\s*latest\s*$') {
        Add-ArchitectureError 'Cloud Run build must not default to a mutable latest image tag'
    }
    foreach ($required in @(
        'ARG CLEARRA_SOURCE_COMMIT',
        'ARG CLEARRA_ENGINE_BUILD_ID',
        'source_commit',
        '${CLEARRA_SOURCE_COMMIT}',
        'engine_build_id',
        '${CLEARRA_ENGINE_BUILD_ID}',
        'contract_schema_version',
        'supply_semantics_id',
        'artifact_schema_version',
        "CLEARRA_SEARCH_CONTRACT_REVISION!=='clearra.search.contract.v2'",
        "CLEARRA_SUPPLY_SEMANTICS_ID!=='clearra.supply.projected-terminal-lookahead.v1'",
        "CLEARRA_ARTIFACT_SCHEMA_VERSION!=='clearra.solution-data.v1'"
    )) {
        if ($currentDocker.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Current-source job image is missing compile/runtime identity guard '$required'"
        }
    }
    foreach ($required in @(
        'clearra.runtime.identity.v2',
        'clearra.search.contract.v2',
        'sourceCommit',
        'engineBuildId',
        'contractSchemaVersion',
        'supplySemanticsId',
        'artifactSchemaVersion',
        'runtimeIdentityMatches'
    )) {
        if ($runtimeIdentity -notlike "*$required*") {
            Add-ArchitectureError "Runtime identity contract is missing '$required'"
        }
    }
    foreach ($required in @(
        'productBuildIdentityMatchesRuntime',
        'payload?.runtime_identity',
        'Clearra executable identity does not match the configured job runtime identity'
    )) {
        if ($jobRunner.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Cloud job startup probe is missing child executable identity marker '$required'"
        }
    }
    foreach ($required in @(
        'CLEARRA_WASM_BUILD_CONTRACT_VERSION = 2',
        'CLEARRA_WASM_MANIFEST_BYTES = 1280',
        'product-build-identity/v1',
        'runtime_identity: productBuildIdentityFromEnvironment(environment)'
    )) {
        if ($wasmBuildContract.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "WASM artifact contract is missing product identity marker '$required'"
        }
    }
    foreach ($required in @(
        'serializeClearraWasmManifest',
        'CLEARRA_SOURCE_COMMIT=',
        'CLEARRA_ENGINE_BUILD_ID='
    )) {
        if ($wasmBuild.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "WASM build is missing compile identity propagation '$required'"
        }
    }
    foreach ($required in @(
        'contract_version === 2',
        '30a6cc08ce00320997ccf86982a1b6770d67ff7e1f7aeabb8bb22dea77dbaa0d',
        'assertClearraWasmTerminalResponseIdentities',
        'expectedRuntimeIdentity'
    )) {
        if ($webWasmRuntime.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Browser WASM runtime is missing manifest/response identity marker '$required'"
        }
    }
    foreach ($required in @(
        'release CLI requires identical full lowercase source and engine commit IDs',
        'identity?.source_commit !== expectedCommit',
        'identity?.engine_build_id !== expectedCommit',
        'clearra.supply.projected-terminal-lookahead.v1',
        'clearra.solution-data.v1'
    )) {
        if ($releasePackage.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Linux release package is missing product identity assertion '$required'"
        }
    }
    foreach ($required in @(
        'const RUNTIME_SERVICE_ACCOUNT_ID = "clearra-current-job"',
        'const BUILD_SERVICE_ACCOUNT_ID = "clearra-build"',
        'const JOB_SECRET = "clearra-job-token"',
        'const DISCORD_SECRET = "discord-bot-token"',
        'const POLICY_TROUBLESHOOTER_SERVICE = "policytroubleshooter.googleapis.com"',
        'const PROJECT_PAB_SEARCH_PERMISSION = "resourcemanager.projects.searchPolicyBindings"',
        'const SECRET_ACCESS_PERMISSION = "secretmanager.versions.access"',
        'const SECRET_SET_IAM_POLICY_PERMISSION = "secretmanager.secrets.setIamPolicy"',
        'const SECRET_MANAGER_ENDPOINT_ENV = "CLOUDSDK_API_ENDPOINT_OVERRIDES_SECRETMANAGER"',
        'const GLOBAL_SECRET_MANAGER_ENDPOINT = "https://secretmanager.googleapis.com/"',
        'roles/cloudbuild.builds.editor',
        'roles/run.admin',
        'roles/artifactregistry.reader',
        'roles/iam.serviceAccountUser',
        'roles/iam.serviceAccountViewer',
        'roles/iam.securityReviewer',
        'roles/iam.denyReviewer',
        'roles/secretmanager.viewer',
        'roles/serviceusage.serviceUsageConsumer',
        'roles/secretmanager.secretAccessor',
        'runtime service account must have zero project-level roles',
        'runtime service account must not access the Discord token Secret',
        'runtime service account must not access any non-job Secret',
        'observeServiceAccountEventually',
        'projects", "describe", projectId, "--format=json(projectId,projectNumber)',
        '`projects/${projectNumber}/secrets/`',
        'projects", "get-ancestors", projectId, "--format=json(id,type)',
        'search-target-policy-bindings',
        '--filter=policyKind=PRINCIPAL_ACCESS_BOUNDARY',
        '--filter=config.name=${POLICY_TROUBLESHOOTER_SERVICE}',
        'policy-intelligence',
        'troubleshoot-policy',
        'overallAccessState === "CAN_ACCESS"',
        'overallAccessState === "CANNOT_ACCESS"',
        'Policy Troubleshooter response contains unknown policy state',
        'Policy Troubleshooter response contains evaluation errors',
        'runtime service account has effective non-job Secret access',
        'runtime service account has inherited effective job Secret access before binding',
        'verifyPreBindingSecretAuthority',
        'isGlobalJobSecret',
        'isGlobalDiscordSecret',
        '"secrets", "locations", "list"',
        '`projects/${projectNumber}/locations/`',
        '`https://secretmanager.${location}.rep.googleapis.com/`',
        'CLOUDSDK_API_ENDPOINT_OVERRIDES_SECRETMANAGER',
        'assertSecretInventoryUnchanged',
        'Secret location or metadata catalog drifted during bootstrap',
        'env: gcloudProcessEnvironment(execution)',
        'key.toUpperCase() === SECRET_MANAGER_ENDPOINT_ENV',
        '--condition=None',
        'environment.ComSpec',
        'gcloud.cmd',
        'shell: false',
        'cloud_runtime_service_account=failed'
    )) {
        if ($runtimeServiceAccountBootstrap.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Cloud runtime service-account bootstrap is missing '$required'"
        }
    }
    if ($runtimeServiceAccountBootstrap -match '(?s)"secrets"\s*,\s*"versions"\s*,\s*"access"' -or
        $runtimeServiceAccountBootstrap -like '*print-access-token*') {
        Add-ArchitectureError 'Cloud runtime service-account bootstrap must never read a Secret payload or access token'
    }
    if ($runtimeServiceAccountBootstrap -match '(?s)"services"\s*,\s*"enable"' -or
        $runtimeServiceAccountBootstrap -match '(?s)"beta"\s*,\s*"policy-intelligence"') {
        Add-ArchitectureError 'Cloud runtime service-account bootstrap must not enable APIs or use the unavailable beta policy command'
    }
    if ($runtimeServiceAccountBootstrap -notmatch '(?s)"services"\s*,\s*"list"\s*,\s*"--enabled"') {
        Add-ArchitectureError 'Cloud runtime service-account bootstrap must prove the Policy Troubleshooter API is already enabled'
    }
    if ([regex]::Matches(
            $runtimeServiceAccountBootstrap,
            '(?m)^\s{2}assertParentlessProject\(run, projectId\);\s*$'
        ).Count -ne 2 -or
        [regex]::Matches(
            $runtimeServiceAccountBootstrap,
            '(?m)^\s{2}assertNoProjectPabBindings\(run, projectId, projectNumber\);\s*$'
        ).Count -ne 2) {
        Add-ArchitectureError 'Cloud runtime service-account bootstrap must re-observe exact parentless ancestry and empty PAB bindings after preparation'
    }
    $initialSecretInventoryIndex = $runtimeServiceAccountBootstrap.IndexOf(
        'const initialSecretInventory = requiredSecretInventory',
        [System.StringComparison]::Ordinal
    )
    $preBindingSecretAuthorityIndex = $runtimeServiceAccountBootstrap.IndexOf(
        'const preBindingAuthority = verifyPreBindingSecretAuthority',
        [System.StringComparison]::Ordinal
    )
    $preBindingInventoryIndex = $runtimeServiceAccountBootstrap.IndexOf(
        'const preBindingSecretInventory = requiredSecretInventory',
        [System.StringComparison]::Ordinal
    )
    $secretBindingWriteIndex = $runtimeServiceAccountBootstrap.IndexOf(
        '"add-iam-policy-binding"',
        [System.StringComparison]::Ordinal
    )
    $postBindingInventoryIndex = $runtimeServiceAccountBootstrap.IndexOf(
        'const postBindingSecretInventory = requiredSecretInventory',
        [System.StringComparison]::Ordinal
    )
    $postBindingAuthorityIndex = $runtimeServiceAccountBootstrap.IndexOf(
        'verifyExclusiveSecretAuthority(',
        [System.StringComparison]::Ordinal
    )
    $sealedInventoryIndex = $runtimeServiceAccountBootstrap.IndexOf(
        'const sealedSecretInventory = requiredSecretInventory',
        [System.StringComparison]::Ordinal
    )
    if ($initialSecretInventoryIndex -lt 0 -or
        $preBindingInventoryIndex -le $initialSecretInventoryIndex -or
        $preBindingSecretAuthorityIndex -le $preBindingInventoryIndex -or
        $secretBindingWriteIndex -le $preBindingSecretAuthorityIndex -or
        $postBindingInventoryIndex -le $secretBindingWriteIndex -or
        $postBindingAuthorityIndex -le $postBindingInventoryIndex -or
        $sealedInventoryIndex -le $postBindingAuthorityIndex) {
        Add-ArchitectureError 'Cloud runtime bootstrap must prove pre-binding exclusivity before the write, then refresh, verify, and seal the full Secret inventory'
    }
    if ([regex]::Matches(
            $runtimeServiceAccountBootstrap,
            '(?m)^\s{2}const (?:initial|preBinding|postBinding|sealed)SecretInventory = requiredSecretInventory\(run, projectId, projectNumber\);\s*$'
        ).Count -ne 4) {
        Add-ArchitectureError 'Cloud runtime bootstrap must enumerate the complete global/regional Secret inventory before, after, and after final authority validation'
    }
    foreach ($required in @(
        'bootstrap is idempotent and reads no Secret values',
        'rejects project roles and non-job Secret access',
        'rejects inherited access and unknown group evaluation',
        'proves effective exclusivity before a job binding write',
        'checks every regional Secret with its exact endpoint',
        'rejects incomplete regional catalogs before mutation',
        'rejects Secret catalog drift around post-binding proof',
        'accepts the exact numeric Secret parent only',
        'pins parentless ancestry and zero PAB bindings',
        'enforces fake metadata and getIamPolicy permissions',
        'requires build and deploy caller authority',
        're-observes ambiguous IAM mutations',
        'invokes the Windows gcloud shim without a Node shell',
        'must not access the Discord token Secret',
        'cannot act as the build service account',
        'policyTroubleshooterEnabled: false',
        'unknownEffectiveSecrets',
        'troubleshooterFault: "evaluation-error"',
        'troubleshooterFault: "missing-explanation"',
        'secretResourceParent: "999999999999"',
        'denyPabSearch: true',
        'ancestryDriftsAfterFirstCheck: true',
        'pabDriftsAfterFirstCheck: true',
        'newlyCreatedOverprivileged.mutations("secrets add-iam-policy-binding")',
        'regionalJobLeaf',
        'regionalCatalogFailure: "asia-northeast1"',
        'locationCatalogFault: "empty"',
        'locationCatalogDriftsAfterFirstRead: true',
        'locationCatalogDriftsAfterPreBinding: true',
        'secretCatalogDriftsAfterFirstRead: true',
        'secretCatalogDriftsAfterPreBinding: true',
        'gcloudProcessEnvironment(undefined, ambientEnvironment)'
    )) {
        if ($runtimeServiceAccountBootstrapTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Cloud runtime service-account regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'Never submit the working tree (`gcloud builds submit ... .`)',
        'node scripts/release/create-exact-source-archive.mjs',
        '--source-commit $sourceCommit',
        '--output $archivePath',
        'source.tar.gz',
        'tar -xzf $archivePath -C $configContext',
        'The local `.tar.gz` itself is the Cloud Build source boundary',
        'tracked public-source',
        'transfer it with its SHA-256',
        'recheck the digest',
        'extract it on Oracle',
        'separately frozen private overlay',
        'cloudbuild-current-job-service.yaml',
        'cloudbuild-command-sync.yaml',
        'cloudbuild-job-service.yaml',
        'same-full-40-character-accepted-commit',
        '$configContext'
    )) {
        if ($cloudDeploy.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Cloud deployment contract is missing exact-archive marker '$required'"
        }
    }
    $cloudBuildSubmissionCount = [regex]::Matches(
        $cloudDeploy,
        '(?m)^\s*gcloud builds submit\s+`\s*$'
    ).Count
    $archiveSourceArgumentCount = [regex]::Matches(
        $cloudDeploy,
        '(?m)^\s+\$archivePath\s*$'
    ).Count
    $exactArchiveCount = [regex]::Matches(
        $cloudDeploy,
        '(?m)^\s*node scripts/release/create-exact-source-archive\.mjs `\r?\n\s+--source-commit \$sourceCommit `\r?\n\s+--output \$archivePath\s*$'
    ).Count
    if ($cloudBuildSubmissionCount -lt 3 -or
        $archiveSourceArgumentCount -ne $cloudBuildSubmissionCount -or
        $exactArchiveCount -ne $cloudBuildSubmissionCount) {
        Add-ArchitectureError 'Every documented Cloud Build submission, including command sync, must submit the verified exact .tar.gz directly'
    }
    if ($cloudDeploy -match '(?m)^\s*git(?:\s+-c\s+core\.autocrlf=false)?\s+archive\b') {
        Add-ArchitectureError 'Cloud deployment documentation must use the tested exact source archive helper, not a raw Git archive command'
    }
    if ($cloudDeploy -match '(?m)^\s*\.\s*$' -or
        $cloudDeploy -match '(?m)^\s*gcloud\s+builds\s+submit\b[^\r\n]*\s\.\s*$') {
        Add-ArchitectureError 'Cloud deployment documentation must never submit the mutable current directory as a build context'
    }
    foreach ($required in @(
        'command-sync build must come from a fresh temporary commit-byte archive context',
        'node scripts/release/create-exact-source-archive.mjs',
        '--source-commit $sourceCommit',
        '--output $archivePath',
        'source.tar.gz',
        'tar -xzf $archivePath -C $configContext',
        'Join-Path $configContext "apps/clearra-discord-bot/cloudbuild-current-job-service.yaml"',
        '$configContext',
        'The verified archive'
    )) {
        if ($cloudReadme.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord deployment README is missing exact-archive marker '$required'"
        }
    }
    $readmeBuildSubmissionCount = [regex]::Matches(
        $cloudReadme,
        '(?m)^\s*gcloud builds submit\s+`\s*$'
    ).Count
    $readmeArchiveSourceArgumentCount = [regex]::Matches(
        $cloudReadme,
        '(?m)^\s+\$archivePath\s*$'
    ).Count
    $readmeExactArchiveCount = [regex]::Matches(
        $cloudReadme,
        '(?m)^\s*node scripts/release/create-exact-source-archive\.mjs `\r?\n\s+--source-commit \$sourceCommit `\r?\n\s+--output \$archivePath\s*$'
    ).Count
    if ($readmeBuildSubmissionCount -lt 1 -or
        $readmeArchiveSourceArgumentCount -ne $readmeBuildSubmissionCount -or
        $readmeExactArchiveCount -ne $readmeBuildSubmissionCount) {
        Add-ArchitectureError 'Every Discord deployment README Cloud Build submission must submit the verified exact .tar.gz directly'
    }
    if ($cloudReadme -match '(?m)^\s*git(?:\s+-c\s+core\.autocrlf=false)?\s+archive\b') {
        Add-ArchitectureError 'Discord deployment README must use the tested exact source archive helper, not a raw Git archive command'
    }
    if ($cloudReadme -match '(?m)^\s*\.\s*$' -or
        $cloudReadme -match '(?m)^\s*gcloud\s+builds\s+submit\b[^\r\n]*\s\.\s*$') {
        Add-ArchitectureError 'Discord deployment README must never submit the mutable current directory as a build context'
    }
    foreach ($deploymentDoc in @(
        @{ Name = 'Cloud Run deployment contract'; Text = $cloudDeploy },
        @{ Name = 'Discord deployment README'; Text = $cloudReadme }
    )) {
        foreach ($required in @(
            'node apps/clearra-discord-bot/scripts/prepare-cloud-runtime-service-account.mjs',
            '$jobBearerSecret = "clearra-job-token"',
            'zero project-level roles',
            'never a Secret version payload',
            'Service Account User',
            'policytroubleshooter.googleapis.com',
            'the helper never enables',
            'serviceusage.services.enable',
            'Service Usage Admin',
            'roles/serviceusage.serviceUsageAdmin',
            'projects get-ancestors',
            'PRINCIPAL_ACCESS_BOUNDARY',
            'resourcemanager.projects.searchPolicyBindings',
            'policy-intelligence troubleshoot-policy iam',
            'Secret Manager Viewer',
            'Service Account Viewer',
            'Security Reviewer',
            'Deny Reviewer',
            'Service Usage Consumer',
            'Browser',
            'groups.read',
            'secretmanager.secrets.setIamPolicy',
            'CAN_ACCESS',
            'CANNOT_ACCESS',
            'UNKNOWN',
            'global catalog plus every supported regional',
            'gcloud secrets locations list',
            'https://secretmanager.googleapis.com/',
            'https://secretmanager.LOCATION.rep.googleapis.com/',
            'CLOUDSDK_API_ENDPOINT_OVERRIDES_SECRETMANAGER',
            'before any Secret binding write',
            'freshly re-enumerates',
            'catalog drift',
            '--revision-suffix=$revisionSuffix',
            '--tag=$candidateTag',
            '--no-traffic',
            '$candidateUrl/health',
            'node apps/clearra-discord-bot/scripts/verify-cloud-run-candidate.mjs',
            'CLEARRA_CANDIDATE_JOB_TOKEN',
            '--to-revisions="$candidateRevision=100"',
            '--to-revisions="$priorRevision=100"',
            'candidate identity or zero-traffic isolation check failed',
            'contractSchemaVersion',
            'supplySemanticsId',
            'artifactSchemaVersion',
            'CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID=clearra.supply.projected-terminal-lookahead.v1',
            'CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION=clearra.solution-data.v1',
            '$oracleRemoteWrapper',
            '--operation capture-rollback-authority',
            'capture-oracle-rollback-authority.mjs',
            'independently computes the exact',
            'root:root mode 0755 regular non-symlink files',
            'mode-0666 launcher is stale authority',
            '--script-release-sha256 $oracleCandidateReleaseSha256',
            '--operation verify-candidate',
            'produce-oracle-deployment-proof.mjs candidate',
            'verify-oracle-candidate-proof.mjs',
            '--oracle-release-sha256 $oracleCandidateReleaseSha256',
            '/run/clearra-deploy/clearra-oracle-candidate-$deploymentNonce.json',
            '$priorCapture.priorOracleReleaseId -cnotmatch',
            '$priorCapture.priorOracleRelease -cne "/opt/clearra/releases/$($priorCapture.priorOracleReleaseId)"',
            'if ($candidateOracleExit -ne 0)',
            '--operation restore-prior-and-verify',
            'candidate Oracle verification failed; exact prior Oracle authority was restored and Cloud traffic was not changed',
            'restore-oracle-release',
            'produce-oracle-deployment-proof.mjs rollback',
            'verify-oracle-rollback-proof.mjs',
            '--prior-runtime-identity-sha256 $priorRuntimeIdentitySha256',
            '$serviceRolledBack = gcloud run services describe $serviceName',
            'prior Cloud revision did not become the sole 100-percent revision',
            '$candidateTaggedAfter = @($serviceAfter.status.traffic',
            'candidate did not become the sole 100-percent revision with its exact tagged URL preserved',
            'stable-URL rebinding is forbidden',
            'After global command synchronization'
        )) {
            if ($deploymentDoc.Text.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
                Add-ArchitectureError "$($deploymentDoc.Name) is missing staged cutover marker '$required'"
            }
        }
        $deployIndex = $deploymentDoc.Text.IndexOf(
            'gcloud run deploy $serviceName',
            [System.StringComparison]::Ordinal
        )
        $noTrafficIndex = $deploymentDoc.Text.IndexOf(
            '--no-traffic',
            [System.StringComparison]::Ordinal
        )
        $healthIndex = $deploymentDoc.Text.IndexOf(
            '$candidateUrl/health',
            [System.StringComparison]::Ordinal
        )
        $smokeIndex = $deploymentDoc.Text.IndexOf(
            'node apps/clearra-discord-bot/scripts/verify-cloud-run-candidate.mjs',
            [System.StringComparison]::Ordinal
        )
        $cutoverIndex = $deploymentDoc.Text.IndexOf(
            '--to-revisions="$candidateRevision=100"',
            [System.StringComparison]::Ordinal
        )
        $captureIndex = $deploymentDoc.Text.IndexOf(
            '--operation capture-rollback-authority',
            [System.StringComparison]::Ordinal
        )
        $scriptReleaseDigestMarker = '--script-release-sha256 $oracleCandidateReleaseSha256'
        $scriptReleaseDigestBindingCount = [regex]::Matches(
            $deploymentDoc.Text,
            [regex]::Escape($scriptReleaseDigestMarker)
        ).Count
        $priorRevisionCaptureIndex = $deploymentDoc.Text.IndexOf(
            '$priorRevision = [string]$priorTraffic[0].revisionName',
            [System.StringComparison]::Ordinal
        )
        $oracleCandidateProofIndex = $deploymentDoc.Text.IndexOf(
            '--operation verify-candidate',
            [System.StringComparison]::Ordinal
        )
        $candidateFailureIndex = $deploymentDoc.Text.IndexOf(
            'if ($candidateOracleExit -ne 0)',
            [System.StringComparison]::Ordinal
        )
        $preCutoverRestoreIndex = $deploymentDoc.Text.IndexOf(
            '--operation restore-prior-and-verify',
            [System.StringComparison]::Ordinal
        )
        $preCutoverAbortIndex = $deploymentDoc.Text.IndexOf(
            'candidate Oracle verification failed; exact prior Oracle authority was restored and Cloud traffic was not changed',
            [System.StringComparison]::Ordinal
        )
        $rollbackIndex = $deploymentDoc.Text.IndexOf(
            '--to-revisions="$priorRevision=100"',
            [System.StringComparison]::Ordinal
        )
        $candidateTagReobserveIndex = $deploymentDoc.Text.IndexOf(
            '$candidateTaggedAfter = @($serviceAfter.status.traffic',
            [System.StringComparison]::Ordinal
        )
        $postSyncPolicyIndex = $deploymentDoc.Text.IndexOf(
            'After global command synchronization',
            [System.StringComparison]::Ordinal
        )
        $rollbackTrafficAuthorityIndex = $deploymentDoc.Text.IndexOf(
            '$serviceRolledBack = gcloud run services describe $serviceName',
            [System.StringComparison]::Ordinal
        )
        $oracleRestoreIndex = $deploymentDoc.Text.LastIndexOf(
            '--operation restore-prior-and-verify',
            [System.StringComparison]::Ordinal
        )
        $captureScriptDigestIndex = $deploymentDoc.Text.IndexOf(
            $scriptReleaseDigestMarker,
            $captureIndex,
            [System.StringComparison]::Ordinal
        )
        $candidateScriptDigestIndex = $deploymentDoc.Text.IndexOf(
            $scriptReleaseDigestMarker,
            $oracleCandidateProofIndex,
            [System.StringComparison]::Ordinal
        )
        $preCutoverRestoreScriptDigestIndex = $deploymentDoc.Text.IndexOf(
            $scriptReleaseDigestMarker,
            $preCutoverRestoreIndex,
            [System.StringComparison]::Ordinal
        )
        $postCutoverRestoreScriptDigestIndex = $deploymentDoc.Text.LastIndexOf(
            $scriptReleaseDigestMarker,
            [System.StringComparison]::Ordinal
        )
        if ($deployIndex -lt 0 -or
            $priorRevisionCaptureIndex -lt 0 -or
            $captureIndex -le $priorRevisionCaptureIndex -or
            $captureIndex -lt 0 -or
            $scriptReleaseDigestBindingCount -ne 4 -or
            $captureScriptDigestIndex -le $captureIndex -or
            $captureScriptDigestIndex -ge $deployIndex -or
            $captureIndex -ge $deployIndex -or
            $noTrafficIndex -le $deployIndex -or
            $healthIndex -le $noTrafficIndex -or
            $smokeIndex -le $healthIndex -or
            $oracleCandidateProofIndex -le $smokeIndex -or
            $candidateScriptDigestIndex -le $oracleCandidateProofIndex -or
            $candidateScriptDigestIndex -ge $candidateFailureIndex -or
            $candidateFailureIndex -le $oracleCandidateProofIndex -or
            $preCutoverRestoreIndex -le $candidateFailureIndex -or
            $preCutoverRestoreScriptDigestIndex -le $preCutoverRestoreIndex -or
            $preCutoverRestoreScriptDigestIndex -ge $preCutoverAbortIndex -or
            $preCutoverAbortIndex -le $preCutoverRestoreIndex -or
            $cutoverIndex -le $oracleCandidateProofIndex -or
            $cutoverIndex -le $preCutoverRestoreIndex -or
            $cutoverIndex -le $preCutoverAbortIndex -or
            $candidateTagReobserveIndex -le $cutoverIndex -or
            $rollbackIndex -le $cutoverIndex -or
            $rollbackIndex -le $candidateTagReobserveIndex -or
            $rollbackTrafficAuthorityIndex -le $rollbackIndex -or
            $oracleRestoreIndex -le $rollbackTrafficAuthorityIndex -or
            $postCutoverRestoreScriptDigestIndex -le $oracleRestoreIndex -or
            $postSyncPolicyIndex -le $candidateTagReobserveIndex) {
            Add-ArchitectureError "$($deploymentDoc.Name) must capture prior authority before mutation, verify or restore Oracle before cutover, then re-observe and restore both authorities in order"
        }

        if ($deploymentDoc.Text -match '(?m)^\s*sudo\s+(?:node\s+)?"?/opt/clearra/releases/.*(?:produce-oracle-deployment-proof|verify-oracle-|restore-oracle-release)') {
            Add-ArchitectureError "$($deploymentDoc.Name) must cross the authenticated Oracle remote wrapper instead of running Oracle sudo locally"
        }
    }

    $cloudCandidateTagReobserveIndex = $cloudDeploy.IndexOf(
        '$candidateTaggedAfter = @($serviceAfter.status.traffic',
        [System.StringComparison]::Ordinal
    )
    $cloudPostRollbackRestoreIndex = $cloudDeploy.LastIndexOf(
        '--operation restore-prior-and-verify',
        [System.StringComparison]::Ordinal
    )
    $cloudSyncHeadingIndex = $cloudDeploy.IndexOf(
        '## Exact-SHA command synchronization',
        [System.StringComparison]::Ordinal
    )
    if ($cloudSyncHeadingIndex -le $cloudCandidateTagReobserveIndex -or
        $cloudSyncHeadingIndex -le $cloudPostRollbackRestoreIndex) {
        Add-ArchitectureError 'Cloud Run command synchronization must remain after exact tagged-candidate observation and the pre-sync rollback contract'
    }

    if ($cloudDeploy -like '*CLEARRA_ORACLE_CANDIDATE_VERIFIED*' -or
        $cloudReadme -like '*CLEARRA_ORACLE_CANDIDATE_VERIFIED*') {
        Add-ArchitectureError 'Cloud cutover must not trust an unbound reusable Oracle verification boolean'
    }
    foreach ($required in @(
        'active Oracle release is outside the immutable release root',
        'active Oracle settings must be a root-owned regular file',
        'active Oracle job URL must be a credential-free HTTPS /jobs URL',
        'prior Cloud runtime health identity is unavailable',
        '/etc/clearra-gateway/settings.pre-v0.7.5-',
        'openSync(temporaryPath, "wx", 0o600)',
        'linkSync(temporaryPath, backupPath)'
    )) {
        if ($oracleRollbackCapture.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle rollback capture is missing pre-mutation authority marker '$required'"
        }
    }
    foreach ($required in @(
        'active Oracle release tree digest does not match the expected artifact',
        'active Oracle settings digest does not match the expected snapshot',
        'current Oracle Gateway process has no READY record',
        'Oracle Gateway has no fresh successful bounded end-to-end operation',
        'restored prior runtime identity digest does not match the captured authority',
        '/run/clearra-deploy',
        'directoryMetadata.uid !== 0',
        '(directoryMetadata.mode & 0o777) !== 0o700',
        'linkSync(temporaryPath, proofPath)'
    )) {
        if ($oracleProofProducer.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Trusted Oracle proof producer is missing observation/security marker '$required'"
        }
    }
    foreach ($proofConsumer in @(
        @{ Name = 'candidate'; Text = $oracleCandidateProof },
        @{ Name = 'rollback'; Text = $oracleRollbackProof }
    )) {
        foreach ($required in @(
            'root-only namespace',
            'metadata.uid !== 0',
            '(metadata.mode & 0o777) !== 0o600',
            'remove(proofPath)'
        )) {
            if ($proofConsumer.Text.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
                Add-ArchitectureError "Oracle $($proofConsumer.Name) proof consumer is missing one-shot root-only marker '$required'"
            }
        }
    }
    foreach ($required in @(
        'Prior Oracle release does not match its captured tree digest.',
        'Prior Oracle settings backup does not match its captured digest.',
        'service_transition_started=1',
        'restore_verified=1',
        '[ "$service_transition_started" -eq 1 ] && [ "$restore_verified" -ne 1 ]',
        '"$systemctl_path" stop "$service_name" >/dev/null 2>&1 || true',
        'Restored Oracle process does not run from the prior immutable release.'
    )) {
        if ($oracleRestore.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle rollback helper is missing fail-closed restoration marker '$required'"
        }
    }
    foreach ($required in @(
        'release tree symlink escapes the immutable root',
        'release tree symlink is dangling',
        'clearra-release-tree-v1'
    )) {
        if ($oracleReleaseDigest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle release digest is missing immutable-tree marker '$required'"
        }
    }
    foreach ($testContract in @(
        @{ Name = 'capture'; Text = $oracleRollbackCaptureTest; Marker = 'freezes exact prior release, settings, job, and runtime' },
        @{ Name = 'producer'; Text = $oracleProofProducerTest; Marker = 'rejects stale settings, process, and operation evidence' },
        @{ Name = 'candidate'; Text = $oracleCandidateProofTest; Marker = 'rejects every stale deployment authority' },
        @{ Name = 'rollback'; Text = $oracleRollbackProofTest; Marker = 'rejects stale authority and missing live checks' },
        @{ Name = 'restore'; Text = $oracleRestoreTest; Marker = 'keeps the service stopped after every partial or unverified restore' },
        @{ Name = 'tree'; Text = $oracleReleaseDigestTest; Marker = 'rejects an external or mutable symlink target' }
    )) {
        if ($testContract.Text.IndexOf($testContract.Marker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle $($testContract.Name) recurrence contract is missing '$($testContract.Marker)'"
        }
    }

    foreach ($required in @(
        'ClearraJobExecutor',
        'expectedRuntimeIdentity',
        'deadlineUnixMs',
        'CLEARRA_CANDIDATE_JOB_TOKEN',
        'normalized_solution_set_hash',
        'candidate_smoke=failed'
    )) {
        if ($candidateSmoke.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Cloud Run candidate smoke is missing production contract marker '$required'"
        }
    }
    foreach ($required in @(
        'candidate smoke submits one bounded exact-runtime job',
        'expectedRuntimeIdentity',
        'invalid PC result contract'
    )) {
        if ($candidateSmokeTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Cloud Run candidate smoke regression is missing '$required'"
        }
    }
    foreach ($required in @('"idempotency-key"', 'Bearer ${this.authorizationToken}')) {
        if ($productionJobExecutor.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Production job executor is missing authenticated smoke transport marker '$required'"
        }
    }

    foreach ($required in @(
        '"fetch", "--no-tags"',
        'origin/main',
        'event=workflow_dispatch',
        'status=success',
        'head_sha=',
        'per_page=1',
        'active runtime identity does not match the accepted source'
    )) {
        if ($acceptedSourcePreflight.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Accepted-source preflight is missing authority marker '$required'"
        }
    }
    foreach ($required in @(
        'exact main, canonical acceptance, and active runtime',
        'missing acceptance',
        'runtime drift'
    )) {
        if ($acceptedSourcePreflightTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Accepted-source preflight regression is missing '$required'"
        }
    }
    foreach ($deploymentDoc in @(
        @{ Name = 'Cloud Run deployment contract'; Text = $cloudDeploy },
        @{ Name = 'Discord deployment README'; Text = $cloudReadme }
    )) {
        $runtimeBootstrapMarker = 'node apps/clearra-discord-bot/scripts/prepare-cloud-runtime-service-account.mjs'
        $policyTroubleshooterPrerequisiteIndex = $deploymentDoc.Text.IndexOf(
            'gcloud services enable policytroubleshooter.googleapis.com',
            [System.StringComparison]::Ordinal
        )
        $preflightIndex = $deploymentDoc.Text.IndexOf(
            'node apps/clearra-discord-bot/scripts/verify-accepted-source.mjs',
            [System.StringComparison]::Ordinal
        )
        $buildBootstrapIndex = $deploymentDoc.Text.IndexOf(
            $runtimeBootstrapMarker,
            [Math]::Max(0, $preflightIndex),
            [System.StringComparison]::Ordinal
        )
        $buildIndex = $deploymentDoc.Text.IndexOf(
            '$buildConfig = Join-Path $configContext',
            [System.StringComparison]::Ordinal
        )
        $deployIndex = $deploymentDoc.Text.IndexOf(
            'gcloud run deploy $serviceName',
            [System.StringComparison]::Ordinal
        )
        $deploymentBootstrapIndex = if ($deployIndex -ge 0) {
            $deploymentDoc.Text.LastIndexOf(
                $runtimeBootstrapMarker,
                $deployIndex,
                [System.StringComparison]::Ordinal
            )
        } else {
            -1
        }
        $jobSecretIndex = if ($deployIndex -ge 0) {
            $deploymentDoc.Text.LastIndexOf(
                '$jobBearerSecret = "clearra-job-token"',
                $deployIndex,
                [System.StringComparison]::Ordinal
            )
        } else {
            -1
        }
        $runtimeBootstrapCommandCount = [regex]::Matches(
            $deploymentDoc.Text,
            '(?m)^node apps/clearra-discord-bot/scripts/prepare-cloud-runtime-service-account\.mjs\s+`\s*$'
        ).Count
        if ($policyTroubleshooterPrerequisiteIndex -lt 0 -or
            $preflightIndex -le $policyTroubleshooterPrerequisiteIndex -or
            $buildBootstrapIndex -le $preflightIndex -or
            $buildIndex -le $buildBootstrapIndex) {
            Add-ArchitectureError "$($deploymentDoc.Name) must place the explicit Policy Troubleshooter prerequisite before accepted-source and Cloud IAM/build mutation"
        }
        if ($runtimeBootstrapCommandCount -lt 2 -or
            $deployIndex -lt 0 -or
            $jobSecretIndex -le $buildIndex -or
            $deploymentBootstrapIndex -le $jobSecretIndex -or
            $deploymentBootstrapIndex -le $buildIndex -or
            $deploymentBootstrapIndex -ge $deployIndex) {
            Add-ArchitectureError "$($deploymentDoc.Name) must repeat the exact runtime-SA/Secret/caller preflight immediately before Cloud Run deployment"
        }
    }
    $syncHeadingIndex = $cloudDeploy.IndexOf('## Exact-SHA command synchronization', [System.StringComparison]::Ordinal)
    $syncPreflightIndex = $cloudDeploy.IndexOf(
        'node apps/clearra-discord-bot/scripts/verify-accepted-source.mjs',
        [Math]::Max(0, $syncHeadingIndex),
        [System.StringComparison]::Ordinal
    )
    $syncActiveHealthIndex = $cloudDeploy.IndexOf(
        '--active-health-url',
        [Math]::Max(0, $syncHeadingIndex),
        [System.StringComparison]::Ordinal
    )
    $syncBuildIndex = $cloudDeploy.IndexOf(
        'gcloud builds submit',
        [Math]::Max(0, $syncHeadingIndex),
        [System.StringComparison]::Ordinal
    )
    if ($syncHeadingIndex -lt 0 -or
        $syncPreflightIndex -le $syncHeadingIndex -or
        $syncActiveHealthIndex -le $syncPreflightIndex -or
        $syncBuildIndex -le $syncActiveHealthIndex) {
        Add-ArchitectureError 'Command sync must verify current main, canonical acceptance, and active runtime identity before its mutation'
    }

    foreach ($required in @(
        'remoteIdentityRequired',
        'externalJobEndpoint',
        '!isLoopbackHostname',
        'Remote Clearra execution requires an explicit CLEARRA_JOB_URL.',
        'External or production remote Clearra execution requires'
    )) {
        if ($gatewayConfig.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle Gateway config is missing remote runtime authority marker '$required'"
        }
    }
    if ($gatewayCommandTests.IndexOf(
            'external remote execution requires explicit endpoint and exact current identity without NODE_ENV',
            [System.StringComparison]::Ordinal
        ) -lt 0) {
        Add-ArchitectureError 'Oracle Gateway tests must fail closed for external remote execution even when NODE_ENV is absent'
    }
    foreach ($deploymentDoc in @(
        @{ Name = 'Cloud Run deployment contract'; Text = $cloudDeploy },
        @{ Name = 'Discord deployment README'; Text = $cloudReadme }
    )) {
        $productionIndex = $deploymentDoc.Text.LastIndexOf('NODE_ENV=production', [System.StringComparison]::Ordinal)
        $remoteAuthorityIndex = $deploymentDoc.Text.LastIndexOf('CLEARRA_WORKER_AUTHORITY=remote', [System.StringComparison]::Ordinal)
        if ($productionIndex -lt 0 -or $remoteAuthorityIndex -le $productionIndex) {
            Add-ArchitectureError "$($deploymentDoc.Name) must pin production mode before enabling remote Oracle execution"
        }
    }
}
