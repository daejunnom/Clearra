# This file is dot-sourced by Invoke-WorkspaceSurfaceArchitectureValidation.
# It intentionally contains ordered validation statements, not a standalone entrypoint.

$supplyValidator = @(
    Read-Text "crates/clearra-validation/src/validators/supply_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/supply_bag_pattern_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/supply_diagnostic_builder.rs"
    Read-Text "crates/clearra-validation/src/validators/supply_fixed_queue_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/supply_observed_queue_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/supply_validator_tests.rs"
) -join "`n"
$supplyBagProfile = Read-Text "crates/clearra-supply/src/bag/bag_profile.rs"
if ($supplyValidator -notlike "*invariant_observed_supply_ambiguity_is_warning_not_error*") {
    Add-ArchitectureError "clearra-validation supply validator tests must carry invariant marker 'invariant_observed_supply_ambiguity_is_warning_not_error'"
}
foreach ($requiredMarker in @("FixedSequence", "BagAlignedPattern", "validate_fixed_sequence", "validate_bag_aligned_pattern", "fixed_sequence_duplicate_is_allowed_because_boundary_is_not_implied", "bag_aligned_pattern_duplicate_is_an_error")) {
    if ($supplyValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-validation supply validator must keep fixed sequence and bag-aligned pattern semantics split with marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("BagProfileEntry", "multiplicity", "weight", "standard_7", "arbitrary_multiset_bag_can_repeat_piece_kinds", "PatternUniverseHint", "SparseRecommended")) {
    if ($supplyBagProfile -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-supply BagProfile must own generalized multiset bag marker '$requiredMarker'"
    }
}
$geometryBoardBackend = Read-Text "crates/clearra-geometry/src/layout/board_backend.rs"
$geometryBoard128Layout = Read-Text "crates/clearra-geometry/src/layout/board128_layout.rs"
$geometryWideBoardLayout = Read-Text "crates/clearra-geometry/src/layout/wide_board_layout.rs"
$boardValidator = Read-Text "crates/clearra-validation/src/validators/board_validator.rs"
foreach ($requiredMarker in @("BoardBackendKind", "Board64", "Board128", "Wide", "BoardLayoutBackend", "backend_kind_for_size")) {
    if ($geometryBoardBackend -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-geometry BoardBackendKind must own layout backend marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("Board128Layout", "TooManyCells", "all_cells_mask", "accepts_standard_twelve_line_analysis_layout")) {
    if ($geometryBoard128Layout -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-geometry Board128Layout must own widened fast path marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("WideBoardLayout", "cell_count", "wide_layout_keeps_custom_size_without_bit_width_limit")) {
    if ($geometryWideBoardLayout -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-geometry WideBoardLayout must own dynamic board marker '$requiredMarker'"
    }
}
$searchAreaMultiset = Read-Text "crates/clearra-core-executor/src/area/area_multiset_feasibility.rs"
foreach ($requiredMarker in @("AreaMultisetFeasibility", "from_mixed_piece_set", "from_mixed_bag_profile", "piece_area_multiset_fingerprint", "can_fill_exactly", "bounded_area_subset_sum", "generic_feasibility_does_not_use_missing_cells_mod_four")) {
    if ($searchAreaMultiset -notlike "*$requiredMarker*") {
        Add-ArchitectureError "AreaMultisetFeasibility must avoid tetromino-only missing cell assumptions marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("validate_board_backend_mvp3_guard", "ECustomBoardUnsupportedMvp", "custom_board_runtime_not_connected", "board128_backend_is_guarded_until_search_runtime_is_generic")) {
    if ($boardValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-validation BoardValidator must guard MVP3 board backends before runtime marker '$requiredMarker'"
    }
}
$pieceIdDomain = Read-Text "crates/clearra-core-domain/src/ids/piece_id.rs"
$customPieceDefinition = Read-Text "crates/clearra-piece-registry/src/custom/custom_piece_definition.rs"
$customOperationTable = Read-Text "crates/clearra-piece-registry/src/custom/custom_operation_table.rs"
$customOperationTableTests = Read-Text "crates/clearra-piece-registry/src/custom/custom_operation_table_tests.rs"
$mixedPieceSet = Read-Text "crates/clearra-piece-registry/src/registry/mixed_piece_set.rs"
$mixedBagProfile = Read-Text "crates/clearra-piece-registry/src/registry/mixed_bag_profile.rs"
$pieceRegistryBridge = Read-Text "crates/clearra-piece-registry/src/registry/piece_registry_bridge.rs"
$pieceSetValidator = @(
    Read-Text "crates/clearra-validation/src/validators/piece_set_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/piece_budget_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/piece_set_diagnostic_builder.rs"
    Read-Text "crates/clearra-validation/src/validators/piece_set_mixed_guard_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/piece_set_standard_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/piece_set_validator_tests.rs"
) -join "`n"
$futureCustomPiecesDoc = Read-Text "docs/future-custom-pieces.md"
foreach ($requiredMarker in @("pub struct PieceDefinitionId", "as_str", "piece_definition_id_is_stable_string_identity_not_order_index")) {
    if ($pieceIdDomain -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-core-domain PieceDefinitionId must own stable custom piece id marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("id: PieceDefinitionId", "label: String", "rotations: Vec<CustomPieceRotation>", "spawn_bounds: PieceSpawnBounds", "display: PieceDisplayMetadata", "area: usize", "symmetry: PieceSymmetryClass", "canonical_key: String", "rotation_states", "custom_piece_definition_carries_mvp3_schema_without_runtime_search_support")) {
    if ($customPieceDefinition -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CustomPieceDefinition must carry MVP3 schema marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("CustomOperationTableSchema", "CustomOperationSchema", "CUSTOM_OPERATION_TABLE_SCHEMA_VERSION", "piece_area", "rotation_states", "piece_definition_fingerprint")) {
    if ($customOperationTable -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CustomOperationTableSchema must carry MVP3 custom operation schema marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("custom_operation_table_schema_preserves_piece_area_and_rotation_states", "custom_operation_table_fingerprint_uses_stable_piece_definition_id")) {
    if ($customOperationTableTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CustomOperationTableSchema tests must carry MVP3 operation schema marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("MixedPieceSet", "MixedPieceSetEntry", "standard_plus_custom", "stable_piece_ids", "standard_piece_definition_id", "mixed_piece_set_keeps_stable_piece_ids_independent_of_entry_order")) {
    if ($mixedPieceSet -notlike "*$requiredMarker*") {
        Add-ArchitectureError "MixedPieceSet must carry stable mixed registry marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("MixedBagProfile", "MixedBagEntry", "BagBoundaryModels", "piece_set_id", "multiplicity", "weight", "mixed_bag_profile_references_piece_set_stable_ids_with_multiplicity_and_weight")) {
    if ($mixedBagProfile -notlike "*$requiredMarker*") {
        Add-ArchitectureError "MixedBagProfile must carry custom bag registry marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PieceRegistryBridge", "PieceRegistryRuntimePath", "StandardFastPath", "UnsupportedExtension", "custom_operation_tables", "piece_definition_id_fingerprint", "piece_area_multiset_fingerprint", "custom_piece_runtime_not_connected", "piece_registry_bridge_keeps_standard_fast_path_unaffected", "piece_registry_bridge_exposes_custom_operation_schema_and_guard_reason")) {
    if ($pieceRegistryBridge -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PieceRegistryBridge must preserve standard fast path and expose custom schema guard marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("validate_mixed_piece_set_mvp3_guard", "validate_mixed_bag_profile_mvp3_guard", "ECustomPieceUnsupportedMvp", "ECustomBagUnsupportedMvp", "custom_piece_runtime_not_connected", "custom_bag_runtime_not_connected", "custom_piece_registry_is_recognized_but_blocked_before_search_runtime", "custom_bag_profile_is_guarded_before_piece_definition_id_supply_runtime")) {
    if ($pieceSetValidator -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PieceSetValidator must guard MVP3 custom pieces before runtime marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PieceDefinitionId", "Mixed piece sets", "Custom bag profiles", "custom operation table schema", "piece registry bridge", "area multiset feasibility", "missing_cells % 4", "piece definition id fingerprint", "BoardStateBackend", "Board128", "Wide", "AreaScope", "area_decomposition_contract_tests", "E_CUSTOM_PIECE_UNSUPPORTED_MVP", "E_CUSTOM_BAG_UNSUPPORTED_MVP", "E_CUSTOM_BOARD_UNSUPPORTED_MVP", "tests/fixtures/pieces/mixed_custom_piece_set.json")) {
    if ($futureCustomPiecesDoc -notlike "*$requiredMarker*") {
        Add-ArchitectureError "docs/future-custom-pieces.md must document custom piece contract marker '$requiredMarker'"
    }
}
