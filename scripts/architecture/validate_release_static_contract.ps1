# Release-blocking static validation is intentionally limited to boundaries,
# forbidden APIs, public ABI shape, unsafe isolation, and unsupported capability
# disclosure. Runtime correctness belongs to executed adversarial tests.

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
