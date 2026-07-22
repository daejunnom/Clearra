# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# T keeps custom/MVP2/MVP3 expansion guarded instead of silently falling back.

function Invoke-GuardedExpansionContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-ui-schema/src/capability/capability_state.rs",
            "crates/clearra-ui-schema/src/capability/capability_report_entry_schema.rs",
            "crates/clearra-ui-schema/src/capability/mod.rs",
            "crates/clearra-supply/src/custom_bag/runtime_guard.rs",
            "crates/clearra-supply/src/custom_bag/mod.rs",
            "crates/clearra-rules/src/custom/kick_exactness_guard.rs",
            "crates/clearra-rules/src/custom/mod.rs",
            "crates/clearra-validation/src/validators/guarded_expansion_validator.rs",
            "crates/clearra-validation/src/validators/supply_validator.rs",
            "crates/clearra-validation/src/validators/supply_diagnostic_builder.rs",
            "crates/clearra-invariant-tests/tests/custom_piece_domain_model_contract_tests.rs",
            "core-c/src/board/board128.c",
            "core-c/src/board/wide_board.c",
            "core-c/src/cache/cache_identity.c",
            "docs/architecture.md",
            "docs/mvp-scope.md",
            "docs/test-policy.md"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "T guarded expansion required file is missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-ui-schema/src/capability/capability_state.rs"
        Read-Text "crates/clearra-ui-schema/src/capability/capability_report_entry_schema.rs"
        Read-Text "crates/clearra-supply/src/custom_bag/runtime_guard.rs"
        Read-Text "crates/clearra-supply/src/custom_bag/mod.rs"
        Read-Text "crates/clearra-rules/src/custom/kick_exactness_guard.rs"
        Read-Text "crates/clearra-rules/src/custom/mod.rs"
        Read-Text "crates/clearra-validation/src/validators/guarded_expansion_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/supply_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/supply_diagnostic_builder.rs"
        Read-Text "crates/clearra-invariant-tests/tests/custom_piece_domain_model_contract_tests.rs"
        Read-Text "core-c/src/board/board128.c"
        Read-Text "core-c/src/board/wide_board.c"
        Read-Text "core-c/src/cache/cache_identity.c"
        Read-Text "docs/architecture.md"
        Read-Text "docs/mvp-scope.md"
        Read-Text "docs/test-policy.md"
    ) -join "`n"
foreach ($requiredMarker in @(
            "pub enum CapabilityState",
            "Unsupported",
            "SchemaOnly",
            "ValidationGuard",
            "Preview",
            "BasicApproximation",
            "RuntimeConnected",
            "ExactSupported",
            "runtime_execution_allowed",
            "exact_claim_allowed",
            "CapabilityReportEntrySchema",
            "CustomBagRuntimeGuard",
            "custom_bag_not_silent_standard_fallback",
            "custom_bag_runtime_not_connected",
            "CustomKickExactnessGuard",
            "srs_plus_builtin_profile_supports_exact_180",
            "imported_verified_kick_supports_exact_180_after_verification",
            "unverified_custom_kick_rejected_before_c_execution",
            "validate_custom_kick_before_c_execution",
            "custom_piece_schema_validates_but_runtime_guarded",
            "generic_cache_key_includes_piece_definition_id",
            "piece_definition_id_fingerprint",
            "piece_area_multiset_fingerprint",
            "piece_set_profile",
            "clr_board128_make_descriptor",
            "clr_wide_board_make_descriptor",
            "Board128/Wide",
            "Built-in SRS+"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "T guarded expansion surface must expose marker '$requiredMarker'"
        }
    }
$customBagGuard = Read-Text "crates/clearra-supply/src/custom_bag/runtime_guard.rs"
if ($customBagGuard -like "*SupplyProfileKind::Standard7Bag*") {
        Add-ArchitectureError "T custom bag guard must not route custom bag runtime to Standard7Bag"
    }
$board128 = Read-Text "core-c/src/board/board128.c"
$wide = Read-Text "core-c/src/board/wide_board.c"
if ($board128 -like "*cell_count > 128*" -and $board128 -like "*cell_count <= 64*") {
        # Expected descriptor guard: Board128 is explicitly 65..128 cells.
    } else {
        Add-ArchitectureError "T Board128 descriptor must validate only 65..128 cells"
    }
if ($wide -notlike "*cell_count <= 128*") {
        Add-ArchitectureError "T Wide descriptor must reject Board64/Board128-sized layouts instead of truncating"
    }
}
