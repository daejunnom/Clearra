function Invoke-CliBoundaryArchitectureValidation() {
$cliDispatcher = Read-Text "crates/clearra-cli/src/output/cli_output_dispatcher.rs"
$commandRenderer = Read-Text "crates/clearra-cli/src/output/command_renderer.rs"
if ($cliDispatcher -like "*RenderFormatDispatcher*" -or $cliDispatcher -like "*clearra_output*") {
        Add-ArchitectureError "CliOutputDispatcher must only handle stdout/stderr/exit code, not render format dispatch"
    }
foreach ($file in Get-RustFiles "crates/clearra-cli/src/commands") {
        $contents = Get-Content -LiteralPath $file.FullName -Raw
        if ($contents.Contains("CliOutput::success(") -and -not $contents.Contains("CommandRenderer::render(")) {
            Add-ArchitectureError "$($file.FullName) must render command success output through CommandRenderer"
        }
        foreach ($line in Get-Content -LiteralPath $file.FullName) {
            if ($line -match 'CliOutput::success\(\s*format!') {
                Add-ArchitectureError "$($file.FullName) must not pass format! directly to CliOutput::success"
            }
            if ($line -match 'CliOutput::success\(\s*"') {
                Add-ArchitectureError "$($file.FullName) must not pass literal strings directly to CliOutput::success"
            }
        }
    }
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $Root "crates/clearra-cli/src/commands") -File -Filter "*_command.rs") {
        $relativePath = Get-RepositoryRelativePath $file.FullName
        $contents = Get-RustProductionContents (Get-Content -LiteralPath $file.FullName -Raw)
        $isConvertCommand = $file.Name -eq "convert_command.rs"

        foreach ($forbiddenPattern in @(
            @{ Pattern = '^\s*use\s+serde::Deserialize\b'; Reason = "commands must not own serde DTOs" },
            @{ Pattern = '^\s*#\[derive\([^\)]*\bDeserialize\b'; Reason = "commands must not own serde DTOs" },
            @{ Pattern = '\bfs::read_to_string\s*\('; Reason = "commands must not own fixture/file IO; use fixture or assemble adapters" },
            @{ Pattern = '\bstd::fs::read_to_string\s*\('; Reason = "commands must not own fixture/file IO; use fixture or assemble adapters" },
            @{ Pattern = '\bserde_json::from_str\s*\('; Reason = "commands must not own raw JSON parsing; use adapter crates or assemblers" },
            @{ Pattern = '^\s*(pub\s+)?struct\s+ScenarioFixture\b'; Reason = "scenario fixture DTO belongs in fixture/pc_scenario_fixture.rs" },
            @{ Pattern = '^\s*(pub\s+)?struct\s+ScenarioFixtureExpected\b'; Reason = "scenario expected contract belongs in fixture/pc_scenario_expected.rs" },
            @{ Pattern = '\bcompare_accepted_(sample|retained)_trace_keys\b'; Reason = "fixture trace-key contract belongs in fixture/pc_scenario_expected.rs" }
        )) {
            if ($isConvertCommand -and $forbiddenPattern.Pattern -eq '\bserde_json::from_str\s*\(') {
                continue
            }
            if ($contents -match $forbiddenPattern.Pattern) {
                Add-ArchitectureError "$relativePath leaks adapter/fixture responsibility: $($forbiddenPattern.Reason)"
            }
        }

        if ($contents -match '^\s*(pub\s+)?struct\s+\w*Fixture\w*\b' -or
            $contents -match '^\s*(pub\s+)?struct\s+\w*Expected\w*\b') {
            Add-ArchitectureError "$relativePath must not define fixture/expected DTO structs inside a command handler"
        }
    }
$commandRendererProduction = Get-RustProductionContents $commandRenderer
if ($commandRendererProduction -match '\btyped_value_for_key\b') {
        $typedValueLineCount = Get-FunctionLineCount $commandRendererProduction "typed_value_for_key"
        if ($typedValueLineCount -gt 100) {
            Add-ArchitectureError "CommandRenderer typed_value_for_key is $typedValueLineCount lines; CLI must not own domain key inference"
        }
        foreach ($forbiddenMarker in @("_count", "_available", "_complete", "_probability", "_score", "total_solution_count", "retained_trace_count")) {
            if ($commandRendererProduction -like "*$forbiddenMarker*") {
                Add-ArchitectureError "CommandRenderer must not infer typed JSON values from domain key marker '$forbiddenMarker'"
            }
        }
    }
}
function Invoke-CliCommandSurfaceArchitectureValidation() {
    . (Join-Path $PSScriptRoot "validate_cli_command_surface_contract_checks.ps1")
}