# This file is dot-sourced by an architecture validation wrapper.

function Invoke-PathPercentCoverCliContractValidation() {
foreach ($requiredPath in @(
            "crates/clearra-core-executor/src/service/percent_service.rs",
            "crates/clearra-app/src/commands/path_app_command.rs",
            "crates/clearra-core-executor/src/service/cover_service.rs",
            "crates/clearra-cli/src/output/summary_render_contract.rs",
            "crates/clearra-cli/tests/product_cli_surface_contract.rs",
            "tests/golden/product/percent_bag_pattern.json",
            "tests/golden/product/path_representative.json",
            "tests/golden/product/cover_template_basic.json",
            "scripts/path-percent-cover-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "X6 Path / Percent / Cover CLI required file is missing: $requiredPath"
        }
    }
$percentService = (Read-Text "crates/clearra-core-executor/src/service/percent_service.rs") + "`n" +
    (Read-Text "crates/clearra-core-executor/src/service/percent_service_tests.rs")
foreach ($requiredMarker in @(
            "queue pattern universe -> multiset-grouped C Packing -> pattern-specific C BuildUp coverage rows -> PatternBitSet union -> weighted probability",
            "ObservedQueueExpansion::expand",
            "ProblemCompiler::compile_scenario_pc",
            "BuildUpRunner::run",
            "PatternBitSet",
            "union_probability",
            "total_pattern_count",
            "covered_pattern_count",
            "verified_pattern_count",
            "materialized_pattern_count",
            "covered_pattern_count_basis",
            "observed-materialized-pattern-specific",
            "materialized_pattern_universe",
            "probability_complete",
            "renormalized",
            "percent_reports_total_pattern_count",
            "percent_reports_covered_pattern_count",
            "percent_reports_probability_complete",
            "observed_coverage_verifies_all_materialized_patterns_when_complete",
            "bag_aligned_pattern_universe_not_collapsed_to_pattern_zero_when_materialized",
            "percent_not_ranked_by_pattern_zero_only_when_complete_requested"
        )) {
        if ($percentService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X6 PercentService must expose workflow marker '$requiredMarker'"
        }
    }
$pathCommand = Read-Text "crates/clearra-app/src/commands/path_app_command.rs"
foreach ($requiredMarker in @(
            "SearchProblem -> C Packing / BuildUp -> representative replay -> retained trace -> output",
            "path_reports_representative_trace",
            "retained_representative_trace",
            "representative_trace_source",
            "total_solution_count",
            "unique_solution_count",
            "retained_trace_count",
            "solution_trace_count",
            "trace_retention_truncated",
            "trace_retention_reason",
            "path_distinguishes_retained_trace_from_total_count"
        )) {
        if ($pathCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X6 PathAppCommand must expose retained trace marker '$requiredMarker'"
        }
    }
$coverService = Read-Text "crates/clearra-core-executor/src/service/cover_service.rs"
foreach ($requiredMarker in @(
            "BuildTemplate -> SlotDomain -> SlotAssignment -> BuildUpProblem -> C BuildUp -> CoverageRow -> CoverageMatrix -> UnionProbability",
            "BuildCoverageExecution::from_c_buildup_rows",
            "c_coverage_row_count",
            "coverage_reducer",
            "pattern-bitset-union",
            "union_probability_reducer",
            "cover_reports_union_probability",
            "cover_reports_c_coverage_row_count",
            "slot_assignment_count_is_not_success_probability",
            "success_probability_source"
        )) {
        if ($coverService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X6 CoverService must expose build coverage marker '$requiredMarker'"
        }
    }
$summaryContract = Read-Text "crates/clearra-cli/src/output/summary_render_contract.rs"
foreach ($requiredMarker in @(
            "probability_complete",
            "renormalized",
            "truncation_reason",
            "path_reports_representative_trace",
            "path_distinguishes_retained_trace_from_total_count",
            "cover_reports_union_probability",
            "cover_reports_c_coverage_row_count",
            "slot_assignment_count_is_not_success_probability",
            "success_probability_source"
        )) {
        if ($summaryContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X6 SummaryRenderContract must type marker '$requiredMarker'"
        }
    }
$productE2E = Read-Text "crates/clearra-cli/tests/product_cli_surface_contract.rs"
foreach ($requiredMarker in @(
            "percent_reports_total_and_covered_pattern_count",
            "probability_complete",
            "renormalized",
            "path_reports_representative_trace",
            "path_distinguishes_retained_trace_from_total_count",
            "cover_reports_build_union_probability",
            "cover_reports_union_probability",
            "cover_reports_c_coverage_row_count",
            "slot_assignment_count_is_not_success_probability"
        )) {
        if ($productE2E -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X6 product contract E2E must assert marker '$requiredMarker'"
        }
    }
foreach ($golden in @(
            "tests/golden/product/percent_bag_pattern.json",
            "tests/golden/product/path_representative.json",
            "tests/golden/product/cover_template_basic.json"
        )) {
        $contents = Read-Text $golden
        foreach ($forbiddenMarker in @(
                "observed_truncated_renormalized=true",
                "retained_sample_is_all_paths=true",
                "slot_assignment_count_as_success_probability=true"
            )) {
            if ($contents.Contains($forbiddenMarker)) {
                Add-ArchitectureError "$golden must not contain forbidden X6 marker '$forbiddenMarker'"
            }
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "X6 Path / Percent / Cover CLI",
            "percent reports probability complete",
            "observed truncated universe is not renormalized",
            "path reports representative trace",
            "path distinguishes retained trace from total count",
            "cover reports union probability",
            "cover reports C coverage row count",
            "slot assignment count is not success probability"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document X6 marker '$requiredMarker'"
        }
    }
}
