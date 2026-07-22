# This file is dot-sourced by an architecture validation wrapper.

function Invoke-BuildCoverageProductPathValidation() {
foreach ($requiredPath in @(
            "crates/clearra-build-coverage/src/coverage/build_coverage_executor.rs",
            "crates/clearra-build-coverage/src/coverage/build_coverage_executor_tests.rs",
            "crates/clearra-core-executor/src/service/cover_service.rs",
            "crates/clearra-cli/src/commands/cover_command.rs"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M21 build coverage product path required file is missing: $requiredPath"
        }
    }
$buildCoverageCargo = Read-Text "crates/clearra-build-coverage/Cargo.toml"
if ($buildCoverageCargo -like "*test = false*") {
        Add-ArchitectureError "M21 build coverage crate-local tests must not be disabled with test = false"
    }
$buildCoverageExecutor = (Read-Text "crates/clearra-build-coverage/src/coverage/build_coverage_executor.rs") + "`n" +
    (Read-Text "crates/clearra-build-coverage/src/coverage/build_coverage_executor_tests.rs")
foreach ($requiredMarker in @(
            "BuildCoverageExecution",
            "from_c_buildup_rows",
            "AssignmentExactCoverBridge",
            "AssignmentCsp",
            "BuildCoverageMatrix::from_assignments_with_coverages",
            "BuildUnionCoverage::from_matrix",
            "BuildCoverageResult::from_union",
            "WeightedPatternSet::uniform",
            "CoveragePatternCountMismatch",
            "NoCoverageRows",
            "CoverageRowAssignmentCountMismatch",
            "c_buildup_coverage_row_generated",
            "coverage_row_universe_mismatch_rejected"
        )) {
        if ($buildCoverageExecutor -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M21 BuildCoverageExecution must bridge C BuildUp rows to union probability marker '$requiredMarker'"
        }
    }
$buildCoverageResult = Read-Text "crates/clearra-build-coverage/src/coverage/build_coverage_result.rs"
if ($buildCoverageResult -notlike "*build_coverage_result_uses_union_probability*") {
        Add-ArchitectureError "M21 BuildCoverageResult must have union probability regression test marker 'build_coverage_result_uses_union_probability'"
    }
$buildCoverageMod = Read-Text "crates/clearra-build-coverage/src/coverage/mod.rs"
foreach ($requiredMarker in @(
            "pub mod build_coverage_executor",
            "BuildCoverageExecution",
            "BuildCoverageExecutionError"
        )) {
        if ($buildCoverageMod -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M21 build coverage executor must be exported marker '$requiredMarker'"
        }
    }
$packingRunner = Read-Text "crates/clearra-core-executor/src/packing/packing_runner.rs"
$packingProblemPreparer = Read-Text "crates/clearra-core-executor/src/packing/packing_problem_preparer.rs"
$packingRunnerTests = Read-Text "crates/clearra-core-executor/src/packing/packing_runner_tests.rs"
$packingSurface = "$packingRunner`n$packingProblemPreparer`n$packingRunnerTests"
foreach ($requiredMarker in @(
            "SearchProblemPreset::Build",
            "CPackingProblemBuilder::from_search_problem",
            "build_preset_builds_c_packing_candidate_buffer"
        )) {
        if ($packingSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M21 PackingRunner must generate build preset C packing candidates marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @("repeated_standard_pieces", "packing_portable_reference", "portable_reference_packing_fallback_allowed")) {
    if ($packingSurface -like "*$forbiddenMarker*") {
        Add-ArchitectureError "M21 PackingRunner must not use fixture-backed packing marker '$forbiddenMarker'"
    }
}
$buildupRunner = Get-BuildUpRunnerValidationSurface
$buildupCoverageBridge = Read-Text "crates/clearra-core-executor/src/buildup/buildup_coverage_bridge.rs"
$buildupReplayBridge = Read-Text "crates/clearra-core-executor/src/buildup/buildup_replay_bridge.rs"
$buildupSurface = "$buildupRunner`n$buildupCoverageBridge`n$buildupReplayBridge"
foreach ($requiredMarker in @(
            "SearchProblemPreset::Build =>",
            "selected_pattern_id()",
            "source_pattern_ids(candidate.candidate_id)",
            "ScenarioPackingWitness::solved(",
            "query.pattern_count()",
            "build_preset_generates_c_coverage_rows_for_build_coverage"
        )) {
        if ($buildupSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M21 BuildUpRunner must produce build coverage rows with build pattern universe marker '$requiredMarker'"
        }
    }
$coverService = Read-Text "crates/clearra-core-executor/src/service/cover_service.rs"
foreach ($requiredMarker in @(
            "execute_build_coverage",
            "PackingRunner::run",
            "BuildUpRunner::run",
            "BuildCoverageExecution::from_c_buildup_rows",
            "m21-build-coverage-product-path",
            "BuildTemplate -> SlotDomain -> SlotAssignment -> BuildUpProblem -> C BuildUp -> CoverageRow -> CoverageMatrix -> UnionProbability",
            "C BuildUp coverage row",
            "BuildCoverageResult uses union probability",
            "c_buildup_coverage_row_generated",
            "coverage_row_identity_validated",
            "slot_domain_policy",
            "build_coverage_probability"
        )) {
        if ($coverService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M21 CoverService must run build coverage through C BuildUp rows marker '$requiredMarker'"
        }
    }
$coreExecutor = Read-Text "crates/clearra-core-executor/src/core_executor.rs"
if ($coreExecutor -notlike "*execute_build_coverage*") {
        Add-ArchitectureError "M21 CoreExecutor must expose execute_build_coverage for canonical BuildCoverageQuery"
    }
$coverCommand = Read-Text "crates/clearra-cli/src/commands/cover_command.rs"
$coverAppCommand = Read-Text "crates/clearra-app/src/commands/cover_app_command.rs"
foreach ($requiredMarker in @(
            "execute_build_coverage",
            "BuildCoverageQuery",
            "validate_build_coverage_query"
        )) {
        if ($coverAppCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M21 cover app command must keep canonical query and call executor marker '$requiredMarker'"
        }
    }
if ($coverCommand -like "*CoreExecutor::execute(&problem)*") {
        Add-ArchitectureError "M21 cover CLI must not drop canonical BuildCoverageQuery by calling generic CoreExecutor::execute"
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "M21 Build Coverage Product Path",
            "BuildTemplate -> SlotDomain -> SlotAssignment -> BuildUpProblem -> C BuildUp -> CoverageRow -> CoverageMatrix -> UnionProbability",
            "native JSON template import",
            "each assignment must have a C BuildUp coverage row",
            "C coverage row",
            "identity validation rejects row kind",
            "pattern universe",
            "weight model",
            "pattern count mismatches",
            "AssignmentExactCoverBridge",
            "AssignmentCsp",
            "C BuildUp coverage row",
            "BuildCoverageResult uses union probability"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M21 build coverage product path marker '$requiredMarker'"
        }
    }
}



