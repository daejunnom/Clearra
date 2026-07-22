function Invoke-CCoverageRowBridgeValidation() {
    
foreach ($requiredPath in @(
        "core-c/include/clr_coverage.h",
        "core-c/src/coverage/pattern_bitset_c.c",
        "core-c/src/coverage/coverage_row_builder.c",
        "core-c/src/coverage/coverage_union.c",
        "core-c/src/coverage/coverage_overlap.c",
        "core-c/src/buildup/build_order_language.c",
        "core-c/src/buildup/hold_reachable_language.c",
        "core-c/src/buildup/language_intersection.c",
        "core-c/tests/coverage_tests.c",
        "crates/clearra-core-ffi/src/buildup/coverage_row_view.rs",
        "crates/clearra-core-executor/src/order_language/build_order_language.rs",
        "crates/clearra-core-executor/src/order_language/hold_reachable_language.rs",
        "crates/clearra-core-executor/src/order_language/language_intersection.rs",
        "crates/clearra-core-executor/src/buildup/buildup_coverage_bridge.rs",
        "crates/clearra-coverage/src/matrix/coverage_row_bridge.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M14 Coverage Row Bridge required file is missing: $requiredPath"
        }
    }
$coverageHeader = Read-Text "core-c/include/clr_coverage.h"
foreach ($requiredMarker in @(
        "clr_pattern_bitset_c",
        "clr_coverage_row_view",
        "clr_coverage_row_kind",
        "pattern_universe_id",
        "pattern_weight_model_id",
        "row_kind",
        "CLR_COVERAGE_CAPACITY_EXCEEDED",
        "CLR_COVERAGE_WEIGHT_MODEL_MISMATCH",
        "CLR_COVERAGE_ROW_KIND_UNSUPPORTED",
        "clr_coverage_overlap_report_c",
        "clr_pattern_bitset_union_checked",
        "clr_coverage_pattern_verification",
        "clr_coverage_row_from_verified_build_variant_with_identity",
        "CLR_COVERAGE_PATTERN_NOT_VERIFIED",
        "clr_coverage_test_row_from_build_variant_without_identity",
        "CLEARRA_CORE_TEST",
        "clr_coverage_union_rows",
        "clr_coverage_overlap_count"
    )) {
        if ($coverageHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clr_coverage.h must expose M14 marker '$requiredMarker'"
        }
    }
$patternBitsetC = Read-Text "core-c/src/coverage/pattern_bitset_c.c"
foreach ($requiredMarker in @("CLR_COVERAGE_PATTERN_UNIVERSE_MISMATCH", "CLR_COVERAGE_WEIGHT_MODEL_MISMATCH", "CLR_COVERAGE_PATTERN_OUT_OF_RANGE", "clr_pattern_bitset_count_ones", "pattern_universe_id", "pattern_weight_model_id")) {
        if ($patternBitsetC -notlike "*$requiredMarker*") {
            Add-ArchitectureError "pattern_bitset_c.c must implement M14 bitset marker '$requiredMarker'"
        }
    }
$rowBuilderRoot = Join-Path $Root "core-c/src/coverage"
$rowBuilder = @(
    (Read-Text "core-c/src/coverage/coverage_row_builder.c")
    (Get-ChildItem -LiteralPath $rowBuilderRoot -Recurse -File -Filter *.c |
        Where-Object { $_.FullName -like "*coverage_row_builder_functions*" } |
        Sort-Object FullName |
        ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw })
) -join "`n"
foreach ($requiredMarker in @("clr_coverage_row_from_verified_build_variant_with_identity", "CLR_COVERAGE_INVALID_ARGUMENT", "verification->pattern_id", "verification->accepted", "variant->candidate_id", "variant->canonical_operation_set_id", "CLR_COVERAGE_ROW_KIND_BUILD", "piece_source_id", "pattern_universe_id", "pattern_weight_model_id", "clr_pattern_bitset_insert")) {
        if ($rowBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "coverage_row_builder.c must build stable M14 row marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
        "clr_coverage_row_from_build_variant(",
        "clr_coverage_row_from_build_variant_with_identity",
        "pattern_universe_id = UINT64_C(0)",
        "pattern_weight_model_id = UINT64_C(0)",
        "out_row->candidate_id = variant->operation_set_hash"
    )) {
        if ($rowBuilder -like "*$forbiddenMarker*") {
            Add-ArchitectureError "coverage_row_builder.c must not expose identity-less product row builder marker '$forbiddenMarker'"
        }
    }
$coverageUnion = Read-Text "core-c/src/coverage/coverage_union.c"
foreach ($requiredMarker in @("clr_coverage_union_rows", "clr_pattern_bitset_union_checked", "CLR_COVERAGE_WEIGHT_MODEL_MISMATCH", "CLR_COVERAGE_PIECE_SOURCE_MISMATCH", "CLR_COVERAGE_ROW_KIND_UNSUPPORTED", "piece_source_id", "row_kind")) {
        if ($coverageUnion -notlike "*$requiredMarker*") {
            Add-ArchitectureError "coverage_union.c must implement M14 OR union marker '$requiredMarker'"
        }
    }
$coverageOverlap = Read-Text "core-c/src/coverage/coverage_overlap.c"
foreach ($requiredMarker in @("clr_coverage_overlap_count", "overlap_count", "has_overlap", "CLR_COVERAGE_WEIGHT_MODEL_MISMATCH")) {
        if ($coverageOverlap -notlike "*$requiredMarker*") {
            Add-ArchitectureError "coverage_overlap.c must implement M14 overlap marker '$requiredMarker'"
        }
    }
$buildOrderLanguageC = Read-Text "core-c/src/buildup/build_order_language.c"
foreach ($requiredMarker in @("clearra_build_order_language_add_order", "clearra_build_order_language_accepts_order", "clearra_build_order_language_order_count", "CLEARRA_BUILD_ORDER_LANGUAGE_MAX_ORDERS")) {
        if ($buildOrderLanguageC -notlike "*$requiredMarker*") {
            Add-ArchitectureError "build_order_language.c must implement L order-language marker '$requiredMarker'"
        }
    }
$holdReachableLanguageC = Read-Text "core-c/src/buildup/hold_reachable_language.c"
foreach ($requiredMarker in @("clearra_hold_reachable_language_add_order", "clearra_hold_reachable_language_accepts_order", "clearra_hold_reachable_language_supports_long_carryover", "bag_remainder_key")) {
        if ($holdReachableLanguageC -notlike "*$requiredMarker*") {
            Add-ArchitectureError "hold_reachable_language.c must implement L hold-language marker '$requiredMarker'"
        }
    }
$languageIntersectionC = Read-Text "core-c/src/buildup/language_intersection.c"
foreach ($requiredMarker in @("clearra_explicit_order_scaffold_non_empty", "clearra_explicit_order_scaffold_coverage_bits", "clr_pattern_bitset_insert")) {
        if ($languageIntersectionC -notlike "*$requiredMarker*") {
            Add-ArchitectureError "language_intersection.c must implement L coverage intersection marker '$requiredMarker'"
        }
    }
$coverageRowBuilderC = Read-Text "core-c/src/coverage/coverage_row_builder.c"
foreach ($requiredMarker in @(
        "clearra_test_coverage_row_from_explicit_order_scaffold_with_identity",
        "clearra_explicit_order_scaffold_coverage_bits",
        "CLEARRA_EXPLICIT_ORDER_SCAFFOLD_PATTERN_ID",
        "CLR_COVERAGE_PIECE_SOURCE_MISMATCH"
    )) {
        if ($coverageRowBuilderC -notlike "*$requiredMarker*") {
            Add-ArchitectureError "coverage_row_builder.c must keep the explicit-order test scaffold marker '$requiredMarker'"
        }
    }
$publicCoverageHeader = Read-Text "core-c/include/clr_coverage.h"
foreach ($forbiddenPublicScaffold in @(
        "ClearraBuildOrderLanguage",
        "ClearraHoldReachableLanguage",
        "row_from_language_intersection",
        "row_from_explicit_order_scaffold",
        "PATTERN_ID_LANGUAGE_INTERSECTION",
        "EXPLICIT_ORDER_SCAFFOLD_PATTERN_ID"
    )) {
        if ($publicCoverageHeader -like "*$forbiddenPublicScaffold*") {
            Add-ArchitectureError "clr_coverage.h must not expose test-only language scaffold '$forbiddenPublicScaffold'"
        }
    }
foreach ($coverageSourcePath in @(
        "core-c/src/coverage/pattern_bitset_c.c",
        "core-c/src/coverage/coverage_row_builder.c",
        "core-c/src/coverage/coverage_union.c",
        "core-c/src/coverage/coverage_overlap.c"
    )) {
        $contents = Read-Text $coverageSourcePath
        foreach ($forbiddenMarker in @("probability", "ProbabilityValue", "probability_numerator", "probability_denominator")) {
            if ($contents -like "*$forbiddenMarker*") {
                Add-ArchitectureError "$coverageSourcePath must not own probability calculation marker '$forbiddenMarker'; probability belongs to Rust clearra-coverage"
            }
        }
    }
$ffiCoverage = Read-Text "crates/clearra-core-ffi/src/buildup/coverage_row_view.rs"
foreach ($requiredMarker in @("CCoverageRowView", "CPatternBitSet", "C_COVERAGE_MAX_WORDS", "piece_source_id", "pattern_universe_id", "pattern_weight_model_id", "row_kind", "single_pattern_with_identity", "product_from_build_variant_with_identity", "c_coverage_row_view_can_be_read_by_rust_coverage_layer", "coverage_row_identity_roundtrips_from_native_build_variant")) {
        if ($ffiCoverage -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi coverage_row_view.rs must mirror M14 C row marker '$requiredMarker'"
        }
    }
$coreExecutorLib = Read-Text "crates/clearra-core-executor/src/lib.rs"
if (-not $coreExecutorLib.Contains("#[cfg(test)]") -or
    -not $coreExecutorLib.Contains("pub(crate) mod order_language")) {
        Add-ArchitectureError "clearra-core-executor must keep the explicit-order language scaffold test-only until independent language generation exists"
}
if ($coreExecutorLib -like "*pub mod order_language*") {
        Add-ArchitectureError "clearra-core-executor must not export the explicit-order language scaffold as product authority"
}
$buildOrderLanguageRust = Read-Text "crates/clearra-core-executor/src/order_language/build_order_language.rs"
foreach ($requiredMarker in @("BuildOrderLanguage", "OperationDependencyGraph", "LineClearConstraintSet", "ReachabilityConstraintSet", "build_orders_not_representative_only")) {
        if ($buildOrderLanguageRust -notlike "*$requiredMarker*") {
            Add-ArchitectureError "build_order_language.rs must implement L marker '$requiredMarker'"
        }
    }
$holdReachableLanguageRust = Read-Text "crates/clearra-core-executor/src/order_language/hold_reachable_language.rs"
foreach ($requiredMarker in @("HoldReachableLanguage", "HoldTransitionGraph", "supports_long_carryover", "hold_reachable_orders_support_long_carryover")) {
        if ($holdReachableLanguageRust -notlike "*$requiredMarker*") {
            Add-ArchitectureError "hold_reachable_language.rs must implement L marker '$requiredMarker'"
        }
    }
$languageIntersectionRust = Read-Text "crates/clearra-core-executor/src/order_language/language_intersection.rs"
foreach ($requiredMarker in @("LanguageIntersection", "coverage_bits_for_candidate", "language_intersection_empty_rejects_pattern", "language_intersection_non_empty_sets_coverage_bit", "same_pattern_multiple_variants_counted_once")) {
        if ($languageIntersectionRust -notlike "*$requiredMarker*") {
            Add-ArchitectureError "language_intersection.rs must implement L marker '$requiredMarker'"
        }
    }
$buildupCoverageBridge = (Read-Text "crates/clearra-core-executor/src/buildup/buildup_coverage_bridge.rs") + "`n" +
    (Read-Text "crates/clearra-core-executor/src/buildup/buildup_coverage_bridge_tests.rs")
foreach ($requiredMarker in @(
        "WitnessedPatternCoverageAccumulator",
        "record_verified_variant",
        "into_coverage_bits",
        "PatternVerifiedBuildVariant",
        "CoveragePatternVerificationMismatch",
        "witnessed_pattern_coverage_accumulator_exposes_candidate_identity",
        "verified_pattern_buildup_sets_coverage_bit_directly",
        "two_verified_patterns_set_two_coverage_bits",
        "duplicate_verified_variants_set_one_pattern_bit"
    )) {
        if ($buildupCoverageBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "buildup_coverage_bridge.rs must derive coverage directly from pattern-specific BuildUp evidence marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
        "LanguageIntersection",
        "BuildOrderLanguage",
        "HoldReachableLanguage",
        "operation_id_for_accepted_variant_coverage_witness",
        "CoverageVerificationSource::LanguageIntersection",
        "CCoverageRowView::product_from_build_variant_with_identity",
        "coverage_row_from_raw_words_with_identity_and_piece_source"
    )) {
        if ($buildupCoverageBridge -like "*$forbiddenMarker*") {
            Add-ArchitectureError "buildup_coverage_bridge.rs must not treat synthetic language or native single-pattern rows as product coverage authority marker '$forbiddenMarker'"
        }
    }
foreach ($identitylessHelperName in @("pub fn single_pattern(", "pub fn from_build_variant(")) {
        $helperIndex = $ffiCoverage.IndexOf($identitylessHelperName, [System.StringComparison]::Ordinal)
        if ($helperIndex -ge 0) {
            $cfgIndex = $ffiCoverage.LastIndexOf("#[cfg(test)]", $helperIndex, [System.StringComparison]::Ordinal)
            if ($cfgIndex -lt 0 -or ($helperIndex - $cfgIndex) -gt 120) {
                Add-ArchitectureError "clearra-core-ffi coverage_row_view.rs identity-less helper '$identitylessHelperName' must remain cfg(test)-only"
            }
        }
    }
foreach ($productRustPath in @(
        "crates/clearra-core-executor/src/buildup/buildup_runner.rs",
        "crates/clearra-core-executor/src/spin/spin_target_coverage_bridge.rs",
        "crates/clearra-core-executor/src/spin/spin_target_runner.rs"
    )) {
        $productRustText = Read-Text $productRustPath
        foreach ($forbiddenMarker in @(
            "CCoverageRowView::single_pattern(",
            "CCoverageRowView::from_build_variant(",
            "coverage_row_from_raw_words("
        )) {
            if ($productRustText -like "*$forbiddenMarker*") {
                Add-ArchitectureError "$productRustPath must use identity-aware coverage row bridge; forbidden marker '$forbiddenMarker'"
            }
        }
        if ($productRustText -like "*CCoverageRowView*" -and
            $productRustText -notlike "*product_from_build_variant_with_identity*") {
            Add-ArchitectureError "$productRustPath must use CCoverageRowView::product_from_build_variant_with_identity when reading native build variants"
        }
    }
$coverageBridge = Read-Text "crates/clearra-coverage/src/matrix/coverage_row_bridge.rs"
foreach ($requiredMarker in @(
        "coverage_row_from_raw_words_with_identity",
        "coverage_row_from_raw_words_with_identity_and_piece_source",
        "MissingPieceSourceIdentity",
        "MissingPatternUniverseIdentity",
        "MissingPatternWeightModelIdentity",
        "CoverageRowKind",
        "PatternUniverseId",
        "PatternWeightModelId",
        "TailBitsOutsidePatternUniverse",
        "identity_aware_bridge_reads_bits_when_identity_is_explicit",
        "coverage_row_candidate_id_is_stable_across_reads",
        "or_union_works_for_rows_read_from_raw_c_words",
        "union_probability_from_bridge_rows_never_exceeds_one"
    )) {
        if ($coverageBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-coverage coverage_row_bridge.rs must implement M14 Rust bridge marker '$requiredMarker'"
        }
    }
$coverageRustIdentity = (Read-Text "crates/clearra-coverage/src/row/coverage_row.rs") + "`n" +
    (Read-Text "crates/clearra-coverage/src/row/score_cell_row.rs") + "`n" +
    (Read-Text "crates/clearra-coverage/src/row/spin_coverage_row.rs") + "`n" +
    (Read-Text "crates/clearra-coverage/src/matrix/coverage_matrix.rs") + "`n" +
    (Read-Text "crates/clearra-coverage/src/matrix/score_cell_matrix.rs") + "`n" +
    (Read-Text "crates/clearra-coverage/src/matrix/spin_coverage_matrix.rs") + "`n" +
    (Read-Text "crates/clearra-coverage/src/coverage_contract_tests.rs")
foreach ($requiredMarker in @(
        "new_without_piece_source_for_test",
        "MissingPieceSourceIdentity",
        "product_coverage_row_requires_piece_source_id",
        "score_cell_row_requires_piece_source_id",
        "spin_coverage_row_requires_piece_source_id",
        "identityless_coverage_row_constructor_is_test_only",
        "coverage_union_rejects_piece_source_mismatch"
    )) {
        if ($coverageRustIdentity -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-coverage Rust identity contract must implement marker '$requiredMarker'"
        }
    }
$coverageCargo = Read-Text "crates/clearra-coverage/Cargo.toml"
if ($coverageCargo -like "*clearra-core-ffi*") {
        Add-ArchitectureError "clearra-coverage must not depend on clearra-core-ffi; read raw words so coverage remains above ABI without a dependency cycle"
    }
$cmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
        "src/coverage/pattern_bitset_c.c",
        "src/coverage/coverage_row_builder.c",
        "src/coverage/coverage_union.c",
        "src/coverage/coverage_overlap.c",
        "src/buildup/build_order_language.c",
        "src/buildup/hold_reachable_language.c",
        "src/buildup/language_intersection.c",
        "coverage_tests",
        "CLEARRA_CORE_TEST"
    )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must build M14 source/test marker '$requiredMarker'"
        }
    }
$coverageTests = Read-Text "core-c/tests/coverage_tests.c"
foreach ($requiredMarker in @("coverage_row_builder_uses_stable_candidate_id", "coverage_row_builder_requires_identity", "product_coverage_row_rejects_zero_piece_source_id", "coverage_row_builder_allows_zero_identity_only_in_test_helper", "test_helper_identityless_row_not_exported_in_public_product_path", "clr_coverage_row_from_verified_build_variant_with_identity", "clr_coverage_test_row_from_build_variant_without_identity", "clearra_test_coverage_row_from_explicit_order_scaffold_with_identity", "coverage_row_requires_pattern_specific_buildup_or_intersection", "coverage_row_requires_pattern_specific_validation", "verify_first_cannot_source_coverage", "coverage_pattern_id_injection_without_pattern_verification_rejected", "coverage_row_intersection_rejects_piece_source_mismatch", "pattern_bitset_universe_mismatch_is_error", "pattern_weight_model_mismatch_is_error", "coverage_row_union_uses_or_semantics", "coverage_row_union_rejects_unsupported_row_kind", "coverage_union_rejects_piece_source_mismatch", "same_pattern_universe_different_piece_source_not_or_merged", "coverage_overlap_reports_duplicate_patterns", "build_orders_not_representative_only", "hold_reachable_orders_support_long_carryover", "explicit_order_scaffold_empty_rejects_pattern", "explicit_order_scaffold_empty_does_not_set_bit", "explicit_order_scaffold_non_empty_sets_coverage_bit", "explicit_order_scaffold_non_empty_sets_bit", "same_pattern_multiple_variants_counted_once")) {
        if ($coverageTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/tests/coverage_tests.c must verify M14 marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M14 Coverage Row Bridge", "C coverage row view can be read from Rust", "PatternBitSet universe checked", "coverage row candidate id stable", "OR union works", "probability never exceeds 1.0", "L BuildOrders / HoldReachableOrders / Language Intersection Coverage", "BuildOrders(P) intersects HoldReachableOrders(Q)", "WitnessedPatternCoverageAccumulator", "pattern-specific BuildUp", "test-only scaffold", "not product coverage or pruning authority")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M14 marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("C coverage row bridging", "clr_coverage_row_view", "coverage_row_from_raw_words_with_identity", "TypedCoverageMatrix", "Final probability and objective selection remain in Rust", "order language intersection coverage", "BuildOrders(P)", "HoldReachableOrders(Q)", "WitnessedPatternCoverageAccumulator", "pattern-specific BuildUp", "test-only scaffold", "not product coverage or pruning authority", "same pattern multiple variants counted once")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M14 marker '$requiredMarker'"
        }
    }

}
function Invoke-RustCoverageObjectiveReducerValidation() {
foreach ($requiredPath in @(
        "crates/clearra-coverage/src/reducer/mod.rs",
        "crates/clearra-coverage/src/reducer/coverage_probability_reducer.rs",
        "crates/clearra-objectives/src/reducer/mod.rs",
        "crates/clearra-objectives/src/reducer/objective_reducer.rs",
        "crates/clearra-objectives/src/reducer/dominance_reducer.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M15 Rust Coverage / Objective Reducer required file is missing: $requiredPath"
        }
    }
$coverageLib = Read-Text "crates/clearra-coverage/src/lib.rs"
if ($coverageLib -notlike "*pub mod reducer*") {
        Add-ArchitectureError "clearra-coverage must export M15 reducer module"
    }
$coverageReducer = Read-Text "crates/clearra-coverage/src/reducer/coverage_probability_reducer.rs"
foreach ($requiredMarker in @(
        "CoverageProbabilityReducer",
        "family_probability",
        "TypedCoverageMatrix",
        "matrix.union_all",
        "union_probability",
        "variant_coverage_is_not_summed_for_duplicate_patterns",
        "coverage_union_does_not_sum_variant_probability",
        "family_probability_uses_or_union_not_row_probability_sum",
        "family_probability_uses_pattern_bitset_or",
        "probability_never_exceeds_one"
    )) {
        if ($coverageReducer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "coverage_probability_reducer.rs must implement M15 marker '$requiredMarker'"
        }
    }
$objectivesLib = Read-Text "crates/clearra-objectives/src/lib.rs"
if ($objectivesLib -notlike "*pub mod reducer*") {
        Add-ArchitectureError "clearra-objectives must export M15 reducer module"
    }
$objectiveReducer = Read-Text "crates/clearra-objectives/src/reducer/objective_reducer.rs"
foreach ($requiredMarker in @(
        "ObjectiveReducer",
        "ObjectiveCandidate",
        "ObjectiveCoverageIdentity",
        "TypedCoverageMatrix",
        "stable_canonical_key",
        "AllCollector",
        "UniqueCollector",
        "MinimumCoverObjective",
        "MaxScoreCover",
        "DominanceReducer",
        "retained_trace_count",
        "total_solution_count",
        "objective_reducer_uses_or_probability_not_variant_sum",
        "minimum_cover_works_on_coverage_matrix",
        "unique_result_uses_stable_canonical_key",
        "retained_trace_count_is_separate_from_total_count"
    )) {
        if ($objectiveReducer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "objective_reducer.rs must implement M15 marker '$requiredMarker'"
        }
    }
foreach ($productCoveragePath in @(
        "crates/clearra-core-executor/src/buildup/buildup_runner.rs",
        "crates/clearra-build-coverage/src/coverage/build_coverage_matrix.rs",
        "crates/clearra-build-coverage/src/coverage/build_coverage_executor.rs",
        "crates/clearra-build-coverage/src/coverage/build_union_coverage.rs",
        "crates/clearra-setup-search/src/coverage/setup_coverage_builder.rs",
        "crates/clearra-setup-search/src/coverage/setup_union_coverage.rs",
        "crates/clearra-objectives/src/reducer/objective_reducer.rs",
        "crates/clearra-objectives/src/cover/minimum_cover_objective.rs",
        "crates/clearra-objectives/src/cover/cover_candidate_ranker.rs",
        "crates/clearra-coverage/src/reducer/coverage_probability_reducer.rs",
        "crates/clearra-coverage/src/cover/minimum_cover_solver.rs"
    )) {
        $productCoverageText = Read-Text $productCoveragePath
        foreach ($forbiddenMarker in @(
            "coverage_matrix::CoverageMatrix,",
            "coverage_matrix::{CoverageMatrix,",
            "coverage_matrix::{CoverageMatrix}",
            "matrix::coverage_row::CoverageRow",
            "coverage_row_from_raw_words("
        )) {
            if ($productCoverageText -like "*$forbiddenMarker*") {
                Add-ArchitectureError "$productCoveragePath must not use removed coverage marker '$forbiddenMarker' in production probability/objective paths"
            }
        }
    }
$dominanceReducer = Read-Text "crates/clearra-objectives/src/reducer/dominance_reducer.rs"
foreach ($requiredMarker in @("DominanceReducer", "is_superset", "dominance_reducer_removes_covered_weaker_candidates")) {
        if ($dominanceReducer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "dominance_reducer.rs must implement M15 marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M15 Rust Coverage / Objective Reducer", "Variant coverage is not summed", "family probability uses OR union", "stable canonical keys", "total solution count and retained trace count as separate fields")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M15 marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("Rust coverage/objective reduction", "CoverageProbabilityReducer::family_probability", "ObjectiveReducer::reduce", "retained trace count separated from total count")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M15 marker '$requiredMarker'"
        }
    }
}
