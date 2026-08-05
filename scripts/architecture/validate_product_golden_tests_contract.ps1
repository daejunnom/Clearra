function Invoke-ProductGoldenTestsContractValidation() {
$requiredFixtureFiles = @(
        "tests/fixtures/product/pc_2l_fixed_queue.json",
        "tests/fixtures/product/pc_4l_fixed_candidate_budget.json",
        "tests/fixtures/product/scenario_clear_to_empty.json",
        "tests/fixtures/product/path_representative.json",
        "tests/fixtures/product/percent_uniform_bag.json",
        "tests/fixtures/product/cover_template_basic.json",
        "tests/fixtures/product/continue_token_basic.json",
        "tests/fixtures/product/rules_verify_basic.json",
        "tests/fixtures/render/render_capability_exact.json"
    )
$requiredGoldenFiles = @(
        "tests/golden/product/pc_2l_fixed_queue.json",
        "tests/golden/product/pc_4l_fixed_candidate_budget.json",
        "tests/golden/product/scenario_clear_to_empty.json",
        "tests/golden/product/path_representative.json",
        "tests/golden/product/percent_uniform_bag.json",
        "tests/golden/product/cover_template_basic.json",
        "tests/golden/product/continue_token_basic.json",
        "tests/golden/product/rules_verify_basic.json",
        "tests/golden/ux/product_pc_2l_fixed_queue.txt",
        "tests/golden/ux/product_pc_4l_fixed_candidate_budget.txt",
        "tests/golden/ux/product_scenario_clear_to_empty.txt",
        "tests/golden/ux/product_path_representative.txt",
        "tests/golden/ux/product_percent_uniform_bag.txt",
        "tests/golden/ux/product_cover_template_basic.txt",
        "tests/golden/ux/product_continue_token_basic.txt",
        "tests/golden/ux/product_rules_verify_basic.txt",
        "tests/golden/render/render_capability_exact.json",
        "tests/golden/diagnostics/security_diagnostic_gate.json"
    )
$requiredTestFiles = @(
        "crates/clearra-cli/tests/product_contract_e2e.rs",
        "crates/clearra-cli/tests/product_golden_t4_contract.rs",
        "crates/clearra-cli/tests/product_cli_surface_contract.rs",
        "scripts/product-e2e.ps1",
        "scripts/lib/product-e2e-t4-golden-cases.ps1",
        "scripts/lib/product-e2e-report.ps1",
        "scripts/ux-smoke.ps1",
        "scripts/desktop-host-check.ps1"
    )
foreach ($relativePath in @($requiredFixtureFiles + $requiredGoldenFiles + $requiredTestFiles)) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $relativePath))) {
            Add-ArchitectureError "T4 Product E2E / Golden required file is missing: $relativePath"
        }
    }
$hasPositiveCommandLimit = {
        param(
            [object[]]$Command,
            [string]$Option
        )

        $optionIndex = -1
        for ($index = 0; $index -lt $Command.Count; $index += 1) {
            if ([string]$Command[$index] -eq $Option) {
                $optionIndex = $index
            }
        }
        if ($optionIndex -lt 0 -or $optionIndex + 1 -ge $Command.Count) {
            return $false
        }

        [uint64]$limit = 0
        return [uint64]::TryParse(
            [string]$Command[$optionIndex + 1],
            [Globalization.NumberStyles]::None,
            [Globalization.CultureInfo]::InvariantCulture,
            [ref]$limit
        ) -and $limit -gt 0
    }
$productFixtureRoot = Join-Path $Root "tests/fixtures/product"
foreach ($fixtureFile in @(Get-ChildItem -LiteralPath $productFixtureRoot -File -Filter "*.json")) {
        $fixture = Get-Content -LiteralPath $fixtureFile.FullName -Raw | ConvertFrom-Json
        if (-not ($fixture.PSObject.Properties.Name -contains "command")) {
            continue
        }

        $expectedText = if ($fixture.PSObject.Properties.Name -contains "expected") {
            $fixture.expected | ConvertTo-Json -Depth 32 -Compress
        } else {
            ""
        }
        $goldenPath = Join-Path `
            (Join-Path $Root "tests/golden/product") `
            $fixtureFile.Name
        $goldenMarkers = if (Test-Path -LiteralPath $goldenPath -PathType Leaf) {
            $golden = Get-Content -LiteralPath $goldenPath -Raw | ConvertFrom-Json
            if ($golden.PSObject.Properties.Name -contains "required_markers") {
                @($golden.required_markers | ForEach-Object { [string]$_ })
            } else {
                @()
            }
        } else {
            @()
        }
        $expectsCandidateBudget =
            $expectedText -match '"candidate_budget_exceeded"' -or
            @($goldenMarkers | Where-Object {
                    $_ -match '(^|=)candidate_budget_exceeded$'
                }).Count -gt 0
        $expectsPatternBudget =
            $expectedText -match '"(materialized_)?pattern_budget_exceeded"' -or
            @($goldenMarkers | Where-Object {
                    $_ -match '(^|=)(materialized_)?pattern_budget_exceeded$'
                }).Count -gt 0
        $command = @($fixture.command)

        if ($expectsCandidateBudget -and
            -not (& $hasPositiveCommandLimit $command "--max-candidates")) {
            Add-ArchitectureError "T4 product fixture '$($fixtureFile.Name)' expects candidate_budget_exceeded and must specify a positive --max-candidates value"
        }
        if ($expectsPatternBudget -and
            -not (& $hasPositiveCommandLimit $command "--max-patterns")) {
            Add-ArchitectureError "T4 product fixture '$($fixtureFile.Name)' expects a pattern budget truncation and must specify a positive --max-patterns value"
        }
    }
$productContractTests = @(
        Read-Text "crates/clearra-cli/tests/product_contract_e2e.rs"
        Read-Text "crates/clearra-cli/tests/product_golden_t4_contract.rs"
        Read-Text "crates/clearra-cli/tests/product_cli_surface_contract.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
        "pc_command_uses_search_problem_core_executor",
        "pc_4l_fixed_candidate_budget_golden_contract_is_stable",
        "scenario_clear_to_empty_golden_contract_is_stable",
        "path_reports_representative_trace",
        "percent_uniform_bag_golden_contract_is_stable",
        "setup_command_assembles_exact_residue_request",
        "cover_reports_build_union_probability",
        "continue_token_roundtrip_compiles_to_search_problem",
        "rules_verify_basic_golden_contract_is_stable",
        "assert_markers",
        "json_from_stdout"
    )) {
        if ($productContractTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-cli product tests must keep T4 marker '$requiredMarker'"
        }
    }
$productE2E = @(
        Read-Text "scripts/product-e2e.ps1"
        Read-Text "scripts/lib/product-e2e-t4-golden-cases.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
        "T4 pc 4L fixed candidate budget golden contract",
        "T4 scenario clear-to-empty golden contract",
        "T4 percent uniform bag golden contract",
        "T4 rules verify basic golden contract",
        "tests/fixtures/product/pc_4l_fixed_candidate_budget.json",
        "tests/fixtures/product/scenario_clear_to_empty.json",
        "tests/fixtures/product/percent_uniform_bag.json",
        "tests/fixtures/product/rules_verify_basic.json",
        "Read-ProductE2ERequiredMarkers",
        "Assert-ProductE2EMarkers",
        "Assert-ProductE2ETypedCommandAssertions"
    )) {
        if ($productE2E -notlike "*$requiredMarker*") {
            Add-ArchitectureError "scripts/product-e2e.ps1 must keep T4 ProductE2E marker '$requiredMarker'"
        }
    }
foreach ($golden in @(
        @{ Path = "tests/golden/product/pc_2l_fixed_queue.json"; Marker = "route=search-problem-core-executor" },
        @{ Path = "tests/golden/product/pc_4l_fixed_candidate_budget.json"; Marker = "route=search-problem-core-executor" },
        @{ Path = "tests/golden/product/pc_4l_fixed_candidate_budget.json"; Marker = "coverage_source=pattern-specific-exact-buildability" },
        @{ Path = "tests/golden/product/scenario_clear_to_empty.json"; Marker = "completion_goal=clear-to-empty" },
        @{ Path = "tests/golden/product/path_representative.json"; Marker = "representative_trace_source=retained-trace" },
        @{ Path = "tests/golden/product/percent_uniform_bag.json"; Marker = "coverage_reducer=pattern-bitset-union" },
        @{ Path = "tests/golden/product/cover_template_basic.json"; Marker = "union_probability_reducer=BuildCoverageResult uses union probability" },
        @{ Path = "tests/golden/product/continue_token_basic.json"; Marker = "compiled_goal=clear-to-empty" },
        @{ Path = "tests/golden/product/rules_verify_basic.json"; Marker = "kick_verification_failures=0" },
        @{ Path = "tests/golden/render/render_capability_exact.json"; Marker = '"render_exact": true' },
        @{ Path = "tests/golden/diagnostics/security_diagnostic_gate.json"; Marker = '"evidence"' }
    )) {
        $contents = Read-Text $golden.Path
        if ($contents -notlike "*$($golden.Marker)*") {
            Add-ArchitectureError "T4 golden '$($golden.Path)' must pin marker '$($golden.Marker)'"
        }
    }
foreach ($uxGolden in @(
        "tests/golden/ux/product_pc_2l_fixed_queue.txt",
        "tests/golden/ux/product_pc_4l_fixed_candidate_budget.txt",
        "tests/golden/ux/product_scenario_clear_to_empty.txt",
        "tests/golden/ux/product_path_representative.txt",
        "tests/golden/ux/product_percent_uniform_bag.txt",
        "tests/golden/ux/product_cover_template_basic.txt",
        "tests/golden/ux/product_continue_token_basic.txt",
        "tests/golden/ux/product_rules_verify_basic.txt"
    )) {
        $contents = Read-Text $uxGolden
        if ([string]::IsNullOrWhiteSpace($contents)) {
            Add-ArchitectureError "T4 UX golden must keep human summary markers: $uxGolden"
        }
    }
$renderCapabilityGolden = Read-Text "tests/golden/render/render_capability_exact.json"
foreach ($requiredMarker in @(
        '"supported": true',
        '"render_exact": true',
        '"runtime_asset_format": "png-atlas"'
    )) {
        if ($renderCapabilityGolden -notlike "*$requiredMarker*") {
            Add-ArchitectureError "T4 render capability golden must disclose exact renderer marker '$requiredMarker'"
        }
    }
$diagnosticsGolden = Read-Text "tests/golden/diagnostics/security_diagnostic_gate.json"
foreach ($requiredMarker in @(
        '"code"',
        '"severity"',
        '"suggested_next_step"',
        '"evidence"'
    )) {
        if ($diagnosticsGolden -notlike "*$requiredMarker*") {
            Add-ArchitectureError "T4 diagnostics golden must keep evidence marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
        "T4 Product E2E / Golden Tests",
        "json_contract_stable",
        "text_output_human_summary_stable",
        "diagnostic_output_contains_evidence",
        "unsupported_features_show_disabled_reason"
    )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document T4 marker '$requiredMarker'"
        }
    }
$testPolicyDoc = Read-Text "docs/test-policy.md"
foreach ($requiredMarker in @(
        "T4 product E2E golden tests",
        "pc_4l_fixed_candidate_budget",
        "scenario_clear_to_empty",
        "render_capability_exact",
        "DesktopHost"
    )) {
        if ($testPolicyDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/test-policy.md must document T4 marker '$requiredMarker'"
        }
    }
}
