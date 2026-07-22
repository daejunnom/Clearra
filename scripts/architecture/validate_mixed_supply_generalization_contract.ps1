# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# G2 keeps implemented supply kinds separate from unsupported extensions.

function Invoke-MixedSupplyGeneralizationContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-supply/src/mixed/mod.rs",
            "crates/clearra-supply/src/mixed/custom_bag_profile.rs",
            "crates/clearra-supply/src/mixed/supply_profile.rs",
            "crates/clearra-supply/src/mixed/supply_provenance.rs",
            "crates/clearra-supply/src/bag/bag_profile.rs",
            "crates/clearra-profiles/src/bag/bag_profile.rs",
            "crates/clearra-validation/src/validators/supply_validator.rs",
            "crates/clearra-validation/src/validators/supply_diagnostic_builder.rs",
            "crates/clearra-validation/src/validators/supply_validator_tests.rs",
            "crates/clearra-piece-registry/src/registry/mixed_bag_profile.rs",
            "crates/clearra-core-ffi/src/problem/mod.rs",
            "core-c/include/clr_supply.h",
            "core-c/src/supply/queue_view.c",
            "core-c/src/cache/cache_identity.c",
            "core-c/tests/supply_tests.c",
            "docs/architecture.md",
            "docs/mvp-scope.md",
            "scripts/mixed-supply-generalization-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G2 required mixed supply generalization file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-supply/src/mixed/custom_bag_profile.rs"
        Read-Text "crates/clearra-supply/src/mixed/supply_profile.rs"
        Read-Text "crates/clearra-supply/src/mixed/supply_provenance.rs"
        Read-Text "crates/clearra-supply/src/bag/bag_profile.rs"
        Read-Text "crates/clearra-profiles/src/bag/bag_profile.rs"
        Read-Text "crates/clearra-validation/src/validators/supply_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/supply_diagnostic_builder.rs"
        Read-Text "crates/clearra-validation/src/validators/supply_validator_tests.rs"
        Read-Text "crates/clearra-piece-registry/src/registry/mixed_bag_profile.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/mod.rs"
        Read-Text "core-c/include/clr_supply.h"
        Read-Text "core-c/src/supply/queue_view.c"
        Read-Text "core-c/src/cache/cache_identity.c"
        Read-Text "core-c/tests/supply_tests.c"
        Read-Text "docs/architecture.md"
        Read-Text "docs/mvp-scope.md"
        Read-Text "scripts/mixed-supply-generalization-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "Standard7Bag",
            "FixedSequence",
            "ObservedWindow",
            "MaterializedPatternUniverse",
            "UnsupportedExtension",
            "ExtensionId",
            "supply_provenance_id",
            "bag_profile_id",
            "piece_set_id",
            "observed_window_id",
            "bag_boundary_evidence",
            "duplicate_witness",
            "ambiguity_report",
            "custom_bag_schema_valid",
            "mixed_bag_schema_validates",
            "custom_bag_runtime_not_connected",
            "standard_7_bag_path_unchanged",
            "custom_bag_runtime_not_connected_until_runtime_exists",
            "supply_provenance_in_cache_key",
            "observed_window_ambiguity_reported",
            "CLR_SUPPLY_PROFILE_UNSUPPORTED",
            "clearra_supply_profile_is_supported",
            "clr_supply_identity_descriptor",
            "compile-and-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G2 mixed supply generalization must expose marker '$requiredMarker'"
        }
    }
$supplyProfile = Read-Text "crates/clearra-supply/src/mixed/supply_profile.rs"
if ($supplyProfile -notlike "*pub fn custom_bag_profile*" -or
        $supplyProfile -notlike "*SupplyProfileKind::UnsupportedExtension*") {
        Add-ArchitectureError "G2 custom bag schema must lower to UnsupportedExtension before execution"
    }
foreach ($forbiddenStableAbi in @(
        "WeightedBagProfileFuture",
        "CLR_SUPPLY_PROFILE_WEIGHTED_BAG_PROFILE_FUTURE",
        "CLR_SUPPLY_PROFILE_MIXED_BAG_PROFILE",
        "CLR_SUPPLY_PROFILE_CUSTOM_BAG_PROFILE"
    )) {
        if ($surface -like "*$forbiddenStableAbi*") {
            Add-ArchitectureError "G2 stable product ABI must not reserve speculative profile '$forbiddenStableAbi'"
        }
    }
$observedValidator = Read-Text "crates/clearra-validation/src/validators/supply_observed_queue_validator.rs"
if ($observedValidator -notlike "*ambiguity_diagnostic*") {
        Add-ArchitectureError "G2 observed window ambiguity must remain reported instead of fixed as exact sequence"
    }
$cache = Read-Text "core-c/src/cache/cache_identity.c"
if ($cache -notlike "*identity.supply_provenance = problem->piece_source.provenance_id*") {
        Add-ArchitectureError "G2 supply provenance must remain part of the C cache identity"
    }
}
