# This file is dot-sourced by an architecture validation wrapper.

function Invoke-SetupSearchProductPathValidation() {
    $requiredPaths = @(
        "crates/clearra-problem/src/query/setup_residue_input.rs",
        "crates/clearra-problem/src/compile/setup_condition_compiler.rs",
        "crates/clearra-core-executor/src/backend/wasm_setup_search_backend.rs",
        "crates/clearra-core-executor/src/backend/wasm_cpu/setup_coverage_graph.rs",
        "crates/clearra-core-executor/src/backend/wasm_cpu/setup_partial_build.rs",
        "crates/clearra-core-executor/src/backend/wasm_cpu/setup_finder.rs",
        "crates/clearra-core-executor/src/setup_finder_report.rs",
        "crates/clearra-app/src/commands/setup_app_command.rs",
        "packages/clearra-ui/src/lib/workspace/SetupFinderWorkspace.svelte"
    )
    foreach ($requiredPath in $requiredPaths) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "exact setup finder required file is missing: $requiredPath"
        }
    }

    foreach ($obsoletePath in @(
        "crates/clearra-core-executor/src/service/setup_service.rs",
        "crates/clearra-setup-search/src/service/setup_search_service.rs",
        "crates/clearra-setup-search/src/service/setup_shape_packer.rs",
        "crates/clearra-setup-search/src/service/setup_candidate_enumerator.rs"
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $obsoletePath)) {
            Add-ArchitectureError "obsolete setup product path must not exist: $obsoletePath"
        }
    }

    $compiler = Read-Text "crates/clearra-problem/src/compile/setup_condition_compiler.rs"
    foreach ($requiredMarker in @(
        "compile_setup_search_conditions",
        "SetupSearchQuery",
        "query.residue()",
        "canonical_pieces",
        "SetupCycleResetBorrowPolicy",
        "hold-empty",
        "with_hold_piece",
        "with_exact_pieces(Some(10))",
        "P7P2",
        "P7P1"
    )) {
        if ($compiler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "setup condition compiler must preserve residue/hold/bag marker '$requiredMarker'"
        }
    }

    $quotient = Read-Text "crates/clearra-core-executor/src/backend/wasm_cpu/setup_coverage_graph.rs"
    foreach ($requiredMarker in @(
        "SetupCoverageGraph",
        "intern_node",
        "source_classes",
        "node_edges",
        "edge_scratch.sort_unstable",
        "edge_scratch.dedup"
    )) {
        if ($quotient -notlike "*$requiredMarker*") {
            Add-ArchitectureError "setup exact coverage quotient marker is missing: '$requiredMarker'"
        }
    }

    $partialBuild = Read-Text "crates/clearra-core-executor/src/backend/wasm_cpu/setup_partial_build.rs"
    foreach ($requiredMarker in @(
        "PartialBuildGraph",
        "PartialBuildGraphBuilder",
        "GeometryCompletionOracle",
        "compact_live_graph",
        "MAX_SETUP_CANDIDATE_LOCKS"
    )) {
        if ($partialBuild -notlike "*$requiredMarker*") {
            Add-ArchitectureError "setup partial BuildUp graph marker is missing: '$requiredMarker'"
        }
    }

    $finder = Read-Text "crates/clearra-core-executor/src/backend/wasm_cpu/setup_finder.rs"
    foreach ($requiredMarker in @(
        "SetupCoverageSession",
        "merge_exact_state_coverage",
        "let joint = build & backward",
        "shape_build_words",
        "shape_joint_words",
        "representative_paths",
        '"setup_coverage_semantics"',
        '"oracle"'
    )) {
        if ($finder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "exact setup family/product coverage marker is missing: '$requiredMarker'"
        }
    }
    foreach ($forbiddenMarker in @(
        "pack_piece_sequence",
        "setup_piece_shape",
        "SetupSearchService",
        "SetupCoreBuildGate"
    )) {
        if ($finder -like "*$forbiddenMarker*") {
            Add-ArchitectureError "exact setup finder must not use fabricated legacy setup marker '$forbiddenMarker'"
        }
    }

    $appCommand = Read-Text "crates/clearra-app/src/commands/setup_app_command.rs"
    $appServices = Read-Text "crates/clearra-app/src/app_services.rs"
    $backend = Read-Text "crates/clearra-core-executor/src/backend/wasm_setup_search_backend.rs"
    $productSurface = "$appCommand`n$appServices`n$backend"
    foreach ($requiredMarker in @(
        "validate_setup_search_query",
        "execute_setup_with_control",
        "WasmSetupSearchBackend::execute_with_control",
        "WasmSetupSearchSession",
        "WasmSetupSearchAdvance::Cancelled"
    )) {
        if ($productSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "setup product execution boundary marker is missing: '$requiredMarker'"
        }
    }

    $tabs = Read-Text "packages/clearra-ui/src/lib/workspace/ProductModeTabs.svelte"
    $route = Read-Text "apps/clearra-web/src/routes/+page.svelte"
    if ($tabs -notlike "*setup*" -or $route -notlike "*SetupFinderWorkspace*") {
        Add-ArchitectureError "setup finder UI must be routed through the shared product workspace"
    }

    $architecture = Read-Text "docs/architecture.md"
    foreach ($requiredMarker in @(
        "M20 Setup Finder Product Path",
        "FamilyQuotient Partial BuildUp",
        "JointCoverage(state) = ForwardCoverage(state) AND BackwardPcLiveness(state)",
        "Oracle",
        "Online coverage is not exposed"
    )) {
        if ($architecture -notlike "*$requiredMarker*") {
            Add-ArchitectureError "architecture setup finder contract marker is missing: '$requiredMarker'"
        }
    }
}
