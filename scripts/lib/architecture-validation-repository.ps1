# This file is dot-sourced inside Invoke-ArchitectureValidation after Root/Errors/Warnings are initialized.
# Keep repository readers, dependency graph helpers, and import guards here instead of in the dispatcher.

function Get-RustPathAttributeCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

$directory = Split-Path -Parent $FullPath
$matches = [regex]::Matches(
    $Text,
    '#\[path\s*=\s*["''](?<path>[^"'']+)["'']\]\s*\r?\n\s*mod\s+[A-Za-z_][A-Za-z0-9_]*\s*;'
)
foreach ($match in $matches) {
    [System.IO.Path]::GetFullPath((Join-Path $directory $match.Groups['path'].Value))
}
}

function Get-RustModuleCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

$directory = Split-Path -Parent $FullPath
$stem = [System.IO.Path]::GetFileNameWithoutExtension($FullPath)
$moduleRoot = if ($stem -in @('lib', 'main', 'mod')) {
    $directory
} else {
    Join-Path $directory $stem
}
$matches = [regex]::Matches(
    $Text,
    '(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?<name>[A-Za-z_][A-Za-z0-9_]*)\s*;'
)
foreach ($match in $matches) {
    $name = $match.Groups['name'].Value
    $hasExplicitPath = [regex]::IsMatch(
        $Text,
        '#\[path\s*=\s*["''][^"'']+["'']\]\s*\r?\n\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+' + [regex]::Escape($name) + '\s*;'
    )
    if ($hasExplicitPath) {
        continue
    }
    foreach ($candidate in @(
        (Join-Path $moduleRoot "$name.rs"),
        (Join-Path $moduleRoot "$name/mod.rs")
    )) {
        if (Test-Path -LiteralPath $candidate) {
            [System.IO.Path]::GetFullPath($candidate)
            break
        }
    }
}
}

function Get-RustIncludeCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

$directory = Split-Path -Parent $FullPath
$matches = [regex]::Matches($Text, 'include!\s*\(\s*["''](?<path>[^"'']+)["'']\s*\)')
foreach ($match in $matches) {
    [System.IO.Path]::GetFullPath((Join-Path $directory $match.Groups['path'].Value))
}
}

function Get-RustLogicalCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

Get-RustPathAttributeCompanionPaths -FullPath $FullPath -Text $Text
Get-RustModuleCompanionPaths -FullPath $FullPath -Text $Text
Get-RustIncludeCompanionPaths -FullPath $FullPath -Text $Text
}

function Get-CLocalIncludeCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

$directory = Split-Path -Parent $FullPath
$matches = [regex]::Matches($Text, '(?m)^\s*#\s*include\s*["''](?<path>[^"'']+)["'']')
foreach ($match in $matches) {
    $candidate = Join-Path $directory $match.Groups['path'].Value
    if (Test-Path -LiteralPath $candidate) {
        [System.IO.Path]::GetFullPath($candidate)
    }
}
}

function Get-CImplementationIncludeCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

$directory = Split-Path -Parent $FullPath
$matches = [regex]::Matches($Text, '(?m)^\s*#\s*include\s*["''](?<path>[^"'']+\.c)["'']')
foreach ($match in $matches) {
    $candidate = Join-Path $directory $match.Groups['path'].Value
    if (Test-Path -LiteralPath $candidate) {
        [System.IO.Path]::GetFullPath($candidate)
    }
}
}

function Get-CNamespaceCompanionPaths {
param([string]$FullPath)

$directory = Split-Path -Parent $FullPath
$stem = [System.IO.Path]::GetFileNameWithoutExtension($FullPath)
$namespace = ($stem -split '_', 2)[0]
Get-ChildItem -LiteralPath $directory -File -Filter "$namespace`_*.c" |
    Sort-Object Name |
    ForEach-Object { $_.FullName }
}

function Get-CLogicalCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

$extension = [System.IO.Path]::GetExtension($FullPath)
if ($extension.Equals('.c', [System.StringComparison]::OrdinalIgnoreCase)) {
    Get-CImplementationIncludeCompanionPaths -FullPath $FullPath -Text $Text
    Get-CNamespaceCompanionPaths -FullPath $FullPath
} elseif ($extension.Equals('.h', [System.StringComparison]::OrdinalIgnoreCase)) {
    Get-CLocalIncludeCompanionPaths -FullPath $FullPath -Text $Text
}
}

function Get-PowerShellLogicalCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

$directory = Split-Path -Parent $FullPath
$matches = [regex]::Matches(
    $Text,
    '\.\s*\(Join-Path\s+\$PSScriptRoot\s+["''](?<path>[^"'']+)["'']\)'
)
foreach ($match in $matches) {
    [System.IO.Path]::GetFullPath((Join-Path $directory $match.Groups['path'].Value))
}
}

function Get-TypeScriptLogicalCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

$directory = Split-Path -Parent $FullPath
$matches = [regex]::Matches(
    $Text,
    '(?m)(?:from\s+|export\s+[^;]*?from\s+)["''](?<path>\.\.?/[^"'']+)["'']'
)
foreach ($match in $matches) {
    $relativePath = $match.Groups['path'].Value
    $basePath = [System.IO.Path]::GetFullPath((Join-Path $directory $relativePath))
    $candidates = if ([System.IO.Path]::HasExtension($basePath)) {
        @($basePath)
    } else {
        @(
            "$basePath.ts",
            "$basePath.tsx",
            "$basePath.js",
            "$basePath.svelte",
            (Join-Path $basePath 'index.ts'),
            (Join-Path $basePath 'index.js')
        )
    }
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            [System.IO.Path]::GetFullPath($candidate)
            break
        }
    }
}
}

function Get-CMakeLogicalCompanionPaths {
param(
    [string]$FullPath,
    [string]$Text
)

$directory = Split-Path -Parent $FullPath
$matches = [regex]::Matches($Text, '(?m)^\s*include\(\s*(?<path>[^)$\s]+)\s*\)')
foreach ($match in $matches) {
    $relativePath = $match.Groups['path'].Value
    $candidate = Join-Path $directory $relativePath
    if (-not [System.IO.Path]::HasExtension($candidate)) { $candidate += '.cmake' }
    if (Test-Path -LiteralPath $candidate) {
        [System.IO.Path]::GetFullPath($candidate)
    }
}
}

function Read-LogicalRepositoryText {
param(
    [string]$FullPath,
    [System.Collections.Generic.HashSet[string]]$Visited
)

$canonicalPath = [System.IO.Path]::GetFullPath($FullPath)
$rootValue = if ($Root -is [System.Management.Automation.PathInfo]) {
    $Root.Path
} else {
    [string]$Root
}
$rootPath = [System.IO.Path]::GetFullPath($rootValue)
if (-not $canonicalPath.StartsWith($rootPath, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Logical repository read escaped the workspace: $canonicalPath"
}
if (-not $Visited.Add($canonicalPath)) {
    return ""
}

$text = Get-Content -LiteralPath $canonicalPath -Raw
$parts = [System.Collections.Generic.List[string]]::new()
$parts.Add($text)
$companions = switch ([System.IO.Path]::GetExtension($canonicalPath).ToLowerInvariant()) {
    '.rs' { @(Get-RustLogicalCompanionPaths -FullPath $canonicalPath -Text $text); break }
    '.ps1' { @(Get-PowerShellLogicalCompanionPaths -FullPath $canonicalPath -Text $text); break }
    '.c' { @(Get-CLogicalCompanionPaths -FullPath $canonicalPath -Text $text); break }
    '.h' { @(Get-CLogicalCompanionPaths -FullPath $canonicalPath -Text $text); break }
    '.ts' { @(Get-TypeScriptLogicalCompanionPaths -FullPath $canonicalPath -Text $text); break }
    '.tsx' { @(Get-TypeScriptLogicalCompanionPaths -FullPath $canonicalPath -Text $text); break }
    '.js' { @(Get-TypeScriptLogicalCompanionPaths -FullPath $canonicalPath -Text $text); break }
    '.svelte' { @(Get-TypeScriptLogicalCompanionPaths -FullPath $canonicalPath -Text $text); break }
    '.cmake' { @(Get-CMakeLogicalCompanionPaths -FullPath $canonicalPath -Text $text); break }
    '.txt' {
        if ([System.IO.Path]::GetFileName($canonicalPath) -eq 'CMakeLists.txt') {
            @(Get-CMakeLogicalCompanionPaths -FullPath $canonicalPath -Text $text)
        } else {
            @()
        }
        break
    }
    default { @() }
}
foreach ($companion in $companions) {
    if (Test-Path -LiteralPath $companion) {
        $parts.Add((Read-LogicalRepositoryText -FullPath $companion -Visited $Visited))
    }
}
return $parts -join "`n"
}

function Read-Text([string]$RelativePath) {
$fullPath = [System.IO.Path]::GetFullPath((Join-Path $Root $RelativePath))
$cacheVariable = Get-Variable -Name LogicalRepositoryTextCache -Scope Script -ErrorAction SilentlyContinue
if ($null -eq $cacheVariable -or $null -eq $cacheVariable.Value) {
    $script:LogicalRepositoryTextCache = @{}
}
if ($script:LogicalRepositoryTextCache.ContainsKey($fullPath)) {
    return $script:LogicalRepositoryTextCache[$fullPath]
}
$visited = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
)
$logicalText = Read-LogicalRepositoryText -FullPath $fullPath -Visited $visited
$script:LogicalRepositoryTextCache[$fullPath] = $logicalText
return $logicalText
}

function Read-PhysicalText([string]$RelativePath) {
Get-Content -LiteralPath (Join-Path $Root $RelativePath) -Raw
}

function Get-PcServiceValidationSurface() {
    return @(
        (Read-Text "crates/clearra-core-executor/src/service/pc_service.rs"),
        (Read-Text "crates/clearra-core-executor/src/service/pc_output_model_builder.rs"),
        (Read-Text "crates/clearra-core-executor/src/service/pc_pipeline_fields.rs"),
        (Read-Text "crates/clearra-app/src/app_services.rs"),
        (Read-Text "crates/clearra-postprocess/src/pc_scoring/pc_scoring_postprocessor.rs"),
        (Read-Text "crates/clearra-core-executor/src/service/pc_policy_labels.rs"),
        (Read-Text "crates/clearra-core-executor/src/service/pc_backend_report_adapter.rs"),
        (Read-Text "crates/clearra-core-executor/src/service/pc_service_tests.rs")
    ) -join "`n"
}
function Get-BuildUpRunnerValidationSurface() {
    return (Get-RustFiles "crates/clearra-core-executor/src/buildup" |
            Sort-Object FullName |
            ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }) -join "`n"
}
function Get-JsonContractValidationSurface() {
    return @(
        (Read-Text "crates/clearra-output/src/json/json_contract.rs"),
        (Read-Text "crates/clearra-output/src/json/backend_gpu_worker_contract.rs"),
        (Read-Text "crates/clearra-output/src/json/product_json_contract.rs"),
        (Read-Text "crates/clearra-output/src/json/pc_json_contract.rs"),
        (Read-Text "crates/clearra-output/src/json/setup_json_contract.rs"),
        (Read-Text "crates/clearra-output/src/json/json_contract_helpers.rs"),
        (Read-Text "crates/clearra-output/src/json/replay_json_contract.rs"),
        (Read-Text "crates/clearra-output/src/json/json_contract_tests.rs")
    ) -join "`n"
}function Get-GpuTestsValidationSurface() {
    return @(
        (Read-Text "core-c/tests/gpu_tests.c"),
        (Read-Text "core-c/tests/gpu_test_support.h"),
        (Read-Text "core-c/tests/gpu_test_support.c"),
        (Read-Text "core-c/tests/gpu_descriptor_tests.c"),
        (Read-Text "core-c/tests/gpu_backend_adapter_tests.c"),
        (Read-Text "core-c/tests/gpu_expander_tests.c"),
        (Read-Text "core-c/tests/gpu_kernel_tests.c"),
        (Read-Text "core-c/tests/gpu_reference_tests.c"),
        (Read-Text "core-c/tests/gpu_worker_tests.c")
    ) -join "`n"
}function Get-CandidateTestsValidationSurface() {
    return @(
        (Read-Text "core-c/tests/candidate_tests.c"),
        (Read-Text "core-c/tests/candidate_tests_support.h"),
        (Read-Text "core-c/tests/candidate_tests_support.c"),
        (Read-Text "core-c/tests/candidate_harddrop_tests.c"),
        (Read-Text "core-c/tests/candidate_locked_tests.c"),
        (Read-Text "core-c/tests/candidate_kick_transition_tests.c"),
        (Read-Text "core-c/tests/candidate_cache_dedupe_tests.c")
    ) -join "`n"
}function Get-PackingTestsValidationSurface() {
    return @(
        (Read-Text "core-c/tests/packing_tests.c"),
        (Read-Text "core-c/tests/packing_tests_support.h"),
        (Read-Text "core-c/tests/packing_tests_support.c"),
        (Read-Text "core-c/tests/packing_problem_tests.c"),
        (Read-Text "core-c/tests/packing_window_tests.c"),
        (Read-Text "core-c/tests/packing_buffer_hash_tests.c"),
        (Read-Text "core-c/tests/packing_operation_set_tests.c")
    ) -join "`n"
}function Get-SchedulerTestsValidationSurface() {
    return @(
        (Read-Text "core-c/tests/scheduler_tests.c"),
        (Read-Text "core-c/tests/scheduler_tests_support.h"),
        (Read-Text "core-c/tests/scheduler_tests_support.c"),
        (Read-Text "core-c/tests/scheduler_gpu_product_tests.c"),
        (Read-Text "core-c/tests/scheduler_backpressure_tests.c"),
        (Read-Text "core-c/tests/scheduler_autotune_tests.c"),
        (Read-Text "core-c/tests/scheduler_memory_fallback_tests.c")
    ) -join "`n"
}function Get-BuildUpTestsValidationSurface() {
    return @(
        (Read-Text "core-c/tests/buildup_tests.c"),
        (Read-Text "core-c/tests/buildup_tests_support.h"),
        (Read-Text "core-c/tests/buildup_tests_support.c"),
        (Read-Text "core-c/tests/buildup_enumeration_support.c"),
        (Read-Text "core-c/tests/buildup_problem_tests.c"),
        (Read-Text "core-c/tests/buildup_impossible_fixture_tests.c"),
        (Read-Text "core-c/tests/buildup_enumeration_tests.c"),
        (Read-Text "core-c/tests/buildup_hold_enumeration_tests.c"),
        (Read-Text "core-c/tests/buildup_export_tests.c")
    ) -join "`n"
}function Get-CliArgsParserSurface() {
    $parserFiles = @(
        "crates/clearra-cli/src/args/cli_parser.rs",
        "crates/clearra-cli/src/args/cli_command_parser.rs",
        "crates/clearra-cli/src/args/parse_pc_args.rs",
        "crates/clearra-cli/src/args/parse_pc_scenario_args.rs",
        "crates/clearra-cli/src/args/parse_path_args.rs",
        "crates/clearra-cli/src/args/parse_percent_args.rs",
        "crates/clearra-cli/src/args/parse_setup_args.rs",
        "crates/clearra-cli/src/args/parse_cover_args.rs",
        "crates/clearra-cli/src/args/parse_rules_args.rs",
        "crates/clearra-cli/src/args/parse_scoring_args.rs",
        "crates/clearra-cli/src/args/parse_convert_args.rs",
        "crates/clearra-cli/src/args/parse_continue_args.rs",
        "crates/clearra-cli/src/args/parse_verify_args.rs",
        "crates/clearra-cli/src/args/parse_helpers.rs",
        "crates/clearra-cli/src/args/parse_option_value.rs",
        "crates/clearra-cli/src/args/parse_piece_arg.rs"
    )

    return ($parserFiles | ForEach-Object { Read-Text $_ }) -join "`n"
}
function Add-ArchitectureError([string]$Message) {
    $Errors.Add($Message)
}
function Add-ArchitectureWarning([string]$Message) {
    $Warnings.Add($Message)
}
function Get-RepositoryRelativePath([string]$FullName) {
    return Resolve-Path -LiteralPath $FullName -Relative
}
function Get-RustProductionContents([string]$Contents) {
    $lines = $Contents -split "`r?`n"
    $kept = New-Object System.Collections.Generic.List[string]
    $index = 0

    while ($index -lt $lines.Count) {
        $line = $lines[$index]
        if ($line -match '^\s*#\[cfg\(test\)\]') {
            $index++

            while ($index -lt $lines.Count -and $lines[$index] -match '^\s*#\[') {
                $index++
            }

            if ($index -ge $lines.Count) {
                break
            }

            $braceDepth = 0
            $startedBlock = $false
            do {
                $current = $lines[$index]
                $openBraces = ([regex]::Matches($current, '\{')).Count
                $closeBraces = ([regex]::Matches($current, '\}')).Count
                if ($openBraces -gt 0) {
                    $startedBlock = $true
                }
                $braceDepth += $openBraces - $closeBraces
                $index++
                if (-not $startedBlock) {
                    break
                }
            } while ($index -lt $lines.Count -and $braceDepth -gt 0)

            continue
        }

        $kept.Add($line)
        $index++
    }

    return ($kept -join "`n")
}
function Get-FunctionLineCount([string]$Contents, [string]$FunctionName) {
    $lines = $Contents -split "`n"
    for ($index = 0; $index -lt $lines.Count; $index++) {
        if ($lines[$index] -match "^\s*fn\s+$([regex]::Escape($FunctionName))\b") {
            $braceDepth = 0
            $started = $false
            for ($cursor = $index; $cursor -lt $lines.Count; $cursor++) {
                foreach ($char in $lines[$cursor].ToCharArray()) {
                    if ($char -eq '{') {
                        $braceDepth++
                        $started = $true
                    } elseif ($char -eq '}') {
                        $braceDepth--
                    }
                }
                if ($started -and $braceDepth -le 0) {
                    return $cursor - $index + 1
                }
            }
        }
    }
    return 0
}
function Test-DependencyLine([string]$Contents, [string]$CrateName) {
    $escaped = [regex]::Escape($CrateName)
    return $Contents -match "(?m)^\s*$escaped\s*(=|\.)"
}
function Get-WorkspaceMembers() {
    $contents = Read-Text "Cargo.toml"
    $match = [regex]::Match($contents, '(?s)members\s*=\s*\[(?<body>.*?)\]')
    $members = New-Object System.Collections.Generic.List[string]
    if (-not $match.Success) {
        Add-ArchitectureError "Cargo.toml must contain a workspace members array"
        return $members
    }

    foreach ($item in [regex]::Matches($match.Groups["body"].Value, '"([^"]+)"')) {
        $members.Add($item.Groups[1].Value)
    }
    return $members
}
function Get-CargoPackageName([string]$RelativePath) {
    $contents = Read-Text $RelativePath
    $match = [regex]::Match($contents, '(?m)^\s*name\s*=\s*"([^"]+)"')
    if (-not $match.Success) {
        Add-ArchitectureError "$RelativePath must declare package.name"
        return $null
    }
    return $match.Groups[1].Value
}
function Get-CargoDependencyNames([string]$RelativePath) {
    $names = New-Object System.Collections.Generic.HashSet[string]
    $inDependencySection = $false
    foreach ($line in ((Read-Text $RelativePath) -split "`n")) {
        $trimmed = ($line -replace "#.*$", "").Trim()
        if ($trimmed -match '^\[(.+)\]$') {
            $section = $Matches[1]
            $inDependencySection = $section -eq "dependencies" -or
                $section -eq "dev-dependencies" -or
                $section -eq "build-dependencies" -or
                $section -like "target.*.dependencies"
            continue
        }
        if (-not $inDependencySection -or $trimmed.Length -eq 0) {
            continue
        }
        if ($trimmed -match '^([A-Za-z0-9_-]+)\s*(=|\.)') {
            [void]$names.Add($Matches[1])
        }
    }
    return ,$names
}function Get-CargoRuntimeDependencyNames([string]$RelativePath) {
    $names = New-Object System.Collections.Generic.HashSet[string]
    $inDependencySection = $false
    foreach ($line in ((Read-Text $RelativePath) -split "`n")) {
        $trimmed = ($line -replace "#.*$", "").Trim()
        if ($trimmed -match '^\[(.+)\]$') {
            $section = $Matches[1]
            $inDependencySection = $section -eq "dependencies" -or
                ($section -like "target.*.dependencies" -and $section -notlike "*.dev-dependencies")
            continue
        }
        if (-not $inDependencySection -or $trimmed.Length -eq 0) {
            continue
        }
        if ($trimmed -match '^([A-Za-z0-9_-]+)\s*(=|\.)') {
            [void]$names.Add($Matches[1])
        }
    }
    return ,$names
}
function Get-WorkspaceDependencyGraph() {
    $graph = @{}
    $workspaceCrates = New-Object System.Collections.Generic.HashSet[string]
    foreach ($member in Get-WorkspaceMembers) {
        $manifest = Join-Path $member "Cargo.toml"
        if (-not (Test-Path -LiteralPath (Join-Path $Root $manifest))) {
            Add-ArchitectureError "workspace member '$member' must have a Cargo.toml"
            continue
        }
        $crateName = Get-CargoPackageName $manifest
        if ($null -eq $crateName) {
            continue
        }
        if ($workspaceCrates.Contains($crateName)) {
            Add-ArchitectureError "workspace contains duplicate crate package name '$crateName'"
        }
        [void]$workspaceCrates.Add($crateName)
        $graph[$crateName] = @{
            Manifest = $manifest
            Dependencies = Get-CargoDependencyNames $manifest
        }
    }

    foreach ($crateName in $graph.Keys) {
        foreach ($dependencyName in $graph[$crateName].Dependencies) {
            if ($dependencyName -like "clearra-*" -and -not $workspaceCrates.Contains($dependencyName)) {
                Add-ArchitectureError "$($graph[$crateName].Manifest) depends on non-workspace Clearra crate '$dependencyName'"
            }
        }
    }

    return $graph
}
function Assert-CargoDoesNotDepend([string]$RelativePath, [string[]]$Forbidden, [string]$Reason) {
    $dependencyNames = Get-CargoDependencyNames $RelativePath
    foreach ($crateName in $Forbidden) {
        if ($dependencyNames.Contains($crateName)) {
            Add-ArchitectureError "$RelativePath must not depend on forbidden crate $crateName ($Reason)"
        }
    }
}function Assert-CargoRuntimeDoesNotDepend([string]$RelativePath, [string[]]$Forbidden, [string]$Reason) {
    $dependencyNames = Get-CargoRuntimeDependencyNames $RelativePath
    foreach ($crateName in $Forbidden) {
        if ($dependencyNames.Contains($crateName)) {
            Add-ArchitectureError "$RelativePath must not have runtime dependency on forbidden crate $crateName ($Reason)"
        }
    }
}
function Assert-DependencyGraphDoesNotDepend($Graph, [string]$CrateName, [string[]]$Forbidden, [string]$Reason) {
    if (-not $Graph.ContainsKey($CrateName)) {
        Add-ArchitectureError "workspace dependency graph is missing crate '$CrateName'"
        return
    }
    $dependencyNames = $Graph[$CrateName].Dependencies
    foreach ($forbiddenCrate in $Forbidden) {
        if ($dependencyNames.Contains($forbiddenCrate)) {
            Add-ArchitectureError "$($Graph[$CrateName].Manifest) must not depend on forbidden crate $forbiddenCrate ($Reason)"
        }
    }
}
function Test-GeneratedOrTestRustFile([System.IO.FileInfo]$File) {
    $relative = (Resolve-Path -LiteralPath $File.FullName -Relative).Replace("\", "/")
    $isExtractedTestCompanion = $relative -match '(^|/)(tests?|test_[^/]+|[^/]+_tests?)(_functions|_types)(/|$)'
    return $relative -like "*/target/*" -or
        $relative -like "*/tests/*" -or
        $relative -like "*/fixtures/*" -or
        $isExtractedTestCompanion -or
        $File.Name -like "*_tests.rs" -or
        $File.Name -like "test_*.rs" -or
        $File.Name -eq "tests.rs"
}
function Get-ProductionRustFilesIn([string]$RelativeDir) {
    Get-RustFiles $RelativeDir | Where-Object { -not (Test-GeneratedOrTestRustFile $_) }
}
function Assert-ProductionImportAbsence([string]$RelativeDir, [string[]]$Forbidden, [string]$Reason) {
    foreach ($file in Get-ProductionRustFilesIn $RelativeDir) {
        $relativePath = Resolve-Path -LiteralPath $file.FullName -Relative
        $contents = Get-RustProductionContents (Get-Content -LiteralPath $file.FullName -Raw)
        foreach ($marker in $Forbidden) {
            if ($contents.Contains($marker)) {
                Add-ArchitectureError "$relativePath must not contain forbidden production marker '$marker' ($Reason)"
            }
        }
    }
}
function Get-RustImportCrateReferences([string]$Contents) {
    $references = New-Object System.Collections.Generic.HashSet[string]
    foreach ($match in [regex]::Matches($Contents, '(?m)^\s*(?:pub\s+)?use\s+(clearra_[A-Za-z0-9_]+)\b')) {
        [void]$references.Add($match.Groups[1].Value)
    }
    foreach ($match in [regex]::Matches($Contents, '(?m)^\s*extern\s+crate\s+(clearra_[A-Za-z0-9_]+)\b')) {
        [void]$references.Add($match.Groups[1].Value)
    }
    foreach ($match in [regex]::Matches($Contents, '\b(clearra_[A-Za-z0-9_]+)::')) {
        [void]$references.Add($match.Groups[1].Value)
    }
    return ,$references
}
function Assert-ProductionImportGraphDoesNotImport([string]$RelativeDir, [string[]]$ForbiddenCrates, [string]$Reason) {
    foreach ($file in Get-ProductionRustFilesIn $RelativeDir) {
        $relativePath = Resolve-Path -LiteralPath $file.FullName -Relative
        $contents = Get-RustProductionContents (Get-Content -LiteralPath $file.FullName -Raw)
        $references = Get-RustImportCrateReferences $contents
        foreach ($forbiddenCrate in $ForbiddenCrates) {
            if ($references.Contains($forbiddenCrate)) {
                Add-ArchitectureError "$relativePath imports forbidden crate '$forbiddenCrate' ($Reason)"
            }
        }
    }
}
function Get-RustFiles([string]$RelativeDir) {
    $dir = Join-Path $Root $RelativeDir
    if (-not (Test-Path -LiteralPath $dir)) {
        return @()
    }
    Get-ChildItem -LiteralPath $dir -Recurse -File -Filter *.rs
}
function Get-ProductionRustFiles() {
    Get-ChildItem -LiteralPath (Join-Path $Root "crates") -Recurse -File -Filter *.rs |
        Where-Object { $_.FullName -like "*\src\*" -and -not (Test-GeneratedOrTestRustFile $_) }
}
function Get-NormalizedRelativePath([System.IO.FileInfo]$File) {
    $relative = Resolve-Path -LiteralPath $File.FullName -Relative
    return ($relative -replace '^[.][\\/]', '').Replace("\", "/")
}
