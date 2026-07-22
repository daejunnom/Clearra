# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Product pruning authority belongs to connected native engine factories only.

function Invoke-ProofCarryingPruningContractValidation() {
    foreach ($requiredFile in @(
        'core-c/include/clr_pruning.h',
        'core-c/src/pruning/prune_reason.c',
        'core-c/src/pruning/domain_propagation.c',
        'core-c/src/packing/packing_pruner.c',
        'crates/clearra-core-domain/src/pruning/pruning_proof_ledger.rs',
        'crates/clearra-core-ffi/src/native/pruning.rs'
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "proof-carrying pruning file missing: $requiredFile"
        }
    }

    foreach ($file in @(Get-RustFiles 'crates/clearra-core-executor/src')) {
        $text = Get-RustProductionContents (Get-Content -LiteralPath $file.FullName -Raw)
        if ($text -match '\b(AuthorizedPrune|ReachabilityEngineSeal|ClearStateDomainEngineSeal|CompleteReachabilitySearch|CompleteClearStateDomainTable)\b') {
            Add-ArchitectureError "executor exposes unconnected proof authority: $(Get-RepositoryRelativePath $file.FullName)"
        }
    }

    $reasonFactory = Read-Text 'core-c/src/pruning/prune_reason.c'
    foreach ($required in @(
        'reason == CLR_PRUNE_PLACEMENT_COLLISION',
        'reason == CLR_PRUNE_TARGET_MASK_OVERFLOW'
    )) {
        if ($reasonFactory -notlike "*$required*") {
            Add-ArchitectureError "native pruning factory is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'reason == CLR_PRUNE_CELL_DOMAIN_EMPTY_FOR_ALL_REACHABLE_CLEAR_STATES',
        'reason == CLR_PRUNE_REACHABILITY_IMPOSSIBLE',
        'reason == CLR_PRUNE_BUILD_ORDERS_HOLD_REACHABLE_INTERSECTION_EMPTY'
    )) {
        if ($reasonFactory -like "*$forbidden*") {
            Add-ArchitectureError "unconnected pruning reason has engine authority '$forbidden'"
        }
    }

    $packingPruner = Read-Text 'core-c/src/packing/packing_pruner.c'
    foreach ($required in @(
        'clr_prune_reason_has_connected_engine_factory',
        'CLR_PRUNING_EVIDENCE_CAPACITY_UNAVAILABLE',
        '*out_keep_candidate = true'
    )) {
        if ($packingPruner -notlike "*$required*") {
            Add-ArchitectureError "native packing pruner is missing '$required'"
        }
    }

    $domain = Read-Text 'core-c/src/pruning/domain_propagation.c'
    if ($domain -match 'return\s+CLR_PRUNE_PROOF_GLOBAL_SAFE') {
        Add-ArchitectureError 'domain propagation must not mint GlobalSafe without a connected complete engine'
    }

    $ledger = Read-Text 'crates/clearra-core-domain/src/pruning/pruning_proof_ledger.rs'
    foreach ($required in @(
        'record_engine_drop_evidence',
        'PruneReason::PlacementCollision | PruneReason::TargetMaskOverflow',
        'EvidenceRejectedUnconnectedReason',
        'CandidateKeptForCompleteEvidence'
    )) {
        if ($ledger -notlike "*$required*") {
            Add-ArchitectureError "Rust pruning evidence ledger is missing '$required'"
        }
    }
}
