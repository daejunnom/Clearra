# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Q keeps search truth separate from connected PostProcess WebGPU bulk work.

function Invoke-PostProcessPipelineContractValidation() {
    foreach ($requiredFile in @(
        'crates/clearra-postprocess/Cargo.toml',
        'crates/clearra-postprocess/src/pc_scoring/pc_scoring_postprocessor.rs',
        'crates/clearra-postprocess/src/score_batch/candidate_execution_aggregate.rs',
        'crates/clearra-postprocess/src/score_batch/score_matrix.rs',
        'crates/clearra-postprocess/src/coverage_batch/gpu_coverage_union.rs',
        'crates/clearra-postprocess-gpu/Cargo.toml',
        'crates/clearra-postprocess-gpu/src/post_gpu_capability.rs',
        'crates/clearra-postprocess-gpu/src/post_gpu_result.rs',
        'crates/clearra-postprocess-gpu/src/post_gpu_trust_state.rs',
        'crates/clearra-postprocess-gpu/src/postprocess_gpu_backend.rs'
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "Q PostProcess pipeline file missing: $requiredFile"
        }
    }

    $gpuManifest = Read-Text 'crates/clearra-postprocess-gpu/Cargo.toml'
    if ($gpuManifest -notlike '*clearra-webgpu*') {
        Add-ArchitectureError 'PostProcess GPU must depend on the connected clearra-webgpu backend'
    }

    $surface = @(
        Read-Text 'crates/clearra-postprocess/src/pc_scoring/pc_scoring_postprocessor.rs'
        Read-Text 'crates/clearra-postprocess/src/score_batch/candidate_execution_aggregate.rs'
        Read-Text 'crates/clearra-postprocess/src/score_batch/score_matrix.rs'
        Read-Text 'crates/clearra-postprocess-gpu/src/post_gpu_capability.rs'
        Read-Text 'crates/clearra-postprocess-gpu/src/post_gpu_result.rs'
        Read-Text 'crates/clearra-postprocess-gpu/src/post_gpu_trust_state.rs'
        Read-Text 'crates/clearra-postprocess-gpu/src/postprocess_gpu_backend.rs'
        Read-Text 'crates/clearra-postprocess/src/coverage_batch/gpu_coverage_union.rs'
    ) -join "`n"
    foreach ($required in @(
        'CandidateExecutionAggregate', 'ReplayTrace', 'ScoreMatrix::materialize',
        'PostGpuCapabilityState', 'Connected', 'Unavailable', 'RejectedMismatch',
        'TrustedDeterministic', 'TrustedCpuSampleConfirmed',
        'WebGpuBackend::run_bitset_union', 'fallback_reason',
        'post_backend_selected', 'can_claim_exact'
    )) {
        if ($surface -notlike "*$required*") {
            Add-ArchitectureError "PostProcess GPU contract is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'PostGpuCapabilityState::Preview', 'NeedsCpuConfirm',
        'PostGpuTrustState::FallbackUsed', 'PackingCandidate', 'clearra_core_executor',
        'clearra_core_ffi', 'SpecialSpinCaseRegistry', 'Fumen', 'serde_json',
        'BuildVariantBatch', 'ReplayEventBatch::from_build_variants',
        'pattern_bitset_or_used: true'
    )) {
        if ($surface -like "*$forbidden*") {
            Add-ArchitectureError "PostProcess GPU contains forbidden surface '$forbidden'"
        }
    }
}
