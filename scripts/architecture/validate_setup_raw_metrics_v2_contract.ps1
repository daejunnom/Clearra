# This file is dot-sourced by an architecture validation wrapper.

function Invoke-SetupRawMetricsV2ContractValidation() {
foreach ($requiredPath in @(
            "crates/clearra-setup-search/src/evaluate/setup_raw_metrics_v2.rs",
            "crates/clearra-setup-search/src/coverage/setup_raw_coverage_export.rs",
            "crates/clearra-output/src/json/setup_json_contract.rs",
            "crates/clearra-ui-schema/src/setup_explorer/setup_raw_metrics_schema.rs",
            "crates/clearra-ui-schema/src/setup_explorer/setup_explorer_schema.rs",
            "scripts/setup-raw-metrics-v2-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "X5 Setup Raw Metrics v2 required file is missing: $requiredPath"
        }
    }
$rawMetricsV2 = Read-Text "crates/clearra-setup-search/src/evaluate/setup_raw_metrics_v2.rs"
foreach ($requiredMarker in @(
            "SetupRawMetricsV2",
            "SETUP_RAW_METRICS_SCHEMA_VERSION",
            "SETUP_RAW_METRICS_KIND",
            "schema_version",
            "metrics_kind",
            "setup_raw_metrics",
            "shape_family_id",
            "shape_family_count",
            "tiling_variant_count",
            "build_variant_count",
            "covered_pattern_count",
            "coverage_probability",
            "post_pc_solution_count",
            "score_basis",
            "score_aggregation_attached",
            "backend_report",
            "raw_coverage_export_path",
            "setup_raw_coverage_export",
            "coverage_overlap_report",
            "build_variant_metrics",
            "diagnostic_evidence",
            "raw_metrics_sufficient_for_filtering",
            "condition_summary_field_absent"
        )) {
        if ($rawMetricsV2 -notlike "*$requiredMarker*") {
            Add-ArchitectureError "SetupRawMetricsV2 must expose marker '$requiredMarker'"
        }
    }
$rawCoverageExport = Read-Text "crates/clearra-setup-search/src/coverage/setup_raw_coverage_export.rs"
foreach ($requiredMarker in @(
            "SetupRawCoverageExport",
            "SETUP_RAW_COVERAGE_EXPORT_SCHEMA_VERSION",
            "SETUP_RAW_COVERAGE_EXPORT_KIND",
            "pattern_universe_id",
            "pattern_weight_model_id",
            "pattern_count",
            "rows",
            "family_unions",
            "overlap_report",
            "SetupCoverageOverlapReport",
            "to_machine_readable_snapshot",
            "from_machine_readable_snapshot",
            "raw_coverage_export_roundtrip",
            "coverage_overlap_report_is_not_hidden"
        )) {
        if ($rawCoverageExport -notlike "*$requiredMarker*") {
            Add-ArchitectureError "SetupRawCoverageExport must expose machine-readable marker '$requiredMarker'"
        }
    }
$setupJsonContract = Read-Text "crates/clearra-output/src/json/setup_json_contract.rs"
foreach ($requiredMarker in @(
            "setup_raw_metrics_schema_version",
            "schema_version",
            "metrics_kind",
            "score_aggregation_attached",
            "setup_raw_metrics",
            "setup_raw_coverage_export",
            "coverage_overlap_report",
            "build_variant_metrics",
            "diagnostic_evidence",
            "raw_coverage_schema_version",
            "raw_coverage_export_kind",
            "export_kind",
            "pattern_universe_id",
            "pattern_weight_model_id",
            "pattern_count",
            "rows",
            "family_unions",
            "overlap_report"
        )) {
        if ($setupJsonContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "setup JSON contract must expose Setup Raw Metrics v2 marker '$requiredMarker'"
        }
    }
$setupExplorerSchema = (Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_raw_metrics_schema.rs") + "`n" +
        (Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_explorer_schema.rs") + "`n" +
        (Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_diagnostic_columns.rs")
foreach ($requiredMarker in @(
            "SetupRawMetricsSchema",
            "SetupRawCoverageExportSchema",
            "SetupRawMetricsSchema::v2",
            "SetupRawCoverageExportSchema::v2",
            "gui_setup_explorer_consumes_raw_metrics_schema",
            "coverage_overlap_report",
            "rows",
            "family_unions",
            "overlap_report"
        )) {
        if ($setupExplorerSchema -notlike "*$requiredMarker*") {
            Add-ArchitectureError "GUI setup explorer schema must consume Setup Raw Metrics v2 marker '$requiredMarker'"
        }
    }
foreach ($file in Get-RustFiles "crates/clearra-setup-search/src") {
        $relativePath = Get-NormalizedRelativePath $file
        if (Test-GeneratedOrTestRustFile $file) { continue }
        $contents = Get-RustProductionContents (Get-Content -LiteralPath $file.FullName -Raw)
        foreach ($forbiddenMarker in @("condition_summary", "setup_condition_summary", "ConditionSummary")) {
            if ($contents.Contains($forbiddenMarker)) {
                Add-ArchitectureError "$relativePath must not reintroduce setup condition summary marker '$forbiddenMarker' after X5"
            }
        }
    }
foreach ($filePath in @(
            "crates/clearra-setup-search/src/evaluate/setup_raw_metrics_v2.rs",
            "crates/clearra-setup-search/src/coverage/setup_raw_coverage_export.rs"
        )) {
        $contents = Read-Text $filePath
        foreach ($forbiddenMarker in @(
                "raw_count_as_probability",
                "good_condition",
                "bad_condition",
                "hide_overlap",
                "variant_probability_sum"
            )) {
            if ($contents.Contains($forbiddenMarker)) {
                Add-ArchitectureError "$filePath must not use Setup Raw Metrics v2 forbidden marker '$forbiddenMarker'"
            }
        }
    }
}
