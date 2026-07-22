# This file is dot-sourced by an architecture validation wrapper.

function Invoke-SetupSearchProductPathValidation() {
foreach ($requiredPath in @(
            "crates/clearra-setup-search/src/service/setup_core_buildup_gate.rs",
            "crates/clearra-setup-search/src/enumerate/build_variant_enumerator.rs",
            "crates/clearra-setup-search/src/family/mod.rs",
            "crates/clearra-core-domain/src/solution/shape_family.rs",
            "crates/clearra-core-domain/src/solution/tiling_variant.rs",
            "crates/clearra-core-domain/src/solution/build_variant.rs",
            "crates/clearra-core-executor/src/service/setup_service.rs"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M20 setup search product path required file is missing: $requiredPath"
        }
    }
$setupCoreGate = Read-Text "crates/clearra-setup-search/src/service/setup_core_buildup_gate.rs"
foreach ($requiredMarker in @(
            "SetupCoreBuildGate",
            "ProblemCompiler::compile_setup",
            "PackingRunner::run",
            "BuildUpRunner::run",
            "BuildUpVariantProof::new",
            "proof_for_candidate",
            "with_build_input",
            "successful_build_variant_count",
            "coverage_row_count"
        )) {
        if ($setupCoreGate -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M20 SetupCoreBuildGate must bridge SetupQuery through SearchProblem/Packing/BuildUp marker '$requiredMarker'"
        }
    }
$buildVariantEnumerator = Read-Text "crates/clearra-setup-search/src/enumerate/build_variant_enumerator.rs"
foreach ($requiredMarker in @(
            "BuildUpVariantProof",
            "from_core_buildup",
            "proof.has_build_variant",
            "matches_tiling",
            "placed_piece_count",
            "setup_build_variant_generated_through_c_buildup_requires_matching_identity",
            "build_variant_requires_core_buildup_proof"
        )) {
        if ($buildVariantEnumerator -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M20 BuildVariantEnumerator must require C BuildUp proof marker '$requiredMarker'"
        }
    }
if ($buildVariantEnumerator -like "*pub fn from_tiling*") {
        Add-ArchitectureError "M20 BuildVariantEnumerator must not keep direct from_tiling product constructor"
    }
$solutionSurface = @(
        Read-Text "crates/clearra-core-domain/src/solution/shape_family.rs"
        Read-Text "crates/clearra-core-domain/src/solution/tiling_variant.rs"
        Read-Text "crates/clearra-core-domain/src/solution/build_variant.rs"
        Read-Text "crates/clearra-setup-search/src/family/mod.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "ShapeFamily",
            "ShapeKey",
            "VisualGroupKey",
            "TilingVariant",
            "PieceCountVector",
            "OperationPlacement",
            "CellPartitionKey",
            "TilingKey",
            "BuildVariant",
            "OperationSetKey",
            "operation_order",
            "hold_decisions",
            "ReachabilityEvidence",
            "same_shape_preserves_distinct_tiling_variants",
            "shape_key_does_not_drop_tiling_variant",
            "same_tiling_preserves_distinct_build_orders",
            "tiling_key_does_not_drop_build_variant",
            "same_mask_different_piece_definition_not_same_tiling",
            "loj_jol_same_shape_distinct_build_variants",
            "lsj_same_shape_distinct_tiling_variant",
            "loj_jol_lsj_fixtures_preserved",
            "shape_family_not_used_as_probability_source"
        )) {
        if ($solutionSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "K Shape/Tiling/BuildVariant contract marker is missing: '$requiredMarker'"
        }
    }
$setupSearchService = (Read-Text "crates/clearra-setup-search/src/service/setup_search_service.rs") + "`n" +
    (Read-Text "crates/clearra-setup-search/src/service/setup_search_service_tests.rs")
foreach ($requiredMarker in @(
            "SetupCoreBuildGate::from_query",
            "BuildVariantEnumerator::from_core_buildup",
            "proof_for_candidate",
            "core_packing_buildup_build_variants_attached",
            "SetupQuery->SearchProblem->C PackingProblem->C PackingResult->C BuildUpResult",
            "build_variant_source",
            "C BuildUp",
            "packing_candidate_count",
            "core_buildup_variant_count",
            "core_coverage_row_count",
            "shape_family_id",
            "shape_family_count",
            "tiling_variant_count",
            "build_variant_count",
            "coverage_source",
            "coverage_pattern_count",
            "verified_pattern_count",
            "materialized_pattern_count",
            "covered_pattern_count",
            "covered_pattern_count_basis",
            "observed-materialized-pattern-specific",
            "materialized_pattern_universe",
            "coverage_probability",
            "probability_complete",
            "queue_prefix",
            "queue_prefix_len",
            "hold_required",
            "hold_piece",
            "bag_boundary_offsets",
            "bag_boundary_ambiguous",
            "requires_180",
            "rule_profile_evidence",
            "post_pc_solution_count",
            "score_basis",
            "backend_report",
            "raw_coverage_export_path",
            "setup_raw_metrics",
            "setup_raw_coverage_export",
            "condition_summary_field_absent",
            "setup_raw_coverage_export_is_machine_readable"
        )) {
        if ($setupSearchService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M20 SetupSearchService must use C Packing/BuildUp for build variants marker '$requiredMarker'"
        }
    }
$setupCoveragePlan = Read-Text "crates/clearra-setup-search/src/service/setup_coverage_plan.rs"
$setupProbability = Read-Text "crates/clearra-setup-search/src/coverage/setup_probability.rs"
foreach ($requiredMarker in @(
            "SetupCoverageBuilder",
            "SetupUnionCoverage",
            "PatternBitSet::from_patterns",
            "build_union",
            "setup_probability_uses_pattern_bitset_union"
        )) {
        if ("$setupCoveragePlan`n$setupProbability" -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M20 setup coverage must keep PatternBitSet OR union path marker '$requiredMarker'"
        }
    }
$postPcEvaluator = Read-Text "crates/clearra-setup-search/src/evaluate/post_pc_evaluator.rs"
$postPcInput = Read-Text "crates/clearra-setup-search/src/evaluate/post_pc_scenario_input.rs"
foreach ($requiredMarker in @(
            "PostPcScenarioInput",
            "PcScenarioQuery",
            "ProblemCompiler::compile_scenario_pc",
            "CoreExecutor::execute",
            "setup_post_pc_compiles_to_scenario_preset"
        )) {
        if ("$postPcEvaluator`n$postPcInput" -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M20 post-setup PC must remain a SearchProblem scenario preset marker '$requiredMarker'"
        }
    }
$setupExecutorService = Read-Text "crates/clearra-core-executor/src/service/setup_service.rs"
foreach ($requiredMarker in @(
            "PackingRunner::run",
            "BuildUpRunner::run",
            "m20-setup-search-product-path",
            "shape-family-tiling-build-core-buildup",
            "build_variant_source",
            "core_buildup_variant_count",
            "core_coverage_row_count",
            "shape_family_id",
            "shape_family_count",
            "tiling_variant_count",
            "build_variant_count",
            "coverage_source",
            "coverage_pattern_count",
            "verified_pattern_count",
            "materialized_pattern_count",
            "covered_pattern_count",
            "covered_pattern_count_basis",
            "observed_verified_pattern_count_reported",
            "coverage_probability",
            "probability_complete",
            "queue_prefix",
            "queue_prefix_len",
            "hold_required",
            "hold_piece",
            "bag_boundary_offsets",
            "bag_boundary_ambiguous",
            "requires_180",
            "rule_profile_evidence",
            "post_pc_solution_count",
            "score_basis",
            "backend_report",
            "raw_coverage_export_path",
            "setup_raw_metrics",
            "setup_raw_coverage_export"
        )) {
        if ($setupExecutorService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M20 CoreExecutor setup service must expose Packing/BuildUp product path marker '$requiredMarker'"
        }
    }
$setupOutputContract = Read-Text "crates/clearra-output/src/json/setup_json_contract.rs"
foreach ($requiredMarker in @(
            "shape_family_count",
            "queue_prefix",
            "queue_prefix_len",
            "hold_required",
            "hold_piece",
            "bag_boundary_offsets",
            "bag_boundary_ambiguous",
            "requires_180",
            "requires_180_evidence",
            "rule_profile_evidence"
        )) {
        if ($setupOutputContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M10 setup JSON raw metrics contract must expose marker '$requiredMarker'"
        }
    }
foreach ($filePath in @(
            "crates/clearra-core-executor/src/service/setup_service.rs",
            "crates/clearra-output/src/json/setup_json_contract.rs"
        )) {
        $contents = Read-Text $filePath
        foreach ($forbiddenMarker in @("condition_summary", "setup_condition_summary", "ConditionSummary")) {
            if ($contents.Contains($forbiddenMarker)) {
                Add-ArchitectureError "$filePath must not reintroduce setup condition summary marker '$forbiddenMarker' after M10"
            }
        }
    }
foreach ($file in Get-RustFiles "crates/clearra-setup-search/src") {
        $relativePath = Get-NormalizedRelativePath $file
        if (Test-GeneratedOrTestRustFile $file) { continue }
        $contents = Get-RustProductionContents (Get-Content -LiteralPath $file.FullName -Raw)
        foreach ($forbiddenMarker in @("condition_summary", "ConditionSummary")) {
            if ($contents.Contains($forbiddenMarker)) {
                Add-ArchitectureError "$relativePath must not reintroduce setup condition summary marker '$forbiddenMarker' after M20"
            }
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "M20 Setup Search Product Path",
            "SetupQuery -> ShapeFamily candidates -> TilingVariant candidates -> SearchProblem -> C Packing -> C BuildUp -> BuildVariant -> PatternBitSet coverage -> setup union probability -> raw metrics/export",
            "SetupCoreBuildGate",
            "BuildUpVariantProof",
            "BuildVariantEnumerator::from_core_buildup",
            "SetupCoreBuildGate::proof_for_candidate",
            "occupied shape",
            "hold requirement",
            "placed piece count",
            "PatternBitSet OR semantics",
            "condition_summary is absent",
            "X3 MVP2 Setup Raw Metrics",
            "shape_family_id",
            "shape_family_count",
            "tiling_variant_count",
            "build_variant_count",
            "covered_pattern_count",
            "coverage_probability",
            "queue_prefix",
            "queue_prefix_len",
            "hold_required",
            "hold_piece",
            "bag_boundary_offsets",
            "bag_boundary_ambiguous",
            "requires_180",
            "rule_profile_evidence",
            "post_pc_solution_count",
            "score_basis",
            "backend_report",
            "raw_coverage_export_path",
            "raw metrics sufficient for filtering",
            "GUI setup explorer can consume schema"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M20 setup search product path marker '$requiredMarker'"
        }
    }
}



