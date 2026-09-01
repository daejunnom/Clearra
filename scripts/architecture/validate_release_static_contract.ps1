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
                $_.FullName -notmatch '[\\/](node_modules|dist|dist-server|build|target|coverage|tests?|\.cache|\.svelte-kit)[\\/]'
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

function Assert-ReleaseYamlExactFlowSequence {
    param(
        [string]$Text,
        [string]$Key,
        [string[]]$ExpectedValues,
        [string]$Contract
    )

    $keyPattern = '(?m)^    ' + [regex]::Escape($Key) + '\s*:'
    if ([regex]::Matches($Text, $keyPattern).Count -ne 1) {
        Add-ArchitectureError "$Contract must have exactly one '$Key' key"
        return
    }
    $sequencePattern = '(?m)^    ' + [regex]::Escape($Key) +
        ':\s*(?:\r?\n\s*)?\[([^\]]*)\]\s*$'
    $match = [regex]::Match($Text, $sequencePattern)
    if (-not $match.Success) {
        Add-ArchitectureError "$Contract must use a canonical YAML flow sequence"
        return
    }
    $actualValues = @($match.Groups[1].Value.Split(',') |
        ForEach-Object { $_.Trim() } |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $actualUnique = @($actualValues | Sort-Object -Unique)
    if ($actualValues.Count -ne $ExpectedValues.Count -or
        $actualUnique.Count -ne $ExpectedValues.Count -or
        @($ExpectedValues | Where-Object { $actualValues -notcontains $_ }).Count -ne 0) {
        Add-ArchitectureError "$Contract must be exactly [$($ExpectedValues -join ', ')], got [$($actualValues -join ', ')]"
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
        'apps/clearra-web', 'packages/clearra-ui/src/lib/wasm'
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
    $rustExact = Read-PhysicalText 'scripts/lib/rust-exact-tests.ps1'
    $noProductDebt = Read-PhysicalText 'scripts/lib/no-product-debt.ps1'
    $renderGolden = Read-PhysicalText 'scripts/lib/render-golden-gate.ps1'
    $desktopHost = Read-PhysicalText 'scripts/desktop-host-check.ps1'
    $coreCTestHelper = Read-PhysicalText 'scripts/lib/clearra-core-c-task-helpers.ps1'
    $cMake = Read-Text 'core-c/CMakeLists.txt'
    $releaseShardTest = Read-PhysicalText 'scripts/test_release_acceptance_shards.ps1'

    foreach ($required in @(
        'Get-ClearraReleaseAcceptanceTasks',
        '"AdversarialCorrectness"',
        'Invoke-AdversarialCorrectnessGate'
    )) {
        if ($clearra -notlike "*$required*") {
            Add-ArchitectureError "ReleaseAcceptance must execute adversarial correctness gate '$required'"
        }
    }
    foreach ($required in @(
        'CLEARRA_CORE_ADVERSARIAL_TESTS=ON', 'clearra_adversarial_tests',
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
    foreach ($requiredExecutionMarker in @(
        '$trustedExecution',
        'requires executed CTest evidence'
    )) {
        if ($coreCTestHelper -notlike "*$requiredExecutionMarker*") {
            Add-ArchitectureError "Adversarial CTest helper is missing execution contract '$requiredExecutionMarker'"
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
    foreach ($requiredSingleOwnerMarker in @(
        'ClearraReleaseAcceptanceMode',
        'adversarial_rust_tests=deferred owner=RustExactTests reason=single-release-suite'
    )) {
        if ($adversarial -notlike "*$requiredSingleOwnerMarker*") {
            Add-ArchitectureError "Adversarial release gate is missing single-owner marker '$requiredSingleOwnerMarker'"
        }
    }
    foreach ($requiredSingleOwnerMarker in @(
        'Assert-AdversarialRustCasesInOutput',
        'adversarial_rust_tests=executed owner=RustExactTests',
        'complete_required_keeps_candidate status=passed source=rust-test owner=RustExactTests'
    )) {
        if ($rustExact -notlike "*$requiredSingleOwnerMarker*") {
            Add-ArchitectureError "RustExactTests is missing delegated release evidence '$requiredSingleOwnerMarker'"
        }
    }
    foreach ($requiredSingleOwnerMarker in @(
        'complete_required_keeps_candidate status=deferred owner=RustExactTests reason=single-release-suite',
        'renderer_png_artifact status=deferred owner=RenderGolden reason=single-release-suite',
        'renderer_gif_artifact status=deferred owner=RenderGolden reason=single-release-suite',
        'desktop_real_app_request status=deferred owner=DesktopHost reason=single-release-suite'
    )) {
        if ($noProductDebt -notlike "*$requiredSingleOwnerMarker*") {
            Add-ArchitectureError "NoProductDebt is missing delegated release evidence '$requiredSingleOwnerMarker'"
        }
    }
    foreach ($requiredSingleOwnerMarker in @(
        'renderer_png_artifact status=passed source=rust-test owner=RenderGolden',
        'renderer_gif_artifact status=passed source=rust-test owner=RenderGolden'
    )) {
        if ($renderGolden -notlike "*$requiredSingleOwnerMarker*") {
            Add-ArchitectureError "RenderGolden is missing delegated release evidence '$requiredSingleOwnerMarker'"
        }
    }
    foreach ($requiredSingleOwnerMarker in @(
        'case_tauri_command_calls_clearra_gui_host_only::tauri_command_calls_clearra_gui_host_only',
        "-EvidenceId 'desktop_real_app_request'",
        'no_product_debt_evidence=$EvidenceId status=passed source=rust-test owner=DesktopHost',
        'ArchitectureValidatedByNoProductDebt',
        'desktop_architecture=deferred owner=NoProductDebt reason=single-release-suite'
    )) {
        if ($desktopHost -notlike "*$requiredSingleOwnerMarker*") {
            Add-ArchitectureError "DesktopHost is missing delegated release evidence '$requiredSingleOwnerMarker'"
        }
    }
    $taskDispatch = Read-PhysicalText 'scripts/lib/clearra-task-dispatch.ps1'
    foreach ($requiredSingleOwnerMarker in @(
        'ClearraNoProductDebtArchitecturePassed',
        'ClearraReleaseAcceptanceShard',
        'FoundationDesktopHost',
        '$architectureOwnedByParallelLeaf',
        '$script:ClearraNoProductDebtArchitecturePassed = $true',
        '$desktopHostArgs["ArchitectureValidatedByNoProductDebt"] = $true'
    )) {
        if (-not $taskDispatch.Contains($requiredSingleOwnerMarker)) {
            Add-ArchitectureError "Release task dispatch is missing DesktopHost single-owner marker '$requiredSingleOwnerMarker'"
        }
    }
    if (-not $clearra.Contains('$script:ClearraNoProductDebtArchitecturePassed = $false')) {
        Add-ArchitectureError 'Release runner must reset NoProductDebt architecture evidence before task execution'
    }
    foreach ($requiredShardTestMarker in @(
        'release_acceptance_shard_test=full-local-order',
        'release_acceptance_shard_test=foundation-order',
        'release_acceptance_shard_test=foundation-no-product-debt-leaf',
        'release_acceptance_shard_test=foundation-adversarial-correctness-leaf',
        'release_acceptance_shard_test=foundation-desktop-host-leaf',
        'release_acceptance_shard_test=sanitizer-order',
        'release_acceptance_shard_test=rust-order',
        'release_acceptance_shard_test=pages-order',
        'release_acceptance_shard_test=shard-union-equals-full',
        'release_acceptance_shard_test=selector-scope',
        'release_acceptance_shard_test=delegated-evidence-owners'
    )) {
        if ($releaseShardTest.IndexOf($requiredShardTestMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "ReleaseAcceptance shard regression is missing '$requiredShardTestMarker'"
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
    $releasePublicationFinalizer = Read-Text '.github/workflows/finalize-release-publication.yml'
    $pages = Read-Text '.github/workflows/pages.yml'
    $pagesRollback = Read-Text '.github/workflows/pages-rollback.yml'
    $discordDeployWorkflow = Read-Text '.github/workflows/discord-deploy.yml'
    $discordRecoveryWorkflow = Read-Text '.github/workflows/discord-deploy-recovery.yml'
    $discordRecoveryAuthority = Read-Text 'scripts/release/discord-deployment-recovery.mjs'
    $discordRuntimeRecovery = Read-Text 'scripts/release/invoke-discord-runtime-recovery-v080.ps1'
    $pagesRollbackAuthority = Read-Text 'scripts/release/pages-rollback-authority.mjs'
    $pagesRollbackAuthorityTest = Read-Text 'scripts/release/pages-rollback-authority.test.mjs'
    $pagesRollbackPackage = Read-Text 'scripts/release/pages-rollback-package.mjs'
    $pagesRollbackPackageTest = Read-Text 'scripts/release/pages-rollback-package.test.mjs'
    $pagesDeploymentAuthority = Read-Text 'scripts/release/pages-deployment-authority.mjs'
    $pagesDeploymentAuthorityTest = Read-Text 'scripts/release/pages-deployment-authority.test.mjs'
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
    $managedCandidateSmoke = Read-Text 'apps/clearra-discord-bot/scripts/run-cloud-candidate-smoke-job.mjs'
    $managedCandidateSmokeTest = Read-Text 'apps/clearra-discord-bot/test/cloud-candidate-smoke-job.test.mjs'
    $cloudCandidateRelease = Read-Text 'scripts/release/cloud/candidate-release-v080.mjs'
    $cloudCandidateReleaseTest = Read-Text 'scripts/release/cloud/candidate-release-v080.test.mjs'
    $githubWifBootstrap = Read-Text 'scripts/release/github/github-wif-bootstrap.mjs'
    $githubWifReadme = Read-Text 'scripts/release/github/README.md'
    $remainingWorkPlan = Read-Text 'docs/v0.8.0-remaining-work-plan.md'
    $oracleProofProducer = Read-Text 'apps/clearra-discord-bot/scripts/produce-oracle-deployment-proof.mjs'
    $oracleRuntimeAuthority = Read-Text 'apps/clearra-discord-bot/scripts/oracle-runtime-authority.mjs'
    $oracleCandidateProof = Read-Text 'apps/clearra-discord-bot/scripts/verify-oracle-candidate-proof.mjs'
    $oracleRollbackProof = Read-Text 'apps/clearra-discord-bot/scripts/verify-oracle-rollback-proof.mjs'
    $oracleRestore = Read-Text 'apps/clearra-discord-bot/scripts/restore-oracle-release'
    $oracleDeployLauncher = Read-Text 'scripts/release/oracle/clearra-oracle-release-deploy-v080'
    $oracleDeployInvoker = Read-Text 'scripts/release/oracle/invoke-release-deploy-v080.ps1'
    $oracleDeployInvokerTest = Read-Text 'scripts/release/oracle/invoke-release-deploy-v080.test.ps1'
    $oracleCandidateSettings = Read-Text 'scripts/release/oracle/candidate-settings-v080.mjs'
    $oracleCandidateSettingsTest = Read-Text 'scripts/release/oracle/candidate-settings-v080.test.mjs'
    $oracleAcceptedLayerBuilder = Read-Text 'scripts/release/oracle/create-local-layers-v080.sh'
    $oracleActionsLayerBuilder = Read-Text 'scripts/release/oracle/create-actions-layers-v080.sh'
    $oracleActionsLayerBuilderTest = Read-Text 'scripts/release/oracle/create-actions-layers-v080.test.mjs'

    foreach ($required in @(
        'const GITHUB_IMMUTABLE_REPOSITORY =',
        'daejunnom@${GITHUB_REPOSITORY_OWNER_ID}/Clearra@${GITHUB_REPOSITORY_ID}',
        'const GITHUB_SUBJECT_PREFIX = `repo:${GITHUB_IMMUTABLE_REPOSITORY}`;',
        '`${GITHUB_SUBJECT_PREFIX}:ref:${GITHUB_REF}`',
        '`${GITHUB_SUBJECT_PREFIX}:environment:discord-path-confirmation`',
        '`${GITHUB_SUBJECT_PREFIX}:environment:discord-runtime-rollback`',
        '`${GITHUB_SUBJECT_PREFIX}:environment:discord-global-command-sync`',
        'const LEGACY_GITHUB_SUBJECT_PREFIX = `repo:${GITHUB_REPOSITORY}`;',
        'const BUILDER_REMOVABLE_LEGACY_WIF_MEMBERS =',
        'const DEPLOYER_REMOVABLE_LEGACY_WIF_MEMBERS =',
        'const ROLLBACK_REMOVABLE_LEGACY_WIF_MEMBERS =',
        'const COMMAND_SYNC_REMOVABLE_LEGACY_WIF_MEMBERS =',
        'removableLegacyMembers:',
        'remove replaced legacy name-only',
        'function isWifRemoval(planned)'
    )) {
        if ($githubWifBootstrap.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "GitHub WIF bootstrap is missing immutable exact-subject marker '$required'"
        }
    }
    foreach ($publicContract in @(
        @{ Name = 'GitHub WIF README'; Text = $githubWifReadme },
        @{ Name = 'v0.8.0 remaining-work plan'; Text = $remainingWorkPlan }
    )) {
        if ($publicContract.Text.IndexOf(
                'repo:daejunnom@271715321/Clearra@1309293231',
                [System.StringComparison]::Ordinal
            ) -lt 0) {
            Add-ArchitectureError "$($publicContract.Name) is missing the immutable GitHub repository subject prefix"
        }
        if ($publicContract.Text.IndexOf(
                'legacy',
                [System.StringComparison]::OrdinalIgnoreCase
            ) -lt 0) {
            Add-ArchitectureError "$($publicContract.Name) is missing the bounded legacy WIF migration contract"
        }
    }
    if ($githubWifBootstrap.IndexOf(
            'repo:${GITHUB_REPOSITORY}:',
            [System.StringComparison]::Ordinal
        ) -ge 0) {
        Add-ArchitectureError 'GitHub WIF bootstrap retains the mutable legacy repository subject prefix'
    }
    $builderBucketRoles = [regex]::Match(
        $githubWifBootstrap,
        '(?s)const BUILDER_SOURCE_BUCKET_ROLES = Object\.freeze\(\[(.*?)\]\);'
    )
    if (-not $builderBucketRoles.Success -or
        $builderBucketRoles.Groups[1].Value -notmatch '(?s)^\s*"roles/storage\.bucketViewer",\s*"roles/storage\.objectCreator",\s*"roles/storage\.objectViewer",\s*$') {
        Add-ArchitectureError 'GitHub WIF bootstrap must grant builder bucket metadata, upload, and read authority on the exact Cloud Build source bucket'
    }
    $builderProjectRoles = [regex]::Match(
        $githubWifBootstrap,
        '(?s)const BUILDER_PROJECT_ROLES = Object\.freeze\(\[(.*?)\]\);'
    )
    if (-not $builderProjectRoles.Success -or
        $builderProjectRoles.Groups[1].Value -match 'roles/storage\.') {
        Add-ArchitectureError 'GitHub WIF builder Storage authority must not be project-wide'
    }
    foreach ($forbiddenStorageRole in @(
        'roles/storage.admin',
        'roles/storage.legacyBucketReader',
        'roles/storage.legacyBucketWriter'
    )) {
        if ($githubWifBootstrap.IndexOf(
                $forbiddenStorageRole,
                [System.StringComparison]::Ordinal
            ) -ge 0) {
            Add-ArchitectureError "GitHub WIF bootstrap retains forbidden broad or legacy Storage role '$forbiddenStorageRole'"
        }
    }

    foreach ($required in @(
        'group: discord-production',
        'cancel-in-progress: false',
        'queue: max',
        'environment: discord-path-confirmation',
        'environment: discord-global-command-sync',
        'discord-prestage-recovery-authority-',
        'discord-live-recovery-authority-',
        '--gcs-source-staging-dir="gs://clearra-cloud_cloudbuild/source"',
        'if: always() && needs.candidate.result == ''success''',
        'if: always() && needs.promote.result == ''success''',
        '-Operation cleanup-prestage-backup'
    )) {
        if ($discordDeployWorkflow.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord deploy cancellation/recovery contract is missing '$required'"
        }
    }
    foreach ($forbidden in @('environment: discord-runtime-rollback', 'rollback-after-sync:')) {
        if ($discordDeployWorkflow.IndexOf($forbidden, [System.StringComparison]::Ordinal) -ge 0) {
            Add-ArchitectureError "Primary Discord workflow retains forbidden rollback authority '$forbidden'"
        }
    }
    foreach ($required in @(
        'workflows: ["Deploy Discord Production"]',
        'group: discord-production',
        'cancel-in-progress: false',
        'queue: max',
        'environment: discord-runtime-rollback',
        'ref: ${{ github.sha }}',
        'GCP_ROLLBACK_WORKLOAD_IDENTITY_PROVIDER',
        'GCP_ROLLBACK_SERVICE_ACCOUNT',
        'actions/artifacts/$ARTIFACT_ID/zip',
        '-RecoveryAuthorityPath',
        'contains(fromJSON(''["failure","cancelled","timed_out"]''), github.event.workflow_run.conclusion)',
        'GCP_COMMAND_SYNC_SERVICE_ACCOUNT',
        'force-cancel path'
    )) {
        if ($discordRecoveryWorkflow.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord recovery workflow is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'ref: ${{ github.event.workflow_run.head_sha }}',
        'GCP_DEPLOY_SERVICE_ACCOUNT'
    )) {
        if ($discordRecoveryWorkflow.IndexOf($forbidden, [System.StringComparison]::Ordinal) -ge 0) {
            Add-ArchitectureError "Discord recovery workflow widens authority with '$forbidden'"
        }
    }
    foreach ($required in @(
        'entry.run_number',
        'entry.run_started_at',
        'entry.updated_at',
        'entry.repository?.id !== CLEARRA_REPOSITORY_ID',
        'entry.head_repository?.id !== CLEARRA_REPOSITORY_ID',
        'verifyDiscordRecoveryResult',
        'validateRecoveryArtifact(prestageMatches[0]',
        'job-steps-prove-no-prestage-upload-or-runtime-mutation',
        'validateNoPrestageArtifactAuthority',
        'Discord prestage artifact is absent after its upload step succeeded'
    )) {
        if ($discordRecoveryAuthority.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord recovery authority is missing '$required'"
        }
    }
    foreach ($required in @(
        'discord-deployment-recovery.mjs verify-result',
        'preserved-latest-zero-traffic-tagless',
        'Cloud residue readback retains candidate traffic or a direct-routing tag',
        'recovery_authority=$RecoveryAuthorityPath',
        '--stage prestage',
        '--stage live'
    )) {
        if ($discordRuntimeRecovery.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord runtime recovery is missing '$required'"
        }
    }
    $oracleFreezeHelper = Read-Text 'scripts/release/oracle/clearra-oracle-freeze-v080'
    $oracleInactiveStageTemplate = Read-Text 'scripts/release/oracle/clearra-oracle-inactive-stage-v080.template'
    $oracleRemoteOverlayCopyTest = Read-Text 'scripts/release/oracle/remote-overlay-copy-v080.test.py'
    $oracleFreezeInvoker = Read-Text 'scripts/release/oracle/invoke-freeze-v080.ps1'
    $oracleFreezeInvokerTest = Read-Text 'scripts/release/oracle/invoke-freeze-v080.test.ps1'
    $oracleInactiveStageInvoker = Read-Text 'scripts/release/oracle/invoke-inactive-stage-v080.ps1'
    $oracleInactiveStageInvokerTest = Read-Text 'scripts/release/oracle/invoke-inactive-stage-v080.test.ps1'
    $oracleFreezeTest = Read-Text 'scripts/release/oracle-freeze-v080.test.mjs'
    $oracleObservation = Read-Text 'apps/clearra-discord-bot/scripts/observe-oracle-candidate.mjs'
    $oracleObservationTest = Read-Text 'apps/clearra-discord-bot/test/oracle-candidate-observation.test.mjs'
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
    $acceptedCtk3Dist = Read-Text 'scripts/tools/accepted-ctk3-dist.mjs'
    $acceptedCtk3DistTest = Read-Text 'scripts/tools/accepted-ctk3-dist.test.mjs'
    $acceptedPagesBuild = Read-Text 'scripts/release/accepted-pages-build.mjs'
    $acceptedPagesBuildTest = Read-Text 'scripts/release/accepted-pages-build.test.mjs'
    $discordPackage = Read-Text 'apps/clearra-discord-bot/package.json'
    $uiPackage = Read-Text 'packages/clearra-ui/package.json'
    $webPackage = Read-Text 'apps/clearra-web/package.json'
    $uiContractTypecheck = Read-Text 'packages/clearra-ui/tsconfig.contract.json'
    $webContractTypecheck = Read-Text 'apps/clearra-web/tsconfig.contract.json'
    $productProcessSurface = Read-Text 'scripts/lib/product-process-surface.ps1'
    $productProcessContractTest = Read-Text 'scripts/test_product_process_contract.ps1'
    $remoteTagVerifier = Read-Text 'scripts/release/verify-remote-annotated-tag.mjs'
    $remoteTagVerifierTest = Read-Text 'scripts/release/verify-remote-annotated-tag.test.mjs'
    $discordCheckpointFinalizer = Read-Text 'scripts/release/finalize-discord-production-checkpoint.mjs'
    $discordCheckpointFinalizerTest = Read-Text 'scripts/release/finalize-discord-production-checkpoint.test.mjs'
    $gitAttributes = Read-Text '.gitattributes'
    $exactSourceArchive = Read-Text 'scripts/release/create-exact-source-archive.mjs'
    $exactSourceTarContract = Read-Text 'scripts/release/exact-source-tar-contract.mjs'
    $exactSourceArchiveTest = Read-Text 'scripts/release/create-exact-source-archive.test.mjs'
    $releaseCliSmokeTest = Read-Text 'scripts/tools/validate-release-cli-smokes.test.mjs'
    $releaseRegressionRunner = Read-Text 'scripts/tools/run-release-regression-tests.mjs'
    $releaseRegressionRunnerTest = Read-Text 'scripts/tools/run-release-regression-tests.test.mjs'
    $canonicalAcceptanceRun = Read-Text 'scripts/release/canonical-acceptance-run.mjs'
    $canonicalAcceptanceRunTest = Read-Text 'scripts/release/canonical-acceptance-run.test.mjs'
    $canonicalAcceptanceEvidence = Read-Text 'scripts/release/canonical-acceptance-evidence.mjs'
    $canonicalAcceptanceEvidenceTest = Read-Text 'scripts/release/canonical-acceptance-evidence.test.mjs'
    $canonicalReleaseEvidence = Read-Text 'scripts/release/canonical-release-evidence.mjs'
    $discordCommandSyncAuthority = Read-Text 'scripts/release/discord-command-sync-authority.mjs'
    $discordCommandSyncAuthorityTest = Read-Text 'scripts/release/discord-command-sync-authority.test.mjs'
    $discordCatalogRelease = Read-Text 'apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs'
    $discordCatalogReleaseTest = Read-Text 'apps/clearra-discord-bot/test/discord-command-catalog-release.test.mjs'
    $productionObservation = Read-Text 'scripts/release/observe-production-surfaces.mjs'
    $productionObservationTest = Read-Text 'scripts/release/observe-production-surfaces.test.mjs'
    $productionProbeAdapter = Read-Text 'scripts/release/production-surface-probe-adapter.mjs'
    $productionProbeAdapterTest = Read-Text 'scripts/release/production-surface-probe-adapter.test.mjs'
    $productionProbeMaterializer = Read-Text 'scripts/release/materialize-production-probe-spec.mjs'
    $productionProbeMaterializerTest = Read-Text 'scripts/release/materialize-production-probe-spec.test.mjs'
    $cloudCandidateSmokeReport = Read-Text 'scripts/release/cloud-candidate-smoke-report.mjs'
    $finalSourceValidator = Read-Text 'scripts/release/validate-final-source-revalidation.mjs'
    $finalSourceValidatorTest = Read-Text 'scripts/release/validate-final-source-revalidation.test.mjs'
    $finalSourceJournal = Read-Text 'scripts/release/final-source-attempt-journal.mjs'
    $finalSourceJournalTest = Read-Text 'scripts/release/final-source-attempt-journal.test.mjs'
    $finalSourceEventContract = Read-Text 'scripts/release/final-source-event-contract.mjs'
    $finalSourceEventContractTest = Read-Text 'scripts/release/final-source-event-contract.test.mjs'
    $finalSourceStageEvidence = Read-Text 'scripts/release/final-source-stage-evidence.mjs'
    $finalSourceStageEvidenceTest = Read-Text 'scripts/release/final-source-stage-evidence.test.mjs'
    $releasePublicationEvidence = Read-Text 'scripts/release/release-publication-evidence.mjs'
    $releasePublicationEvidenceTest = Read-Text 'scripts/release/release-publication-evidence.test.mjs'
    $remainingWorkPlan = Read-Text 'docs/v0.8.0-remaining-work-plan.md'

    foreach ($required in @(
        'validate-release-metadata.mjs',
        'node scripts/tools/run-release-regression-tests.mjs',
        'validate-release-cli-smokes.mjs',
        'release tag must point at the exact current main commit',
        'release tag is no longer the exact current main commit',
        'finalize-discord-production-checkpoint.mjs verify-tag',
        'finalize-discord-production-checkpoint.mjs verify-release',
        '--target "$GITHUB_SHA"',
        'group: canonical-release-${{ github.sha }}',
        'Require exact main and zero prior canonical success',
        'canonical release acceptance forbids workflow reruns',
        'node scripts/release/canonical-acceptance-run.mjs',
        '--require zero',
        '--require one',
        'accepted_run_id: ${{ steps.accepted_run.outputs.accepted_run_id }}',
        'accepted_run_attempt: ${{ steps.accepted_run.outputs.accepted_run_attempt }}',
        'if: github.event_name == ''workflow_dispatch''',
        'run-id: ${{ needs.metadata.outputs.accepted_run_id }}',
        'github-token: ${{ github.token }}',
        '--expected-run-id "$ACCEPTED_RUN_ID"',
        '--expected-run-attempt "$ACCEPTED_RUN_ATTEMPT"',
        'canonical-evidence:',
        'canonical-acceptance-evidence.mjs collect',
        'canonical-acceptance-evidence.mjs verify',
        'release-publication-evidence.mjs recover',
        'release-publication-evidence.mjs capture',
        'release-publication-receipt-${{ github.sha }}-run-${{ github.run_id }}-attempt-${{ github.run_attempt }}',
        'retention-days: 90',
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
        'clearra.discord-production-checkpoint-receipt.v1',
        'readCompleteRunArtifactCatalog',
        'validateExactCandidateArtifactCatalog',
        'validateSuccessfulDiscordDeploymentAuthority',
        'materializeCompletedJobTopology',
        'deployment_topology_contract',
        'extractClosedCanonicalJsonArtifactZip',
        'checkpoint tag/artifact/observation chronology is invalid',
        'production observation report is outside its exact completed job window',
        '"tag", "-a", "--cleanup=verbatim", "-F", "-", tag, sourceCommit',
        'assertRemoteMainAndAbsentTag(runGit, sourceCommit, tag)',
        'remote checkpoint tag object differs from the locally verified object',
        'remote annotated tag message differs from recomputed checkpoint receipt',
        'github-actions[bot]',
        'GitHub Release asset differs from accepted bytes'
    )) {
        if ($discordCheckpointFinalizer.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord checkpoint tag finalizer is missing '$required'"
        }
    }
    foreach ($required in @(
        'rejects an in-progress or unsuccessful Discord checkpoint run',
        'rejects missing, duplicate, foreign, expired, or wrong-attempt candidate artifacts',
        'rejects a failed candidate upload and artifact timestamps outside its step window',
        'seals all four completed Discord jobs and every contract step into the receipt topology',
        'rejects equal or inverted observation, artifact, completion, and tagger chronology',
        'preserves exact canonical receipt bytes through local and remote annotated tag readback',
        'rejects a non-bot or noncanonical immutable three-asset Release'
    )) {
        if ($discordCheckpointFinalizerTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord checkpoint tag finalizer regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'probeDiscordProductionSurface',
        'getGlobalCommands',
        'probeCloudProductionSurface',
        'validateCloudCandidateSmokeReport',
        'job_smoke_report_sha256',
        'stable_health_sha256',
        'tagged_health_sha256',
        'probePagesProductionSurface',
        'clearra-build-identity.json',
        'deployment_readback_sha256',
        'identity_readback_sha256',
        'file changed after probe-spec materialization'
    )) {
        if ($productionProbeAdapter.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Production surface adapter is missing fail-closed marker '$required'"
        }
    }
    foreach ($required in @(
        'Discord adapter performs one independent GET and binds the sealed catalog reports',
        'Cloud adapter binds service/revision control plane and both existing health URLs',
        'Cloud adapter rejects a smoke report without managed execution authority',
        'Pages adapter validates the sealed report, live deployment status, and accepted-build identity'
    )) {
        if ($productionProbeAdapterTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Production surface adapter regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'clearra.production-observation-probe-authority.v1',
        'production-surface-probe-adapter.mjs',
        'runtime: "powershell"',
        'smoke_report_file_sha256',
        'catalog_file_sha256',
        'sync_report_file_sha256',
        'tracked Oracle probe adapter differs from its approved SHA-256',
        'production probe interval must be exactly'
    )) {
        if ($productionProbeMaterializer.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Production probe-spec materializer is missing '$required'"
        }
    }
    foreach ($required in @(
        'materializes three tracked Node adapters and one explicit Oracle owner boundary',
        'probe authority rejects hash drift, secret fields, and mixed source identity',
        'interval must be exactly 1200 seconds',
        'CLEARRA_ORACLE_IDENTITY_FILE',
        'DISCORD_TOKEN'
    )) {
        if ($productionProbeMaterializerTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Production probe-spec materializer regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'clearra.cloud.candidate-smoke.v1',
        'zero_traffic_verified',
        'service_readback_sha256',
        'revision_readback_sha256',
        'smoke_job',
        'execution_name',
        'execution_readback_sha256',
        'Cloud candidate smoke report did not pass its bounded job contract'
    )) {
        if ($cloudCandidateSmokeReport.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Cloud candidate smoke report contract is missing '$required'"
        }
    }
    foreach ($required in @(
        'clearra.cloud.zero-traffic-candidate.v1',
        'candidate-${commitPrefix}',
        'image@sha256',
        '--no-traffic',
        '--min=0',
        '--min-instances=0',
        '--max=4',
        '--max-instances=4',
        '--set-secrets=CLEARRA_JOB_TOKEN=',
        '${authority.jobBearerSecretVersion}',
        'run", "jobs", "deploy',
        '--tasks=1',
        '--parallelism=1',
        '--max-retries=0',
        '--set-secrets=CLEARRA_CANDIDATE_JOB_TOKEN=',
        '--candidate-url',
        'labels.execution_name',
        'execution_readback_sha256',
        'validateCloudCandidateSmokeReport',
        'writeCanonicalReportNew',
        'gcloudProcessInvocation',
        'CLOSED_GCLOUD_ATOM',
        'CLOSED_GCLOUD_LOG_FILTER',
        'gcloud candidate arguments are not a closed command surface',
        'environment?.ComSpec',
        'gcloud.cmd',
        'shell: false'
    )) {
        if ($cloudCandidateRelease.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Managed Cloud candidate producer is missing '$required'"
        }
    }
    foreach ($required in @(
        'deploy resolves one tag to image@sha256 and independently seals zero traffic readback',
        'deploy and readback reject tag, traffic, image, and Secret-reference drift',
        "scale readback accepts Cloud Run's omitted default zero and rejects explicit drift",
        'smoke deploys one digest-bound managed-secret Job against the zero-traffic URL',
        'managed smoke log readback retries boundedly and rejects ambiguous attestations',
        'helper never reads a Secret payload or accepts a bearer value',
        'gcloud runner uses a closed Windows command shim and native non-Windows argv',
        'Windows gcloud command shim preserves one closed argument vector without cloud access',
        ' --verbosity=debug '
    )) {
        if ($cloudCandidateReleaseTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Managed Cloud candidate producer regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'ClearraJobExecutor',
        'expectedRuntimeIdentity',
        'deadlineUnixMs',
        'CLEARRA_CANDIDATE_JOB_TOKEN',
        'normalized_solution_set_hash',
        'candidate_smoke_job=failed'
    )) {
        if ($managedCandidateSmoke.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Managed Cloud candidate smoke Job is missing '$required'"
        }
    }
    foreach ($required in @(
        'managed-secret smoke Job submits one bounded exact-runtime /jobs request',
        'managed-secret smoke Job fails closed without authority or an exact PC result',
        'managed-secret smoke Job source never prints or serializes its bearer'
    )) {
        if ($managedCandidateSmokeTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Managed Cloud candidate smoke Job regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'materialize-production-probe-spec.mjs',
        'clearra.cloud.candidate-smoke.v1',
        'probe-spec',
        'managed-secret'
    )) {
        if ($cloudDeploy.IndexOf($required, [System.StringComparison]::OrdinalIgnoreCase) -lt 0 -or
            $remainingWorkPlan.IndexOf($required, [System.StringComparison]::OrdinalIgnoreCase) -lt 0) {
            Add-ArchitectureError "Release runbook and plan are missing observation adapter marker '$required'"
        }
    }
    foreach ($required in @(
        'rejects workflow defaults that can replace protected shells',
        'rejects a preceding step that can poison protected executable resolution',
        'rejects a skipped dependency injected into the metadata root job',
        'rejects a wrong runner whose comment contains the expected runner',
        'rejects a custom shell that only echoes the protected script path',
        'rejects a parent-commit archive hidden behind the expected SHA comment',
        'rejects Linux product builds on tag publication runs',
        'rejects release acceptance on tag publication runs',
        'rejects bounded release regressions on tag publication runs',
        'rejects bypassing the bounded release regression owner',
        'rejects publication without always handling expected skipped jobs',
        'rejects metadata that binds publication to the tag run itself',
        'rejects tag publication that downloads artifacts from its own run',
        'rejects accepted-run lookup without the exact head SHA binding',
        'rejects late validation without the bound run attempt',
        'rejects publication dependencies spoofed in a later job',
        'rejects removal of exact-source release concurrency',
        'rejects a canonical dispatch that permits a prior success',
        'rejects a canonical dispatch that permits workflow reruns',
        'rejects metadata without the accepted run attempt',
        'rejects canonical evidence without every accepted producer dependency',
        'rejects a truncated canonical job-evidence lookup',
        'rejects release gate evidence with a fixed run attempt',
        'rejects tag publication without canonical evidence verification',
        'rejects tag publication without downloaded product byte verification',
        'rejects ownership transfer moved before the archive helper succeeds',
        'rejects duplicate Windows exact-source archive unit coverage',
        'rejects removal of the dedicated CTK3 build and test owner',
        'rejects a publish-pattern name for the internal CTK3 artifact',
        'rejects a Discord suite that rebuilds CTK3',
        'rejects Discord consumption of a different CTK3 artifact',
        'rejects canonical acceptance consumption of a different CTK3 artifact',
        'rejects canonical acceptance without the accepted CTK3 release path',
        'rejects runtime-backed capability registry coverage in metadata',
        'rejects a standalone release smoke validation outside its mutation owner',
        'rejects Discord without the accepted CTK3 dependency',
        'rejects canonical acceptance without the exact Pages base path',
        'rejects an accepted Pages build without run-attempt binding',
        'rejects a publication-pattern accepted Pages artifact name',
        'rejects Pages acceptance lookup without GitHub-output binding',
        'rejects Pages accepted-run resolver drift',
        'rejects Pages download from an unbound workflow run',
        'rejects Pages rebuilding the already accepted GUI',
        'rejects Pages without closed artifact verification',
        'rejects Pages deploy without the bound acceptance authority',
        'rejects Pages upload without external artifact ID propagation',
        'rejects manually transcribed Pages rollback artifact authority',
        'rejects Pages rollback admission without sealed report resolution',
        'rejects Pages deployment without the tracked sealed authority producer',
        'rejects short-lived Pages deployment authority evidence',
        'rejects Pages download of a differently named accepted artifact',
        'rejects deployed Pages authority without exact base-path binding'
    )) {
        if ($releaseCliSmokeTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Release CLI smoke gate regression coverage is missing '$required'"
        }
    }
    foreach ($requiredPagesAcceptedRunMarker in @(
        'node scripts/release/canonical-acceptance-run.mjs',
        '--source-commit "$checked_sha"',
        '--require one',
        '--format github-output >> "$GITHUB_OUTPUT"',
        'node authority-source/scripts/release/canonical-acceptance-run.mjs',
        '--expected-run-id "$ACCEPTED_RUN_ID"',
        '--expected-run-attempt "$ACCEPTED_RUN_ATTEMPT"',
        'accepted-pages-build-${{ inputs.accepted_sha }}-run-${{ needs.accepted-source.outputs.accepted_run_id }}-attempt-${{ needs.accepted-source.outputs.accepted_run_attempt }}'
    )) {
        if ($pages.IndexOf($requiredPagesAcceptedRunMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages exact accepted-run binding is missing '$requiredPagesAcceptedRunMarker'"
        }
    }
    if ([regex]::Matches($pages, 'node scripts/release/canonical-acceptance-run\.mjs').Count -ne 1 -or
        [regex]::Matches($pages, 'node authority-source/scripts/release/canonical-acceptance-run\.mjs').Count -ne 1 -or
        $pages -match 'workflow_runs|actions/workflows/release-cli\.yml/runs|per_page=') {
        Add-ArchitectureError 'Pages must use the shared exact-one canonical acceptance resolver for initial and predeploy authority'
    }
    foreach ($requiredReleaseAcceptanceMarker in @(
        'group: canonical-release-${{ github.sha }}',
        'cancel-in-progress: false',
        'Require exact main and zero prior canonical success',
        '--require zero',
        'echo "accepted_run_id=$GITHUB_RUN_ID" >> "$GITHUB_OUTPUT"',
        'echo "accepted_run_attempt=$GITHUB_RUN_ATTEMPT" >> "$GITHUB_OUTPUT"',
        '--format github-output >> "$GITHUB_OUTPUT"',
        'ACCEPTED_RUN_ID: ${{ needs.metadata.outputs.accepted_run_id }}',
        'ACCEPTED_RUN_ATTEMPT: ${{ needs.metadata.outputs.accepted_run_attempt }}',
        '--expected-run-id "$ACCEPTED_RUN_ID"',
        '--expected-run-attempt "$ACCEPTED_RUN_ATTEMPT"',
        'canonical-evidence:',
        'canonical-acceptance-evidence.mjs collect',
        'canonical-acceptance-evidence.mjs verify',
        '--products dist'
    )) {
        if ($release.IndexOf($requiredReleaseAcceptanceMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Product release canonical acceptance is missing '$requiredReleaseAcceptanceMarker'"
        }
    }
    if ([regex]::Matches($release, 'node scripts/release/canonical-acceptance-run\.mjs').Count -ne 3 -or
        [regex]::Matches($release, '(?m)^\s*--require zero\s*$').Count -ne 1 -or
        [regex]::Matches($release, '(?m)^\s*--format github-output >> "\$GITHUB_OUTPUT"\s*$').Count -ne 1 -or
        $release -match 'workflow_runs\[0\]|accepted_run_id="\$\(gh api') {
        Add-ArchitectureError 'Product release must enforce the shared zero-before-dispatch and exact-one-after-success transition'
    }
    foreach ($requiredResolverMarker in @(
        'value.total_count !== expectedCount',
        'value.workflow_runs.length !== expectedCount',
        'run.event !== "workflow_dispatch"',
        'run.status !== "completed"',
        'run.conclusion !== "success"',
        'run.head_branch !== "main"',
        'run.head_sha !== sourceCommit',
        'run.path !== WORKFLOW_PATH',
        'attempt !== "1"',
        'resolveCanonicalAcceptanceHistory',
        'list.workflow_runs.length !== list.total_count',
        'dependencies.getAttempt',
        '/attempts/${runAttempt}',
        '"branch=main"',
        '"per_page=100"'
    )) {
        if ($canonicalAcceptanceRun.IndexOf($requiredResolverMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Canonical acceptance resolver is missing '$requiredResolverMarker'"
        }
    }
    foreach ($requiredResolverTestMarker in @(
        'requires exact zero before a dispatch',
        'binds exactly one complete run identity',
        'rejects duplicate success and malformed counts',
        'rejects a different bound run or attempt',
        'rejects every workflow rerun attempt',
        'preserves a hidden first-attempt success after a failed rerun',
        'rejects duplicate successes across workflow attempt history',
        'rejects truncated or cross-owned workflow attempt history',
        'non-truncating page'
    )) {
        if ($canonicalAcceptanceRunTest.IndexOf($requiredResolverTestMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Canonical acceptance resolver regression is missing '$requiredResolverTestMarker'"
        }
    }
    if ($canonicalAcceptanceRun.IndexOf('status=success', [System.StringComparison]::Ordinal) -ge 0) {
        Add-ArchitectureError 'Canonical acceptance history must not hide earlier attempts behind the mutable latest-run success filter'
    }
    foreach ($requiredEvidenceMarker in @(
        'clearra.canonical-acceptance-evidence.v1',
        'verifyAcceptedCtk3Dist',
        'verifyAcceptedPagesBuild',
        'verifyCanonicalAcceptanceEvidence',
        'validateReleaseJobs',
        'clearra.release-acceptance-shard.v1',
        'createShardedReleaseGateReports',
        'delegated_evidence',
        'isolated-six-shard',
        'release_version',
        'pages_base_path',
        'downloaded release products differ from canonical acceptance evidence',
        'release-acceptance',
        'surface_reports',
        'release_artifacts',
        'canonical-acceptance-evidence.mjs verify'
    )) {
        $evidenceSurface = if ($requiredEvidenceMarker -eq 'canonical-acceptance-evidence.mjs verify') {
            $release
        } else {
            $canonicalAcceptanceEvidence
        }
        if ($evidenceSurface.IndexOf($requiredEvidenceMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Canonical acceptance evidence is missing '$requiredEvidenceMarker'"
        }
    }
    foreach ($requiredEvidenceTestMarker in @(
        'deterministically bind toolchains and four surfaces',
        'six isolated shard reports preserve unique stage ownership and delegated evidence',
        'shard toolchain collection invokes only the closed shard tool set',
        'rejects duplicate jobs and any failed required step',
        'hashes three real products',
        'downloaded release products differ'
    )) {
        if ($canonicalAcceptanceEvidenceTest.IndexOf($requiredEvidenceTestMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Canonical acceptance evidence regression is missing '$requiredEvidenceTestMarker'"
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
        'realpathSync.native(resolve(output))',
        'const helperPath = realpathSync.native(',
        'const invokedPath =',
        'realpathSync.native(resolve(process.argv[1]))',
        'if (invokedPath === modulePath)',
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
        'accepts the canonical helper through an equivalent Windows short-path alias',
        'enters the CLI through a Windows repository junction alias',
        'rejects a canonical helper directory junction that escapes the repository',
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
        -ExpectedKeys @('name', 'on', 'permissions', 'concurrency', 'env', 'jobs') `
        -Contract 'Release workflow top level'
    $releasePermissionsStart = $release.IndexOf("`npermissions:", [System.StringComparison]::Ordinal)
    $releaseConcurrencyStart = $release.IndexOf("`nconcurrency:", [System.StringComparison]::Ordinal)
    $releaseEnvironmentStart = $release.IndexOf("`nenv:", [System.StringComparison]::Ordinal)
    $releaseJobsStart = $release.IndexOf("`njobs:", [System.StringComparison]::Ordinal)
    if ($releasePermissionsStart -lt 0 -or
        $releaseConcurrencyStart -le $releasePermissionsStart -or
        $releaseEnvironmentStart -le $releaseConcurrencyStart -or
        $releaseJobsStart -le $releaseEnvironmentStart) {
        Add-ArchitectureError 'Release workflow environment boundary is missing'
    }
    else {
        $releasePermissions = $release.Substring(
            $releasePermissionsStart,
            $releaseConcurrencyStart - $releasePermissionsStart
        )
        Assert-ReleaseYamlExactKeySet `
            -Text $releasePermissions `
            -Indentation 2 `
            -ExpectedKeys @('contents', 'actions') `
            -Contract 'Release workflow permissions'
        Assert-ReleaseYamlExactScalar `
            -Text $releasePermissions `
            -Indentation 2 `
            -Key 'contents' `
            -ExpectedValue 'write' `
            -Contract 'Release contents permission'
        Assert-ReleaseYamlExactScalar `
            -Text $releasePermissions `
            -Indentation 2 `
            -Key 'actions' `
            -ExpectedValue 'read' `
            -Contract 'Accepted-run lookup permission'
        $releaseConcurrency = $release.Substring(
            $releaseConcurrencyStart,
            $releaseEnvironmentStart - $releaseConcurrencyStart
        )
        Assert-ReleaseYamlExactKeySet `
            -Text $releaseConcurrency `
            -Indentation 2 `
            -ExpectedKeys @('group', 'cancel-in-progress') `
            -Contract 'Release exact-source concurrency'
        Assert-ReleaseYamlExactScalar `
            -Text $releaseConcurrency `
            -Indentation 2 `
            -Key 'group' `
            -ExpectedValue 'canonical-release-${{ github.sha }}' `
            -Contract 'Release exact-source concurrency group'
        Assert-ReleaseYamlExactScalar `
            -Text $releaseConcurrency `
            -Indentation 2 `
            -Key 'cancel-in-progress' `
            -ExpectedValue 'false' `
            -Contract 'Release exact-source concurrency policy'
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
    $ctk3JobStart = $release.IndexOf("`n  ctk3:", [System.StringComparison]::Ordinal)
    $linuxJobStart = $release.IndexOf("`n  linux-cli:", [System.StringComparison]::Ordinal)
    $discordJobStart = $release.IndexOf("`n  discord-bot:", [System.StringComparison]::Ordinal)
    $releaseFoundationNoProductDebtJobStart = $release.IndexOf("`n  release-acceptance-foundation-no-product-debt:", [System.StringComparison]::Ordinal)
    $releaseFoundationAdversarialCorrectnessJobStart = $release.IndexOf("`n  release-acceptance-foundation-adversarial-correctness:", [System.StringComparison]::Ordinal)
    $releaseFoundationDesktopHostJobStart = $release.IndexOf("`n  release-acceptance-foundation-desktop-host:", [System.StringComparison]::Ordinal)
    $releaseSanitizerJobStart = $release.IndexOf("`n  release-acceptance-sanitizer:", [System.StringComparison]::Ordinal)
    $releaseRustJobStart = $release.IndexOf("`n  release-acceptance-rust:", [System.StringComparison]::Ordinal)
    $releasePagesJobStart = $release.IndexOf("`n  release-acceptance-pages:", [System.StringComparison]::Ordinal)
    $releaseAcceptanceJobStart = $release.IndexOf("`n  release-acceptance:", [System.StringComparison]::Ordinal)
    $windowsCliJobStart = $release.IndexOf("`n  windows-cli:", [System.StringComparison]::Ordinal)
    $windowsGuiJobStart = $release.IndexOf("`n  windows-gui:", [System.StringComparison]::Ordinal)
    $canonicalEvidenceJobStart = $release.IndexOf("`n  canonical-evidence:", [System.StringComparison]::Ordinal)
    $publishBoundaryStart = $release.IndexOf("`n  publish:", [System.StringComparison]::Ordinal)
    if ($metadataJobStart -lt 0 -or
        $ctk3JobStart -le $metadataJobStart -or
        $linuxJobStart -le $ctk3JobStart -or
        $discordJobStart -le $linuxJobStart -or
        $releaseFoundationNoProductDebtJobStart -le $discordJobStart -or
        $releaseFoundationAdversarialCorrectnessJobStart -le $releaseFoundationNoProductDebtJobStart -or
        $releaseFoundationDesktopHostJobStart -le $releaseFoundationAdversarialCorrectnessJobStart -or
        $releaseSanitizerJobStart -le $releaseFoundationDesktopHostJobStart -or
        $releaseRustJobStart -le $releaseSanitizerJobStart -or
        $releasePagesJobStart -le $releaseRustJobStart -or
        $releaseAcceptanceJobStart -le $releasePagesJobStart -or
        $windowsCliJobStart -le $releaseAcceptanceJobStart -or
        $windowsGuiJobStart -le $windowsCliJobStart -or
        $canonicalEvidenceJobStart -le $windowsGuiJobStart -or
        $publishBoundaryStart -le $canonicalEvidenceJobStart) {
        Add-ArchitectureError 'Exact source archive workflow job boundaries are missing'
    }
    else {
        $metadataJob = $release.Substring($metadataJobStart, $ctk3JobStart - $metadataJobStart)
        $ctk3Job = $release.Substring($ctk3JobStart, $linuxJobStart - $ctk3JobStart)
        $linuxJob = $release.Substring($linuxJobStart, $discordJobStart - $linuxJobStart)
        $discordJob = $release.Substring($discordJobStart, $releaseFoundationNoProductDebtJobStart - $discordJobStart)
        $releaseFoundationNoProductDebtJob = $release.Substring(
            $releaseFoundationNoProductDebtJobStart,
            $releaseFoundationAdversarialCorrectnessJobStart - $releaseFoundationNoProductDebtJobStart
        )
        $releaseFoundationAdversarialCorrectnessJob = $release.Substring(
            $releaseFoundationAdversarialCorrectnessJobStart,
            $releaseFoundationDesktopHostJobStart - $releaseFoundationAdversarialCorrectnessJobStart
        )
        $releaseFoundationDesktopHostJob = $release.Substring(
            $releaseFoundationDesktopHostJobStart,
            $releaseSanitizerJobStart - $releaseFoundationDesktopHostJobStart
        )
        $releaseFoundationJob = $releaseFoundationNoProductDebtJob
        $releaseSanitizerJob = $release.Substring(
            $releaseSanitizerJobStart,
            $releaseRustJobStart - $releaseSanitizerJobStart
        )
        $releaseRustJob = $release.Substring(
            $releaseRustJobStart,
            $releasePagesJobStart - $releaseRustJobStart
        )
        $releasePagesJob = $release.Substring(
            $releasePagesJobStart,
            $releaseAcceptanceJobStart - $releasePagesJobStart
        )
        $releaseAcceptanceJob = $release.Substring(
            $releaseAcceptanceJobStart,
            $windowsCliJobStart - $releaseAcceptanceJobStart
        )
        $windowsCliJob = $release.Substring(
            $windowsCliJobStart,
            $windowsGuiJobStart - $windowsCliJobStart
        )
        $windowsGuiJob = $release.Substring(
            $windowsGuiJobStart,
            $canonicalEvidenceJobStart - $windowsGuiJobStart
        )
        $canonicalEvidenceJob = $release.Substring(
            $canonicalEvidenceJobStart,
            $publishBoundaryStart - $canonicalEvidenceJobStart
        )
        Assert-ReleaseYamlExactKeySet `
            -Text $metadataJob `
            -Indentation 4 `
            -ExpectedKeys @('outputs', 'runs-on', 'steps') `
            -Contract 'Linux metadata job'
        Assert-ReleaseYamlExactKeySet `
            -Text $releaseAcceptanceJob `
            -Indentation 4 `
            -ExpectedKeys @('if', 'needs', 'runs-on', 'steps') `
            -Contract 'Canonical acceptance fan-in job'
        Assert-ReleaseYamlExactScalar `
            -Text $metadataJob `
            -Indentation 4 `
            -Key 'runs-on' `
            -ExpectedValue 'ubuntu-latest' `
            -Contract 'Linux metadata runner'
        Assert-ReleaseYamlExactScalar `
            -Text $releaseAcceptanceJob `
            -Indentation 4 `
            -Key 'if' `
            -ExpectedValue 'github.event_name == ''workflow_dispatch''' `
            -Contract 'Canonical acceptance fan-in dispatch-only condition'
        Assert-ReleaseYamlExactFlowSequence `
            -Text $releaseAcceptanceJob `
            -Key 'needs' `
            -ExpectedValues @(
                'metadata',
                'release-acceptance-foundation-no-product-debt',
                'release-acceptance-foundation-adversarial-correctness',
                'release-acceptance-foundation-desktop-host',
                'release-acceptance-sanitizer',
                'release-acceptance-rust',
                'release-acceptance-pages'
            ) `
            -Contract 'Canonical acceptance exact six-shard fan-in dependencies'
        Assert-ReleaseYamlExactScalar `
            -Text $releaseAcceptanceJob `
            -Indentation 4 `
            -Key 'runs-on' `
            -ExpectedValue 'ubuntu-latest' `
            -Contract 'Canonical acceptance fan-in runner'
        foreach ($shardJob in @(
            @{ Name = 'Foundation NoProductDebt'; Text = $releaseFoundationNoProductDebtJob; Needs = @('metadata') },
            @{ Name = 'Foundation AdversarialCorrectness'; Text = $releaseFoundationAdversarialCorrectnessJob; Needs = @('metadata') },
            @{ Name = 'Foundation DesktopHost'; Text = $releaseFoundationDesktopHostJob; Needs = @('metadata') },
            @{ Name = 'Sanitizer'; Text = $releaseSanitizerJob; Needs = @('metadata') },
            @{ Name = 'Rust'; Text = $releaseRustJob; Needs = @('metadata', 'ctk3') },
            @{ Name = 'Pages'; Text = $releasePagesJob; Needs = @('metadata') }
        )) {
            Assert-ReleaseYamlExactKeySet `
                -Text $shardJob.Text `
                -Indentation 4 `
                -ExpectedKeys @('if', 'needs', 'runs-on', 'timeout-minutes', 'steps') `
                -Contract "$($shardJob.Name) canonical acceptance shard job"
            Assert-ReleaseYamlExactScalar `
                -Text $shardJob.Text `
                -Indentation 4 `
                -Key 'if' `
                -ExpectedValue 'github.event_name == ''workflow_dispatch''' `
                -Contract "$($shardJob.Name) canonical acceptance dispatch-only condition"
            Assert-ReleaseYamlExactScalar `
                -Text $shardJob.Text `
                -Indentation 4 `
                -Key 'runs-on' `
                -ExpectedValue 'windows-latest' `
                -Contract "$($shardJob.Name) canonical acceptance runner"
            if ($shardJob.Needs.Count -eq 1) {
                Assert-ReleaseYamlExactScalar `
                    -Text $shardJob.Text `
                    -Indentation 4 `
                    -Key 'needs' `
                    -ExpectedValue $shardJob.Needs[0] `
                    -Contract "$($shardJob.Name) canonical acceptance dependency"
            }
            else {
                Assert-ReleaseYamlExactFlowSequence `
                    -Text $shardJob.Text `
                    -Key 'needs' `
                    -ExpectedValues $shardJob.Needs `
                    -Contract "$($shardJob.Name) canonical acceptance dependencies"
            }
        }
        foreach ($job in @(
            @{ Name = 'CTK3'; Text = $ctk3Job; Runner = 'ubuntu-latest' },
            @{ Name = 'Linux CLI'; Text = $linuxJob; Runner = 'ubuntu-latest' },
            @{ Name = 'Windows CLI'; Text = $windowsCliJob; Runner = 'windows-latest' },
            @{ Name = 'Windows GUI'; Text = $windowsGuiJob; Runner = 'windows-latest' }
        )) {
            Assert-ReleaseYamlExactKeySet `
                -Text $job.Text `
                -Indentation 4 `
                -ExpectedKeys @('if', 'needs', 'runs-on', 'steps') `
                -Contract "$($job.Name) job"
            Assert-ReleaseYamlExactScalar `
                -Text $job.Text `
                -Indentation 4 `
                -Key 'if' `
                -ExpectedValue 'github.event_name == ''workflow_dispatch''' `
                -Contract "$($job.Name) dispatch-only condition"
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
        Assert-ReleaseYamlExactKeySet `
            -Text $discordJob `
            -Indentation 4 `
            -ExpectedKeys @('if', 'needs', 'runs-on', 'steps') `
            -Contract 'Discord job'
        Assert-ReleaseYamlExactScalar `
            -Text $discordJob `
            -Indentation 4 `
            -Key 'if' `
            -ExpectedValue 'github.event_name == ''workflow_dispatch''' `
            -Contract 'Discord dispatch-only condition'
        Assert-ReleaseYamlExactFlowSequence `
            -Text $discordJob `
            -Key 'needs' `
            -ExpectedValues @('metadata', 'ctk3') `
            -Contract 'Discord dependency on metadata and accepted CTK3'
        Assert-ReleaseYamlExactScalar `
            -Text $discordJob `
            -Indentation 4 `
            -Key 'runs-on' `
            -ExpectedValue 'ubuntu-latest' `
            -Contract 'Discord runner'
        Assert-ReleaseYamlExactKeySet `
            -Text $canonicalEvidenceJob `
            -Indentation 4 `
            -ExpectedKeys @('if', 'needs', 'runs-on', 'steps') `
            -Contract 'Canonical acceptance evidence job'
        Assert-ReleaseYamlExactScalar `
            -Text $canonicalEvidenceJob `
            -Indentation 4 `
            -Key 'if' `
            -ExpectedValue 'github.event_name == ''workflow_dispatch''' `
            -Contract 'Canonical acceptance evidence dispatch-only condition'
        Assert-ReleaseYamlExactFlowSequence `
            -Text $canonicalEvidenceJob `
            -Key 'needs' `
            -ExpectedValues @('metadata', 'ctk3', 'linux-cli', 'discord-bot', 'release-acceptance', 'windows-cli', 'windows-gui') `
            -Contract 'Canonical acceptance evidence producer dependencies'
        Assert-ReleaseYamlExactScalar `
            -Text $canonicalEvidenceJob `
            -Indentation 4 `
            -Key 'runs-on' `
            -ExpectedValue 'ubuntu-latest' `
            -Contract 'Canonical acceptance evidence runner'

        Assert-ReleaseExactStepSkeleton `
            -Text $ctk3Job `
            -ExpectedSteps @(
                '- uses: actions/checkout@v4',
                '- uses: actions/setup-node@v4',
                '- name: Install JavaScript workspace',
                '- name: Validate accepted CTK3 artifact contract',
                '- name: Build and test CTK3 once',
                '- name: Seal the accepted CTK3 distribution',
                '- name: Upload accepted CTK3 distribution'
            ) `
            -Contract 'Accepted CTK3 single owner'
        foreach ($requiredCtk3OwnerMarker in @(
            'run: node --test scripts/tools/accepted-ctk3-dist.test.mjs',
            'run: npm test --workspace ctk3',
            'run: node scripts/tools/accepted-ctk3-dist.mjs --seal packages/ctk3/dist --source-commit "$CLEARRA_SOURCE_COMMIT" --run-id "$GITHUB_RUN_ID" --run-attempt "$GITHUB_RUN_ATTEMPT"',
            'uses: actions/upload-artifact@v4',
            'name: ctk3-accepted-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}',
            'path: packages/ctk3/dist',
            'if-no-files-found: error'
        )) {
            if ($ctk3Job.IndexOf($requiredCtk3OwnerMarker, [System.StringComparison]::Ordinal) -lt 0) {
                Add-ArchitectureError "Accepted CTK3 owner is missing '$requiredCtk3OwnerMarker'"
            }
        }
        if ([regex]::Matches($release, '(?m)^        run: npm test --workspace ctk3\s*$').Count -ne 1) {
            Add-ArchitectureError 'CTK3 package build and test must have exactly one workflow owner'
        }
        if ([regex]::Matches($release, 'name: ctk3-accepted-\$\{\{ github\.sha \}\}-run-\$\{\{ needs\.metadata\.outputs\.accepted_run_id \}\}-attempt-\$\{\{ needs\.metadata\.outputs\.accepted_run_attempt \}\}').Count -ne 4) {
            Add-ArchitectureError 'Accepted CTK3 artifact must have one producer and exactly three same-attempt consumers'
        }
        if ($ctk3Job -match 'name:\s*clearra-.*-v') {
            Add-ArchitectureError 'Internal accepted CTK3 artifact must not match the publication artifact pattern'
        }
        Assert-ReleaseExactStepSkeleton `
            -Text $discordJob `
            -ExpectedSteps @(
                '- uses: actions/checkout@v4',
                '- uses: actions/setup-node@v4',
                '- name: Download accepted CTK3 distribution',
                '- name: Install JavaScript workspace',
                '- name: Verify accepted CTK3 distribution',
                '- name: Verify Clearrabot contracts',
                '- name: Require the product capability and alias parser authority'
            ) `
            -Contract 'Discord accepted CTK3 consumer'
        foreach ($requiredDiscordConsumerMarker in @(
            'uses: actions/download-artifact@v4',
            'name: ctk3-accepted-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}',
            'path: packages/ctk3/dist',
            'run: node scripts/tools/accepted-ctk3-dist.mjs --verify packages/ctk3/dist --expected-source-commit "$CLEARRA_SOURCE_COMMIT" --expected-run-id "$GITHUB_RUN_ID" --expected-run-attempt "$GITHUB_RUN_ATTEMPT"',
            'run: npm run test:built --workspace @clearra/discord-bot',
            'run: node --test tests/contracts/product_capability_registry.test.mjs'
        )) {
            if ($discordJob.IndexOf($requiredDiscordConsumerMarker, [System.StringComparison]::Ordinal) -lt 0) {
                Add-ArchitectureError "Discord accepted CTK3 consumer is missing '$requiredDiscordConsumerMarker'"
            }
        }
        foreach ($requiredAcceptanceConsumerMarker in @(
            'uses: actions/download-artifact@v4',
            'name: ctk3-accepted-${{ github.sha }}-run-${{ needs.metadata.outputs.accepted_run_id }}-attempt-${{ needs.metadata.outputs.accepted_run_attempt }}',
            'path: packages/ctk3/dist',
            'CLEARRA_ACCEPTED_CTK3_DIST: ${{ github.workspace }}/packages/ctk3/dist',
            'CLEARRA_ACCEPTED_RUN_ID: ${{ github.run_id }}',
            'CLEARRA_ACCEPTED_RUN_ATTEMPT: ${{ github.run_attempt }}',
            'run: powershell -NoProfile -File scripts/clearra.ps1 -Task ReleaseAcceptance -ReleaseAcceptanceShard Rust -ExecutionSurface Trusted'
        )) {
            if ($releaseRustJob.IndexOf($requiredAcceptanceConsumerMarker, [System.StringComparison]::Ordinal) -lt 0) {
                Add-ArchitectureError "Windows Rust acceptance CTK3 consumer is missing '$requiredAcceptanceConsumerMarker'"
            }
        }

        foreach ($requiredShardMarker in @(
            '-ReleaseAcceptanceShard FoundationNoProductDebt -ExecutionSurface Trusted',
            '-ReleaseAcceptanceShard FoundationAdversarialCorrectness -ExecutionSurface Trusted',
            '-ReleaseAcceptanceShard FoundationDesktopHost -ExecutionSurface Trusted',
            '-ReleaseAcceptanceShard Sanitizer -ExecutionSurface Trusted',
            '-ReleaseAcceptanceShard Rust -ExecutionSurface Trusted',
            '-ReleaseAcceptanceShard Pages -ExecutionSurface Trusted',
            'node scripts/release/canonical-acceptance-evidence.mjs shard `',
            '--shards release-shard-evidence \'
        )) {
            $markerCount = [regex]::Matches(
                $release,
                [regex]::Escape($requiredShardMarker)
            ).Count
            $expectedCount = if ($requiredShardMarker -eq 'node scripts/release/canonical-acceptance-evidence.mjs shard `') { 6 } else { 1 }
            if ($markerCount -ne $expectedCount) {
                Add-ArchitectureError "Canonical ReleaseAcceptance shard contract differs for '$requiredShardMarker'"
            }
        }
        if ($canonicalAcceptanceEvidence.IndexOf('canonical six-shard ReleaseAcceptance fan-in', [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError 'Canonical ReleaseAcceptance evidence must bind the six-shard fan-in command'
        }
        $releaseAcceptanceCacheText = @(
            $releaseFoundationNoProductDebtJob,
            $releaseFoundationAdversarialCorrectnessJob,
            $releaseFoundationDesktopHostJob,
            $releaseSanitizerJob,
            $releaseRustJob,
            $releasePagesJob
        ) -join "`n"
        if ([regex]::Matches($releaseAcceptanceCacheText, 'actions/cache/restore@v4').Count -ne 6 -or
            [regex]::Matches($release, 'actions/cache/save@v4').Count -ne 0 -or
            [regex]::Matches($release, '(?m)^      - uses: actions/cache@v4\s*$').Count -ne 2) {
            Add-ArchitectureError 'Canonical ReleaseAcceptance must use six restore-only cache readers without automatic or explicit cache writers'
        }
        foreach ($shardCacheJob in @(
            $releaseFoundationNoProductDebtJob,
            $releaseFoundationAdversarialCorrectnessJob,
            $releaseFoundationDesktopHostJob,
            $releaseSanitizerJob,
            $releaseRustJob,
            $releasePagesJob
        )) {
            foreach ($requiredRestoreMarker in @(
                'actions/cache/restore@v4',
                '~/AppData/Local/Clearra/build',
                'key: release-acceptance-${{ runner.os }}-bindgen-0.2.126-',
                'restore-keys: |'
            )) {
                if ($shardCacheJob.IndexOf($requiredRestoreMarker, [System.StringComparison]::Ordinal) -lt 0) {
                    Add-ArchitectureError "Canonical ReleaseAcceptance isolated restore is missing '$requiredRestoreMarker'"
                }
            }
        }
        if ($release -match 'npm test --workspace @clearra/discord-bot') {
            Add-ArchitectureError 'Release workflow must consume the accepted CTK3 build through the Discord built-only suite'
        }
        if ($metadataJob.IndexOf('apps/clearra-discord-bot/test/capability-registry.test.mjs', [System.StringComparison]::Ordinal) -ge 0) {
            Add-ArchitectureError 'Linux metadata must not duplicate the Discord capability-registry suite'
        }
        if ($metadataJob -match '(?m)^        run: node scripts/tools/validate-release-cli-smokes\.mjs\s*$') {
            Add-ArchitectureError 'Linux metadata must not duplicate the release smoke validator outside its mutation owner'
        }
        foreach ($requiredDiscordPackageMarker in @(
            '"test": "npm run build --workspace ctk3 && npm run test:built"',
            '"test:built": "node --test ./test/*.test.mjs"'
        )) {
            if ($discordPackage.IndexOf($requiredDiscordPackageMarker, [System.StringComparison]::Ordinal) -lt 0) {
                Add-ArchitectureError "Discord package is missing single-owner script '$requiredDiscordPackageMarker'"
            }
        }

        $canonicalPreflightStart = $metadataJob.IndexOf(
            "`n      - name: Require exact main and zero prior canonical success",
            [System.StringComparison]::Ordinal
        )
        $linuxRegressionStart = $metadataJob.IndexOf(
            "`n      - name: Validate independent release regressions with bounded workers",
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
        $windowsArchiveStart = $releaseFoundationJob.IndexOf(
            "`n      - name: Archive the exact accepted source on Windows",
            [System.StringComparison]::Ordinal
        )
        $windowsArchiveEnd = $releaseFoundationJob.IndexOf(
            "`n      - id: release_toolchain_cache",
            [System.StringComparison]::Ordinal
        )
        if ($canonicalPreflightStart -lt 0 -or
            $linuxRegressionStart -le $canonicalPreflightStart -or
            $linuxArchiveStart -le $linuxRegressionStart -or
            $linuxArchiveEnd -le $linuxArchiveStart -or
            $windowsArchiveStart -lt 0 -or
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
            $windowsArchiveStep = $releaseFoundationJob.Substring(
                $windowsArchiveStart,
                $windowsArchiveEnd - $windowsArchiveStart
            )
            $linuxStepsStart = $metadataJob.IndexOf("`n    steps:", [System.StringComparison]::Ordinal)
            $windowsStepsStart = $releaseFoundationJob.IndexOf("`n    steps:", [System.StringComparison]::Ordinal)
            if ($linuxStepsStart -lt 0 -or
                $linuxStepsStart -ge $linuxRegressionStart -or
                $windowsStepsStart -lt 0 -or
                $windowsStepsStart -ge $windowsArchiveStart) {
                Add-ArchitectureError 'Exact source archive protected step preludes are missing'
            }
            else {
                $linuxProtectedPrelude = $metadataJob.Substring(
                    $linuxStepsStart,
                    $linuxArchiveEnd - $linuxStepsStart
                )
                $windowsProtectedPrelude = $releaseFoundationJob.Substring(
                    $windowsStepsStart,
                    $windowsArchiveEnd - $windowsStepsStart
                )
                Assert-ReleaseExactStepSkeleton `
                    -Text $linuxProtectedPrelude `
                    -ExpectedSteps @(
                        '- uses: actions/checkout@v4',
                        '- uses: actions/setup-node@v4',
                        '- name: Require exact main and zero prior canonical success',
                        '- name: Validate independent release regressions with bounded workers',
                        '- name: Archive the exact accepted source on Linux'
                    ) `
                    -Contract 'Linux exact source archive protected prelude'
                Assert-ReleaseExactStepSkeleton `
                    -Text $windowsProtectedPrelude `
                    -ExpectedSteps @(
                        '- uses: actions/checkout@v4',
                        '- uses: actions/setup-node@v4',
                        '- name: Archive the exact accepted source on Windows'
                    ) `
                    -Contract 'Windows exact source archive protected prelude'
                Assert-ReleaseExactText `
                    -Text $metadataJob.Substring(
                        $linuxStepsStart,
                        $canonicalPreflightStart - $linuxStepsStart
                    ) `
                    -Expected "`n    steps:`n      - uses: actions/checkout@v4`n      - uses: actions/setup-node@v4`n        with:`n          node-version: 22" `
                    -Contract 'Linux protected checkout and Node setup'
                Assert-ReleaseExactText `
                    -Text $releaseFoundationJob.Substring(
                        $windowsStepsStart,
                        $windowsArchiveStart - $windowsStepsStart
                    ) `
                    -Expected "`n    steps:`n      - uses: actions/checkout@v4`n      - uses: actions/setup-node@v4`n        with:`n          node-version: 22" `
                    -Contract 'Windows protected checkout and Node setup'
            }
            foreach ($step in @(
                @{ Name = 'Linux archive regression'; Text = $linuxRegressionStep; Shell = 'bash'; Metadata = $true },
                @{ Name = 'Linux accepted source archive'; Text = $linuxArchiveStep; Shell = 'bash'; Metadata = $true },
                @{ Name = 'Windows accepted source archive'; Text = $windowsArchiveStep; Shell = 'pwsh'; Metadata = $false }
            )) {
                Assert-ReleaseYamlExactKeySet `
                    -Text $step.Text `
                    -Indentation 8 `
                    -ExpectedKeys $(if ($step.Metadata) { @('if', 'shell', 'run') } else { @('shell', 'run') }) `
                    -Contract "$($step.Name) step"
                if ($step.Metadata) {
                    Assert-ReleaseYamlExactScalar `
                        -Text $step.Text `
                        -Indentation 8 `
                        -Key 'if' `
                        -ExpectedValue 'github.event_name == ''workflow_dispatch''' `
                        -Contract "$($step.Name) dispatch-only condition"
                }
                Assert-ReleaseYamlExactScalar `
                    -Text $step.Text `
                    -Indentation 8 `
                    -Key 'shell' `
                    -ExpectedValue $step.Shell `
                    -Contract "$($step.Name) shell"
            }
            if ($linuxRegressionStep -notmatch '(?m)^        run: node scripts/tools/run-release-regression-tests\.mjs\s*$' -or
                [regex]::Matches($release, 'scripts/tools/run-release-regression-tests\.mjs').Count -ne 1) {
                Add-ArchitectureError 'Independent release regressions must have one bounded dispatch-only Linux metadata owner'
            }
            foreach ($requiredRegression in @(
                'scripts/release/accepted-pages-build.test.mjs',
                'scripts/release/canonical-acceptance-evidence.test.mjs',
                'scripts/release/canonical-acceptance-run.test.mjs',
                'scripts/release/create-exact-source-archive.test.mjs',
                'scripts/release/deployment-impact.test.mjs',
                'scripts/release/discord-catalog-recovery-authority.test.mjs',
                'scripts/release/discord-deploy-workflow.test.mjs',
                'scripts/release/discord-deployment-recovery.test.mjs',
                'scripts/release/discord-deployment-state.test.mjs',
                'scripts/release/discord-production-checkpoint-receipt.test.mjs',
                'scripts/release/discord-recovery-debt.test.mjs',
                'scripts/release/final-source-attempt-journal.test.mjs',
                'scripts/release/final-source-event-contract.test.mjs',
                'scripts/release/final-source-stage-evidence.test.mjs',
                'scripts/release/finalize-discord-production-checkpoint.test.mjs',
                'scripts/release/observe-production-surfaces.test.mjs',
                'scripts/release/pages-deployment-authority.test.mjs',
                'scripts/release/pages-rollback-authority.test.mjs',
                'scripts/release/pages-rollback-package.test.mjs',
                'scripts/release/release-publication-evidence.test.mjs',
                'scripts/release/validate-final-source-revalidation.test.mjs',
                'scripts/release/validate-release-metadata.test.mjs',
                'scripts/release/verify-remote-annotated-tag.test.mjs',
                'scripts/tools/run-focused-js-tests.test.mjs',
                'scripts/tools/run-release-regression-tests.test.mjs',
                'scripts/tools/validate-release-cli-smokes.test.mjs'
            )) {
                $quotedRegression = '"' + $requiredRegression + '"'
                if ([regex]::Matches(
                        $releaseRegressionRunner,
                        [regex]::Escape($quotedRegression)
                    ).Count -ne 1) {
                    Add-ArchitectureError "Bounded release regression manifest must own '$requiredRegression' exactly once"
                }
            }
            foreach ($requiredRunnerMarker in @(
                'export const ACTIONS_TEST_WORKER_CAP = 4;',
                'availableParallelism()',
                '`--test-concurrency=${workers}`',
                'shell: false',
                'stdio: "inherit"',
                'release regression runner does not accept arguments'
            )) {
                if ($releaseRegressionRunner.IndexOf($requiredRunnerMarker, [System.StringComparison]::Ordinal) -lt 0) {
                    Add-ArchitectureError "Bounded release regression runner is missing '$requiredRunnerMarker'"
                }
            }
            if ($releaseRegressionRunner.IndexOf('audit-upstream-drift.test.mjs', [System.StringComparison]::Ordinal) -ge 0) {
                Add-ArchitectureError 'Upstream drift authority must remain a serial release step outside the independent regression pool'
            }
            foreach ($requiredRunnerTest in @(
                'derives a positive Actions worker budget capped at four logical processors',
                'keeps one closed duplicate-free manifest for every independent release regression',
                'builds one shell-free Node test pool with explicit bounded file concurrency',
                'runs the complete pool exactly once and propagates its failure'
            )) {
                if ($releaseRegressionRunnerTest.IndexOf($requiredRunnerTest, [System.StringComparison]::Ordinal) -lt 0) {
                    Add-ArchitectureError "Bounded release regression runner coverage is missing '$requiredRunnerTest'"
                }
            }
            foreach ($duplicateMetadataBuild in @(
                'Install JavaScript workspace for product authority',
                'Build CTK3 workspace for product authority'
            )) {
                if ($metadataJob.IndexOf($duplicateMetadataBuild, [System.StringComparison]::Ordinal) -ge 0) {
                    Add-ArchitectureError "Linux metadata must not duplicate product build step '$duplicateMetadataBuild'"
                }
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
                $publishHeader -notmatch '(?m)^    needs:\s*\r?\n      \[metadata, release-acceptance, linux-cli, windows-cli, windows-gui, discord-bot\]\s*$' -or
                [regex]::Matches($publishHeader, '(?m)^    if\s*:').Count -ne 1 -or
                $publishHeader -notmatch '(?m)^    if: always\(\) && github\.ref_type == ''tag'' && needs\.metadata\.result == ''success''\s*$') {
                Add-ArchitectureError 'Release publish must depend on every exact acceptance job, tolerate expected skips, and require successful metadata on tags'
            }
        }
    }
    foreach ($required in @(
        'apps/clearra-discord-bot/scripts/restore-oracle-release text eol=lf',
        'scripts/release/create-exact-source-archive.mjs text eol=lf',
        'scripts/release/exact-source-tar-contract.mjs text eol=lf',
        'tests/fixtures/contracts/oracle_candidate_settings_v080.v1.txt text eol=lf'
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
        'node scripts/release/canonical-acceptance-run.mjs',
        [System.StringComparison]::Ordinal
    )
    $canonicalEvidenceIndex = $release.LastIndexOf(
        'node scripts/release/canonical-acceptance-evidence.mjs verify',
        [System.StringComparison]::Ordinal
    )
    $remoteTagIndex = $release.LastIndexOf(
        'node scripts/release/verify-remote-annotated-tag.mjs',
        [System.StringComparison]::Ordinal
    )
    $checkpointTagIndex = $release.LastIndexOf(
        'node scripts/release/finalize-discord-production-checkpoint.mjs verify-tag',
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
    $checkpointReleaseIndex = $release.LastIndexOf(
        'node scripts/release/finalize-discord-production-checkpoint.mjs verify-release',
        [System.StringComparison]::Ordinal
    )
    if ($lateMainIndex -lt 0 -or
        $lateAcceptanceIndex -le $lateMainIndex -or
        $canonicalEvidenceIndex -le $lateAcceptanceIndex -or
        $immutabilityPreconditionIndex -le $canonicalEvidenceIndex -or
        $remoteTagIndex -le $immutabilityPreconditionIndex -or
        $checkpointTagIndex -le $remoteTagIndex -or
        $draftCreateIndex -le $checkpointTagIndex -or
        $publishDraftIndex -le $draftCreateIndex -or
        $immutableCheckIndex -le $publishDraftIndex -or
        $checkpointReleaseIndex -le $immutableCheckIndex) {
        Add-ArchitectureError 'Release publication must rebind the remote annotated tag, build an asset-complete draft, publish it, and verify immutable state in order'
    }
    if ($release -notmatch '(?m)^\s*fi\r?\n\s*node scripts/release/verify-remote-annotated-tag\.mjs \\\s*$') {
        Add-ArchitectureError 'Release publication must run the remote annotated-tag verifier immediately after its late preconditions'
    }
    foreach ($required in @(
        'accepted_sha',
        'node scripts/release/canonical-acceptance-run.mjs',
        '--format github-output >> "$GITHUB_OUTPUT"',
        'pages-deployment-authority.mjs',
        'PAGES_AUTHORITY_REPORT_PATH:',
        'Pages source is no longer the exact current main commit'
    )) {
        if ($pages -notlike "*$required*") {
            Add-ArchitectureError "Pages workflow is missing accepted-source identity gate '$required'"
        }
    }
    if ($pages -match '(?ms)^\s*push:\s*\r?\n\s*branches:\s*\[main\]') {
        Add-ArchitectureError 'Pages must not deploy an unaccepted main push'
    }
    if ($pages.IndexOf('cancel-in-progress: false', [System.StringComparison]::Ordinal) -lt 0) {
        Add-ArchitectureError 'Pages deployment must serialize with rollback capture and restore without cancelling an in-flight authority'
    }
    $pagesRunBlockIndent = $null
    foreach ($line in ($pages -split "`n")) {
        if ($line -match '^(\s*)run:\s*(.*)$') {
            $pagesRunBlockIndent = $Matches[1].Length
            if ($Matches[2] -match '\$\{\{\s*inputs\.') {
                Add-ArchitectureError 'Pages shell steps must receive workflow inputs through environment variables, never direct expression interpolation'
            }
            continue
        }
        if ($null -ne $pagesRunBlockIndent) {
            if ($line.Trim().Length -eq 0) {
                continue
            }
            $lineIndent = $line.Length - $line.TrimStart().Length
            if ($lineIndent -le $pagesRunBlockIndent) {
                $pagesRunBlockIndent = $null
            }
            elseif ($line -match '\$\{\{\s*inputs\.') {
                Add-ArchitectureError 'Pages shell steps must receive workflow inputs through environment variables, never direct expression interpolation'
            }
        }
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
        'accepted_run_id: ${{ steps.accepted_run.outputs.accepted_run_id }}',
        'accepted_run_attempt: ${{ steps.accepted_run.outputs.accepted_run_attempt }}',
        'name: accepted-pages-build-${{ inputs.accepted_sha }}-run-${{ needs.accepted-source.outputs.accepted_run_id }}-attempt-${{ needs.accepted-source.outputs.accepted_run_attempt }}',
        'run-id: ${{ needs.accepted-source.outputs.accepted_run_id }}',
        'node scripts/release/accepted-pages-build.mjs',
        '--accepted-run-attempt "$ACCEPTED_RUN_ATTEMPT"',
        '--base-path "$EXPECTED_BASE_PATH"',
        'pages_artifact_id: ${{ steps.pages-artifact.outputs.artifact_id }}',
        'PAGES_ARTIFACT_ID: ${{ needs.build.outputs.pages_artifact_id }}',
        'run: node authority-source/scripts/release/pages-deployment-authority.mjs',
        'name: clearra-pages-deployment-authority-${{ inputs.accepted_sha }}-run-${{ github.run_id }}-attempt-${{ github.run_attempt }}',
        'retention-days: 90'
    )) {
        if ($pages.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages workflow is missing exact product build identity '$required'"
        }
    }
    foreach ($required in @(
        'CLEARRA_WEB_BASE_PATH: /${{ github.event.repository.name }}',
        'node scripts/release/accepted-pages-build.mjs `',
        '--accepted-run-id $env:GITHUB_RUN_ID `',
        '--accepted-run-attempt $env:GITHUB_RUN_ATTEMPT `',
        'name: accepted-pages-build-${{ github.sha }}',
        'path: apps/clearra-web/build',
        'include-hidden-files: true'
    )) {
        if ($release.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Canonical release acceptance is missing accepted Pages build owner '$required'"
        }
    }
    foreach ($forbiddenPagesRebuildMarker in @(
        'actions/cache@',
        'rustup target add',
        'cargo install',
        'npm ci',
        'npm run build',
        'vite build'
    )) {
        $pagesBuildStart = $pages.IndexOf("`n  build:", [System.StringComparison]::Ordinal)
        $pagesDeployStart = $pages.IndexOf("`n  deploy:", [System.StringComparison]::Ordinal)
        if ($pagesBuildStart -ge 0 -and $pagesDeployStart -gt $pagesBuildStart) {
            $pagesBuildJob = $pages.Substring($pagesBuildStart, $pagesDeployStart - $pagesBuildStart)
            if ($pagesBuildJob.IndexOf($forbiddenPagesRebuildMarker, [System.StringComparison]::Ordinal) -ge 0) {
                Add-ArchitectureError "Pages accepted artifact reuse must not rebuild or install '$forbiddenPagesRebuildMarker'"
            }
        }
    }
    foreach ($required in @(
        'rollback_snapshot_sha:',
        'rollback_capture_run_id:',
        'Resolve sealed rollback capture report before Pages build',
        'PAGES_AUTHORITY_MODE: resolve-forward',
        'Download sealed rollback capture report before Pages build',
        'Verify durable rollback capture before Pages build',
        'CAPTURE_REPORT_PATH: rollback-report-initial/pages-rollback-capture-authority.json',
        'CAPTURE_REPORT_ARTIFACT_ID: ${{ steps.rollback-report.outputs.report_artifact_id }}',
        'Download durable rollback capture before Pages build',
        'Verify exact rollback package before Pages build',
        'Redownload durable rollback capture immediately before deployment',
        'Redownload sealed rollback capture report immediately before deployment',
        'Revalidate exact rollback package immediately before deployment',
        'Revalidate durable rollback capture immediately before deployment',
        'scripts/release/pages-rollback-authority.mjs',
        'scripts/release/pages-rollback-package.mjs',
        'github-token: ${{ github.token }}',
        'run-id: ${{ inputs.rollback_capture_run_id }}'
    )) {
        if ($pages.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages forward workflow is missing durable rollback admission '$required'"
        }
    }
    foreach ($forbiddenManualAuthority in @(
        'rollback_artifact_id:',
        'rollback_artifact_name:',
        'rollback_artifact_digest:',
        'rollback_tar_sha256:'
    )) {
        if ($pages.IndexOf($forbiddenManualAuthority, [System.StringComparison]::Ordinal) -ge 0) {
            Add-ArchitectureError "Pages forward workflow must derive rollback authority from the sealed capture report, not manual input '$forbiddenManualAuthority'"
        }
    }
    if (([regex]::Matches($pages, 'pages-rollback-authority\.mjs')).Count -ne 3 -or
        ([regex]::Matches($pages, 'pages-rollback-package\.mjs')).Count -ne 2) {
        Add-ArchitectureError 'Pages forward workflow must resolve one sealed report and verify rollback metadata and package before build and immediately before deployment'
    }
    foreach ($required in @(
        'name: Preserve or Restore GitHub Pages',
        'mode:',
        '- capture',
        '- restore',
        'snapshot_sha:',
        'expected_current_main:',
        'current_pages_sha:',
        'snapshot_run_id:',
        'restore_authorization:',
        'group: pages',
        'cancel-in-progress: false',
        'Revalidate capture authority immediately before artifact creation',
        'clearra-pages-rollback-${SNAPSHOT_SHA}-authority-${AUTHORITY_SHA}-run-${GITHUB_RUN_ID}-attempt-${GITHUB_RUN_ATTEMPT}',
        'actions/upload-pages-artifact@v3',
        'retention-days: 90',
        'Seal exact rollback capture authority',
        'PAGES_AUTHORITY_MODE: capture-report',
        'CAPTURE_TAR_PATH: ${{ runner.temp }}/artifact.tar',
        'Upload sealed rollback capture authority',
        'Resolve sealed rollback capture report',
        'PAGES_AUTHORITY_MODE: resolve-restore',
        'Download sealed rollback capture report',
        'CAPTURE_REPORT_PATH: rollback-capture-report/pages-rollback-capture-authority.json',
        'actions/download-artifact@v4',
        'github-token: ${{ github.token }}',
        'run-id: ${{ inputs.snapshot_run_id }}',
        'node authority-source/scripts/release/pages-rollback-package.mjs',
        'Upload exact rollback Pages artifact',
        'name: github-pages',
        'compression-level: 0',
        'Revalidate rollback artifact immediately before deployment',
        'actions/deploy-pages@v4',
        'Seal restored Pages authority from API and public readback',
        'pages-deployment-authority.mjs',
        'Upload sealed Pages restore authority'
    )) {
        if ($pagesRollback.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages rollback workflow is missing fail-closed authority '$required'"
        }
    }
    foreach ($forbiddenManualAuthority in @(
        'snapshot_artifact_id:',
        'snapshot_artifact_name:',
        'snapshot_artifact_digest:',
        'snapshot_tar_sha256:'
    )) {
        if ($pagesRollback.IndexOf($forbiddenManualAuthority, [System.StringComparison]::Ordinal) -ge 0) {
            Add-ArchitectureError "Pages rollback workflow must derive capture authority from the sealed report, not manual input '$forbiddenManualAuthority'"
        }
    }
    if ($pagesRollback -match '(?ms)^\s*push:\s*\r?\n\s*branches:') {
        Add-ArchitectureError 'Pages rollback capture or restore must never run automatically on push'
    }
    foreach ($required in @(
        'refs/heads/main',
        '.github/workflows/pages.yml',
        '.github/workflows/pages-rollback.yml',
        'validateCanonicalAcceptanceLookup',
        'resolveCanonicalAcceptanceHistory',
        'expectedCount: 1',
        'branch: "main"',
        '/attempts/${runAttempt}',
        '/compare/${snapshotSha}...${authoritySha}',
        'snapshot SHA must be the authority main SHA or its ancestor',
        'capture run must contain exactly one capture-build job',
        'MINIMUM_RETENTION_MS',
        'forward and restore mutations require a fresh workflow dispatch, not a rerun',
        'clearra-pages-rollback-${snapshot}-authority-${authority}-run-${runId}-attempt-${attempt}',
        'clearra.pages.rollback-capture-authority.v1',
        'resolveCaptureReportArtifact',
        'readRollbackCaptureReport',
        'capture run must contain exactly one sealed report artifact',
        'validatePagesIdentity(identity, manifest, currentPagesSha)',
        'ROLLBACK:${currentPagesSha}:TO:${snapshotSha}',
        '/git/ref/tags/${RELEASE_TAG}',
        '/releases/tags/${RELEASE_TAG}',
        'capture run must complete before the consuming Pages mutation starts'
    )) {
        if ($pagesRollbackAuthority.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages rollback authority verifier is missing fail-closed contract '$required'"
        }
    }
    foreach ($required in @(
        'createHash("sha256")',
        'Pages rollback tar header checksum is invalid',
        'Pages rollback tar contains an unsafe member path',
        'Pages rollback tar contains a link or special entry',
        'Pages rollback tar contains a duplicate member path',
        'Downloaded Pages artifact.tar differs from the captured SHA-256',
        'clearra-build-identity.json',
        'wasm/clearra_wasm.manifest.json',
        'validatePagesIdentity(identity, manifest, sha)'
    )) {
        if ($pagesRollbackPackage.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages rollback package verifier is missing exact-package contract '$required'"
        }
    }
    foreach ($required in @(
        'canonical acceptance requires one exact success and rejects duplicate or wrong authority',
        'canonical acceptance query pins the main branch and a non-truncating exact SHA page',
        'capture authority binds the run attempt, job, artifact, retention, and consumer order',
        'capture names are unique per authority, run, and rerun attempt',
        'capture reruns are unique while forward and restore require a fresh dispatch',
        'capture report seals actual artifact ID, digest, run attempt, tar hash, and retention',
        'capture report rejects short retention and wrong active run attempt',
        'resolves exactly one durable sealed report artifact from the completed capture attempt'
    )) {
        if ($pagesRollbackAuthorityTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages rollback authority regression is missing adversarial case '$required'"
        }
    }
    foreach ($required in @(
        'validates the exact tar hash and both complete identity documents',
        'rejects traversal, links, duplicate identities, and forged identity',
        'rejects corrupted headers and data after the tar end marker'
    )) {
        if ($pagesRollbackPackageTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages rollback package regression is missing adversarial case '$required'"
        }
    }
    $captureValidationIndex = $pagesRollback.IndexOf(
        'Revalidate capture authority immediately before artifact creation',
        [System.StringComparison]::Ordinal
    )
    $captureUploadIndex = $pagesRollback.IndexOf(
        'Upload durable Pages rollback artifact',
        [System.StringComparison]::Ordinal
    )
    $captureReadbackIndex = $pagesRollback.IndexOf(
        'Seal exact rollback capture authority',
        [System.StringComparison]::Ordinal
    )
    $captureReportUploadIndex = $pagesRollback.IndexOf(
        'Upload sealed rollback capture authority',
        [System.StringComparison]::Ordinal
    )
    if ($captureValidationIndex -lt 0 -or
        $captureUploadIndex -le $captureValidationIndex -or
        $captureReadbackIndex -le $captureUploadIndex -or
        $captureReportUploadIndex -le $captureReadbackIndex) {
        Add-ArchitectureError 'Pages rollback capture must revalidate, create the exact Pages tar, seal actual authorities, and upload the sealed report in order'
    }
    $rollbackDownloadIndex = $pagesRollback.IndexOf(
        'Download exact rollback package',
        [System.StringComparison]::Ordinal
    )
    $rollbackPackageValidationIndex = $pagesRollback.IndexOf(
        'Verify downloaded rollback package',
        [System.StringComparison]::Ordinal
    )
    $rollbackUploadIndex = $pagesRollback.IndexOf(
        'Upload exact rollback Pages artifact',
        [System.StringComparison]::Ordinal
    )
    $rollbackLateValidationIndex = $pagesRollback.IndexOf(
        'Revalidate rollback artifact immediately before deployment',
        [System.StringComparison]::Ordinal
    )
    $rollbackDeployIndex = $pagesRollback.IndexOf(
        'actions/deploy-pages@v4',
        [System.StringComparison]::Ordinal
    )
    $rollbackReadbackIndex = $pagesRollback.IndexOf(
        'Seal restored Pages authority from API and public readback',
        [System.StringComparison]::Ordinal
    )
    $rollbackAuthorityUploadIndex = $pagesRollback.IndexOf(
        'Upload sealed Pages restore authority',
        [System.StringComparison]::Ordinal
    )
    if ($rollbackDownloadIndex -lt 0 -or
        $rollbackPackageValidationIndex -le $rollbackDownloadIndex -or
        $rollbackUploadIndex -le $rollbackPackageValidationIndex -or
        $rollbackLateValidationIndex -le $rollbackUploadIndex -or
        $rollbackDeployIndex -le $rollbackLateValidationIndex -or
        $rollbackReadbackIndex -le $rollbackDeployIndex -or
        $rollbackAuthorityUploadIndex -le $rollbackReadbackIndex) {
        Add-ArchitectureError 'Pages rollback must download, validate, re-upload, revalidate, deploy, seal, and upload the exact snapshot authority in order'
    }
    foreach ($required in @(
        'clearra.pages.deployment-authority.v1',
        'validateWorkflowRun',
        '/pages/deployments/${encodeURIComponent(deploymentId)}',
        'artifact_api_readback_sha256',
        'deployment_api_readback_sha256',
        'live_identity_sha256',
        'open(target, "wx", 0o600)'
    )) {
        if ($pagesDeploymentAuthority.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages deployment authority producer is missing fail-closed marker '$required'"
        }
    }
    foreach ($required in @(
        'seals forward artifact, run-attempt, deployment status, and live identity API readbacks',
        'restore authority derives accepted identity and queries the deploy action workflow SHA',
        'rejects artifact digest, run attempt, deployment status, and public identity drift',
        'writes one canonical exclusive report file and rejects tampering'
    )) {
        if ($pagesDeploymentAuthorityTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Pages deployment authority regression is missing '$required'"
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
        '$uiProbePath "--clearra" $builtExePath',
        'CLEARRA_ACCEPTED_CTK3_DIST',
        'CLEARRA_ACCEPTED_RUN_ID',
        'CLEARRA_ACCEPTED_RUN_ATTEMPT',
        'ClearraReleaseAcceptanceMode',
        'scripts/tools/accepted-ctk3-dist.mjs',
        '"--verify"',
        '"--expected-source-commit"',
        '"--expected-run-id"',
        '"--expected-run-attempt"',
        'packages/ctk3/dist',
        'npm build ctk3'
    )) {
        if ($productProcessSurface.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Built product acceptance is missing exact terminal-supply artifact probe '$required'"
        }
    }
    foreach ($required in @(
        'release_built_product_verifies_downloaded_ctk3_without_rebuilding',
        'release_built_product_pins_accepted_ctk3_verifier_arguments',
        'release_built_product_runs_both_terminal_supply_probes_after_verification',
        'release_built_product_fails_closed_on_accepted_ctk3_verification'
    )) {
        if ($productProcessContractTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Built product accepted CTK3 regression coverage is missing '$required'"
        }
    }
    foreach ($required in @(
        'clearra-accepted-ctk3.v2.json',
        'clearra.accepted-ctk3-dist.v2',
        '["contract", "files", "run_attempt", "run_id", "source_commit"]',
        'createHash("sha256")',
        'stats.isSymbolicLink()',
        'distribution does not match its sealed file set and hashes',
        'flag: "wx"',
        '--expected-source-commit',
        '--expected-run-id',
        '--expected-run-attempt'
    )) {
        if ($acceptedCtk3Dist.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Accepted CTK3 artifact verifier is missing fail-closed marker '$required'"
        }
    }
    foreach ($required in @(
        'seals and verifies the exact accepted CTK3 file set',
        'rejects payload mutation and unsealed extra files',
        'rejects source or accepted-run drift and malformed manifest authority',
        'rejects missing runtime surfaces and resealing',
        'rejects non-canonical run authority and stale manifest residue'
    )) {
        if ($acceptedCtk3DistTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Accepted CTK3 artifact regression coverage is missing '$required'"
        }
    }
    foreach ($required in @(
        'clearra.pages.identity.v2',
        'acceptedRunId',
        'acceptedRunAttempt',
        'basePath',
        'createHash("sha256")',
        'closed regular-file set and hashes',
        'symlink or reparse point',
        '404 fallback must exactly match index.html',
        'accepted Pages WASM manifest has a mismatched product identity',
        'flag: "wx"'
    )) {
        if ($acceptedPagesBuild.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Accepted Pages build verifier is missing fail-closed marker '$required'"
        }
    }
    foreach ($required in @(
        'stamps and verifies a closed accepted Pages build',
        'rejects extra, missing, and mutated accepted Pages files',
        'rejects source, run, attempt, base-path, and version drift',
        'rejects fallback, WASM identity, and symlink or reparse drift'
    )) {
        if ($acceptedPagesBuildTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Accepted Pages build regression coverage is missing '$required'"
        }
    }
    if ($wasmReleaseGate.IndexOf('apps/clearra-web/scripts/prepare-pages-fallback.mjs', [System.StringComparison]::Ordinal) -lt 0 -or
        $wasmReleaseGate.IndexOf('clearra-web Pages fallback', [System.StringComparison]::Ordinal) -lt 0) {
        Add-ArchitectureError 'WASM release build must finish the accepted Pages artifact with the exact 404 fallback'
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

    foreach ($required in @(
        'canonicalJson',
        'canonicalSha256',
        'sealCanonicalReport',
        'verifyCanonicalReportHash',
        'forbidden secret material'
    )) {
        if ($canonicalReleaseEvidence.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Canonical release evidence helper is missing '$required'"
        }
    }
    foreach ($required in @(
        'clearra.discord.command-catalog.v1',
        'clearra.discord.command-catalog-snapshot.v1',
        'clearra.discord.command-catalog-sync.v1',
        'clearra.discord.command-catalog-restore.v1',
        'persistPriorSnapshot',
        'synchronizeGlobalCommandRegistrationFromObserved',
        'current_before_sha256: priorSnapshot.catalog_sha256',
        'Discord catalog restore refused because the current digest changed',
        'current_after_sha256: readbackDigest',
        'loadCommandRegistrationCredentials',
        'open(target, "wx", 0o600)'
    )) {
        if ($discordCatalogRelease.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord catalog release producer is missing fail-closed marker '$required'"
        }
    }
    foreach ($required in @(
        'clearra.discord.command-sync-authority.v1',
        'verifyAcceptedCtk3Dist',
        'validateCanonicalAcceptanceEvidence',
        'validateCanonicalDiscordCatalog',
        'accepted_ctk3_manifest_sha256',
        'canonical_acceptance_evidence_sha256',
        'canonical_acceptance_evidence_file_sha256',
        'command_catalog_file_sha256',
        'Discord command sync authority file SHA-256 differs',
        'open(target, "wx", 0o600)'
    )) {
        if ($discordCommandSyncAuthority.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord command sync authority is missing '$required'"
        }
    }
    foreach ($required in @(
        'materializes one canonical authority from accepted CTK3, acceptance, and catalog',
        'rejects any divergence among CTK3, canonical acceptance, and catalog authorities',
        'authority validation is closed, hash-bound, and source/run-bound',
        'authority CLI requires every exact named argument once'
    )) {
        if ($discordCommandSyncAuthorityTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord command sync authority regression is missing '$required'"
        }
    }
    foreach ($required in @(
        '--sync-authority',
        '--sync-authority-file-sha256',
        'accepted_run_id: syncAuthority.accepted_run_id',
        'accepted_ctk3_manifest_sha256',
        'canonical_acceptance_evidence_sha256',
        'command_sync_authority_sha256',
        'command_sync_authority_file_sha256'
    )) {
        if ($discordCatalogRelease.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord catalog sync is missing accepted authority binding '$required'"
        }
    }
    if ($discordCatalogRelease -match '(?i)--(?:token|secret|password|credential)') {
        Add-ArchitectureError 'Discord catalog release producer must accept credentials from the environment only'
    }
    foreach ($required in @(
        'persists the exact prior snapshot before one sync write and seals readback',
        'restore is conditional on the exact current digest and verifies the prior readback',
        'sync and restore reports reject canonical-content tampering',
        'sync rejects missing or mismatched accepted authority before any Discord read',
        'forbidden secret material'
    )) {
        if ($discordCatalogReleaseTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord catalog release mutation regression is missing '$required'"
        }
    }
    foreach ($required in @(
        '$acceptedCtk3ArtifactName = "ctk3-accepted-$sourceCommit-run-$acceptedRunId-attempt-$acceptedRunAttempt"',
        '$canonicalAcceptanceArtifactName = "canonical-acceptance-evidence-$sourceCommit-run-$acceptedRunId-attempt-$acceptedRunAttempt"',
        'node scripts/tools/accepted-ctk3-dist.mjs',
        '--expected-source-commit $sourceCommit',
        '--expected-run-id $acceptedRunId',
        '--expected-run-attempt $acceptedRunAttempt',
        'npm ci --ignore-scripts',
        'node scripts/release/discord-command-sync-authority.mjs',
        '--sync-authority $syncAuthorityPath',
        '--sync-authority-file-sha256 $syncAuthorityFileSha256'
    )) {
        if ($cloudDeploy.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Discord command-sync runbook is missing accepted authority marker '$required'"
        }
    }
    if ($cloudDeploy.IndexOf('npm run build --workspace ctk3', [System.StringComparison]::Ordinal) -ge 0) {
        Add-ArchitectureError 'Discord command-sync runbook must not rebuild the accepted CTK3 distribution'
    }
    foreach ($required in @(
        'clearra.production-observation.v1',
        'clearra.production-surface-probe.v1',
        'PRODUCTION_OBSERVATION_SECONDS = 1200',
        '"cloud"',
        '"discord"',
        '"oracle"',
        '"pages"',
        'clearra.oracle.candidate-observation.v1',
        'gatewayStartMonotonicUsec',
        'bootId',
        'freshOperationAt',
        'durationSeconds = PRODUCTION_OBSERVATION_SECONDS',
        'production identity changed during observation',
        'Oracle fresh operation did not occur after the prior read-only observation',
        'Oracle read-only observation time did not increase',
        'Oracle freshness verified-after authority differs from its identity',
        'production observation interval differs from its probe spec',
        'initial observation does not open the claimed window',
        'Oracle fresh operation did not occur inside the observation window',
        'probe adapter SHA-256 changed',
        'shell: false',
        'production surface probe output is not canonical JSON',
        'actual four-surface production observation producer report is required'
    )) {
        $surfaceText = if ($required -eq 'actual four-surface production observation producer report is required') {
            $finalSourceValidator
        } else {
            $productionObservation
        }
        if ($surfaceText.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Production observation evidence is missing '$required'"
        }
    }
    foreach ($required in @(
        'observes Discord, Oracle, Cloud, and Pages through a short injected clock',
        'fails closed when a surface identity changes during the window',
        'rejects stale Oracle operation evidence immediately and report hash tampering',
        'allows the verified candidate operation as sample zero before the claimed window',
        'rejects Oracle freshness before verified-after authority',
        'requires every later Oracle operation to follow the prior remote observation',
        'requires the later Oracle operation to occur strictly inside the window',
        'requires Oracle remote observation time to increase strictly',
        'final report validation rechecks the live Oracle cross-sample contract',
        'rejects Oracle freshness whose verified-after value differs from identity',
        'rejects a re-sealed report that pads the window before sample zero',
        'production validation requires the exact 1200-second two-sample contract',
        'probe spec requires four hash-bound adapters and forbids secret fields'
    )) {
        if ($productionObservationTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Production observation mutation regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'validateFinalSourceRevalidationFromStages',
        'validateDiscordCatalogSyncReport',
        'validateProductionObservationReport',
        'Discord deployment differs from the actual catalog sync producer report',
        'observed Oracle identity differs from the Discord deployment',
        'observed Cloud identity differs from the Discord deployment',
        'observed Pages identity differs from the Pages deployment',
        'final-source revalidation is library-only',
        'use final-source-attempt-journal.mjs materialize with every original producer authority'
    )) {
        if ($finalSourceValidator.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Final-source validator is missing producer binding '$required'"
        }
    }
    foreach ($required in @(
        'direct execution fails closed because journal materialization is the sole production entrypoint',
        'requires actual catalog and observation producer reports and exact bindings',
        'actual Discord command catalog sync producer report is required',
        'differs from the actual catalog sync producer report',
        'observed Pages identity differs',
        'library validation requires the exact acceptance deployment and publication stages'
    )) {
        if ($finalSourceValidatorTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Final-source producer-binding regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'append-stage',
        '--stage-evidence',
        '--stage-evidence-file-sha256',
        '--acceptance-stage-evidence',
        '--deployment-stage-evidence',
        '--publication-stage-evidence',
        '--canonical-acceptance-evidence-file-sha256',
        '--pages-deployment-authority-file-sha256',
        '--discord-command-sync-authority-file-sha256',
        '--discord-catalog-sync-report-file-sha256',
        '--oracle-rollback-capture-file-sha256',
        '--production-observation-report-file-sha256',
        '--release-publication-final-authority-file-sha256',
        'replaceJournalAtomically',
        'bytes are not canonical producer JSON'
    )) {
        if ($finalSourceJournal.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Final-source journal materializer is missing producer input '$required'"
        }
    }
    foreach ($required in @(
        'atomically appends three producer stages and materializes the exact final source',
        'rejects out-of-order, duplicate, and wrong-raw-SHA stage append',
        'a failed atomic replacement leaves the journal byte-for-byte unchanged',
        'materialization rejects incomplete or substituted stage authorities',
        'materialization rejects a substituted reopened producer behind unchanged stage JSON',
        'CLI exposes only stage-batch append and closed materialization inputs'
    )) {
        if ($finalSourceJournalTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Final-source journal regression is missing producer input '$required'"
        }
    }
    foreach ($required in @(
        'discord-command-catalog-release.mjs',
        'observe-production-surfaces.mjs',
        '1,200',
        'current digest'
    )) {
        if ($cloudDeploy.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0 -or
            $remainingWorkPlan.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Release runbook and plan must both require producer evidence '$required'"
        }
    }
    foreach ($required in @(
        'FINAL_SOURCE_STAGE_ORDER',
        'FINAL_SOURCE_STAGE_CARDINALITY',
        'validateFinalSourceEventPayload',
        'clearra.final-source-event-evidence.v1'
    )) {
        if ($finalSourceEventContract.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Final-source event contract is missing closed marker '$required'"
        }
    }
    foreach ($required in @(
        'every final-source kind is accepted only through its closed source-bound payload',
        'event evidence rejects extra fields, source drift, and unapproved producer identity',
        'event payloads fail closed on secrets, prior authority, and kind/source mutation'
    )) {
        if ($finalSourceEventContractTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Final-source event mutation regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'clearra.final-source-stage-evidence.v1',
        'STAGE_PRODUCER_ROLES',
        'createAcceptanceStageEvidence',
        'createDeploymentStageEvidence',
        'createPublicationStageEvidence',
        'selectCanonicalDriftEvidencePaths',
        'discord-command-sync-authority',
        'oracle-rollback-capture',
        'pages-deployment',
        'production observation adapters differ from its exact probe spec',
        'release-publication-receipt'
    )) {
        if ($finalSourceStageEvidence.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Final-source stage producer is missing '$required'"
        }
    }
    foreach ($required in @(
        'deployment stage projects only fieldwise-validated actual producer authorities',
        'deployment stage rejects Pages, Discord, Cloud, and Oracle cross-producer drift',
        'deployment stage binds the exact 1200-second interval and adapter set to its probe spec',
        'Oracle capture and observation adapters are closed, source-bound, and secret-free',
        'release-freeze selection uses the final registry evidence entry without retry hardcoding',
        'acceptance stage hashes LF Git blobs rather than core.autocrlf worktree bytes'
    )) {
        if ($finalSourceStageEvidenceTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Final-source stage regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'clearra.release-publication-receipt.v1',
        'clearra.release-publication-evidence.v1',
        'clearra.release-publication-final-authority.v1',
        'expectedReleasePublicationReceiptArtifactName',
        'expectedReleasePublicationEvidenceArtifactName',
        'resolveReleasePublicationFinalAuthority',
        'createGithubCliPublicationDependencies',
        'downloaded publication receipt ZIP differs from the artifact API digest',
        'publication evidence workflow run did not complete successfully'
    )) {
        if ($releasePublicationEvidence.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Release publication authority is missing '$required'"
        }
    }
    foreach ($required in @(
        'local resolver uses closed gh api argv without reading a token environment variable',
        'failed tag attempts recover only an exact accepted partial draft before publication',
        'finalizer rerun creates evidence only when every prior attempt is non-success',
        'global resolver admits exactly one completed successful finalizer artifact'
    )) {
        if ($releasePublicationEvidenceTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Release publication authority regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'workflow_run:',
        'workflows: ["Publish Product Release"]',
        "github.event.workflow_run.conclusion == 'success'",
        "github.event.workflow_run.event == 'push'",
        "github.event.workflow_run.head_branch == 'v0.8.0'",
        'release-publication-evidence.mjs finalize',
        '--finalizer-workflow-run-id "$GITHUB_RUN_ID"',
        '--finalizer-workflow-run-attempt "$GITHUB_RUN_ATTEMPT"',
        "if: steps.finalization.outputs.upload_required == 'true'",
        'retention-days: 90'
    )) {
        if ($releasePublicationFinalizer.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Release publication finalizer is missing '$required'"
        }
    }
    foreach ($required in @(
        'final-source-stage-evidence.mjs acceptance',
        'final-source-stage-evidence.mjs deployment',
        'final-source-stage-evidence.mjs publication',
        'final-source-attempt-journal.mjs initialize',
        'final-source-attempt-journal.mjs append-stage',
        'release-publication-evidence.mjs resolve',
        '--source-root $sourceRoot',
        '--acceptance-stage-evidence-file-sha256 $acceptanceStageEvidenceFileSha256',
        '--deployment-stage-evidence-file-sha256 $deploymentStageEvidenceFileSha256',
        '--publication-stage-evidence-file-sha256 $publicationStageEvidenceFileSha256',
        '--canonical-acceptance-evidence-file-sha256 $canonicalAcceptanceEvidenceFileSha256',
        '--pages-deployment-authority-file-sha256 $pagesDeploymentAuthorityFileSha256',
        '--pages-rollback-capture-file-sha256 $pagesRollbackCaptureFileSha256',
        '--discord-catalog-file-sha256 $catalogFileSha256',
        '--discord-prior-snapshot-file-sha256 $priorCatalogFileSha256',
        '--discord-command-sync-authority-file-sha256 $syncAuthorityFileSha256',
        '--discord-catalog-sync-report-file-sha256 $syncReportFileSha256',
        '--cloud-candidate-smoke-report-file-sha256 $candidateSmokeReportFileSha256',
        '--oracle-rollback-capture-file-sha256 $oracleRollbackCaptureEvidenceFileSha256',
        '--oracle-observation-file-sha256 $oracleObservationEvidenceFileSha256',
        '--production-probe-spec-file-sha256 $probeSpecFileSha256',
        '--production-observation-report-file-sha256 $observationReportFileSha256',
        '--release-publication-evidence-file-sha256 $releasePublicationEvidenceFileSha256',
        '--release-publication-final-authority-file-sha256 $releasePublicationFinalAuthorityFileSha256',
        '--release-publication-receipt-file-sha256 $releasePublicationReceiptFileSha256'
    )) {
        if ($cloudDeploy.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Cloud release runbook is missing closed final-source marker '$required'"
        }
    }
    if ($cloudDeploy.IndexOf('node scripts/release/validate-final-source-revalidation.mjs', [System.StringComparison]::Ordinal) -ge 0) {
        Add-ArchitectureError 'Cloud release runbook must not invoke the library-only final-source validator directly'
    }
    foreach ($required in @(
        '$oracleRollbackCaptureEvidencePath',
        '$oracleObservationEvidencePath',
        '-EvidenceOutput $oracleRollbackCaptureEvidencePath',
        '-EvidenceOutput $oracleObservationEvidencePath',
        'durable Oracle observation failed'
    )) {
        if ($cloudDeploy.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Cloud release runbook is missing durable Oracle evidence marker '$required'"
        }
    }
    if (([regex]::Matches($cloudDeploy, '-EvidenceOutput \$oracle(?:RollbackCapture|Observation)EvidencePath')).Count -ne 2) {
        Add-ArchitectureError 'Cloud release runbook must persist exactly one rollback capture and one direct Oracle observation evidence file'
    }
    $evidenceInitializationIndex = $cloudDeploy.IndexOf(
        '## Initialize exact-source evidence before mutation',
        [System.StringComparison]::Ordinal
    )
    $acceptanceStageIndex = $cloudDeploy.IndexOf(
        'final-source-stage-evidence.mjs acceptance',
        [System.StringComparison]::Ordinal
    )
    $acceptanceAppendIndex = $cloudDeploy.IndexOf(
        '--stage-evidence $acceptanceStageEvidencePath',
        [System.StringComparison]::Ordinal
    )
    $currentSourceBuildIndex = $cloudDeploy.IndexOf(
        '## Build the current-source image',
        [System.StringComparison]::Ordinal
    )
    $deploymentTemplateIndex = $cloudDeploy.IndexOf(
        '## Deployment template',
        [System.StringComparison]::Ordinal
    )
    if ($evidenceInitializationIndex -lt 0 -or
        $acceptanceStageIndex -le $evidenceInitializationIndex -or
        $acceptanceAppendIndex -le $acceptanceStageIndex -or
        $currentSourceBuildIndex -le $acceptanceAppendIndex -or
        $deploymentTemplateIndex -le $currentSourceBuildIndex -or
        ([regex]::Matches($cloudDeploy, 'final-source-stage-evidence\.mjs acceptance')).Count -ne 1 -or
        ([regex]::Matches($cloudDeploy, 'final-source-attempt-journal\.mjs initialize')).Count -ne 1 -or
        ([regex]::Matches($cloudDeploy, '--name \$canonicalAcceptanceArtifactName')).Count -ne 1) {
        Add-ArchitectureError 'Cloud release runbook must initialize and append the exact acceptance stage once before build and public deployment mutation'
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
    if ($cloudBuildSubmissionCount -lt 2 -or
        $archiveSourceArgumentCount -ne $cloudBuildSubmissionCount -or
        $exactArchiveCount -lt $cloudBuildSubmissionCount) {
        Add-ArchitectureError 'Every remaining documented Cloud Build submission must submit the verified exact .tar.gz directly'
    }
    if ($cloudDeploy -match '(?m)^\s*git(?:\s+-c\s+core\.autocrlf=false)?\s+archive\b') {
        Add-ArchitectureError 'Cloud deployment documentation must use the tested exact source archive helper, not a raw Git archive command'
    }
    if ($cloudDeploy -match '(?m)^\s*\.\s*$' -or
        $cloudDeploy -match '(?m)^\s*gcloud\s+builds\s+submit\b[^\r\n]*\s\.\s*$') {
        Add-ArchitectureError 'Cloud deployment documentation must never submit the mutable current directory as a build context'
    }
    foreach ($required in @(
        'discord-command-catalog-release.mjs',
        'fresh extraction of',
        'independent prior GET before',
        'conditional restore is authorized only when',
        'node scripts/release/create-exact-source-archive.mjs',
        '--source-commit $sourceCommit',
        '--output $archivePath',
        'source.tar.gz',
        'tar -xzf $archivePath -C $configContext',
        'Join-Path $configContext "apps/clearra-discord-bot/cloudbuild-current-job-service.yaml"',
        '$configContext'
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
            '$jobBearerSecretVersion = "<numeric-enabled-Secret-version>"',
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
            'scripts/release/cloud/candidate-release-v080.mjs deploy',
            '--job-bearer-secret-version $jobBearerSecretVersion',
            '$candidate.imageDigest -cnotmatch',
            '$candidateUrl/health',
            'scripts/release/cloud/candidate-release-v080.mjs smoke',
            '--image-digest $candidateImage',
            '--candidate-url $candidateUrl',
            'No bearer is',
            '--to-revisions="$candidateRevision=100"',
            '--to-revisions="$priorRevision=100"',
            'canonical zero-traffic candidate authority is incomplete',
            'contractSchemaVersion',
            'supplySemanticsId',
            'artifactSchemaVersion',
            'CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID=clearra.supply.projected-terminal-lookahead.v1',
            'CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION=clearra.solution-data.v1',
            '$oracleRemoteWrapper',
            'scripts/release/oracle/invoke-release-deploy-v080.ps1',
            '$oracleIdentityFile',
            '-IdentityFile $oracleIdentityFile',
            '-Operation capture-rollback-authority',
            'capture-oracle-rollback-authority.mjs',
            'independently computes the exact',
            'root:root mode 0755 regular non-symlink files',
            'mode-0666 launcher is stale authority',
            'scripts/release/oracle/candidate-settings-v080.mjs',
            '--hash-only',
            '-ScriptReleaseSha256 $oracleCandidateReleaseSha256',
            '-Operation verify-candidate',
            'produce-oracle-deployment-proof.mjs candidate',
            'verify-oracle-candidate-proof.mjs',
            '-OracleReleaseSha256 $oracleCandidateReleaseSha256',
            '/run/clearra-deploy/clearra-oracle-candidate-$deploymentNonce.json',
            '$priorCapture.priorOracleReleaseId -cnotmatch',
            '$priorCapture.priorOracleRelease -cne "/opt/clearra/releases/$($priorCapture.priorOracleReleaseId)"',
            'if ($candidateOracleExit -ne 0)',
            '-Operation restore-prior-and-verify',
            'candidate Oracle verification failed; exact prior Oracle authority was restored and Cloud traffic was not changed',
            'restore-oracle-release',
            'produce-oracle-deployment-proof.mjs rollback',
            'verify-oracle-rollback-proof.mjs',
            '-PriorRuntimeAuthorityKind $priorRuntimeAuthorityKind',
            '-PriorRuntimeAuthoritySha256 $priorRuntimeAuthoritySha256',
            'clearra.rollback.legacy-health-no-runtime.v1',
            'missing identity never falls back',
            'prior Cloud revision changed during rollback authority capture',
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
            'scripts/release/cloud/candidate-release-v080.mjs deploy',
            [System.StringComparison]::Ordinal
        )
        $noTrafficIndex = $deploymentDoc.Text.IndexOf(
            'canonical zero-traffic candidate authority is incomplete',
            [System.StringComparison]::Ordinal
        )
        $healthIndex = $deploymentDoc.Text.IndexOf(
            '$candidateUrl/health',
            [System.StringComparison]::Ordinal
        )
        $smokeIndex = $deploymentDoc.Text.IndexOf(
            'scripts/release/cloud/candidate-release-v080.mjs smoke',
            [System.StringComparison]::Ordinal
        )
        $cutoverIndex = $deploymentDoc.Text.IndexOf(
            '--to-revisions="$candidateRevision=100"',
            [System.StringComparison]::Ordinal
        )
        $captureIndex = $deploymentDoc.Text.IndexOf(
            '-Operation capture-rollback-authority',
            [System.StringComparison]::Ordinal
        )
        $scriptReleaseDigestMarker = '-ScriptReleaseSha256 $oracleCandidateReleaseSha256'
        $scriptReleaseDigestBindingCount = [regex]::Matches(
            $deploymentDoc.Text,
            [regex]::Escape($scriptReleaseDigestMarker)
        ).Count
        $expectedScriptReleaseDigestBindingCount = if ($deploymentDoc.Name -eq 'Cloud Run deployment contract') {
            5
        } else {
            4
        }
        $priorRevisionCaptureIndex = $deploymentDoc.Text.IndexOf(
            '$priorRevision = [string]$priorTraffic[0].revisionName',
            [System.StringComparison]::Ordinal
        )
        $oracleCandidateProofIndex = $deploymentDoc.Text.IndexOf(
            '-Operation verify-candidate',
            [System.StringComparison]::Ordinal
        )
        $candidateFailureIndex = $deploymentDoc.Text.IndexOf(
            'if ($candidateOracleExit -ne 0)',
            [System.StringComparison]::Ordinal
        )
        $preCutoverRestoreIndex = $deploymentDoc.Text.IndexOf(
            '-Operation restore-prior-and-verify',
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
            '-Operation restore-prior-and-verify',
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
            $scriptReleaseDigestBindingCount -ne $expectedScriptReleaseDigestBindingCount -or
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
        '-Operation restore-prior-and-verify',
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
    foreach ($deploymentDoc in @(
        @{ Name = 'apps/clearra-discord-bot/CLOUD_RUN_JOB_SERVICE.md'; Text = $cloudDeploy },
        @{ Name = 'apps/clearra-discord-bot/README.md'; Text = $cloudReadme }
    )) {
        if ($deploymentDoc.Text.IndexOf(
                'settings\.pre-v0\.7\.5-',
                [System.StringComparison]::Ordinal
            ) -ge 0) {
            Add-ArchitectureError "$($deploymentDoc.Name) must use the v0.8.0 Oracle settings backup namespace"
        }
        foreach ($forbiddenCloudPath in @(
            'gcloud run deploy $serviceName',
            'verify-cloud-run-candidate.mjs',
            'CLEARRA_CANDIDATE_JOB_TOKEN',
            '${jobBearerSecret}:latest'
        )) {
            if ($deploymentDoc.Text.IndexOf($forbiddenCloudPath, [System.StringComparison]::Ordinal) -ge 0) {
                Add-ArchitectureError "$($deploymentDoc.Name) retains stale local Cloud candidate path '$forbiddenCloudPath'"
            }
        }
    }
    foreach ($required in @(
        'settings.pre-v0.8.0-$deployment_nonce',
        'v0.8.0-$commit_prefix',
        'clearra-current-job-v080-$commit_prefix'
    )) {
        if ($oracleDeployLauncher.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle release launcher is missing v0.8.0 identity marker '$required'"
        }
    }
    foreach ($forbidden in @(
        'settings.pre-v0.7.5-$deployment_nonce',
        'v0.7.5-$commit_prefix',
        'clearra-current-job-v075-$commit_prefix'
    )) {
        if ($oracleDeployLauncher.IndexOf($forbidden, [System.StringComparison]::Ordinal) -ge 0) {
            Add-ArchitectureError "Oracle release launcher retains stale v0.7.5 identity marker '$forbidden'"
        }
    }
    foreach ($required in @(
        'renderCandidateSettingsV080',
        'lines.length !== 13',
        'Buffer.from(`${lines.join("\n")}\n`, "utf8")',
        'createHash("sha256").update(bytes).digest("hex")',
        '--hash-only',
        'process.stdout.write(`${authority.sha256}\n`)'
    )) {
        if ($oracleCandidateSettings.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Canonical Oracle candidate settings renderer is missing '$required'"
        }
    }
    foreach ($required in @(
        'renders the canonical 13-line Oracle candidate settings fixture',
        'remote launcher emits bytes identical to the canonical fixture',
        'hash-only CLI prints only the canonical SHA-256',
        'rejects non-canonical URLs and source commits',
        'oracle_candidate_settings_v080.v1.txt',
        'a14111258028ad8d0ec3449720bc803895f346e3a92a5e2d30e9861ff1c5c61e'
    )) {
        if ($oracleCandidateSettingsTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Canonical Oracle candidate settings regression is missing '$required'"
        }
    }
    foreach ($required in @(
        '# candidate-settings-v080: begin',
        'CLEARRA_CANDIDATE_SETTINGS_V080',
        '# candidate-settings-v080: end'
    )) {
        if ($oracleDeployLauncher.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle release launcher is missing canonical candidate-settings marker '$required'"
        }
    }
    foreach ($required in @(
        '<accepted-ctk3-dist-directory>',
        'repo-local packages/ctk3/dist is not accepted artifact authority',
        'cp -a -- "$accepted_ctk3_root" "$temporary_root/packages/ctk3/dist"',
        '--expected-source-commit "$source_commit"',
        '--expected-run-id "$accepted_run_id"',
        '--expected-run-attempt "$accepted_run_attempt"',
        'publish_archive "$dist_archive" --directory="$temporary_root" packages/ctk3/dist',
        'oracle_ctk3_authority=accepted source_commit=%s run_id=%s run_attempt=%s'
    )) {
        if ($oracleAcceptedLayerBuilder.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle local layer builder is missing accepted CTK3 authority marker '$required'"
        }
    }
    if ($oracleAcceptedLayerBuilder.IndexOf(
            'dist_root="$repository_root/packages/ctk3/dist"',
            [System.StringComparison]::Ordinal
        ) -ge 0) {
        Add-ArchitectureError 'Oracle local layer builder must not consume repo-local CTK3 dist as accepted authority'
    }
    foreach ($required in @(
        'v0.8 Oracle local layer builder freezes only the closed runtime set',
        'repo-local-poison',
        'oracle_ctk3_authority=accepted source_commit='
    )) {
        if ($oracleFreezeTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle accepted CTK3 layer regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'accepted-ctk3-dist.mjs',
        'node_modules/tetris-fumen',
        'ctk3-dist.tar',
        'node_modules.tar',
        'oracle_actions_ctk3_authority=accepted source_commit=%s run_id=%s run_attempt=%s',
        '--format=posix',
        '--sort=name',
        '--mtime=@0',
        'ln -- "$temporary_archive" "$destination"'
    )) {
        if ($oracleActionsLayerBuilder.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle Actions layer builder is missing '$required'"
        }
    }
    foreach ($forbidden in @('src/admin', 'private-overlay')) {
        if ($oracleActionsLayerBuilder.IndexOf($forbidden, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
            Add-ArchitectureError "Oracle Actions layer builder must not probe or consume private overlay marker '$forbidden'"
        }
    }
    foreach ($required in @(
        'Actions layer freeze consumes accepted CTK3 and only the production dependency',
        'Actions layer freeze is deterministic and refuses overwrite',
        'assert.doesNotMatch(script, /src\/admin/u)',
        'assert.doesNotMatch(script, /private-overlay/u)'
    )) {
        if ($oracleActionsLayerBuilderTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle Actions layer regression is missing '$required'"
        }
    }
    foreach ($remoteInvoker in @(
        @{ Name = 'freeze'; Text = $oracleFreezeInvoker },
        @{ Name = 'inactive stage'; Text = $oracleInactiveStageInvoker }
    )) {
        foreach ($required in @(
            "DefaultParameterSetName = 'LocalOverlay'",
            "ParameterSetName = 'RemoteOverlay'",
            '/opt/clearra/sealed-release-inputs/private-overlay-no-config-$Sha256.tar',
            'Assert-CanonicalRemoteOverlayAuthority',
            '$remoteOverlayMode = $PSCmdlet.ParameterSetName -ceq',
            "'--remote-overlay-archive', `$RemoteOverlayArchive",
            "'--remote-overlay-sha256', `$RemoteOverlaySha256",
            "return 'NUL'",
            "return '/dev/null'"
        )) {
            if ($remoteInvoker.Text.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
                Add-ArchitectureError "Oracle $($remoteInvoker.Name) remote-overlay invoker is missing '$required'"
            }
        }
        if ($remoteInvoker.Text -match '(?im)(Get-Content|Get-FileHash|ReadAllBytes|ReadAllText)[^\r\n]*\$RemoteOverlayArchive') {
            Add-ArchitectureError "Oracle $($remoteInvoker.Name) invoker must not read remote private-overlay bytes locally"
        }
        foreach ($forbidden in @(
            'Copy-RemoteSealedOverlay',
            'install -o 1001',
            "'private-overlay-no-config.tar', `$RemoteOverlayArchive"
        )) {
            if ($remoteInvoker.Text.IndexOf($forbidden, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
                Add-ArchitectureError "Oracle $($remoteInvoker.Name) invoker exposes the remote sealed overlay through '$forbidden'"
            }
        }
    }
    foreach ($rootHelper in @(
        @{ Name = 'freeze'; Text = $oracleFreezeHelper },
        @{ Name = 'inactive stage'; Text = $oracleInactiveStageTemplate }
    )) {
        foreach ($required in @(
            'copy_remote_overlay() {',
            'os.O_NOFOLLOW',
            'os.O_EXCL',
            'os.fstat(source_fd)',
            'metadata.st_nlink != 1',
            'stat.S_IMODE(metadata.st_mode) != 0o600',
            'os.fsync(destination_fd)',
            'os.open(destination, os.O_RDONLY',
            'digest.hexdigest() != expected',
            'os.unlink(destination)'
        )) {
            if ($rootHelper.Text.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
                Add-ArchitectureError "Oracle $($rootHelper.Name) root helper sealed-overlay copy is missing '$required'"
            }
        }
    }
    foreach ($required in @(
        'source symlink',
        'source hardlink',
        'wrong source mode',
        'wrong source hash',
        'canonical filename drift',
        'writable parent',
        'symlink parent',
        'occupied O_EXCL destination',
        'source path swap was not bound to the opened fd',
        'post-fsync destination drift',
        'post-copy failure cleanup left runtime residue'
    )) {
        if ($oracleRemoteOverlayCopyTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle remote-overlay dynamic regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'cleanup_runtime_root',
        '/opt/clearra/.v080-stage-@COMMIT_PREFIX@-$stage_nonce',
        '/opt/clearra/.v080-input-@COMMIT_PREFIX@-$stage_nonce',
        '/home/ubuntu/.clearra-v080-upload-@COMMIT_PREFIX@-$stage_nonce'
    )) {
        if ($oracleInactiveStageTemplate.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle inactive-stage cleanup is missing '$required'"
        }
    }
    if ($oracleInactiveStageInvoker.IndexOf(
            '$manifest.layers.overlay.sha256 -cne $RemoteOverlaySha256',
            [System.StringComparison]::Ordinal
        ) -lt 0) {
        Add-ArchitectureError 'Oracle inactive-stage remote overlay must bind the sealed digest to the frozen manifest'
    }
    foreach ($remoteInvokerTest in @(
        @{ Name = 'freeze'; Text = $oracleFreezeInvokerTest },
        @{ Name = 'inactive stage'; Text = $oracleInactiveStageInvokerTest }
    )) {
        foreach ($required in @(
            'Remote-overlay AuditOnly did not remain a typed local-only audit.',
            'accepted local and remote overlay inputs together.',
            '/opt/clearra/sealed-release-inputs/private-overlay-no-config-',
            "'/dev/null'",
            "'NUL'"
        )) {
            if ($remoteInvokerTest.Text.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
                Add-ArchitectureError "Oracle $($remoteInvokerTest.Name) remote-overlay regression is missing '$required'"
            }
        }
    }
    foreach ($required in @(
        "[ValidateSet('capture-prestage-authority', 'cleanup-prestage-backup', 'capture-rollback-authority', 'verify-candidate', 'observe-candidate', 'classify-current-authority', 'restore-prior-and-verify')]",
        'SHA256:mdw7bdzZOBrd6sCebPmMVuTaps+ct2OaOle/gaZMBKU',
        '2f7f658642c2dec4f9ad9e34d959b0215bdcf877e5636daebb003888434a8fd0',
        '157.151.254.175',
        'CLEARRA_ORACLE_IDENTITY_FILE',
        'Test-Path -LiteralPath $IdentityFile -PathType Leaf',
        'Get-Item -LiteralPath $IdentityFile -Force',
        "'-i', `$IdentityFile",
        'oracle_release_deploy_invoker=audit-ok',
        'Test-JsonSafePositiveInteger',
        'clearra.oracle.candidate-observation.v1',
        'EvidenceOutput',
        'Assert-EvidenceOutputPath',
        'ConvertTo-CanonicalJson',
        'Write-CanonicalEvidenceOutput',
        '[IO.FileMode]::CreateNew',
        '[IO.FileShare]::None',
        '[Text.UTF8Encoding]::new($false)',
        '$stream.Flush($true)',
        '$observation.freshOperationAt = Get-CanonicalTimestamp',
        '$observation.observedAt = Get-CanonicalTimestamp',
        'Oracle candidate observation timestamps are out of order.'
    )) {
        if ($oracleDeployInvoker.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Typed Oracle release deploy invoker is missing '$required'"
        }
    }
    if ($oracleDeployInvoker -match '(?im)(Get-Content|Get-FileHash|ReadAllBytes|ReadAllText)[^\r\n]*\$IdentityFile') {
        Add-ArchitectureError 'Typed Oracle release deploy invoker must never read or hash identity-file contents'
    }
    foreach ($required in @(
        "Assert-AuditResult -Output `$observation -Operation 'observe-candidate'",
        'Test-Path -LiteralPath `$IdentityFile -PathType Leaf',
        'Get-Item -LiteralPath `$IdentityFile -Force',
        'Read-CanonicalEvidenceFile',
        'locked-identity',
        'Oracle observation evidence did not preserve canonical UTC timestamps and source identity.',
        'Oracle observation accepted a mismatched verified-after echo.',
        'an operation before verified-after',
        'an observation before its operation',
        'Oracle evidence output changed after a rejected overwrite.',
        'Oracle evidence output accepted a linked parent path.',
        'oracle_release_deploy_wrapper_test=pass'
    )) {
        if ($oracleDeployInvokerTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Typed Oracle release deploy invoker regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'clearra.oracle.candidate-observation.v1',
        'inspectActiveOracle',
        'requireExactObjectKeys',
        'positiveSafeInteger',
        'Number.isSafeInteger',
        'ExecMainStartTimestampMonotonic',
        '/proc/sys/kernel/random/boot_id',
        'readyRecordObserved',
        'verifiedAfter',
        'freshOperationAt',
        'observedAt',
        'runtimeIdentity',
        'shell: false'
    )) {
        if ($oracleObservation.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Read-only Oracle candidate observer is missing '$required'"
        }
    }
    if ($oracleProofProducer.IndexOf('export function inspectActiveOracle', [System.StringComparison]::Ordinal) -lt 0) {
        Add-ArchitectureError 'Trusted Oracle proof producer must export its read-only active observation boundary'
    }
    foreach ($required in @(
        'observe-candidate)',
        'require_root_regular_readonly',
        'apps/clearra-discord-bot/scripts/observe-oracle-candidate.mjs',
        'exec "$node_path" "$observer_script"'
    )) {
        if ($oracleDeployLauncher.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle release launcher is missing read-only observation marker '$required'"
        }
    }
    foreach ($required in @(
        'produces a closed read-only Oracle candidate observation',
        'rejects stale operation, process, release, settings, and key drift',
        'rejects process-instance and observation freshness drift',
        'remote observation launcher operation remains read-only',
        '9007199254740992'
    )) {
        if ($oracleObservationTest.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Read-only Oracle candidate observation regression is missing '$required'"
        }
    }
    foreach ($required in @(
        'active Oracle release is outside the immutable release root',
        'active Oracle settings must be a root-owned regular file',
        'active Oracle job URL must be a credential-free HTTPS /jobs URL',
        '/etc/clearra-gateway/settings.pre-v0.8.0-',
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
        'canonicalJournalTimestamp',
        'restored prior runtime authority does not match the captured authority',
        '/run/clearra-deploy',
        'directoryMetadata.uid !== 0',
        '(directoryMetadata.mode & 0o777) !== 0o700',
        'linkSync(temporaryPath, proofPath)'
    )) {
        if ($oracleProofProducer.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Trusted Oracle proof producer is missing observation/security marker '$required'"
        }
    }
    if ($oracleProofProducerTest.IndexOf(
        'trusted Oracle producer selects the latest canonical operation regardless of journal order',
        [System.StringComparison]::Ordinal
    ) -lt 0) {
        Add-ArchitectureError 'Trusted Oracle proof producer regression is missing journal-order-independent latest-operation coverage'
    }
    foreach ($required in @(
        'clearra.rollback.runtime-authority.v1',
        'clearra.rollback.runtime-identity.v1',
        'clearra.rollback.legacy-health-no-runtime.v1',
        '^v0\.7\.4-([0-9a-f]{7})$',
        '^clearra-current-job-v075-([0-9a-f]{7})$',
        'legacyRelease[1] !== legacyRevision[1]',
        'assertPriorRuntimeAuthorityContext',
        'Object.hasOwn(health, "runtime")',
        'RUNTIME_HEALTH_KEYS',
        'RUNTIME_IDENTITY_KEYS',
        'dynamic-nonnegative-safe-integer',
        'normalizeRuntimeIdentity(runtime)'
    )) {
        if ($oracleRuntimeAuthority.IndexOf($required, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle runtime authority is missing fail-closed marker '$required'"
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
    foreach ($authorityConsumer in @(
        @{ Name = 'capture'; Text = $oracleRollbackCapture; Marker = 'observePriorRuntimeAuthority' },
        @{ Name = 'producer'; Text = $oracleProofProducer; Marker = 'observePriorRuntimeAuthority' },
        @{ Name = 'rollback proof'; Text = $oracleRollbackProof; Marker = 'assertPriorRuntimeAuthorityContext' }
    )) {
        if ($authorityConsumer.Text.IndexOf($authorityConsumer.Marker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle $($authorityConsumer.Name) does not independently consume the runtime-authority context"
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
        @{ Name = 'capture'; Text = $oracleRollbackCaptureTest; Marker = 'freezes exact v0.7.4 legacy authority without inventing identity' },
        @{ Name = 'capture-v2'; Text = $oracleRollbackCaptureTest; Marker = 'rejects health or identity key drift before backup' },
        @{ Name = 'producer'; Text = $oracleProofProducerTest; Marker = 'rejects stale settings, process, and operation evidence' },
        @{ Name = 'producer-v2'; Text = $oracleProofProducerTest; Marker = 'preserves strict v2 runtime authority' },
        @{ Name = 'candidate'; Text = $oracleCandidateProofTest; Marker = 'rejects every stale deployment authority' },
        @{ Name = 'rollback'; Text = $oracleRollbackProofTest; Marker = 'rejects stale authority and missing live checks' },
        @{ Name = 'rollback-context'; Text = $oracleRollbackProofTest; Marker = 'independently rejects broadened legacy context' },
        @{ Name = 'restore'; Text = $oracleRestoreTest; Marker = 'keeps the service stopped after every partial or unverified restore' },
        @{ Name = 'tree'; Text = $oracleReleaseDigestTest; Marker = 'rejects an external or mutable symlink target' }
    )) {
        if ($testContract.Text.IndexOf($testContract.Marker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "Oracle $($testContract.Name) recurrence contract is missing '$($testContract.Marker)'"
        }
    }
    foreach ($candidateFixture in @(
        @{ Name = 'candidate verifier'; Text = $oracleCandidateProofTest },
        @{ Name = 'trusted producer'; Text = $oracleProofProducerTest }
    )) {
        if ($candidateFixture.Text -cnotmatch 'oracleReleaseId\s*:\s*"v0\.8\.0-[0-9a-f]{7}"' -or
            $candidateFixture.Text -cnotmatch 'candidateRevision\s*:\s*"clearra-current-job-v080-[0-9a-f]{7}"') {
            Add-ArchitectureError "Oracle $($candidateFixture.Name) success fixture must bind one v0.8.0/v080 candidate identity"
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
        'resolveCanonicalAcceptanceRun',
        'expectedCount: 1',
        '../../../scripts/release/canonical-acceptance-run.mjs',
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
            'scripts/release/cloud/candidate-release-v080.mjs deploy',
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
                '$jobBearerSecretVersion = "<numeric-enabled-Secret-version>"',
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
        $explicitEightWorkerCount = [regex]::Matches(
            $deploymentDoc.Text,
            'CLEARRA_SEARCH_WORKERS_PER_SESSION=8'
        ).Count
        $explicitEightVcpuAuthorityCount = [regex]::Matches(
            $deploymentDoc.Text,
            'CLEARRA_EXPECTED_VCPUS=8'
        ).Count
        if ($explicitEightWorkerCount -lt 1 -or
            $explicitEightVcpuAuthorityCount -lt 1 -or
            $cloudCandidateRelease.IndexOf(
                'CLEARRA_SEARCH_WORKERS_PER_SESSION: "8"',
                [System.StringComparison]::Ordinal
            ) -lt 0 -or
            $cloudCandidateRelease.IndexOf(
                'CLEARRA_EXPECTED_VCPUS: "8"',
                [System.StringComparison]::Ordinal
            ) -lt 0 -or
            $deploymentDoc.Text.IndexOf(
                'CLEARRA_SEARCH_WORKERS_PER_SESSION=auto',
                [System.StringComparison]::Ordinal
            ) -ge 0 -or
            $cloudCandidateRelease.IndexOf(
                '--cpu=8',
                [System.StringComparison]::Ordinal
            ) -lt 0 -or
            -not [regex]::IsMatch(
                $deploymentDoc.Text,
                'startup CPU boost',
                [System.Text.RegularExpressions.RegexOptions]::IgnoreCase
            )) {
            Add-ArchitectureError "$($deploymentDoc.Name) must pin the 8-vCPU authority and worker allocation and document startup CPU boost visibility"
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
    $syncCanonicalCatalogIndex = $cloudDeploy.IndexOf(
        'canonical --source-commit $sourceCommit --output $catalogPath',
        [Math]::Max(0, $syncHeadingIndex),
        [System.StringComparison]::Ordinal
    )
    $syncMutationIndex = $cloudDeploy.IndexOf(
        'sync --source-commit $sourceCommit --application-id $applicationId',
        [Math]::Max(0, $syncHeadingIndex),
        [System.StringComparison]::Ordinal
    )
    if ($syncHeadingIndex -lt 0 -or
        $syncPreflightIndex -le $syncHeadingIndex -or
        $syncActiveHealthIndex -le $syncPreflightIndex -or
        $syncCanonicalCatalogIndex -le $syncActiveHealthIndex -or
        $syncMutationIndex -le $syncCanonicalCatalogIndex) {
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
