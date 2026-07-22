# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Get-SrpSourceRoots() {
    foreach ($relativeRoot in @('crates', 'core-c', 'scripts', 'tools', 'apps', 'packages', 'gui')) {
        $path = Join-Path $Root $relativeRoot
        if (Test-Path -LiteralPath $path) { Get-Item -LiteralPath $path }
    }
}

function Test-SrpExcludedPath([string]$FullName) {
    return $FullName -match '[\\/](node_modules|dist|dist-server|build|coverage|models|checkpoints|\.cache|target)[\\/]' -or
        $FullName -match '[\\/]tools[\\/]vendor[\\/]'
}

function Assert-NoGeneratedFragmentDirectoryPattern() {
    $forbiddenNames = @(
        '*_steps', '*_functions', '*_tests_functions', '*_tests_cases',
        '*_cases', '*_api', 'impl_*_methods'
    )
    foreach ($root in Get-SrpSourceRoots) {
        foreach ($directory in Get-ChildItem -LiteralPath $root.FullName -Directory -Recurse) {
            if (Test-SrpExcludedPath $directory.FullName) { continue }
            foreach ($pattern in $forbiddenNames) {
                if ($directory.Name -like $pattern) {
                    Add-ArchitectureError "$(Get-RepositoryRelativePath $directory.FullName) is a file-level fragment tree; keep cohesive private helpers and behavior tests with their owner module"
                    break
                }
            }
        }
    }
}

function Assert-NoCMakeManifestFragmentTrees() {
    foreach ($relativePath in @('core-c/cmake/sources', 'core-c/cmake/tests')) {
        if (Test-Path -LiteralPath (Join-Path $Root $relativePath)) {
            Add-ArchitectureError "$relativePath splits one CMake manifest into file-per-section fragments"
        }
    }
}

function Assert-NoEmptyRustInherentImplShells() {
    $pattern = '(?ms)^[ \t]*impl(?<head>[^\{\r\n]*)\{[ \t\r\n]*\}'
    foreach ($file in Get-SrpGovernedFiles | Where-Object { $_.Extension -eq '.rs' }) {
        $text = Get-Content -LiteralPath $file.FullName -Raw
        foreach ($match in [regex]::Matches($text, $pattern)) {
            if ($match.Groups['head'].Value -notmatch '\bfor\b') {
                Add-ArchitectureError "$(Get-RepositoryRelativePath $file.FullName) contains an empty inherent impl shell left by method-per-file splitting"
            }
        }
    }
}

function Assert-NoEmptyCTranslationUnits() {
    $allowedAggregate = 'core-c\tests\test_board64.c'
    foreach ($file in Get-SrpGovernedFiles | Where-Object { $_.Extension -eq '.c' }) {
        $relative = (Get-RepositoryRelativePath $file.FullName) -replace '^[.][\\/]', ''
        if ($relative -eq $allowedAggregate) { continue }
        $text = Get-Content -LiteralPath $file.FullName -Raw
        $body = [regex]::Replace($text, '(?s)/\*.*?\*/', '')
        $body = [regex]::Replace($body, '(?m)//.*$', '')
        $body = [regex]::Replace($body, '(?m)^\s*#.*$', '')
        if ([string]::IsNullOrWhiteSpace($body)) {
            Add-ArchitectureError "$relative is an include/comment-only C translation unit"
        }
    }
}

function Assert-NoRustTestRootSupportTargets() {
    foreach ($crateTests in Get-ChildItem -LiteralPath (Join-Path $Root 'crates') -Directory -Recurse |
            Where-Object { $_.Name -eq 'tests' }) {
        foreach ($file in Get-ChildItem -LiteralPath $crateTests.FullName -File -Filter '*_support.rs') {
            Add-ArchitectureError "$(Get-RepositoryRelativePath $file.FullName) is auto-discovered as an integration test target; keep support below its owning test directory"
        }
    }
}

function Assert-NoLiteralRustImplementationIncludes() {
    foreach ($file in Get-SrpGovernedFiles | Where-Object { $_.Extension -eq '.rs' }) {
        $text = Get-Content -LiteralPath $file.FullName -Raw
        if ($text -match 'include!\s*\(\s*"[^"]+\.rs"\s*\)') {
            Add-ArchitectureError "$(Get-RepositoryRelativePath $file.FullName) uses a literal Rust implementation fragment; merge it into the cohesive owner module"
        }
    }
}

function Assert-NoCImplementationIncludes() {
    $allowedAggregate = 'core-c\tests\test_board64.c'
    foreach ($file in Get-SrpGovernedFiles | Where-Object { $_.Extension -eq '.c' }) {
        $relative = (Get-RepositoryRelativePath $file.FullName) -replace '^[.][\\/]', ''
        if ($relative -eq $allowedAggregate) { continue }
        $text = Get-Content -LiteralPath $file.FullName -Raw
        if ($text -match '(?m)^\s*#include\s+"[^"]+\.c"') {
            Add-ArchitectureError "$relative includes a C implementation fragment; private functions belong in the cohesive translation unit"
        }
    }
}

function Assert-NoPrivateHelperModuleClusters() {
    foreach ($file in Get-SrpGovernedFiles | Where-Object { $_.Extension -eq '.rs' }) {
        $text = Get-Content -LiteralPath $file.FullName -Raw
        if ($text -match '(?m)^\s*mod\s+helper_[A-Za-z0-9_]*\s*\{') {
            Add-ArchitectureError "$(Get-RepositoryRelativePath $file.FullName) wraps private helper functions in method-per-module shells"
        }
    }

    $typescriptHelpers = foreach ($file in Get-SrpGovernedFiles |
            Where-Object { $_.Extension -in @('.ts', '.tsx') }) {
        $lineCount = @(Get-Content -LiteralPath $file.FullName).Count
        if ($lineCount -gt 120 -or
            $file.BaseName -notmatch '^(start|cancel|complete|is)[A-Z]') {
            continue
        }
        $text = Get-Content -LiteralPath $file.FullName -Raw
        $functionCount = [regex]::Matches(
            $text,
            '(?m)^\s*(export\s+)?(async\s+)?function\s+|^\s*(export\s+)?const\s+\w+\s*=\s*(async\s*)?\('
        ).Count
        if ($functionCount -eq 1) {
            $file
        }
    }
    foreach ($cluster in @($typescriptHelpers | Group-Object DirectoryName)) {
        if ($cluster.Count -ge 3) {
            $names = @($cluster.Group | ForEach-Object Name) -join ', '
            Add-ArchitectureError "$($cluster.Name) contains a one-function private helper cluster ($names); keep one parent-state registry module"
        }
    }
}

function Assert-PriorityBehaviorModuleBoundaries() {
    $moduleGroups = @(
        @{
            Owner = 'crates/clearra-core-executor/src/buildup/buildup_runner_tests.rs'
            Children = @(
                'buildup_runner_behavior/native_behavior.rs',
                'buildup_runner_behavior/coverage_source.rs',
                'buildup_runner_behavior/objective.rs',
                'buildup_runner_behavior/execution_retention.rs',
                'buildup_runner_behavior/replay_trace.rs'
            )
        },
        @{
            Owner = 'crates/clearra-output/src/json/json_contract_tests.rs'
            Children = @(
                'json_contract_behavior/render_contract.rs',
                'json_contract_behavior/diagnostic_replay_contract.rs',
                'json_contract_behavior/pc_score_contract.rs',
                'json_contract_behavior/pc_trace_contract.rs',
                'json_contract_behavior/pc_backend_contract.rs',
                'json_contract_behavior/resource_contract.rs'
            )
        },
        @{
            Owner = 'crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_contract_tests.rs'
            Children = @(
                'gpu_worker_contract_behavior/descriptor.rs',
                'gpu_worker_contract_behavior/scheduling.rs',
                'gpu_worker_contract_behavior/trust_fallback.rs',
                'gpu_worker_contract_behavior/memory_lifetime.rs'
            )
        },
        @{
            Owner = 'crates/clearra-core-executor/src/spin/spin_target_runner_tests.rs'
            Children = @(
                'spin_target_runner_behavior/recognition.rs',
                'spin_target_runner_behavior/unknown_policy.rs',
                'spin_target_runner_behavior/coverage_probability.rs',
                'spin_target_runner_behavior/kick_evidence.rs',
                'spin_target_runner_behavior/replay_execution.rs'
            )
        }
    )

    foreach ($group in $moduleGroups) {
        $ownerPath = Join-Path $Root $group.Owner
        if (-not (Test-Path -LiteralPath $ownerPath)) {
            Add-ArchitectureError "SRP behavior owner is missing: $($group.Owner)"
            continue
        }
        $ownerText = Get-Content -LiteralPath $ownerPath -Raw
        if ($ownerText -match '#\[test\]' -or $ownerText -match '(?m)^\s*mod\s+case_') {
            Add-ArchitectureError "$($group.Owner) must index shared fixtures and behavior modules, not own test cases"
        }
        $ownerDirectory = Split-Path -Parent $ownerPath
        foreach ($child in $group.Children) {
            $childPath = Join-Path $ownerDirectory $child
            if (-not (Test-Path -LiteralPath $childPath)) {
                Add-ArchitectureError "$($group.Owner) behavior module is missing: $child"
                continue
            }
            $childText = Get-Content -LiteralPath $childPath -Raw
            if ($childText -notmatch '#\[test\]' -or $childText -notmatch '(?m)^\s*mod\s+case_') {
                Add-ArchitectureError "$child must own executable behavior tests, not marker-only structure"
            }
            if ($ownerText -notlike "*$child*") {
                Add-ArchitectureError "$($group.Owner) must declare behavior module $child"
            }
        }
    }
}

function Get-PowerShellFunctionNames([string]$RelativePath) {
    $tokens = $null
    $parseErrors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $Root $RelativePath),
        [ref]$tokens,
        [ref]$parseErrors
    )
    if ($parseErrors.Count -gt 0) {
        return @()
    }
    return @($ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst]
    }, $true) | ForEach-Object Name)
}

function Assert-ProductE2EStageBoundaries() {
    $rootScript = 'scripts/product-e2e.ps1'
    $stageFunctions = [ordered]@{
        'scripts/lib/product-e2e-build.ps1' = @(
            'Remove-StaleProductE2EClearraCliBinary', 'Resolve-ProductE2EBinary',
            'Get-ProductE2ECoreBuildDir', 'Find-ProductE2ENativeLibraryDir',
            'Resolve-ProductE2ENativeLibraryDir'
        )
        'scripts/lib/product-e2e-assertions.ps1' = @(
            'Get-ProductE2EExcerpt', 'ConvertTo-ProductE2EScalar',
            'Add-ProductE2EJsonMarkers', 'ConvertTo-ProductE2EMarkerText',
            'Read-ProductE2ERequiredMarkers', 'Assert-ProductE2EMarkers',
            'New-ProductE2EFailureMessage', 'Get-ProductE2EFixtureMaterial'
        )
        'scripts/lib/product-e2e-run.ps1' = @(
            'Invoke-ProductE2EClearra', 'Invoke-ProductE2ECommandCase',
            'Invoke-ProductE2EFixtureCase', 'Invoke-ProductE2EBackendParityCase',
            'Invoke-ProductE2EBackendCapabilityReportCase',
            'Invoke-ProductE2EBackendEquivalenceCase',
            'Invoke-ProductE2EOpening2LBackendEquivalenceCase',
            'Invoke-ProductE2EScenario4LBackendEquivalenceCase',
            'Invoke-ProductE2EGpuNoFallbackCase',
            'Invoke-ProductE2EGpuNoFallbackUnavailableCase',
            'Invoke-ProductE2EGpuAllowFallbackReasonCase',
            'Invoke-ProductE2EGpuBackendTrustStateCase', 'Get-FixtureCommandArgs'
        )
        'scripts/lib/product-e2e-report.ps1' = @('Write-ProductE2EReport')
    }

    if (@(Get-PowerShellFunctionNames $rootScript).Count -ne 0) {
        Add-ArchitectureError "$rootScript must orchestrate Product E2E stages without owning stage functions"
    }
    $rootText = Read-Text $rootScript
    foreach ($stage in $stageFunctions.Keys) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $stage))) {
            Add-ArchitectureError "Product E2E stage is missing: $stage"
            continue
        }
        $actual = @(Get-PowerShellFunctionNames $stage | Sort-Object)
        $expected = @($stageFunctions[$stage] | Sort-Object)
        if (@(Compare-Object $expected $actual).Count -gt 0) {
            Add-ArchitectureError "$stage function ownership differs from its build/preflight/assert/run/report stage contract"
        }
        $leaf = Split-Path -Leaf $stage
        if ($rootText -notlike "*$leaf*") {
            Add-ArchitectureError "$rootScript must compose Product E2E stage $leaf"
        }
    }
}

function Assert-AuditedSrpBoundaries() {
    $packingStages = @(
        'core-c/src/packing/geometry_catalog.c',
        'core-c/src/packing/geometry_catalog_internal.h',
        'core-c/src/packing/geometry_exact_cover.c',
        'core-c/src/packing/geometry_residual_memo.c',
        'core-c/src/packing/geometry_solution_graph.c',
        'core-c/src/packing/geometry_buildable_stream.c',
        'core-c/src/packing/packing_candidate_materializer.c',
        'core-c/src/buildup/buildup_search.c'
    )
    foreach ($stage in $packingStages) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $stage))) {
            Add-ArchitectureError "Geometry exact-cover SRP stage is missing: $stage"
        }
    }
    # SRP ownership is a physical-file property. Read-Text intentionally expands
    # CMake companion units and Rust modules, which would make every cohesive
    # owner appear to contain all of its collaborators.
    $orchestrator = Read-PhysicalText 'core-c/src/packing/geometry_exact_cover.c'
    foreach ($foreignResponsibility in @(
        'clearra_placement_candidates_generate(',
        'clearra_geometry_catalog_compile(',
        'clearra_geometry_catalog_release(',
        'clearra_packing_host_reduce('
    )) {
        if ($orchestrator -like "*$foreignResponsibility*") {
            Add-ArchitectureError "Geometry exact-cover search owns foreign stage '$foreignResponsibility'"
        }
    }

    $scriptFunctions = [ordered]@{
        'scripts/clearra.ps1' = @()
        'scripts/lib/product-process-surface.ps1' = @(
            'Get-ClearraBuiltBinaryPath', 'Ensure-ClearraBuiltBinary',
            'Invoke-ProductE2EBuiltTask',
            'Set-ClearraReleaseUxSmokeBinaryArgs', 'Invoke-ProductProcessE2ETask'
        )
        'scripts/lib/gpu-worker-tasks.ps1' = @(
            'Invoke-GpuWorkerCargoCheck', 'Invoke-GpuWorkerAcceptanceTask',
            'Invoke-GpuWorkerNativeTask'
        )
        'scripts/lib/clearra-task-dispatch.ps1' = @(
            'Invoke-StrictProductPathTask', 'Invoke-ClearraTask'
        )
    }
    foreach ($script in $scriptFunctions.Keys) {
        $actual = @(Get-PowerShellFunctionNames $script | Sort-Object)
        $expected = @($scriptFunctions[$script] | Sort-Object)
        if (@(Compare-Object $expected $actual).Count -gt 0) {
            Add-ArchitectureError "$script function ownership differs from its SRP contract"
        }
    }

    $processOwner = 'crates/clearra-cli/tests/process_e2e.rs'
    $processOwnerText = Read-PhysicalText $processOwner
    if ($processOwnerText -match '#\[test\]') {
        Add-ArchitectureError "$processOwner must own shared process fixtures and behavior modules only"
    }
    foreach ($behavior in @(
        'process_e2e_opening.rs', 'process_e2e_scenario.rs', 'process_e2e_routing.rs',
        'process_e2e_path_percent.rs', 'process_e2e_setup_backend.rs',
        'process_e2e_verify_continue.rs'
    )) {
        $relative = "crates/clearra-cli/tests/process_e2e/$behavior"
        if (-not (Test-Path -LiteralPath (Join-Path $Root $relative))) {
            Add-ArchitectureError "process E2E behavior module is missing: $relative"
            continue
        }
        if ((Read-Text $relative) -notmatch '#\[test\]') {
            Add-ArchitectureError "$relative must contain executable behavior tests"
        }
        if ($processOwnerText -notlike "*$behavior*") {
            Add-ArchitectureError "$processOwner must declare $behavior"
        }
    }
}

function Assert-LargeModulesCarryPermanentCohesionRationale() {
    foreach ($file in Get-SrpGovernedFiles) {
        $lineCount = @(Get-Content -LiteralPath $file.FullName).Count
        if ($lineCount -lt 1000) { continue }
        $text = Get-Content -LiteralPath $file.FullName -Raw
        if ($text -notmatch '(?im)SRP rationale:\s*.+change reason') {
            Add-ArchitectureError "$(Get-RepositoryRelativePath $file.FullName) has $lineCount lines and needs a permanent behavior-level SRP rationale describing its single change reason"
        }
        if ($text -match '(?im)SRP rationale:.*(temporary|expires|expiry)') {
            Add-ArchitectureError "$(Get-RepositoryRelativePath $file.FullName) uses a temporary large-file exemption; permanent cohesion rationale is required"
        }
    }
}

function Assert-PowerShellSourcesParse() {
    foreach ($file in Get-SrpGovernedFiles | Where-Object { $_.Extension -in @('.ps1', '.psm1') }) {
        $tokens = $null
        $parseErrors = $null
        [System.Management.Automation.Language.Parser]::ParseFile(
            $file.FullName,
            [ref]$tokens,
            [ref]$parseErrors
        ) | Out-Null
        if ($parseErrors.Count -gt 0) {
            Add-ArchitectureError "$(Get-RepositoryRelativePath $file.FullName) does not parse as PowerShell"
        }
    }
}

function Assert-CohesiveSrpPolicyDocumented() {
    $policy = Read-Text 'docs/srp-debt.md'
    foreach ($marker in @(
        'one reason to change',
        'size alone is not SRP debt',
        'private helpers stay with their cohesive owner',
        'tests group domain behavior',
        'method-per-file fragmentation is forbidden',
        'marker-per-file validation is forbidden',
        'one-function private helper cluster',
        'SRP rationale',
        'temporary large-file exemption is forbidden',
        'build, run, assertions, and report'
    )) {
        if ($policy -notlike "*$marker*") {
            Add-ArchitectureError "docs/srp-debt.md must document cohesive SRP marker '$marker'"
        }
    }
}

function Invoke-SrpPolicyArchitectureValidation() {
    Invoke-FileSizeArchitectureValidation
    Assert-NoGeneratedFragmentDirectoryPattern
    Assert-NoCMakeManifestFragmentTrees
    Assert-NoEmptyRustInherentImplShells
    Assert-NoEmptyCTranslationUnits
    Assert-NoRustTestRootSupportTargets
    Assert-NoLiteralRustImplementationIncludes
    Assert-NoCImplementationIncludes
    Assert-NoPrivateHelperModuleClusters
    Assert-PriorityBehaviorModuleBoundaries
    Assert-ProductE2EStageBoundaries
    Assert-AuditedSrpBoundaries
    Assert-LargeModulesCarryPermanentCohesionRationale
    Assert-PowerShellSourcesParse
    Assert-CohesiveSrpPolicyDocumented
}
