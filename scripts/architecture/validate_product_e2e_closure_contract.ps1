function Invoke-ProductE2EClosureContractValidation {
$requiredProductFiles = @(
        "tests/fixtures/product/pc_2l_fixed_queue.json",
        "tests/fixtures/product/path_representative.json",
        "tests/fixtures/product/percent_bag_pattern.json",
        "tests/fixtures/product/setup_basic.json",
        "tests/fixtures/product/cover_template_basic.json",
        "tests/fixtures/product/continue_token_basic.json",
        "tests/fixtures/product/rules_list.json",
        "tests/fixtures/product/scoring_list.json",
        "tests/fixtures/product/convert_fumen_like_json.json",
        "tests/fixtures/product/verify_all.json",
        "tests/fixtures/product/verify_kicks.json",
        "tests/golden/product/pc_2l_fixed_queue.json",
        "tests/golden/product/path_representative.json",
        "tests/golden/product/percent_bag_pattern.json",
        "tests/golden/product/setup_basic.json",
        "tests/golden/product/cover_template_basic.json",
        "tests/golden/product/continue_token_basic.json",
        "tests/golden/product/rules_list.json",
        "tests/golden/product/scoring_list.json",
        "tests/golden/product/convert_fumen_like_json.json",
        "tests/golden/product/verify_all.json",
        "tests/golden/product/verify_kicks.json",
        "tests/golden/ux/product_pc_2l_fixed_queue.txt",
        "tests/golden/ux/product_path_representative.txt",
        "tests/golden/ux/product_percent_bag_pattern.txt",
        "tests/golden/ux/product_setup_basic.txt",
        "tests/golden/ux/product_cover_template_basic.txt",
        "tests/golden/ux/product_continue_token_basic.txt",
        "tests/golden/ux/product_rules_list.txt",
        "tests/golden/ux/product_scoring_list.txt",
        "tests/golden/ux/product_convert_fumen_like_json.txt",
        "tests/golden/ux/product_verify_all.txt",
        "tests/golden/ux/product_verify_kicks.txt"
    )
$requiredProductTestFiles = @(
        "crates/clearra-cli/tests/product_contract_e2e.rs",
        "crates/clearra-cli/tests/product_cli_surface_contract.rs"
    )
foreach ($relativePath in @($requiredProductFiles + $requiredProductTestFiles)) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $relativePath))) {
            Add-ArchitectureError "MVP1 Product E2E closure file is missing: $relativePath"
        }
    }
$productContractE2E = (Read-Text "crates/clearra-cli/tests/product_contract_e2e.rs") + "`n" +
        (Read-Text "crates/clearra-cli/tests/product_cli_surface_contract.rs")
foreach ($requiredMarker in @(
            "product_fixture_stdout",
            "continue_fixture_stdout",
            "pc_command_uses_search_problem_core_executor",
            "path_reports_representative_trace",
            "percent_reports_total_and_covered_pattern_count",
            "setup_reports_raw_metrics_and_union_probability",
            "cover_reports_build_union_probability",
            "continue_token_roundtrip_compiles_to_search_problem",
            "rules_command_uses_app_response_route",
            "scoring_command_uses_app_response_route",
            "convert_command_uses_app_response_route",
            "verify_command_uses_app_response_route",
            "verify_kicks_command_uses_app_response_route",
            "tests/golden/product/pc_2l_fixed_queue.json",
            "tests/golden/product/path_representative.json",
            "tests/golden/product/percent_bag_pattern.json",
            "tests/golden/product/setup_basic.json",
            "tests/golden/product/cover_template_basic.json",
            "tests/golden/product/continue_token_basic.json",
            "tests/golden/product/rules_list.json",
            "tests/golden/product/scoring_list.json",
            "tests/golden/product/convert_fumen_like_json.json",
            "tests/golden/product/verify_all.json",
            "tests/golden/product/verify_kicks.json",
            "coverage_reducer",
            "pattern-bitset-union",
            "search-problem-core-executor",
            "buildup_backend_owner"
        )) {
        if (-not $productContractE2E.Contains($requiredMarker)) {
            Add-ArchitectureError "MVP1 Product E2E closure tests must pin marker '$requiredMarker'"
        }
    }
foreach ($golden in @(
            @{ Path = "tests/golden/product/pc_2l_fixed_queue.json"; Marker = "route=search-problem-core-executor" },
            @{ Path = "tests/golden/product/pc_2l_fixed_queue.json"; Marker = "coverage_reducer=pattern-bitset-union" },
            @{ Path = "tests/golden/product/pc_2l_fixed_queue.json"; Marker = "packing_backend=cpu" },
            @{ Path = "tests/golden/product/pc_2l_fixed_queue.json"; Marker = "buildup_backend_owner=cpu" },
            @{ Path = "tests/golden/product/path_representative.json"; Marker = "representative_trace_source=retained-trace" },
            @{ Path = "tests/golden/product/percent_bag_pattern.json"; Marker = "total_pattern_count=1" },
            @{ Path = "tests/golden/product/percent_bag_pattern.json"; Marker = "renormalized=false" },
            @{ Path = "tests/golden/product/setup_basic.json"; Marker = "setup_raw_metrics=attached" },
            @{ Path = "tests/golden/product/cover_template_basic.json"; Marker = "union_probability_reducer=BuildCoverageResult uses union probability" },
            @{ Path = "tests/golden/product/continue_token_basic.json"; Marker = "compiled_goal=clear-to-empty" },
            @{ Path = "tests/golden/product/rules_list.json"; Marker = "kind=rules" },
            @{ Path = "tests/golden/product/scoring_list.json"; Marker = "kind=scoring" },
            @{ Path = "tests/golden/product/convert_fumen_like_json.json"; Marker = "kind=convert" },
            @{ Path = "tests/golden/product/verify_all.json"; Marker = "build_coverage=ok" },
            @{ Path = "tests/golden/product/verify_kicks.json"; Marker = "kick_verification_failures=0" }
        )) {
        $contractPath = Join-Path $Root $golden.Path
        if (-not (Test-Path -LiteralPath $contractPath)) {
            continue
        }
        $contents = Get-Content -LiteralPath $contractPath -Raw
        if (-not $contents.Contains($golden.Marker)) {
            Add-ArchitectureError "Product E2E closure golden '$($golden.Path)' must pin marker '$($golden.Marker)'"
        }
    }
$verifyAppCommand = Read-Text "crates/clearra-app/src/commands/verify_app_command.rs"
foreach ($requiredVerifyMarker in @(
            "CoverAppCommand::new(default_cover_query())",
            "vec![SlotDomain::new(slot_id, vec![PieceKind::I])]"
        )) {
        if (-not $verifyAppCommand.Contains($requiredVerifyMarker)) {
            Add-ArchitectureError "VerifyAppCommand must keep MVP1 cover verification on the executable product query marker '$requiredVerifyMarker'"
        }
    }
}
