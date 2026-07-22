# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-Mvp3AcceptanceTestsContractValidation() {
$requiredFiles = @(
        "scripts/mvp3-acceptance.ps1",
        "scripts/custom-piece-domain-check.ps1",
        "scripts/mixed-supply-generalization-check.ps1",
        "scripts/board128-wide-runtime-check.ps1",
        "scripts/area-multiset-feasibility-check.ps1",
        "crates/clearra-invariant-tests/tests/custom_piece_domain_model_contract_tests.rs",
        "crates/clearra-invariant-tests/tests/area_decomposition_contract_tests.rs",
        "crates/clearra-invariant-tests/tests/board_backend_contract_tests.rs",
        "crates/clearra-validation/src/validators/supply_validator_tests.rs",
        "docs/architecture.md",
        "docs/test-policy.md"
    )
foreach ($relativePath in $requiredFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $relativePath))) {
            Add-ArchitectureError "T7 MVP3 acceptance tests required file is missing: $relativePath"
        }
    }
$customPieceTests = Read-Text "crates/clearra-invariant-tests/tests/custom_piece_domain_model_contract_tests.rs"
foreach ($requiredMarker in @(
        "custom_piece_schema_validates_but_runtime_guarded",
        "custom_piece_schema_validates",
        "custom_piece_runtime_not_connected_until_runtime_exists",
        "PieceRegistryRuntimePath::UnsupportedExtension",
        "custom_piece_runtime_not_connected",
        "mixed_piece_runtime_not_connected",
        "generic_cache_key_includes_piece_definition_id",
        "piece_definition_id_included_in_cache_keys",
        "piece_definition_id_fingerprint",
        "piece_area_multiset_fingerprint",
        "piece_set_profile_id"
    )) {
        if (-not $customPieceTests.Contains($requiredMarker)) {
            Add-ArchitectureError "custom piece invariant tests must keep T7 marker '$requiredMarker'"
        }
    }
$areaTests = Read-Text "crates/clearra-invariant-tests/tests/area_decomposition_contract_tests.rs"
foreach ($requiredMarker in @(
        "mixed_piece_area_multiset_feasibility",
        "missing_cells_mod_4_not_used_for_generic_feasibility",
        "AreaTileabilityRules::new([4, 3, 3])",
        "AreaTileabilityRules::new([3, 3])",
        "AreaTileabilityRules::standard_tetrominoes",
        "assert_ne!(6 % 4, 0)",
        "assert!(generic.can_compose_area(6))"
    )) {
        if (-not $areaTests.Contains($requiredMarker)) {
            Add-ArchitectureError "area feasibility invariant tests must keep T7 marker '$requiredMarker'"
        }
    }
$boardTests = Read-Text "crates/clearra-invariant-tests/tests/board_backend_contract_tests.rs"
foreach ($requiredMarker in @(
        "board128_descriptor_tests",
        "wide_board_runtime_not_connected",
        "BoardBackendKind::Board128",
        "BoardBackendKind::Wide",
        "BoardRuntimeUnsupportedReason::WideBoardRuntimeNotConnected",
        "DiagnosticCode::EWideBoardRuntimeNotConnected",
        "descriptor_supported",
        "packing_supported"
    )) {
        if (-not $boardTests.Contains($requiredMarker)) {
            Add-ArchitectureError "board backend invariant tests must keep T7 marker '$requiredMarker'"
        }
    }
$supplyTests = Read-Text "crates/clearra-validation/src/validators/supply_validator_tests.rs"
foreach ($requiredMarker in @(
        "custom_bag_not_silent_standard_fallback",
        "SupplyProfileKind::UnsupportedExtension",
        "SupplyProfileKind::Standard7Bag",
        "custom_bag_runtime_not_connected",
        "DiagnosticCode::ECustomBagUnsupportedMvp",
        "validate_supply_profile_mvp3_guard"
    )) {
        if (-not $supplyTests.Contains($requiredMarker)) {
            Add-ArchitectureError "supply validator tests must keep T7 marker '$requiredMarker'"
        }
    }
$checkScripts = @(
        Read-Text "scripts/custom-piece-domain-check.ps1"
        Read-Text "scripts/mixed-supply-generalization-check.ps1"
        Read-Text "scripts/board128-wide-runtime-check.ps1"
        Read-Text "scripts/area-multiset-feasibility-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
        "cargo check -p clearra-invariant-tests --tests",
        "clearra-validation",
        "clearra-geometry",
        "clearra-core-executor",
        "cargo check -p",
        "test_executable_launched=false"
    )) {
        if (-not $checkScripts.Contains($requiredMarker)) {
            Add-ArchitectureError "MVP3 compile-only check scripts must keep T7 marker '$requiredMarker'"
        }
    }
$taskList = Read-Text "scripts/lib/architecture-validation-tasks.ps1"
foreach ($requiredMarker in @(
        "T7 MVP3 Acceptance Tests",
        "Invoke-Mvp3AcceptanceTestsContractValidation"
    )) {
        if (-not $taskList.Contains($requiredMarker)) {
            Add-ArchitectureError "architecture validation task list must include T7 marker '$requiredMarker'"
        }
    }
$docSurface = @(
        Read-Text "docs/architecture.md"
        Read-Text "docs/test-policy.md"
    ) -join "`n"
foreach ($requiredMarker in @(
        "T7 MVP3 Acceptance Tests",
        "custom_piece_schema_validates_but_runtime_guarded",
        "mixed_piece_area_multiset_feasibility",
        "missing_cells_mod_4_not_used_for_generic_feasibility",
        "board128_descriptor_tests",
        "wide_board_runtime_not_connected",
        "custom_bag_not_silent_standard_fallback",
        "generic_cache_key_includes_piece_definition_id"
    )) {
        if (-not $docSurface.Contains($requiredMarker)) {
            Add-ArchitectureError "docs must document T7 marker '$requiredMarker'"
        }
    }
}
