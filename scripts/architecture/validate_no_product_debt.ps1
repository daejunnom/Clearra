# Release-blocking checks for product-code debt. Tests, fixture support, and
# historical documents are excluded explicitly; product source is not.
. (Join-Path $PSScriptRoot '../lib/clearra-local-diagnostics-policy.ps1')
. (Join-Path $PSScriptRoot '../lib/clearra-path-helpers.ps1')

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
    # One authority owns declared output roots, Git exclusion and link safety.
    # A declared ignored build/ root is allowed; target/, undeclared or tracked
    # output and junction/symlink escapes remain release-blocking.
    try { Assert-ClearraRepositoryArtifactPolicy $Root.Path }
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

    $policySwitch = '-Execution' + 'Policy'
    $bypassValue = 'By' + 'pass'
    $bypassPattern = '(?i)["'']' + [regex]::Escape($policySwitch) +
        '["'']\s*,\s*["'']' + [regex]::Escape($bypassValue) + '["'']'
    foreach ($scriptFile in Get-ChildItem -LiteralPath (Join-Path $Root 'scripts') -Recurse -File -Filter '*.ps1') {
        if ((Get-Content -LiteralPath $scriptFile.FullName -Raw) -match $bypassPattern) {
            Add-ArchitectureError "NoProductDebt PowerShell child overrides execution policy: $($scriptFile.FullName)"
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
}
