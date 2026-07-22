function New-ArchitectureValidationTask(
    [string]$Name,
    [string]$FunctionName,
    [switch]$RequiresWorkspaceDependencyGraph
) {
    return [pscustomobject]@{
        Name = $Name
        FunctionName = $FunctionName
        RequiresWorkspaceDependencyGraph = [bool]$RequiresWorkspaceDependencyGraph
    }
}

function New-AdvisoryMarkerAuditTasks() {
    return @(
        New-ArchitectureValidationTask "Dependency Architecture" "Invoke-DependencyArchitectureValidation" -RequiresWorkspaceDependencyGraph
        New-ArchitectureValidationTask "Adversarial Release Wiring" "Invoke-AdversarialReleaseGateWiringValidation"
        New-ArchitectureValidationTask "Architecture Authority Policy" "Invoke-ArchitectureValidationAuthorityPolicy"
        New-ArchitectureValidationTask "A Product Boundary" "Invoke-ProductBoundaryValidation"
        New-ArchitectureValidationTask "B Forbidden Algorithms" "Invoke-ForbiddenAlgorithmsValidation"
        New-ArchitectureValidationTask "M Proof-carrying Pruning" "Invoke-ProofCarryingPruningContractValidation"
        New-ArchitectureValidationTask "C App Boundary" "Invoke-AppBoundaryContractValidation"
        New-ArchitectureValidationTask "CLI Boundary Architecture" "Invoke-CliBoundaryArchitectureValidation"
        New-ArchitectureValidationTask "Test Policy Architecture" "Invoke-TestPolicyArchitectureValidation"
        New-ArchitectureValidationTask "T1 C Core Test Matrix" "Invoke-CCoreTestMatrixContractValidation"
        New-ArchitectureValidationTask "Runner Progress Contract" "Invoke-RunnerProgressContractValidation"
        New-ArchitectureValidationTask "Security Architecture" "Invoke-SecurityArchitectureValidation"
        New-ArchitectureValidationTask "S4 Native Unsafe Boundary" "Invoke-UnsafeBoundaryArchitectureValidation"
        New-ArchitectureValidationTask "T2 Rust FFI Safety Tests" "Invoke-RustFfiSafetyTestsContractValidation"
        New-ArchitectureValidationTask "T3 Coverage Probability Invariants" "Invoke-CoverageProbabilityInvariantTestsContractValidation"
        New-ArchitectureValidationTask "T4 Product E2E Golden Tests" "Invoke-ProductGoldenTestsContractValidation"
        New-ArchitectureValidationTask "T5 Security Regression Tests" "Invoke-SecurityRegressionTestsContractValidation"
        New-ArchitectureValidationTask "SRP Policy Architecture" "Invoke-SrpPolicyArchitectureValidation"
        New-ArchitectureValidationTask "M0 Architecture Reset" "Invoke-ArchitectureResetValidation"
        New-ArchitectureValidationTask "M2 C Memory Scope" "Invoke-CMemoryScopeValidation"
        New-ArchitectureValidationTask "M3 SearchProblem Canonical Model" "Invoke-SearchProblemCanonicalModelValidation"
        New-ArchitectureValidationTask "M4 C Compact Problem Descriptor" "Invoke-CCompactProblemDescriptorValidation"
        New-ArchitectureValidationTask "M5 C Board64 Core" "Invoke-CBoard64CoreValidation"
        New-ArchitectureValidationTask "M6 C Piece Operation Table" "Invoke-CPieceOperationTableValidation"
        New-ArchitectureValidationTask "M7 C Rule Kick Compact Model" "Invoke-CRuleKickCompactModelValidation"
        New-ArchitectureValidationTask "M8 Sfinder Candidate" "Invoke-CSfinderCandidateValidation"
        New-ArchitectureValidationTask "M9 C Reachability" "Invoke-CReachabilityValidation"
        New-ArchitectureValidationTask "M10 C Geometry Packing" "Invoke-CGeometryPackingValidation"
        New-ArchitectureValidationTask "M11 C Host Reducer" "Invoke-CHostReducerValidation"
        New-ArchitectureValidationTask "M12 C BuildUp Problem Builder" "Invoke-CBuildUpProblemBuilderValidation"
        New-ArchitectureValidationTask "M13 C BuildUp Verifier" "Invoke-CBuildUpVerifierValidation"
        New-ArchitectureValidationTask "M14 C Coverage Row Bridge" "Invoke-CCoverageRowBridgeValidation"
        New-ArchitectureValidationTask "M15 Rust Coverage Objective Reducer" "Invoke-RustCoverageObjectiveReducerValidation"
        New-ArchitectureValidationTask "M16 Replay Output Bridge" "Invoke-ReplayOutputBridgeValidation"
        New-ArchitectureValidationTask "M17 Core Executor" "Invoke-CoreExecutorValidation"
        New-ArchitectureValidationTask "M18 CLI Product Path" "Invoke-CliProductPathValidation"
        New-ArchitectureValidationTask "M19 Backend Policy Fallback" "Invoke-BackendPolicyFallbackValidation"
        New-ArchitectureValidationTask "M20 Setup Search Product Path" "Invoke-SetupSearchProductPathValidation"
        New-ArchitectureValidationTask "M21 Build Coverage Product Path" "Invoke-BuildCoverageProductPathValidation"
        New-ArchitectureValidationTask "M22 Rules Kicks Runtime" "Invoke-RulesKicksRuntimeValidation"
        New-ArchitectureValidationTask "M23 Supply Runtime" "Invoke-SupplyRuntimeValidation"
        New-ArchitectureValidationTask "X6 Board128 Wide Backend" "Invoke-Board128WideBackendValidation"
        New-ArchitectureValidationTask "X7 Exact Cover DLX Generalization" "Invoke-ExactCoverDlxGeneralizationValidation"
        New-ArchitectureValidationTask "M24 GPU Packing Backend" "Invoke-GpuPackingBackendValidation"
        New-ArchitectureValidationTask "U1 GPU Packing Backend Contract" "Invoke-GpuPackingBackendContractValidation"
        New-ArchitectureValidationTask "M24G GPU Backend Adapter Contract" "Invoke-GpuBackendAdapterContractValidation"
        New-ArchitectureValidationTask "M24A GPU Batch Source" "Invoke-GpuBatchSourceContractValidation"
        New-ArchitectureValidationTask "M24B GPU Portable Expander" "Invoke-GpuExpanderContractValidation"
        New-ArchitectureValidationTask "M24H Native GPU Finish-or-Remove" "Invoke-GpuRealKernelContractValidation"
        New-ArchitectureValidationTask "M24C GPU Reference Equivalence" "Invoke-GpuReferenceEquivalenceContractValidation"
        New-ArchitectureValidationTask "M24D GPU Host Reducer Product Path" "Invoke-GpuHostReducerContractValidation"
        New-ArchitectureValidationTask "M24E GPU BuildUp Product Path" "Invoke-GpuBuildUpProductPathContractValidation"
        New-ArchitectureValidationTask "M24F GPU Product Equivalence" "Invoke-GpuProductEquivalenceContractValidation"
        New-ArchitectureValidationTask "M24I GPU Scheduler Metrics" "Invoke-GpuSchedulerMetricsContractValidation"
        New-ArchitectureValidationTask "M25 Hybrid Scheduler" "Invoke-HybridSchedulerValidation"
        New-ArchitectureValidationTask "U2 Hybrid Scheduler Contract" "Invoke-HybridSchedulerContractValidation"
        New-ArchitectureValidationTask "U3 GUI Host Boundary" "Invoke-GuiHostBoundaryContractValidation"
        New-ArchitectureValidationTask "U4 Render Exactness Gate" "Invoke-RenderExactnessGateContractValidation"
        New-ArchitectureValidationTask "U5 Asset Import Security" "Invoke-AssetImportSecurityContractValidation"
        New-ArchitectureValidationTask "U6 Tauri Svelte Desktop Host" "Invoke-TauriSvelteDesktopHostContractValidation"
        New-ArchitectureValidationTask "U7 WASM Command Runtime" "Invoke-WasmCommandRuntimeContractValidation"
        New-ArchitectureValidationTask "U8 WebGPU Backend" "Invoke-WebGpuBackendContractValidation"
        New-ArchitectureValidationTask "U9 Job Worker Progress" "Invoke-JobWorkerProgressContractValidation"
        New-ArchitectureValidationTask "T Guarded Expansion" "Invoke-GuardedExpansionContractValidation"
        New-ArchitectureValidationTask "X0 MVP2 Scope Gate" "Invoke-Mvp2ScopeGateContractValidation"
        New-ArchitectureValidationTask "G0 MVP3 Scope Gate" "Invoke-Mvp3ScopeGateContractValidation"
        New-ArchitectureValidationTask "G1 Custom Piece Domain Model" "Invoke-CustomPieceDomainModelContractValidation"
        New-ArchitectureValidationTask "G2 Mixed Supply Generalization" "Invoke-MixedSupplyGeneralizationContractValidation"
        New-ArchitectureValidationTask "G3 Board128 Wide Runtime" "Invoke-Board128WideBackendValidation"
        New-ArchitectureValidationTask "G4 Generic Operation Candidate Reachability" "Invoke-GenericOperationCandidateReachabilityContractValidation"
        New-ArchitectureValidationTask "G5 Area Multiset Feasibility" "Invoke-AreaMultisetFeasibilityContractValidation"
        New-ArchitectureValidationTask "G6 Generic Exact-Cover DLX" "Invoke-GenericExactCoverDlxContractValidation"
        New-ArchitectureValidationTask "G7 Generic BuildUp" "Invoke-GenericBuildUpContractValidation"
        New-ArchitectureValidationTask "G8 Custom Rule Editor" "Invoke-CustomRuleEditorContractValidation"
        New-ArchitectureValidationTask "G9 Generic GPU Unsupported" "Invoke-GenericGpuDescriptorContractValidation"
        New-ArchitectureValidationTask "G10 Custom Skin Theme Editor" "Invoke-CustomSkinThemeEditorContractValidation"
        New-ArchitectureValidationTask "X1 Rule Kick Expansion" "Invoke-RuleKickExpansionContractValidation"
        New-ArchitectureValidationTask "X2 ScoreProfile Object Model" "Invoke-ScoreProfileObjectModelContractValidation"
        New-ArchitectureValidationTask "X3 Spin Target Classifier KickEvidence" "Invoke-SpinTargetContractValidation"
        New-ArchitectureValidationTask "X4 Score-Aware Objective MaxScoreCover" "Invoke-ScoreAwareObjectiveContractValidation"
        New-ArchitectureValidationTask "X5 Setup Raw Metrics v2" "Invoke-SetupRawMetricsV2ContractValidation"
        New-ArchitectureValidationTask "X6 Path Percent Cover CLI" "Invoke-PathPercentCoverCliContractValidation"
        New-ArchitectureValidationTask "X7 Fumen Transform PNG GIF Renderer" "Invoke-FumenRenderProductContractValidation"
        New-ArchitectureValidationTask "X8 GUI Editor Schema v2" "Invoke-GuiEditorSchemaV2ContractValidation"
        New-ArchitectureValidationTask "X9 GPU Packing Strengthening" "Invoke-GpuPackingStrengtheningContractValidation"
        New-ArchitectureValidationTask "X10 MVP2 Acceptance Gate" "Invoke-Mvp2AcceptanceGateContractValidation"
        New-ArchitectureValidationTask "T6 MVP2 Acceptance Tests" "Invoke-Mvp2AcceptanceTestsContractValidation"
        New-ArchitectureValidationTask "G11 MVP3 Acceptance Gate" "Invoke-Mvp3AcceptanceGateContractValidation"
        New-ArchitectureValidationTask "T7 MVP3 Acceptance Tests" "Invoke-Mvp3AcceptanceTestsContractValidation"
        New-ArchitectureValidationTask "M25E GPU Memory Scheduler Safety" "Invoke-GpuStageEMemorySchedulerSafetyValidation"
        New-ArchitectureValidationTask "M25F GPU Output Diagnostic GUI Visibility" "Invoke-GpuStageFVisibilityValidation"
        New-ArchitectureValidationTask "M25W Worker External PC E2E Contract" "Invoke-WorkerE2EContractValidation"
        New-ArchitectureValidationTask "M13 MVP1 Product E2E Closure" "Invoke-ProductE2EClosureContractValidation"
        New-ArchitectureValidationTask "M26 Percent Path Product Slice" "Invoke-PercentPathProductSliceValidation"
        New-ArchitectureValidationTask "M27 Scoring Post Processing" "Invoke-ScoringPostProcessingValidation"
        New-ArchitectureValidationTask "Q PostProcess Pipeline GPU" "Invoke-PostProcessPipelineContractValidation"
        New-ArchitectureValidationTask "R Host Runtime Contract" "Invoke-HostRuntimeContractValidation"
        New-ArchitectureValidationTask "M28 GUI Schema" "Invoke-GuiSchemaValidation"
        New-ArchitectureValidationTask "M29 Diagnostics Security Gate" "Invoke-DiagnosticsSecurityGateValidation"
    )
}

function New-CurrentArchitectureValidationTasks() {
    return @(
        New-ArchitectureValidationTask "Dependency Architecture" "Invoke-DependencyArchitectureValidation" -RequiresWorkspaceDependencyGraph
        New-ArchitectureValidationTask "No Product Debt Architecture" "Invoke-NoProductDebtStaticValidation"
        New-ArchitectureValidationTask "Forbidden API Architecture" "Invoke-ReleaseForbiddenApiValidation"
        New-ArchitectureValidationTask "Public ABI Architecture" "Invoke-PublicAbiContractValidation"
        New-ArchitectureValidationTask "Unsafe Boundary Architecture" "Invoke-UnsafeBoundaryArchitectureValidation"
        New-ArchitectureValidationTask "Unsupported Capability Architecture" "Invoke-UnsupportedCapabilityStaticValidation"
        New-ArchitectureValidationTask "SRP Policy Architecture" "Invoke-SrpPolicyArchitectureValidation"
    )
}

function New-AllArchitectureValidationTasks() {
    $tasks = New-Object System.Collections.Generic.List[object]
    $names = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($task in @(
        @(New-CurrentArchitectureValidationTasks)
        @(New-AdvisoryMarkerAuditTasks)
    )) {
        if ($names.Add($task.Name)) {
            $tasks.Add($task)
        }
    }
    return @($tasks.ToArray())
}
