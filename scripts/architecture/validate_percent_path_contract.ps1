# This file is dot-sourced by an architecture validation wrapper.

function Invoke-PercentPathProductSliceValidation() {
$percentService = Read-Text "crates/clearra-core-executor/src/service/percent_service.rs"
foreach ($requiredMarker in @(
            "ObservedQueueExpansion::expand",
            "ProblemCompiler::compile_scenario_pc",
            "PackingRunner::run",
            "BuildUpRunner::run",
            "PatternBitSet",
            "union_probability",
            "total_pattern_count",
            "covered_pattern_count",
            "probability",
            "c_buildup_coverage_row_count",
            "queue pattern universe -> multiset-grouped C Packing -> pattern-specific C BuildUp coverage rows -> PatternBitSet union -> weighted probability"
        )) {
        if ($percentService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M26 PercentService must connect queue universe through C BuildUp coverage and weighted probability marker '$requiredMarker'"
        }
    }
$percentAssembler = Read-Text "crates/clearra-cli/src/assemble/percent_query_assembler.rs"
foreach ($requiredMarker in @(
            "PcScenarioBoard::standard_10",
            "0x3f0",
            "PieceWindow::new"
        )) {
        if ($percentAssembler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M26 percent assembler must compile a scenario SearchProblem input marker '$requiredMarker'"
        }
    }
$pathCommand = Read-Text "crates/clearra-app/src/commands/path_app_command.rs"
foreach ($requiredMarker in @(
            "representative replay -> retained trace -> output",
            "retained_representative_trace",
            "representative_trace_source",
            "total_solution_count",
            "unique_solution_count",
            "retained_trace_count",
            "solution_trace_count",
            "path_distinguishes_retained_trace_from_total_count"
        )) {
        if ($pathCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M26 PathAppCommand must render retained representative trace and count split marker '$requiredMarker'"
        }
    }
$summaryContract = Read-Text "crates/clearra-cli/src/output/summary_render_contract.rs"
foreach ($requiredMarker in @(
            "probability",
            "weighted_probability",
            "c_buildup_coverage_row_count",
            "retained_representative_trace",
            "path_distinguishes_retained_trace_from_total_count"
        )) {
        if ($summaryContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M26 summary render contract must type product slice marker '$requiredMarker'"
        }
    }
$processE2E = Read-Text "crates/clearra-cli/tests/process_e2e.rs"
foreach ($requiredMarker in @(
            "process_e2e_m26_percent_and_path_report_product_contract",
            "covered_pattern_count",
            "probability",
            "retained_representative_trace",
            "retained_trace_count",
            "total_solution_count"
        )) {
        if ($processE2E -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M26 process E2E must verify percent/path product marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "M26 Percent / Path Product Slice",
            "queue pattern universe -> multiset-grouped C Packing -> pattern-specific C BuildUp coverage rows -> PatternBitSet union -> weighted probability",
            "SearchProblem -> C Packing / BuildUp -> representative replay -> retained trace -> output",
            "percent reports total pattern count",
            "percent reports covered pattern count",
            "percent reports probability",
            "path reports retained representative trace",
            "path distinguishes retained trace from total count"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M26 percent/path product marker '$requiredMarker'"
        }
    }
}



