# This file is dot-sourced by an architecture validation wrapper.

function Invoke-SupplyRuntimeValidation() {
foreach ($requiredPath in @(
            "crates/clearra-core-ffi/src/supply/supply_descriptor_compiler.rs",
            "crates/clearra-core-ffi/src/supply/supply_descriptor_compiler_tests.rs",
            "crates/clearra-core-ffi/src/supply/mod.rs",
            "core-c/include/clr_supply.h",
            "core-c/src/supply/queue_view.c",
            "core-c/src/supply/supply_state.c",
            "core-c/src/supply/piece_window.c",
            "core-c/tests/supply_tests.c"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M23 supply product path required file is missing: $requiredPath"
        }
    }
$clrSupply = Read-Text "core-c/include/clr_supply.h"
foreach ($requiredMarker in @(
            "CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE",
            "CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN",
            "CLR_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED",
            "provenance_id",
            "clearra_queue_view_preserves_provenance",
            "clearra_hold_state_occupied"
        )) {
        if ($clrSupply -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M23 clr_supply.h must expose compact queue/hold provenance marker '$requiredMarker'"
        }
    }
$supplyCompiler = @(
        Read-Text "crates/clearra-core-ffi/src/supply/supply_descriptor_compiler.rs"
        Read-Text "crates/clearra-core-ffi/src/supply/supply_descriptor_compiler_tests.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "SupplyDescriptorCompiler",
            "PcQueueInput::FixedSequence",
            "PcQueueInput::BagAlignedPattern",
            "PcQueueInput::Observed",
            "C_SUPPLY_PROVENANCE_FIXED_SEQUENCE",
            "C_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN",
            "C_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED",
            "observed_expansion_remains_rust_owned",
            "fixed_sequence_passed_to_c",
            "fixed_sequence_passed_to_c_queue_view",
            "bag_pattern_passed_to_c",
            "bag_pattern_passed_to_c_queue_view",
            "hold_state_passed_to_c",
            "bag_window_from_queue"
        )) {
        if ($supplyCompiler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M23 SupplyDescriptorCompiler must lower Rust supply into C descriptors marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "QueueParser",
            "parse_fixed_sequence",
            "parse_bag_aligned_pattern",
            "parse_observed_queue",
            "parse_piece_sequence"
        )) {
        if ($supplyCompiler -like "*$forbiddenMarker*") {
            Add-ArchitectureError "M23 SupplyDescriptorCompiler must not own raw supply parsing marker '$forbiddenMarker'"
        }
    }
$cSupplySurface = @(
        Read-Text "core-c/src/supply/queue_view.c"
        Read-Text "core-c/src/supply/supply_state.c"
        Read-Text "core-c/src/supply/piece_window.c"
        Read-Text "core-c/src/reachability/reachability_checker.c"
        Read-Text "core-c/src/candidate/candidate_search_dispatch.c"
    ) -join "`n"
foreach ($forbiddenMarker in @(
            "QueueParser",
            "parse_fixed_sequence",
            "parse_bag_aligned_pattern",
            "parse_observed_queue",
            "parse_piece_sequence"
        )) {
        if ($cSupplySurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "M23 C runtime must not parse raw supply input marker '$forbiddenMarker'"
        }
    }
$packingProblemBuilder = @(
        Read-Text "crates/clearra-core-ffi/src/problem/packing_problem_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/packing_supply_descriptor_builder.rs"
    ) -join "`n"
if ($packingProblemBuilder -notlike "*SupplyDescriptorCompiler::compile(problem)*") {
        Add-ArchitectureError "M23 CPackingProblemBuilder must use SupplyDescriptorCompiler before C execution"
    }
foreach ($forbiddenMarker in @("fn queue_view(", "PcQueueInput::FixedSequence", "PcQueueInput::Observed")) {
        if ($packingProblemBuilder -like "*$forbiddenMarker*") {
            Add-ArchitectureError "M23 CPackingProblemBuilder must not own queue compacting marker '$forbiddenMarker'"
        }
    }
$buildupProblemBuilder = Read-Text "crates/clearra-core-ffi/src/problem/buildup_problem_builder.rs"
foreach ($requiredMarker in @(
            "SupplyDescriptorCompiler::compile(problem)",
            "initial_hold_automaton",
            "piece_source"
        )) {
        if ($buildupProblemBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M23 CBuildUpProblemBuilder must use supply-owned descriptor marker '$requiredMarker'"
        }
    }
$cMake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
            "src/supply/queue_view.c",
            "src/supply/supply_state.c",
            "src/supply/piece_window.c",
            "supply_tests"
        )) {
        if ($cMake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M23 core-c CMake must build supply compact view marker '$requiredMarker'"
        }
    }
$pcService = Get-PcServiceValidationSurface
$setupService = Read-Text "crates/clearra-core-executor/src/service/setup_service.rs"
$coverService = Read-Text "crates/clearra-core-executor/src/service/cover_service.rs"
foreach ($surface in @($pcService, $setupService, $coverService)) {
        foreach ($requiredMarker in @(
                "compact_supply_provenance_id",
                "compact_piece_source_kind",
                "compact_piece_multiset_count"
            )) {
            if ($surface -notlike "*$requiredMarker*") {
                Add-ArchitectureError "M23 executor services must preserve compact supply result marker '$requiredMarker'"
            }
        }
    }
$cacheIdentity = @(
        Read-Text "core-c/src/cache/cache_identity.h"
        Read-Text "core-c/src/cache/cache_identity.c"
        Read-Text "core-c/src/cache/cache_key.c"
        Read-Text "core-c/tests/cache_identity_tests.c"
    ) -join "`n"
foreach ($requiredMarker in @(
            "supply_provenance",
            "queue_pattern_id",
            "piece_window_start",
            "piece_window_len",
            "different_supply_provenance_does_not_share_cache_key",
            "cache_identity_includes_supply_rule_piece_goal"
        )) {
        if ($cacheIdentity -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M23 cache identity must include supply provenance marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "M23 Supply Runtime",
            "PcQueueInput + HoldSlot + PieceWindow -> SupplyDescriptorCompiler -> clr_piece_source_descriptor + clr_piece_multiset_window + clr_hold_automaton_state + clr_piece_window_descriptor",
            "observed expansion remains Rust-owned",
            "C core does not parse raw supply input",
            "compact_supply_provenance_id",
            "compact_piece_source_kind",
            "supply provenance and piece source pattern identity are part of the cache key"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M23 supply product path marker '$requiredMarker'"
        }
    }
}



