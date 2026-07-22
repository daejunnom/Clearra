# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# G4 keeps custom operation schemas in Rust while the stable C/FFI ABI exposes
# only the connected standard-tetromino runtime.

function Invoke-GenericOperationCandidateReachabilityContractValidation() {
    foreach ($requiredFile in @(
            "core-c/include/clr_piece.h",
            "core-c/src/piece/operation.h",
            "core-c/src/piece/operation_table.c",
            "core-c/src/candidate/candidate.h",
            "core-c/src/reachability/reachability.h",
            "crates/clearra-piece-registry/src/registry/generic_operation_table_descriptor.rs",
            "crates/clearra-piece-registry/src/registry/piece_registry_bridge.rs",
            "crates/clearra-validation/src/validators/piece_set_diagnostic_builder.rs",
            "docs/architecture.md",
            "docs/algorithms.md",
            "docs/future-custom-pieces.md",
            "docs/mvp-scope.md",
            "scripts/generic-operation-candidate-reachability-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G4 required operation/candidate/reachability file missing: $requiredFile"
        }
    }

    $rustSchemaSurface = @(
        Read-Text "crates/clearra-piece-registry/src/registry/generic_operation_table_descriptor.rs"
        Read-Text "crates/clearra-piece-registry/src/registry/piece_registry_bridge.rs"
        Read-Text "crates/clearra-validation/src/validators/piece_set_diagnostic_builder.rs"
        Read-Text "crates/clearra-validation/src/validators/piece_set_validator_tests.rs"
    ) -join "`n"
    foreach ($requiredMarker in @(
            "StandardTetrominoOperationTable",
            "CustomPieceOperationTable",
            "GenericOperationTableDescriptor",
            "custom_operation_table_schema_validates",
            "custom_piece_runtime_not_connected",
            "piece_definition_id_fingerprint",
            "piece_area_multiset_fingerprint",
            "operation_table_version",
            "rotation_state_count",
            "operation_mask_word_count"
        )) {
        if ($rustSchemaSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G4 Rust extension schema must expose marker '$requiredMarker'"
        }
    }

    $connectedRuntimeSurface = @(
        Read-Text "core-c/include/clr_piece.h"
        Read-Text "core-c/src/piece/operation.h"
        Read-Text "core-c/src/piece/operation_table.c"
        Read-Text "core-c/src/candidate/candidate.h"
        Read-Text "core-c/src/reachability/reachability.h"
        Read-Text "crates/clearra-core-ffi/src/problem/mod.rs"
        Read-Text "crates/clearra-core-ffi/src/lib.rs"
    ) -join "`n"
    foreach ($requiredMarker in @(
            "ClearraOperationTable",
            "clearra_operation_table_generate",
            "CLEARRA_STANDARD_OPERATION_TABLE_VERSION"
        )) {
        if ($connectedRuntimeSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G4 connected standard runtime must expose marker '$requiredMarker'"
        }
    }

    foreach ($forbiddenStableAbi in @(
            "clr_generic_operation_table_descriptor",
            "ClearraGenericCandidateGeneratorInput",
            "ClearraGenericReachabilityInput",
            "CGenericOperationTableDescriptor",
            "C_OPERATION_TABLE_GENERIC_SCHEMA_ONLY",
            "CLR_OPERATION_TABLE_GENERIC_SCHEMA_ONLY",
            "CLR_GENERIC_CANDIDATE_RUNTIME_NOT_CONNECTED",
            "CLR_GENERIC_REACHABILITY_RUNTIME_NOT_CONNECTED"
        )) {
        if ($connectedRuntimeSurface -like "*$forbiddenStableAbi*") {
            Add-ArchitectureError "G4 stable C/FFI ABI must not reserve speculative marker '$forbiddenStableAbi'"
        }
    }

    $removedFfiDescriptor = Join-Path $Root "crates/clearra-core-ffi/src/problem/generic_operation_descriptor.rs"
    if (Test-Path -LiteralPath $removedFfiDescriptor) {
        Add-ArchitectureError "G4 schema-only generic operation descriptor must not be exported by clearra-core-ffi"
    }
}
