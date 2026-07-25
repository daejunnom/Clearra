# This file is dot-sourced by an architecture validation wrapper.

function Invoke-CoreExecutorValidation() {
foreach ($requiredPath in @(
        "crates/clearra-core-executor/src/service/pc_service.rs",
        "crates/clearra-core-executor/src/service/cover_service.rs",
        "crates/clearra-core-executor/src/service/percent_service.rs",
        "crates/clearra-core-executor/src/backend/wasm_setup_search_backend.rs",
        "crates/clearra-core-executor/src/backend/wasm_cpu/setup_finder.rs",
        "crates/clearra-core-executor/src/backend/wasm_cpu/setup_partial_build.rs",
        "crates/clearra-core-executor/src/backend/wasm_cpu/setup_coverage_graph.rs",
        "crates/clearra-core-executor/src/backend/backend_kind.rs",
        "crates/clearra-core-executor/src/backend/backend_capability.rs",
        "crates/clearra-core-executor/src/backend/backend_selector.rs",
        "crates/clearra-core-executor/src/backend/backend_fallback.rs",
        "crates/clearra-core-executor/src/backend/buildable_packing_executor.rs",
        "crates/clearra-core-executor/src/backend/buildable_geometry_graph_executor.rs",
        "crates/clearra-core-executor/src/backend/buildable_geometry_task_reducer.rs",
        "crates/clearra-core-executor/src/packing/packing_runner.rs",
        "crates/clearra-core-executor/src/buildup/buildup_runner.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M17 Core Executor required file is missing: $requiredPath"
        }
    }
$executorLib = Read-Text "crates/clearra-core-executor/src/lib.rs"
foreach ($requiredMarker in @("pub mod service", "PackingRunner", "BuildUpRunner", "PcService", "WasmSetupSearchBackend", "CoverService", "PercentService")) {
        if ($executorLib -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-executor lib.rs must export M17 executor marker '$requiredMarker'"
        }
    }
$coreExecutor = Read-Text "crates/clearra-core-executor/src/core_executor.rs"
foreach ($requiredMarker in @("SearchProblemPreset::OpeningPc", "PcService::execute", "SearchProblemPreset::Setup => Err(CoreExecutionError::UnsupportedProblem)", "CoverService::execute")) {
        if ($coreExecutor -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CoreExecutor must stay a thin M17 service router marker '$requiredMarker'"
        }
    }
$setupBackend = Read-Text "crates/clearra-core-executor/src/backend/wasm_setup_search_backend.rs"
foreach ($requiredMarker in @("WasmSetupSearchBackend", "WasmSetupSearchSession", "execute_with_control", "Cancelled")) {
        if ($setupBackend -notlike "*$requiredMarker*") {
            Add-ArchitectureError "WASM setup backend must own the exact setup execution surface marker '$requiredMarker'"
        }
    }
foreach ($obsoletePath in @(
        "crates/clearra-core-executor/src/service/setup_service.rs",
        "crates/clearra-setup-search/src/service/setup_shape_packer.rs",
        "crates/clearra-setup-search/src/service/setup_search_service.rs"
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $obsoletePath)) {
            Add-ArchitectureError "obsolete setup product path must not exist: $obsoletePath"
        }
    }
foreach ($forbiddenMarker in @("CPackingProblemBuilder::from_search_problem", "CBuildUpProblemBuilder::from_packing_candidate", "ObjectiveReducer::reduce")) {
        if ($coreExecutor -like "*$forbiddenMarker*") {
            Add-ArchitectureError "CoreExecutor must not own M17 runner/reducer internals marker '$forbiddenMarker'"
        }
    }
$pcService = Get-PcServiceValidationSurface
foreach ($requiredMarker in @("PackingRunner::run", "BuildUpRunner::run", "SearchProblem->C PackingProblem->C PackingResult->C BuildUpResult->CoverageRows->Rust ObjectiveResult->Rust OutputModel", "CoreExecutionResult::new")) {
        if ($pcService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "PcService must orchestrate M17 core executor flow marker '$requiredMarker'"
        }
    }
$packingRunner = Read-PhysicalText "crates/clearra-core-executor/src/packing/packing_runner.rs"
$packingProblemPreparer = Read-PhysicalText "crates/clearra-core-executor/src/packing/packing_problem_preparer.rs"
$packingNativeBridge = Read-PhysicalText "crates/clearra-core-executor/src/packing/packing_native_bridge.rs"
$packingMetrics = Read-PhysicalText "crates/clearra-core-executor/src/packing/packing_metrics.rs"
$buildablePackingExecutor = Read-PhysicalText "crates/clearra-core-executor/src/backend/buildable_packing_executor.rs"
$buildableGraphExecutor = Read-PhysicalText "crates/clearra-core-executor/src/backend/buildable_geometry_graph_executor.rs"
$buildableTaskReducer = Read-PhysicalText "crates/clearra-core-executor/src/backend/buildable_geometry_task_reducer.rs"
$packingSurface = "$packingRunner`n$packingProblemPreparer`n$packingNativeBridge`n$packingMetrics`n$buildablePackingExecutor`n$buildableGraphExecutor`n$buildableTaskReducer"
foreach ($requiredMarker in @("CPackingProblemBuilder::from_search_problem", "PcBackendSelector::select_with_context", "CPackingState::empty", "execute_selected_buildable_packing", "CoreCNative::compile_geometry_catalog_with_cancellation", "CoreCNative::search_geometry_solution_graph", "stream_buildable_task", "NativeCandidateReducer", "PackingRunResult", "PackingCandidate", ".map_err(PackingRunnerError::Native)")) {
        if ($packingSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "PackingRunner must own M17 C PackingProblem/result boundary marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @("portable_reference_packing_fallback_allowed", "packing_portable_reference", "repeated_standard_pieces", "ResourceReport::complete()")) {
    if ($packingSurface -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PackingRunner must not synthesize fixture-backed product packing marker '$forbiddenMarker'"
    }
}
if (Test-Path -LiteralPath (Join-Path $Root "crates/clearra-core-executor/src/packing/packing_portable_reference.rs")) {
    Add-ArchitectureError "packing_portable_reference.rs must not exist in the product executor"
}
$buildupRunner = Get-BuildUpRunnerValidationSurface
$buildupNativeBridge = Read-Text "crates/clearra-core-executor/src/buildup/buildup_native_bridge.rs"
$buildupCoverageBridge = Read-Text "crates/clearra-core-executor/src/buildup/buildup_coverage_bridge.rs"
$buildupReplayBridge = Read-Text "crates/clearra-core-executor/src/buildup/buildup_replay_bridge.rs"
$buildupObjectiveBridge = Read-Text "crates/clearra-core-executor/src/buildup/buildup_objective_bridge.rs"
$buildupTraceRetention = Read-Text "crates/clearra-core-executor/src/buildup/buildup_trace_retention.rs"
$executionVariants = Read-Text "crates/clearra-core-executor/src/buildup/execution_variant_set.rs"
$candidateAggregates = Read-Text "crates/clearra-core-executor/src/buildup/candidate_execution_aggregate_builder.rs"
$buildupSurface = "$buildupRunner`n$buildupNativeBridge`n$buildupCoverageBridge`n$buildupReplayBridge`n$buildupObjectiveBridge`n$buildupTraceRetention`n$executionVariants`n$candidateAggregates"
foreach ($requiredMarker in @("configure_packing_candidate_view", "enumerate_buildup_variants_with_cancellation", "Err(NativeCoreError::Unavailable)", "CBuildUpExecution", "build_variants", "coverage_rows_from_pattern_verifications_with_cancellation", "PatternVerifiedBuildVariant", "CoveragePatternVerificationMismatch", "WitnessedPatternCoverageAccumulator", "record_verified_variant", "coverage_universe_identity", "coverage_pattern_id_for_problem", "verified_pattern_count_for_execution", "materialized_pattern_universe", "ExecutionVariantSet", "CandidateExecutionAggregate", "trace_material_for_execution", "ObjectiveReducer::reduce", "BuildUpRunResult")) {
        if ($buildupSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "BuildUpRunner must own M17 BuildUp/coverage/objective bridge marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @("coverage_rows_from_c", "CCoverageRowView::single_pattern(candidate", "CCoverageRowView::product_from_build_variant_with_identity", "coverage_row_from_raw_words_with_identity_and_piece_source")) {
        if ($buildupSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "BuildUpRunner must derive coverage rows from accepted BuildVariant views, not raw PackingCandidate rows marker '$forbiddenMarker'"
        }
    }
foreach ($forbiddenMarker in @("portable_reference_buildup_fallback_allowed", "portable_reference_buildup_witness", "PORTABLE_REFERENCE_EXAMPLE_TRACE_KEY", "portable_reference_trace_material", "fallback_build_variant_from_candidate")) {
    if ($buildupSurface -like "*$forbiddenMarker*") {
        Add-ArchitectureError "BuildUpRunner must not synthesize fixture-backed product BuildUp marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("trace_key_for_build_variant", '"bvk2:', "variant.candidate_id", "variant.build_variant_id", "objective_stable_key")) {
        if ($buildupSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "BuildUpRunner must derive native/product objective trace keys from BuildVariant identity marker '$requiredMarker'"
        }
    }

foreach ($forbiddenMarker in @('format!("bvk1:')) {
        if ($buildupSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "BuildUpRunner must not use hash-only BuildVariant identity marker '$forbiddenMarker'"
        }
    }
if ($buildupRunner -like "*fn replay_operations_for_problem*") {
        Add-ArchitectureError "BuildUpRunner must not contain hardcoded replay operation fixtures"
    }
foreach ($forbiddenMarker in @("candidate_index % pattern_count", "coverage_pattern_id_for_candidate")) {
        if ($buildupRunner -like "*$forbiddenMarker*") {
            Add-ArchitectureError "BuildUpRunner must not synthesize coverage_pattern_id from candidate index marker '$forbiddenMarker'"
        }
    }
foreach ($requiredMarker in @("build_coverage_can_select_one_materialized_pattern", "build_coverage_pattern_id_out_of_range_is_rejected")) {
        if ($buildupRunner -notlike "*$requiredMarker*") {
            Add-ArchitectureError "BuildUpRunner must test source-owned coverage_pattern_id marker '$requiredMarker'"
        }
    }
$coreFfiLib = Read-Text "crates/clearra-core-ffi/src/lib.rs"
$nativeCore = @(
        (Read-Text "crates/clearra-core-ffi/src/raw/bindings.rs"),
        (Read-Text "crates/clearra-core-ffi/src/native/mod.rs"),
        (Read-Text "crates/clearra-core-ffi/src/native/geometry_catalog.rs"),
        (Read-Text "crates/clearra-core-ffi/src/native/geometry_solution_graph.rs"),
        (Read-Text "crates/clearra-core-ffi/src/native/packing_candidate_sink.rs"),
        (Read-Text "crates/clearra-core-ffi/src/native/buildup.rs")
    ) -join "`n"
$coreFfiCargo = Read-Text "crates/clearra-core-ffi/Cargo.toml"
$coreExecutorCargo = Read-Text "crates/clearra-core-executor/Cargo.toml"
$cliCargo = Read-Text "crates/clearra-cli/Cargo.toml"
$setupSearchCargo = Read-Text "crates/clearra-setup-search/Cargo.toml"
foreach ($requiredMarker in @(
            "native C core wrappers",
            "pub mod native",
            "CoreCNative",
            "NativeCoreError"
        )) {
        if ($coreFfiLib -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi lib.rs must expose M17 native C ABI wrapper marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
            'unsafe extern "C"',
            "clearra_core_abi_version",
            "clearra_geometry_catalog_compile",
            "clearra_geometry_exact_cover_search_graph",
            "clearra_geometry_solution_graph_stream_buildable_task",
            "clearra_geometry_catalog_rows_buildable_to_sink",
            "clr_buildup_worker_verify_into_buffer",
            "CNativeBuildableGeometryStreamReport",
            "CNativeBuildVariantBuffer",
            "compile_geometry_catalog",
            "search_geometry_solution_graph",
            "verify_buildup_problem",
            "native_core_reports_unavailable_without_link_feature"
        )) {
        if ($nativeCore -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi native module must own real C ABI binding marker '$requiredMarker'"
        }
    }
$coreFfiNativeModule = Read-Text "crates/clearra-core-ffi/src/native/mod.rs"
$coreFfiProductExports = Read-Text "crates/clearra-core-ffi/src/lib.rs"
foreach ($testOnlyExport in @(
        @{ Text = $coreFfiNativeModule; Marker = "pub use packing::CNativePackingCandidateBuffer;" },
        @{ Text = $coreFfiProductExports; Marker = "pub use native::CNativePackingCandidateBuffer;" }
    )) {
    $escaped = [regex]::Escape($testOnlyExport.Marker)
    if ($testOnlyExport.Text -match $escaped -and
        $testOnlyExport.Text -notmatch "(?ms)#\[cfg\(test\)\]\s*$escaped") {
        Add-ArchitectureError "legacy materializing packing buffer export must be guarded by cfg(test): '$($testOnlyExport.Marker)'"
    }
}
foreach ($requiredMarker in @(
            '#[link(name = "clearra_core", kind = "static")]'
        )) {
        if (-not $nativeCore.Contains($requiredMarker)) {
            Add-ArchitectureError "clearra-core-ffi raw bindings must link native C core without a Cargo build script marker '$requiredMarker'"
        }
    }
if (Test-Path -LiteralPath (Join-Path $Root "crates/clearra-core-ffi/build.rs")) {
        Add-ArchitectureError "clearra-core-ffi must not use build.rs for native-c-core; pass the clearra_core library directory with rustc link flags to avoid Cargo build-script launch blocks"
    }
if ($coreFfiCargo -like '*build = "build.rs"*') {
        Add-ArchitectureError "clearra-core-ffi Cargo.toml must not declare build.rs; native-c-core linking is owned by #[link] plus caller-provided rustc -L native flags"
    }
foreach ($requiredMarker in @("native-c-core", 'unsafe_code = "allow"', 'clearra-core-ffi/native-c-core', 'clearra-core-executor/native-c-core')) {
        if ("$coreFfiCargo`n$coreExecutorCargo`n$cliCargo`n$setupSearchCargo" -notlike "*$requiredMarker*") {
            Add-ArchitectureError "Cargo feature graph must expose M17 native C product path marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M17 Core Executor", "SearchProblem -> C PackingProblem -> C PackingResult -> C BuildUpResult -> CoverageRows -> Rust ObjectiveResult -> Rust OutputModel", "CoreExecutor is a thin router", "CLI code must not call clearra-core-ffi")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M17 marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("Core executor orchestration", "PackingRunner::run", "BuildUpRunner::run", "ObjectiveReducer::reduce", "CLI remains outside the C ABI")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M17 marker '$requiredMarker'"
        }
    }
}



