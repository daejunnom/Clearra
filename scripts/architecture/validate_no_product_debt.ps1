# Release-blocking checks for product-code debt. Tests, fixture support, and
# historical documents are excluded explicitly; product source is not.
. (Join-Path $PSScriptRoot '../lib/clearra-local-diagnostics-policy.ps1')

function Test-NoProductDebtAllowlistedPath([string]$RelativePath) {
    $path = $RelativePath.Replace('\', '/')
    return $path -match '(^|/)tests?/' -or
        $path -match '(^|/)test_support/' -or
        $path -match '(^|/)fixtures?/' -or
        $path -match '(^|/)[^/]*_tests\.rs$' -or
        $path -match '(^|/)[^/]*_behavior(/|\.rs$)' -or
        $path -match '^docs/history/'
}

function Get-NoProductDebtFiles([string[]]$Paths) {
    $files = New-Object System.Collections.Generic.List[object]
    foreach ($relativeRoot in $Paths) {
        $fullRoot = Join-Path $Root $relativeRoot
        if (-not (Test-Path -LiteralPath $fullRoot)) {
            continue
        }
        foreach ($file in @(Get-ChildItem -LiteralPath $fullRoot -Recurse -File)) {
            if ($file.Extension -notin @('.rs', '.c', '.h', '.ts', '.tsx', '.js', '.svelte')) {
                continue
            }
            $relative = $file.FullName.Substring($Root.Path.Length).TrimStart('\', '/')
            if ($relative -match '[\\/](node_modules|dist|dist-server|build|target|coverage|\.cache|\.svelte-kit)[\\/]' -or
                (Test-NoProductDebtAllowlistedPath $relative)) {
                continue
            }
            $files.Add([pscustomobject]@{
                RelativePath = $relative.Replace('\', '/')
                FullPath = $file.FullName
                Text = Get-Content -LiteralPath $file.FullName -Raw
            })
        }
    }
    return @($files.ToArray())
}

function Add-NoProductDebtPatternErrors {
    param(
        [object[]]$Files,
        [hashtable]$Patterns,
        [string]$Policy
    )

    foreach ($file in $Files) {
        foreach ($entry in $Patterns.GetEnumerator()) {
            # Patterns are case-sensitive unless they opt in with (?i).  Using
            # PowerShell's default case-insensitive -match makes the stable-ABI
            # guard treat ordinary Rust locals such as `requested_future` as
            # C-style upper-case ABI constants.
            if ($file.Text -cmatch $entry.Value) {
                Add-ArchitectureError "NoProductDebt $Policy found '$($entry.Key)' in $($file.RelativePath)"
            }
        }
    }
}

function Invoke-NoProductDebtStaticValidation {
    foreach ($forbiddenRoot in @('target', 'build')) {
        $forbiddenPath = Join-Path $Root $forbiddenRoot
        if (Test-Path -LiteralPath $forbiddenPath) {
            Add-ArchitectureError "NoProductDebt repository-local artifact directory exists: $forbiddenRoot"
        }
    }
    try { Assert-ClearraLocalToolDirectoryPolicy $Root.Path }
    catch { Add-ArchitectureError "NoProductDebt $($_.Exception.Message)" }
    if ((Read-PhysicalText '.dockerignore') -notmatch '(?m)^/?_local/?\s*$') {
        Add-ArchitectureError 'NoProductDebt raw Docker contexts must exclude local diagnostics'
    }
    $cargoRoot = Join-Path $Root '.cargo'
    if (Test-Path -LiteralPath $cargoRoot) {
        foreach ($cargoTarget in @(Get-ChildItem -LiteralPath $cargoRoot -Directory -Filter 'target*' -Force)) {
            Add-ArchitectureError "NoProductDebt repository-local Cargo target exists: .cargo/$($cargoTarget.Name)"
        }
    }
    $cargoConfig = Join-Path $cargoRoot 'config.toml'
    if ((Test-Path -LiteralPath $cargoConfig) -and
        (Get-Content -LiteralPath $cargoConfig -Raw) -match '(?i)MANIFESTUAC|link-arg=/MANIFEST') {
        Add-ArchitectureError 'NoProductDebt global Cargo linker manifest flags are forbidden; product manifests belong to the product build'
    }

    foreach ($removedPolicyPath in @(
        'scripts/unblock-local-dev.ps1',
        'scripts/dev-sign.ps1',
        'scripts/dev-sign-core-tests.ps1',
        'scripts/dev-sign-cli.ps1',
        'scripts/diagnose-cargo-test-artifact.ps1',
        'scripts/lib/verify-policy-retry.ps1',
        'scripts/lib/product-e2e-preflight.ps1'
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $removedPolicyPath)) {
            Add-ArchitectureError "NoProductDebt removed execution-policy workaround exists: $removedPolicyPath"
        }
    }

    $policySwitch = '-Execution' + 'Policy'
    $bypassValue = 'By' + 'pass'
    $bypassPattern = '(?i)["'']' + [regex]::Escape($policySwitch) +
        '["'']\s*,\s*["'']' + [regex]::Escape($bypassValue) + '["'']'
    foreach ($scriptFile in Get-ChildItem -LiteralPath (Join-Path $Root 'scripts') -Recurse -File -Filter '*.ps1') {
        if ((Get-Content -LiteralPath $scriptFile.FullName -Raw) -match $bypassPattern) {
            Add-ArchitectureError "NoProductDebt PowerShell child overrides execution policy: $($scriptFile.FullName)"
        }
    }

    foreach ($libraryConsumer in @(
        'scripts/lib/no-product-debt.ps1',
        'scripts/lib/rust-exact-tests.ps1',
        'scripts/lib/product-process-surface.ps1',
        'scripts/lib/product-e2e-build.ps1'
    )) {
        $consumerText = Get-Content -LiteralPath (Join-Path $Root $libraryConsumer) -Raw
        if ($consumerText -notlike '*core-c-library-cache*' -or
            $consumerText -notlike '*BUILD_TESTING=OFF*') {
            Add-ArchitectureError "NoProductDebt native library consumer must use the process-free C cache: $libraryConsumer"
        }
    }

    $productFiles = Get-NoProductDebtFiles @(
        'crates',
        'core-c/include',
        'core-c/src',
        'apps/clearra-desktop/src-tauri/src',
        'apps/clearra-web/src',
        'packages/clearra-ui/src'
    )
    # Product source and tracked dependency manifests cannot import the ignored
    # diagnostics boundary. Docs and benchmark tooling are not product owners.
    $manifestPaths = @(& git -C $Root.Path ls-files -- 'Cargo.toml' 'package.json' ':(glob)crates/**/Cargo.toml' ':(glob)apps/**/Cargo.toml' ':(glob)apps/**/package.json' ':(glob)packages/**/package.json' ':(glob)apps/clearra-discord-bot/src/**/*.ts' ':(glob)apps/clearra-discord-bot/src/**/*.mjs')
    if ($LASTEXITCODE -ne 0) { Add-ArchitectureError 'NoProductDebt could not resolve product manifests' }
    $productManifests = foreach ($path in $manifestPaths) {
        $fullPath = Join-Path $Root $path
        if (Test-Path -LiteralPath $fullPath -PathType Leaf) {
            [pscustomobject]@{ RelativePath = $path; Text = Get-Content -LiteralPath $fullPath -Raw }
        }
    }
    try { Assert-ClearraProductExcludesLocalDiagnostics @($productFiles + $productManifests) }
    catch { Add-ArchitectureError "NoProductDebt $($_.Exception.Message)" }

    Add-NoProductDebtPatternErrors $productFiles ([ordered]@{
        portable_reference_packing_fallback_allowed = 'portable_reference_packing_fallback_allowed'
        portable_reference_buildup_fallback_allowed = 'portable_reference_buildup_fallback_allowed'
        portable_reference_fixtures = 'portable_reference_fixtures'
        fixture_fallback = '(?i)fixture[_-]?fallback'
        fallback_build_variant_from_candidate = 'fallback_build_variant_from_candidate'
    }) 'portable fixture fallback'

    Add-NoProductDebtPatternErrors $productFiles ([ordered]@{
        preview_final_response = '(?is)(AppResponse::(?:success|unsupported|error|new)|WasmWorkerJobEvent::FinalResponse\s*\{)[^;\}]{0,500}(Preview|Scaffold|Placeholder|ExampleResult|FixtureFallback)'
        nonfinal_status_as_final = '(?i)"status"\s*:\s*"(preview|scaffold|placeholder)"'
        preview_json_builder = '(?i)preview_json_builder'
        self_declared_response_contract = 'final_response_matches_app_response_contract'
    }) 'non-product final response'

    Add-NoProductDebtPatternErrors $productFiles ([ordered]@{
        count_only_postprocess_batch = 'ReplayEventBatch::from_build_variants|EvidenceBatch::from_replay_events|SpinInterpretationBatch::from_evidence'
        asserted_pattern_union_without_computation = 'pattern_bitset_or_used\s*:\s*true'
        wasm_fake_cpu_fallback = 'WebGpuBackendReport::unavailable\([^\)]*wasm-cpu'
    }) 'semantic scaffold execution'

    Add-NoProductDebtPatternErrors $productFiles ([ordered]@{
        weighted_bag_future = 'WeightedBagProfileFuture'
        future_supply_abi = 'CLR_SUPPLY_PROFILE_WEIGHTED_BAG_PROFILE_FUTURE'
        future_named_c_abi = '(?m)^\s*(#define\s+|[A-Z][A-Z0-9_]*\s*=)[^\r\n]*_FUTURE\b'
        future_binding = '(?i)future[-_ ]binding'
        requires_future_dynamic_runtime = 'requires_future_dynamic_runtime'
        future_named_enum_variant = '(?ms)\benum\s+[A-Za-z][A-Za-z0-9_]*[^\{]*\{(?:(?!\}).)*?^\s*[A-Z][A-Za-z0-9_]*Future[A-Za-z0-9_]*\s*(?:[,\(\{=]|$)'
    }) 'future-named stable enum or ABI'

    $legacyValidator = Join-Path $Root 'scripts/architecture/validate_workspace_surface_legacy_contract.ps1'
    if (Test-Path -LiteralPath $legacyValidator) {
        Add-ArchitectureError 'NoProductDebt dead legacy validator still exists: scripts/architecture/validate_workspace_surface_legacy_contract.ps1'
    }
    foreach ($scriptFile in @(Get-ChildItem -LiteralPath (Join-Path $Root 'scripts/architecture') -File -Filter '*legacy*.ps1')) {
        Add-ArchitectureError "NoProductDebt dead legacy validator is forbidden: scripts/architecture/$($scriptFile.Name)"
    }
    foreach ($scriptFile in @(Get-ChildItem -LiteralPath (Join-Path $Root 'scripts/architecture') -File -Filter '*.ps1')) {
        $text = Get-Content -LiteralPath $scriptFile.FullName -Raw
        if ($text -match '(?is)(Read-(Text|PhysicalText)|Test-Path|Get-ChildItem|Get-RustFiles)[^\r\n]{0,240}crates[/\\]clearra-search') {
            Add-ArchitectureError "NoProductDebt validator reads the removed legacy search tree: scripts/architecture/$($scriptFile.Name)"
        }
    }

    $solverFiles = Get-NoProductDebtFiles @(
        'crates/clearra-core-executor/src/packing',
        'crates/clearra-core-executor/src/buildup',
        'crates/clearra-core-executor/src/service',
        'crates/clearra-app/src',
        'crates/clearra-wasm/src'
    )
    Add-NoProductDebtPatternErrors $solverFiles ([ordered]@{
        hardcoded_native_candidate = '\bCPackingCandidate\s*\{'
        hardcoded_solver_candidate = '(?i)hardcoded[_-]?(packing|solver)?[_-]?candidate'
        fixture_candidate_result = '(?i)fixture[_-]?(packing|solver)?[_-]?candidate'
    }) 'hardcoded solver candidate'

    $scoreFiles = Get-NoProductDebtFiles @(
        'crates/clearra-postprocess/src',
        'crates/clearra-objectives/src/max_score',
        'crates/clearra-core-executor/src/buildup',
        'crates/clearra-core-executor/src/service/pc_pipeline_fields.rs'
    )
    Add-NoProductDebtPatternErrors $scoreFiles ([ordered]@{
        zero_score_cell = '(?m)^\s*score\s*:\s*0(?:u\d+)?\s*,'
        zero_attack_cell = '(?m)^\s*attack\s*:\s*0(?:u\d+)?\s*,'
        zero_materialized_score_cell = '(?is)MaterializedScoreCell::new\([^\)]{0,500},\s*0\s*,\s*0\s*,'
        all_zero_placeholder = '(?i)all[_-]?zero[_-]?placeholder'
    }) 'zero placeholder score matrix'

    $proofFiles = Get-NoProductDebtFiles @(
        'crates/clearra-core-domain/src/pruning',
        'crates/clearra-core-executor/src/pruning'
    )
    Add-NoProductDebtPatternErrors $proofFiles ([ordered]@{
        public_boolean_proof_constructor = '(?is)impl\s+[A-Za-z0-9_]*Proof[A-Za-z0-9_]*\s*\{[^\}]*pub\s+(?:const\s+)?fn\s+(?:new|from_[A-Za-z0-9_]+)\s*\([^\)]*:\s*bool'
        public_boolean_proof_field = '(?is)pub\s+struct\s+[A-Za-z0-9_]*Proof[A-Za-z0-9_]*\s*\{[^\}]*pub\s+[A-Za-z0-9_]+\s*:\s*bool'
    }) 'public pruning proof boolean construction'

    foreach ($file in @(Get-RustFiles 'crates/clearra-core-executor/src')) {
        $text = Get-RustProductionContents (Get-Content -LiteralPath $file.FullName -Raw)
        if ($text -match '\b(AuthorizedPrune|ReachabilityEngineSeal|ClearStateDomainEngineSeal|CompleteReachabilitySearch|CompleteClearStateDomainTable)\b') {
            Add-ArchitectureError "NoProductDebt unconnected Rust pruning proof authority is forbidden: $(Get-RepositoryRelativePath $file.FullName)"
        }
    }

    $desktopManifest = Read-PhysicalText 'apps/clearra-desktop/src-tauri/Cargo.toml'
    $desktopGuiHostDependency = [regex]::Match(
        $desktopManifest,
        '(?m)^clearra-gui-host\s*=\s*\{[^\r\n]*\r?$'
    ).Value
    if ($desktopGuiHostDependency -notmatch '"wasm-cpu-runtime"') {
        Add-ArchitectureError 'NoProductDebt desktop product must enable the exact WASM CPU runtime'
    }
    if ($desktopGuiHostDependency -match '"native-c-core"') {
        Add-ArchitectureError 'NoProductDebt desktop release must not depend on the retired Windows native C execution path'
    }
    if ($desktopGuiHostDependency -notmatch '"webgpu-search"') {
        Add-ArchitectureError 'NoProductDebt desktop product must enable the connected WebGPU search backend'
    }
    $wasmManifest = Read-PhysicalText 'crates/clearra-wasm/Cargo.toml'
    if ($wasmManifest -match '(?m)^clearra-webgpu\s*=') {
        Add-ArchitectureError 'NoProductDebt WASM must not register an unconnected WebGPU search backend'
    }

    $taskExpansion = Read-PhysicalText 'scripts/lib/clearra-task-ui-helpers.ps1'
    $releaseMatch = [regex]::Match(
        $taskExpansion,
        '(?s)"Full"\s*\{\s*return\s+\[string\[\]\]@\((?<body>.*?)\)\s*\}'
    )
    if (-not $releaseMatch.Success) {
        Add-ArchitectureError 'NoProductDebt could not locate the full ReleaseAcceptance task contract'
    } else {
        $lastIndex = -1
        foreach ($stage in @(
            'NoProductDebt',
            'AdversarialCorrectness',
            'CSanitizer',
            'RustExactTests',
            'ProductE2E',
            'WasmBuildTest',
            'DesktopHost',
            'RenderGolden'
        )) {
            $marker = '"' + $stage + '"'
            $index = $releaseMatch.Groups['body'].Value.IndexOf(
                $marker,
                [System.StringComparison]::Ordinal
            )
            if ($index -lt 0) {
                Add-ArchitectureError "NoProductDebt ReleaseAcceptance is missing stage '$stage'"
            } elseif ($index -le $lastIndex) {
                Add-ArchitectureError "NoProductDebt ReleaseAcceptance stage '$stage' is out of order"
            } else {
                $lastIndex = $index
            }
        }
    }
    foreach ($requiredShardExpansionMarker in @(
        'Get-ClearraReleaseAcceptanceTasks $ReleaseAcceptanceShard',
        '$expanded.Add($releaseTask)'
    )) {
        if ($taskExpansion.IndexOf($requiredShardExpansionMarker, [System.StringComparison]::Ordinal) -lt 0) {
            Add-ArchitectureError "NoProductDebt ReleaseAcceptance shard expansion is missing '$requiredShardExpansionMarker'"
        }
    }

    $clearra = Read-PhysicalText 'scripts/clearra.ps1'
    $taskDispatch = Read-PhysicalText 'scripts/lib/clearra-task-dispatch.ps1'
    $pathHelpers = Read-PhysicalText 'scripts/lib/clearra-path-helpers.ps1'
    $runnerSurface = "$clearra`n$taskDispatch"
    foreach ($required in @(
        'Invoke-NoProductDebtGate',
        'Invoke-ClearraCSanitizerGate',
        'Invoke-RustExactTestsGate',
        'Invoke-WasmBuildTestGate',
        'Invoke-RenderGoldenGate'
    )) {
        if ($runnerSurface -notlike "*$required*") {
            Add-ArchitectureError "NoProductDebt release dispatcher is missing '$required'"
        }
    }
    foreach ($pathFunction in @(
        'Get-ClearraArtifactRoot',
        'Get-ClearraReportRoot',
        'Resolve-ClearraReportPath',
        'Assert-ClearraPathOutsideRepository',
        'Get-ClearraCargoTargetDir',
        'Assert-ClearraCanonicalCargoTargetDir'
    )) {
        if ($pathHelpers -notlike "*function $pathFunction*") {
            Add-ArchitectureError "NoProductDebt path policy is missing '$pathFunction'"
        }
    }
    $productProcessSurface = Read-PhysicalText 'scripts/lib/product-process-surface.ps1'
    if ($productProcessSurface -notlike '*Get-ClearraCargoTargetDir*' -or
        $productProcessSurface -notlike '*Assert-ClearraCanonicalCargoTargetDir*') {
        Add-ArchitectureError 'NoProductDebt product execution must use the canonical Cargo artifact target'
    }
    $sanitizerGate = Read-PhysicalText 'scripts/lib/c-sanitizer-gate.ps1'
    foreach ($requiredSanitizerMarker in @(
        "-PersistentBuildName 'core-c-asan-cache'",
        "-PersistentBuildName 'core-c-ubsan-cache'"
    )) {
        if ($sanitizerGate -notlike "*$requiredSanitizerMarker*") {
            Add-ArchitectureError "NoProductDebt sanitizer must use a stable execution surface: missing '$requiredSanitizerMarker'"
        }
    }
    if ($sanitizerGate -like '*New-TransientBuildDir*' -or $sanitizerGate -match '\[Guid\]::NewGuid') {
        Add-ArchitectureError 'NoProductDebt sanitizer must not create a new random executable surface per release run'
    }
    foreach ($requiredStableCTestSurface in @(
        '-PersistentBuildName "core-c-split-cache"',
        '-PersistentBuildName "core-c-asan-cache"',
        '-PersistentBuildName "core-c-ubsan-cache"'
    )) {
        if ($taskDispatch -notlike "*$requiredStableCTestSurface*") {
            Add-ArchitectureError "NoProductDebt CTest execution surface must be stable: missing '$requiredStableCTestSurface'"
        }
    }
    $strictSplitTask = [regex]::Match(
        $taskDispatch,
        '(?s)"StrictCOnlySplit".*?-PersistentBuildName\s+"core-c-split-cache"'
    )
    if (-not $strictSplitTask.Success) {
        Add-ArchitectureError 'NoProductDebt Strict split CTest must require executed evidence from the stable cache'
    }
    $explicitSplitTask = [regex]::Match(
        $taskDispatch,
        '(?s)"COnlySplit"\s*\{.*?-AggregateOnly:\(-not \(Test-ClearraTrustedExecutionSurface \$ExecutionSurface\)\)'
    )
    if (-not $explicitSplitTask.Success) {
        Add-ArchitectureError 'NoProductDebt COnlySplit must select execution only through the explicit execution surface'
    }
    $executionSurface = Read-PhysicalText 'scripts/lib/clearra-execution-surface.ps1'
    foreach ($trustedTask in @('Strict', 'ReleaseAcceptance', 'NoProductDebt', 'AdversarialCorrectness', 'CSanitizer')) {
        if ($executionSurface -notmatch ('"' + [regex]::Escape($trustedTask) + '"')) {
            Add-ArchitectureError "NoProductDebt execution task must require Trusted surface: $trustedTask"
        }
    }
    foreach ($duplicateTarget in @(
        'rust-exact', 'wasm-release', 'render-golden', 'product-e2e-library'
    )) {
        if ($taskDispatch -like "*$duplicateTarget*") {
            Add-ArchitectureError "NoProductDebt duplicate release Cargo target is forbidden: $duplicateTarget"
        }
    }

    $productTask = [regex]::Match(
        $taskDispatch,
        '(?s)"ProductE2E"\s*\{(?<body>.*?)\}\s*"ProductE2EBuilt"'
    )
    if (-not $productTask.Success) {
        Add-ArchitectureError 'NoProductDebt could not locate the ProductE2E dispatcher body'
    } else {
        if ($productTask.Groups['body'].Value -notlike '*Invoke-ProductE2EBuiltTask*') {
            Add-ArchitectureError 'NoProductDebt ProductE2E must execute the single built product surface'
        }
        if ($productTask.Groups['body'].Value -like '*Invoke-ProductLibraryE2ETask*') {
            Add-ArchitectureError 'NoProductDebt ProductE2E must not launch a generated Rust test harness'
        }
    }

    $libraryGate = Read-PhysicalText 'scripts/lib/product-e2e-library-gate.ps1'
    foreach ($required in @(
        'product_e2e_route=source-contract',
        'rust_test_execution=not-built',
        'no Rust source artifact was compiled or launched',
        'process-launch=False'
    )) {
        if ($libraryGate -notlike "*$required*") {
            Add-ArchitectureError "NoProductDebt library ProductE2E gate is missing '$required'"
        }
    }
    if ($libraryGate -match "(?i)'run'\s*,.*clearra-cli" -or
        $libraryGate -match 'CARGO_BIN_EXE_clearra') {
        Add-ArchitectureError 'NoProductDebt library ProductE2E gate must not launch clearra.exe'
    }
    foreach ($forbiddenCargoArgs in @("@('check'", "@('build'", "@('test'")) {
        if ($libraryGate.Contains($forbiddenCargoArgs)) {
            Add-ArchitectureError 'NoProductDebt library ProductE2E gate must not compile Cargo artifacts'
        }
    }

    $executionGate = Read-PhysicalText 'scripts/lib/no-product-debt.ps1'
    foreach ($required in @(
        'native_unavailable_explicit_error',
        'wasm_real_app_response',
        'desktop_real_app_request',
        'max_score_nonzero_profile_matrix',
        'complete_required_keeps_candidate',
        'hold_language_empty_requires_independent_proof',
        'renderer_png_artifact',
        'renderer_gif_artifact'
    )) {
        if ($executionGate -notlike "*$required*") {
            Add-ArchitectureError "NoProductDebt execution gate is missing evidence '$required'"
        }
    }
    $rustExactGate = Read-PhysicalText 'scripts/lib/rust-exact-tests.ps1'
    $renderGoldenGate = Read-PhysicalText 'scripts/lib/render-golden-gate.ps1'
    foreach ($required in @(
        'complete_required_keeps_candidate status=deferred owner=RustExactTests reason=single-release-suite',
        'renderer_png_artifact status=deferred owner=RenderGolden reason=single-release-suite',
        'renderer_gif_artifact status=deferred owner=RenderGolden reason=single-release-suite',
        'desktop_real_app_request status=deferred owner=DesktopHost reason=single-release-suite'
    )) {
        if ($executionGate -notlike "*$required*") {
            Add-ArchitectureError "NoProductDebt release delegation is missing '$required'"
        }
    }
    foreach ($required in @(
        'complete_required_keeps_candidate status=passed source=rust-test owner=RustExactTests',
        'adversarial_rust_tests=executed owner=RustExactTests'
    )) {
        if ($rustExactGate -notlike "*$required*") {
            Add-ArchitectureError "RustExactTests delegated NoProductDebt owner is missing '$required'"
        }
    }
    foreach ($required in @(
        'renderer_png_artifact status=passed source=rust-test owner=RenderGolden',
        'renderer_gif_artifact status=passed source=rust-test owner=RenderGolden'
    )) {
        if ($renderGoldenGate -notlike "*$required*") {
            Add-ArchitectureError "RenderGolden delegated NoProductDebt owner is missing '$required'"
        }
    }
    $desktopHostGate = Read-PhysicalText 'scripts/desktop-host-check.ps1'
    foreach ($required in @(
        'case_tauri_command_calls_clearra_gui_host_only::tauri_command_calls_clearra_gui_host_only',
        "-EvidenceId 'desktop_real_app_request'",
        'no_product_debt_evidence=$EvidenceId status=passed source=rust-test owner=DesktopHost',
        'ArchitectureValidatedByNoProductDebt',
        'desktop_architecture=deferred owner=NoProductDebt reason=single-release-suite'
    )) {
        if ($desktopHostGate -notlike "*$required*") {
            Add-ArchitectureError "DesktopHost delegated NoProductDebt owner is missing '$required'"
        }
    }
    foreach ($requiredSingleOwnerMarker in @(
        '$script:ClearraNoProductDebtArchitecturePassed = $true',
        '$desktopHostArgs["ArchitectureValidatedByNoProductDebt"] = $true'
    )) {
        if (-not $taskDispatch.Contains($requiredSingleOwnerMarker)) {
            Add-ArchitectureError "NoProductDebt release dispatcher is missing DesktopHost delegation marker '$requiredSingleOwnerMarker'"
        }
    }
    if (-not $clearra.Contains('$script:ClearraNoProductDebtArchitecturePassed = $false')) {
        Add-ArchitectureError 'NoProductDebt architecture evidence must reset for every runner invocation'
    }
}
