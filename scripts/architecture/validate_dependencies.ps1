function Assert-WorkspaceDependencyGraphAcyclic($Graph) {
    $remaining = New-Object System.Collections.Generic.HashSet[string]
    foreach ($crateName in $Graph.Keys) {
        [void]$remaining.Add($crateName)
    }

    do {
        $removable = New-Object System.Collections.Generic.List[string]
        foreach ($crateName in @($remaining)) {
            $internalDependencies = @(
                $Graph[$crateName].Dependencies | Where-Object { $remaining.Contains($_) }
            )
            if ($internalDependencies.Count -eq 0) {
                $removable.Add($crateName)
            }
        }

        foreach ($crateName in $removable) {
            [void]$remaining.Remove($crateName)
        }
    } while ($removable.Count -gt 0)

    if ($remaining.Count -gt 0) {
        Add-ArchitectureError "architecture_validation_rejects_dependency_cycle: workspace dependency cycle among $(@($remaining) -join ', ')"
    }
}function Invoke-DependencyArchitectureValidation($WorkspaceDependencyGraph) {
    
Assert-WorkspaceDependencyGraphAcyclic $WorkspaceDependencyGraph
$dependencyRules = @(
        @{
            Crate = "clearra-core-domain"
            Path = "crates/clearra-core-domain/Cargo.toml"
            Forbidden = @("clearra-search", "clearra-validation", "clearra-output", "clearra-cli")
        },
        @{
            Crate = "clearra-problem"
            Path = "crates/clearra-problem/Cargo.toml"
            Forbidden = @("clearra-search", "clearra-scoring", "clearra-output", "clearra-validation", "clearra-cli")
        },
        @{
            Crate = "clearra-core-ffi"
            Path = "crates/clearra-core-ffi/Cargo.toml"
            Forbidden = @("clearra-search", "clearra-output", "clearra-validation", "clearra-cli")
        },
        @{
            Crate = "clearra-core-executor"
            Path = "crates/clearra-core-executor/Cargo.toml"
            Forbidden = @("clearra-search", "clearra-output", "clearra-validation", "clearra-cli")
        },
        @{
            Crate = "clearra-cli"
            Path = "crates/clearra-cli/Cargo.toml"
            Forbidden = @("clearra-search", "clearra-core-ffi", "clearra-core-executor")
        },
        @{
            Crate = "clearra-app"
            Path = "crates/clearra-app/Cargo.toml"
            Forbidden = @("clearra-cli", "clearra-core-ffi")
        },
        @{
            Crate = "clearra-gui-host"
            Path = "crates/clearra-gui-host/Cargo.toml"
            Forbidden = @("clearra-cli", "clearra-core-ffi", "clearra-core-executor")
        },
        @{
            Crate = "clearra-rules"
            Path = "crates/clearra-rules/Cargo.toml"
            Forbidden = @("clearra-cli", "clearra-output")
        },
        @{
            Crate = "clearra-two-line"
            Path = "crates/clearra-two-line/Cargo.toml"
            Forbidden = @("clearra-search", "clearra-output", "clearra-validation", "clearra-cli")
        },
        @{
            Crate = "clearra-output"
            Path = "crates/clearra-output/Cargo.toml"
            Forbidden = @("clearra-cli", "clearra-search", "clearra-validation")
        },
        @{
            Crate = "clearra-fumen"
            Path = "crates/clearra-fumen/Cargo.toml"
            Forbidden = @("clearra-output", "clearra-cli", "clearra-search", "clearra-validation", "clearra-build-coverage", "clearra-core-executor")
        },
        @{
            Crate = "clearra-render"
            Path = "crates/clearra-render/Cargo.toml"
            Forbidden = @("clearra-output", "clearra-cli", "clearra-search", "clearra-validation", "clearra-core-executor")
        },
        @{
            Crate = "clearra-build-coverage"
            Path = "crates/clearra-build-coverage/Cargo.toml"
            Forbidden = @("clearra-output")
        },
        @{
            Crate = "clearra-coverage"
            Path = "crates/clearra-coverage/Cargo.toml"
            Forbidden = @("clearra-scoring", "clearra-output", "clearra-validation", "clearra-cli")
        },
        @{
            Crate = "clearra-setup-search"
            Path = "crates/clearra-setup-search/Cargo.toml"
            Forbidden = @("clearra-output", "clearra-search")
        },
        @{
            Crate = "clearra-scoring"
            Path = "crates/clearra-scoring/Cargo.toml"
            Forbidden = @("clearra-search", "clearra-core-ffi", "clearra-core-executor")
        },
        @{
            Crate = "clearra-postprocess"
            Path = "crates/clearra-postprocess/Cargo.toml"
            Forbidden = @("clearra-search", "clearra-core-ffi", "clearra-core-executor", "clearra-cli", "clearra-output")
        },
        @{
            Crate = "clearra-postprocess-gpu"
            Path = "crates/clearra-postprocess-gpu/Cargo.toml"
            Forbidden = @("clearra-search", "clearra-core-ffi", "clearra-core-executor", "clearra-cli", "clearra-output")
        },
        @{
            Crate = "clearra-wasm"
            Path = "crates/clearra-wasm/Cargo.toml"
            Forbidden = @("clearra-cli", "clearra-core-ffi")
        },
        @{
            Crate = "clearra-webgpu"
            Path = "crates/clearra-webgpu/Cargo.toml"
            Forbidden = @("clearra-cli", "clearra-app")
        },
        @{
            Crate = "clearra-validation"
            Path = "crates/clearra-validation/Cargo.toml"
            Forbidden = @("clearra-search")
        }
    )
foreach ($rule in $dependencyRules) {
        Assert-CargoDoesNotDepend $rule.Path $rule.Forbidden "product dependency graph boundary"
        Assert-DependencyGraphDoesNotDepend $WorkspaceDependencyGraph $rule.Crate $rule.Forbidden "product dependency graph boundary"
    }
Assert-CargoDoesNotDepend "crates/clearra-problem/Cargo.toml" @("clearra-scoring") "clearra-problem owns SpinTargetRequest query contracts without depending on scoring implementation"
Assert-CargoDoesNotDepend "crates/clearra-coverage/Cargo.toml" @("clearra-scoring") "coverage probability layer must not depend on scoring"
Assert-CargoRuntimeDoesNotDepend "crates/clearra-core-executor/Cargo.toml" @("clearra-scoring") "architecture_validation_rejects_core_executor_runtime_scoring"
$coreExecutorSpinModules = Read-Text "crates/clearra-core-executor/src/spin/mod.rs"
foreach ($testOnlyModule in @(
            "spin_input_from_replay",
            "spin_target_coverage_bridge",
            "spin_target_execution_report",
            "spin_target_result_reducer",
            "spin_target_runner",
            "spin_target_runner_error",
            "spin_target_threshold"
        )) {
        $pattern = "(?m)#\[cfg\(test\)\]\s*\r?\n\s*pub mod $([regex]::Escape($testOnlyModule));"
        if ($coreExecutorSpinModules -notmatch $pattern) {
            Add-ArchitectureError "clearra-core-executor spin scoring helper '$testOnlyModule' must remain test-only until clearra-postprocess owns its typed BuildVariant input"
        }
    }
$coreFfiManifest = Read-Text "crates/clearra-core-ffi/Cargo.toml"
foreach ($requiredFeatureMarker in @(
            "default = []",
            "native-memory-binding = []",
            'native-c-core = ["native-memory-binding"]'
        )) {
        if (-not $coreFfiManifest.Contains($requiredFeatureMarker)) {
            Add-ArchitectureError "clearra-core-ffi feature dependency boundary must contain '$requiredFeatureMarker'"
        }
    }
if ($coreFfiManifest.Contains('native-memory-binding = ["native-c-core"]')) {
        Add-ArchitectureError "clearra-core-ffi native-memory-binding must not imply native-c-core"
    }
$importRules = @(
        @{
            Dir = "crates/clearra-core-domain/src"
            ForbiddenCrates = @("clearra_search", "clearra_validation", "clearra_output", "clearra_cli")
            Forbidden = @("clearra_search", "clearra_validation", "clearra_output", "clearra_cli")
        },
        @{
            Dir = "crates/clearra-problem/src"
            ForbiddenCrates = @("clearra_search", "clearra_scoring", "clearra_output", "clearra_validation", "clearra_cli")
            Forbidden = @("clearra_search", "clearra_scoring", "clearra_output", "clearra_validation", "clearra_cli")
        },
        @{
            Dir = "crates/clearra-core-ffi/src"
            ForbiddenCrates = @("clearra_search", "clearra_output", "clearra_validation", "clearra_cli")
            Forbidden = @("clearra_search", "clearra_output", "clearra_validation", "clearra_cli")
        },
        @{
            Dir = "crates/clearra-core-executor/src"
            ForbiddenCrates = @("clearra_search", "clearra_output", "clearra_validation", "clearra_cli")
            Forbidden = @("clearra_search", "clearra_output", "clearra_validation", "clearra_cli")
        },
        @{
            Dir = "crates/clearra-cli/src"
            ForbiddenCrates = @("clearra_search", "clearra_core_ffi", "clearra_core_executor")
            Forbidden = @(
                "clearra_search",
                "clearra_core_ffi",
                "clearra_core_executor",
                "GenericPcSolver",
                "PcSearchService",
                "PcScenarioService",
                "SearchOrchestrator",
                "CClr",
                "RawContextPtr",
                "RawScopePtr",
                "RawPayloadPtr"
            )
        },
        @{
            Dir = "crates/clearra-app/src"
            ForbiddenCrates = @("clearra_core_ffi", "clearra_cli")
            Forbidden = @(
                "clearra_core_ffi",
                "CClr",
                "RawContextPtr",
                "RawScopePtr",
                "RawPayloadPtr",
                "extern `"C`"",
                "NonNull<",
                "*mut ",
                "core_c_raw_pointer_module"
            )
        },
        @{
            # GUI adapters may use the pure clearra_cli_command compiler. The
            # exact Cargo/import-graph rule still forbids the clearra_cli binary
            # crate without rejecting crates that merely share its prefix.
            Dir = "crates/clearra-gui-host/src"
            ForbiddenCrates = @("clearra_cli", "clearra_core_ffi", "clearra_core_executor")
            Forbidden = @(
                "clearra_core_ffi",
                "clearra_core_executor",
                "CClr",
                "RawContextPtr",
                "RawScopePtr",
                "RawPayloadPtr",
                "extern `"C`"",
                "NonNull<",
                "*mut ",
                "std::process::Command",
                "process::Command",
                "CARGO_BIN_EXE_clearra"
            )
        },
        @{
            Dir = "crates/clearra-rules/src"
            ForbiddenCrates = @("clearra_cli", "clearra_output")
            Forbidden = @(
                "clearra_cli",
                "CliOutput",
                "ParsedCliCommand",
                "CARGO_BIN_EXE_clearra",
                "Command::new"
            )
        },
        @{
            Dir = "crates/clearra-two-line/src"
            ForbiddenCrates = @("clearra_search", "clearra_output", "clearra_validation", "clearra_cli")
            Forbidden = @("clearra_search", "clearra_output", "clearra_validation", "clearra_cli")
        },
        @{
            Dir = "crates/clearra-output/src"
            ForbiddenCrates = @("clearra_cli", "clearra_search", "clearra_validation")
            Forbidden = @(
                "clearra_cli",
                "clearra_search",
                "PcSearchService",
                "PcScenarioService",
                "GenericPcSolver",
                "SearchOrchestrator"
            )
        },
        @{
            Dir = "crates/clearra-fumen/src"
            ForbiddenCrates = @("clearra_output", "clearra_cli", "clearra_search", "clearra_validation", "clearra_build_coverage", "clearra_core_executor")
            Forbidden = @(
                "clearra_output",
                "clearra_cli",
                "clearra_search",
                "clearra_validation",
                "clearra_build_coverage",
                "clearra_core_executor",
                "CPackingProblem",
                "BuildUpProblem"
            )
        },
        @{
            Dir = "crates/clearra-render/src"
            ForbiddenCrates = @("clearra_output", "clearra_cli", "clearra_search", "clearra_validation", "clearra_core_executor")
            Forbidden = @(
                "clearra_output",
                "clearra_cli",
                "clearra_search",
                "clearra_validation",
                "clearra_core_executor",
                "SvgRuntimeRenderer",
                "RuntimeRawSvgRenderer",
                "raw_svg_runtime_module",
                "raw_svg_preview_to_renderer=true",
                "render_raw_svg_at_runtime"
            )
        },
        @{
            Dir = "crates/clearra-build-coverage/src"
            ForbiddenCrates = @("clearra_output", "clearra_fumen")
            Forbidden = @("clearra_output", "clearra_fumen", "fumen_like", "FumenLike", "v115@")
        },
        @{
            Dir = "crates/clearra-coverage/src"
            ForbiddenCrates = @("clearra_scoring", "clearra_output", "clearra_validation", "clearra_cli")
            Forbidden = @("clearra_scoring", "clearra_output", "clearra_validation", "clearra_cli")
        },
        @{
            Dir = "crates/clearra-setup-search/src"
            ForbiddenCrates = @("clearra_output", "clearra_fumen", "clearra_search")
            Forbidden = @("clearra_output", "clearra_fumen", "clearra_search", "PcScenarioService", "PcSearchService", "fumen_like", "FumenLike", "v115@")
        },
        @{
            Dir = "crates/clearra-scoring/src"
            ForbiddenCrates = @("clearra_search", "clearra_core_ffi", "clearra_core_executor")
            Forbidden = @(
                "clearra_search",
                "clearra_core_ffi",
                "clearra_core_executor",
                "GenericPcSolver",
                "PcSearchService",
                "PcScenarioService",
                "extern `"C`"",
                "CBuildVariantView",
                "CClr",
                "clr_"
            )
        },
        @{
            Dir = "crates/clearra-postprocess/src"
            ForbiddenCrates = @("clearra_search", "clearra_core_ffi", "clearra_core_executor", "clearra_cli", "clearra_output")
            Forbidden = @(
                "clearra_search",
                "clearra_core_ffi",
                "clearra_core_executor",
                "clearra_cli",
                "clearra_output",
                "CPackingCandidate",
                "PackingCandidate",
                "CoverageRow {",
                "clr_"
            )
        },
        @{
            Dir = "crates/clearra-postprocess-gpu/src"
            ForbiddenCrates = @("clearra_search", "clearra_core_ffi", "clearra_core_executor", "clearra_cli", "clearra_output")
            Forbidden = @(
                "clearra_search",
                "clearra_core_ffi",
                "clearra_core_executor",
                "clearra_cli",
                "clearra_output",
                "SpecialSpinCaseRegistry::finalize",
                "Fumen",
                "JSON",
                "extern `"C`"",
                "clr_"
            )
        },
        @{
            # Browser WASM links the same pure command compiler as native CLI;
            # exact dependency/import checks below still reject clearra_cli.
            Dir = "crates/clearra-wasm/src"
            ForbiddenCrates = @("clearra_cli", "clearra_core_ffi")
            Forbidden = @(
                "clearra_core_ffi",
                "std::process",
                "process::Command",
                "std::fs",
                "File::open",
                "std::path",
                "PathBuf",
                "Path::new",
                "CARGO_BIN_EXE_clearra"
            )
        },
        @{
            Dir = "crates/clearra-webgpu/src"
            ForbiddenCrates = @("clearra_cli", "clearra_app")
            Forbidden = @(
                "clearra_cli",
                "clearra_app",
                "std::fs",
                "File::open",
                "read_to_string",
                "shader_path",
                "user_shader_path",
                "load_wgsl",
                "WGSL_PATH"
            )
        },
        @{
            Dir = "crates/clearra-validation/src"
            ForbiddenCrates = @("clearra_search")
            Forbidden = @("clearra_search", "GenericPcSolver", "PcSearchService", "PcScenarioService", "SearchOrchestrator")
        }
    )
foreach ($rule in $importRules) {
        Assert-ProductionImportAbsence $rule.Dir $rule.Forbidden "product import boundary"
        Assert-ProductionImportGraphDoesNotImport $rule.Dir $rule.ForbiddenCrates "product import graph boundary"
    }
Assert-ProductionImportAbsence "crates/clearra-problem/src" @("clearra_scoring", "SpinClassifier", "SpinTargetPredicate", "ScoreProfileObjectValidator", "CandidateScoreStats") "clearra-problem must not import scoring implementation; use SpinTargetRequest only"
Assert-ProductionImportAbsence "crates/clearra-coverage/src" @("clearra_scoring") "coverage row kinds must use opaque ids, not scoring crate types"
Assert-CargoDoesNotDepend "crates/clearra-cli/Cargo.toml" @("clearra-core-ffi", "clearra-core-executor") "architecture_validation_rejects_cli_to_core_ffi"
Assert-CargoDoesNotDepend "crates/clearra-gui-host/Cargo.toml" @("clearra-cli") "architecture_validation_rejects_gui_to_cli; architecture_validation_rejects_gui_to_cli_dependency"
Assert-CargoDoesNotDepend "crates/clearra-render/Cargo.toml" @("clearra-core-executor") "architecture_validation_rejects_render_to_solver; architecture_validation_rejects_render_to_solver_dependency"
Assert-CargoDoesNotDepend "crates/clearra-fumen/Cargo.toml" @("clearra-core-executor") "architecture_validation_rejects_fumen_to_solver"
Assert-CargoDoesNotDepend "crates/clearra-coverage/Cargo.toml" @("clearra-scoring") "architecture_validation_rejects_coverage_to_scoring"
Assert-CargoDoesNotDepend "crates/clearra-scoring/Cargo.toml" @("clearra-core-ffi") "architecture_validation_rejects_scoring_in_core_search_path; architecture_validation_rejects_scoring_in_core_search_path_dependency"
if (Test-Path -LiteralPath "crates/clearra-spin/Cargo.toml") {
        Assert-CargoDoesNotDepend "crates/clearra-spin/Cargo.toml" @("clearra-scoring") "architecture_validation_rejects_spin_to_scoring"
    }
}
