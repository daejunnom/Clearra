# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Q keeps search truth separate from CPU post-processing.

function Invoke-PostProcessPipelineContractValidation() {
    foreach ($requiredFile in @(
        'crates/clearra-postprocess/Cargo.toml',
        'crates/clearra-postprocess/src/pc_scoring/pc_scoring_postprocessor.rs',
        'crates/clearra-postprocess/src/score_batch/candidate_execution_aggregate.rs',
        'crates/clearra-postprocess/src/score_batch/score_matrix.rs'
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "Q PostProcess pipeline file missing: $requiredFile"
        }
    }

    $surface = @(
        Read-Text 'crates/clearra-postprocess/src/pc_scoring/pc_scoring_postprocessor.rs'
        Read-Text 'crates/clearra-postprocess/src/score_batch/candidate_execution_aggregate.rs'
        Read-Text 'crates/clearra-postprocess/src/score_batch/score_matrix.rs'
    ) -join "`n"
    foreach ($required in @(
        'CandidateExecutionAggregate', 'ReplayTrace', 'ScoreMatrix::materialize'
    )) {
        if ($surface -notlike "*$required*") {
            Add-ArchitectureError "PostProcess contract is missing '$required'"
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
