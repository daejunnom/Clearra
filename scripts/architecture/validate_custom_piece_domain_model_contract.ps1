# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# G1 keeps custom piece schema explicit while runtime remains guarded.

function Invoke-CustomPieceDomainModelContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-core-domain/src/piece/standard_tetromino_piece.rs",
            "crates/clearra-core-domain/src/piece/piece_kind.rs",
            "crates/clearra-piece-registry/src/custom/custom_piece_definition.rs",
            "crates/clearra-piece-registry/src/custom/custom_piece_schema.rs",
            "crates/clearra-piece-registry/src/registry/mixed_piece_set.rs",
            "crates/clearra-piece-registry/src/registry/piece_set_definition.rs",
            "crates/clearra-piece-registry/src/registry/piece_registry_bridge.rs",
            "crates/clearra-validation/src/validators/piece_set_validator.rs",
            "crates/clearra-validation/src/validators/piece_set_diagnostic_builder.rs",
            "crates/clearra-core-ffi/src/problem/mod.rs",
            "core-c/include/clr_piece.h",
            "core-c/src/cache/cache_identity.h",
            "core-c/src/cache/cache_key.c",
            "crates/clearra-invariant-tests/tests/custom_piece_domain_model_contract_tests.rs",
            "tests/fixtures/pieces/mixed_custom_piece_set.json",
            "docs/architecture.md",
            "docs/mvp-scope.md",
            "scripts/custom-piece-domain-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G1 required custom piece domain file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-core-domain/src/piece/standard_tetromino_piece.rs"
        Read-Text "crates/clearra-core-domain/src/piece/piece_kind.rs"
        Read-Text "crates/clearra-core-domain/src/ids/piece_id.rs"
        Read-Text "crates/clearra-piece-registry/src/registry/piece_registry.rs"
        Read-Text "crates/clearra-piece-registry/src/custom/custom_piece_definition.rs"
        Read-Text "crates/clearra-piece-registry/src/custom/custom_piece_schema.rs"
        Read-Text "crates/clearra-piece-registry/src/registry/mixed_piece_set.rs"
        Read-Text "crates/clearra-piece-registry/src/registry/piece_set_definition.rs"
        Read-Text "crates/clearra-piece-registry/src/registry/piece_registry_bridge.rs"
        Read-Text "crates/clearra-validation/src/validators/piece_set_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/piece_set_diagnostic_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/mod.rs"
        Read-Text "core-c/include/clr_piece.h"
        Read-Text "core-c/src/cache/cache_identity.h"
        Read-Text "core-c/src/cache/cache_key.c"
        Read-Text "crates/clearra-invariant-tests/tests/custom_piece_domain_model_contract_tests.rs"
        Read-Text "docs/architecture.md"
        Read-Text "docs/mvp-scope.md"
        Read-Text "scripts/custom-piece-domain-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "StandardTetrominoPiece",
            "CustomPieceDefinition",
            "PieceDefinitionId",
            "PieceDefinition",
            "PieceSetDefinition",
            "piece_definition_id",
            "display_name",
            "area",
            "rotation_states",
            "cells_by_rotation",
            "bounds_by_rotation",
            "spawn_offsets",
            "color_hint",
            "symmetry_class",
            "source_provenance",
            "piece_set_id",
            "standard_fast_path_compatible",
            "mixed_area_multiset",
            "custom_piece_runtime_not_connected",
            "mixed_piece_runtime_not_connected",
            "piece_definition_id_fingerprint",
            "piece_area_multiset_fingerprint",
            "piece_set_profile_id",
            "standard_tetromino_fast_path_unchanged",
            "custom_piece_schema_validates",
            "custom_piece_runtime_not_connected_until_runtime_exists",
            "missing_cells_mod_4_not_used_for_generic_feasibility",
            "piece_definition_id_included_in_cache_keys",
            "compile-and-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G1 custom piece domain model must expose marker '$requiredMarker'"
        }
    }
$pieceKind = Read-Text "crates/clearra-core-domain/src/piece/piece_kind.rs"
foreach ($forbiddenMarker in @("PieceKind::Custom", "PieceKind::Unknown", "Custom(", "custom_piece")) {
        if ($pieceKind -like "*$forbiddenMarker*") {
            Add-ArchitectureError "G1 must not encode custom pieces inside PieceKind using marker '$forbiddenMarker'"
        }
    }
$areaFeasibility = Read-Text "crates/clearra-core-executor/src/area/area_multiset_feasibility.rs"
if ($areaFeasibility -match "missing_cells\s*%\s*4" -or
        $areaFeasibility -match "missing_cells\.rem_euclid\(\s*4\s*\)") {
        Add-ArchitectureError "G1 generic area feasibility must not use missing_cells % 4 as a custom-piece feasibility proof"
    }
$bridge = Read-Text "crates/clearra-piece-registry/src/registry/piece_registry_bridge.rs"
if ($bridge -notlike "*runtime_path == PieceRegistryRuntimePath::UnsupportedExtension*") {
        Add-ArchitectureError "G1 custom schema path must stay visibly guarded instead of silently falling back"
    }
}
