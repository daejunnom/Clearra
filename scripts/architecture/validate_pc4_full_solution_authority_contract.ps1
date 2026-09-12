# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Assert-Pc4ProductDependencyClosure(
    $WorkspaceDependencyGraph,
    [string]$RootCrate
) {
    if (-not $WorkspaceDependencyGraph.ContainsKey($RootCrate)) {
        Add-ArchitectureError "PC4 full-solution authority is missing product crate '$RootCrate'"
        return
    }

    $forbidden = 'clearra-pc-next-probability'
    $visited = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::Ordinal
    )
    $pending = [System.Collections.Generic.Queue[object]]::new()
    $pending.Enqueue([pscustomobject]@{
        Crate = $RootCrate
        Path = @($RootCrate)
    })

    while ($pending.Count -gt 0) {
        $current = $pending.Dequeue()
        if (-not $visited.Add($current.Crate)) { continue }
        foreach ($dependency in $WorkspaceDependencyGraph[$current.Crate].Dependencies) {
            $path = @($current.Path) + @($dependency)
            if ($dependency -eq $forbidden) {
                Add-ArchitectureError "PC4 product dependency closure reaches dormant n-PC probability code: $($path -join ' -> ')"
                continue
            }
            if ($WorkspaceDependencyGraph.ContainsKey($dependency)) {
                $pending.Enqueue([pscustomobject]@{
                    Crate = $dependency
                    Path = $path
                })
            }
        }
    }
}

function Test-Pc4ProductSourceExcluded([System.IO.FileInfo]$File) {
    return $File.FullName -match '[\\/](node_modules|dist|dist-server|build|coverage|target|fixtures|tests?|__tests__|test_support|snapshots|docs?|documentation)[\\/]' -or
        $File.Name -match '(?i)(^test[._-]|[._-](?:test|tests|spec)\.)' -or
        $File.Name -match '(?i)(?:^|[-_.])lock(?:\.|$)' -or
        $File.FullName -match '[\\/](?:credentials?|secrets?)[\\/]' -or
        $File.Name -match '(?i)(?:service[-_]?account|credentials?|secrets?|api[-_]?keys?|id_(?:rsa|dsa|ecdsa|ed25519)).*'
}

function Get-Pc4ProductAuthoritySourceFiles() {
    $roots = @(
        'crates/clearra-pc4-tablebase',
        'crates/clearra-core-executor',
        'crates/clearra-app',
        'crates/clearra-host-contract',
        'crates/clearra-ui-schema',
        'crates/clearra-gui-host',
        'crates/clearra-cli-command',
        'crates/clearra-pc-graph',
        'crates/clearra-problem',
        'crates/clearra-wasm',
        'crates/clearra-wasm-abi',
        'crates/clearra-cli',
        'apps/clearra-web',
        'apps/clearra-desktop',
        'apps/clearra-discord-bot',
        'packages/clearra-ui',
        'scripts/release',
        'scripts/pc4-discovery',
        'scripts/discovery',
        'tools/pc4-discovery',
        'tools/discovery',
        'config',
        'configs',
        '.github/workflows',
        '.github/actions'
    )
    $extensions = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($extension in @(
        '.rs', '.ts', '.tsx', '.mts', '.cts', '.js', '.mjs', '.cjs', '.svelte',
        '.json', '.toml', '.yaml', '.yml', '.ps1', '.sh', '.py', '.config', '.conf'
    )) {
        [void]$extensions.Add($extension)
    }

    foreach ($relativeRoot in $roots) {
        $absoluteRoot = Join-Path $Root $relativeRoot
        if (-not (Test-Path -LiteralPath $absoluteRoot)) { continue }
        foreach ($file in Get-ChildItem -LiteralPath $absoluteRoot -Recurse -File) {
            $isSourceOrConfiguration = $extensions.Contains($file.Extension) -or
                $file.Name -match '^(?i)(?:Dockerfile|Containerfile)(?:\..+)?$'
            if ($isSourceOrConfiguration -and
                -not (Test-Pc4ProductSourceExcluded $file)) {
                $file
            }
        }
    }
}

function Get-Pc4ProductAuthorityProductionText([System.IO.FileInfo]$File) {
    $contents = Get-Content -LiteralPath $File.FullName -Raw
    if ($File.Extension.Equals('.rs', [System.StringComparison]::OrdinalIgnoreCase)) {
        return Get-RustProductionContents $contents
    }
    return $contents
}

function Get-Pc4ProductAuthorityRelativePath([System.IO.FileInfo]$File) {
    $rootValue = if ($Root -is [System.Management.Automation.PathInfo]) {
        $Root.Path
    } else {
        [string]$Root
    }
    $rootPath = [System.IO.Path]::GetFullPath($rootValue).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    )
    $fullPath = [System.IO.Path]::GetFullPath($File.FullName)
    if (-not $fullPath.StartsWith(
        "$rootPath$([System.IO.Path]::DirectorySeparatorChar)",
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw "PC4 authority source escaped the workspace: $fullPath"
    }
    return $fullPath.Substring($rootPath.Length + 1).Replace('\', '/')
}

function Test-Pc4DecisionAuthorityContext(
    [System.IO.FileInfo]$File,
    [string]$Contents
) {
    $relative = Get-Pc4ProductAuthorityRelativePath $File
    return $relative -match '(?i)(?:pc[_-]?4|tablebase|full[-_]?solution)' -or
        $Contents -match '(?i)(?:pc[_-]?4|tablebase|full[-_]?solution)'
}

function Assert-Pc4ProductDecisionSourceAbsence() {
    $globallyForbiddenPatterns = @(
        '(?i)\bclearra[_-]pc[_-]next[_-]probability\b',
        '(?i)\bpc[_-]?next[_-]?probability(?:port|provider|adapter|result)?\b',
        '(?i)\bpc[_-]?survival(?:model)?(?:port|provider|adapter)?\b',
        '(?i)\bpc[_-]?krylov\b',
        '(?i)\bkrylov\b',
        '(?i)\bv[_-]?star\b',
        '(?-i:\bV\*)',
        '(?i)\bpolicy[_-]?advisor\b',
        '(?i)\bpolicy\.bin\b',
        '(?i)\bvalue\.bin\b'
    )
    $pc4DecisionPatterns = @(
        '(?i)\bpolicy[_-]?value\b',
        '(?i)\bpolicy(?:[_-]?(?:action|array|asset))\b',
        '(?i)\bvalue(?:[_-]?(?:array|asset))\b',
        '(?i)\b(?:optimal|best|recommended)[_-]?(?:action|edge|transition)\b',
        '(?i)\b(?:single|one)[_-]?best\b'
    )

    foreach ($file in Get-Pc4ProductAuthoritySourceFiles) {
        $contents = Get-Pc4ProductAuthorityProductionText $file
        $patterns = @($globallyForbiddenPatterns)
        if (Test-Pc4DecisionAuthorityContext -File $file -Contents $contents) {
            $patterns += $pc4DecisionPatterns
        }
        foreach ($pattern in $patterns) {
            if ($contents -match $pattern) {
                $relative = Get-Pc4ProductAuthorityRelativePath $file
                Add-ArchitectureError "$relative contains forbidden PC4 decision-source semantics '$($Matches[0])'; v0.9 product authority is qualified graph/index data plus complete Clearra materialization"
            }
        }
    }
}

function Test-Pc4V090MigrationMode() {
    $markerRelativePath = 'scripts/architecture/pc4-v090-online-authority.mode'
    $markerPath = Join-Path $Root $markerRelativePath
    if (-not (Test-Path -LiteralPath $markerPath)) {
        return $false
    }

    $expected = 'pc4-product-authority=v0.9-online-graph-v1'
    $actual = (Get-Content -LiteralPath $markerPath -Raw).Trim()
    if ($actual -ne $expected) {
        Add-ArchitectureError "$markerRelativePath must contain exactly '$expected' before v0.9 migration checks can run"
        return $false
    }
    return $true
}

function Assert-Pc4V090LegacyStaticBetaMigration() {
    if (-not (Test-Pc4V090MigrationMode)) {
        return
    }

    $legacyAsset = 'apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin'
    if (Test-Path -LiteralPath (Join-Path $Root $legacyAsset)) {
        Add-ArchitectureError "v0.9 PC4 migration mode forbids legacy static-beta asset '$legacyAsset'"
    }

    $legacyPatterns = @(
        '(?i)\bCLR4TB12\b',
        '(?i)\bpc4-compact-exact-v12(?:\.bin)?\b',
        '(?i)\bPC4_COMPACT_TABLEBASE\b',
        '(?i)\b(?:compile|install|release)_pc4_compact_tablebase\b',
        '(?i)\bPc4CompactTablebase(?:Artifact)?\b',
        '(?i)\bAppTablebaseSession\b',
        '(?i)apps/clearra-web/static/tablebase/',
        '(?i)\bclearra_wasm_tablebase_(?:install|release)\b'
    )
    foreach ($file in Get-Pc4ProductAuthoritySourceFiles) {
        $contents = Get-Pc4ProductAuthorityProductionText $file
        foreach ($pattern in $legacyPatterns) {
            if ($contents -match $pattern) {
                $relative = Get-Pc4ProductAuthorityRelativePath $file
                Add-ArchitectureError "v0.9 PC4 migration mode still reaches the CLR4TB12 static-beta product path in '$relative' via '$($Matches[0])'"
            }
        }
    }
}

function Invoke-Pc4FullSolutionAuthorityContractValidation($WorkspaceDependencyGraph) {
    $productRoots = @(
        'clearra-pc4-tablebase',
        'clearra-core-executor',
        'clearra-app',
        'clearra-host-contract',
        'clearra-ui-schema',
        'clearra-gui-host',
        'clearra-cli-command',
        'clearra-wasm',
        'clearra-wasm-abi',
        'clearra-cli'
    )
    foreach ($productRoot in $productRoots) {
        Assert-Pc4ProductDependencyClosure $WorkspaceDependencyGraph $productRoot
    }

    $probabilityManifest = Get-CargoDependencyNames 'crates/clearra-pc-next-probability/Cargo.toml'
    if ($probabilityManifest.Count -ne 0) {
        Add-ArchitectureError 'Dormant n-PC probability seam must remain dependency-free and unable to acquire an I/O implementation'
    }
    $probabilityManifestText = Read-PhysicalText 'crates/clearra-pc-next-probability/Cargo.toml'
    foreach ($forbiddenManifestPattern in @(
        '(?m)^\s*\[features\]\s*$',
        '(?m)^\s*build\s*='
    )) {
        if ($probabilityManifestText -match $forbiddenManifestPattern) {
            Add-ArchitectureError 'Dormant n-PC probability seam must not declare activation features or a build script'
        }
    }
    foreach ($forbiddenSeamPath in @(
        'crates/clearra-pc-next-probability/build.rs',
        'crates/clearra-pc-next-probability/src/bin',
        'crates/clearra-pc-next-probability/examples',
        'crates/clearra-pc-next-probability/benches'
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $forbiddenSeamPath)) {
            Add-ArchitectureError "Dormant n-PC probability seam must not own executable, build, example, or benchmark surface '$forbiddenSeamPath'"
        }
    }
    $probabilityLib = Read-Text 'crates/clearra-pc-next-probability/src/lib.rs'
    if (-not $probabilityLib.Contains('#![no_std]')) {
        Add-ArchitectureError 'Dormant n-PC probability seam must remain no_std'
    }
    Assert-ProductionImportAbsence 'crates/clearra-pc-next-probability/src' @(
        'std::fs',
        'std::net',
        'extern crate std',
        'std::io',
        'std::path',
        'std::process',
        'std::env',
        'include_bytes!',
        'include_str!',
        'include!(',
        'env!(',
        'option_env!(',
        'File::open',
        'OpenOptions',
        'Command::new',
        'extern "C"',
        '#[link(',
        'http://',
        'https://'
    ) 'dormant n-PC probability seam performs no I/O and owns no asset'

    Assert-Pc4ProductDecisionSourceAbsence
    Assert-Pc4V090LegacyStaticBetaMigration

    $lookup = Get-RustProductionContents (
        Read-PhysicalText 'crates/clearra-pc4-tablebase/src/lookup.rs'
    )
    foreach ($required in @(
        'self.request(',
        'Pc4ArtifactRole::FieldHashIndex',
        'Pc4ArtifactRole::GraphOffsets',
        'Pc4ArtifactRole::Graph',
        'self.profile.artifact(artifact).clone()'
    )) {
        if (-not $lookup.Contains($required)) {
            Add-ArchitectureError "PC4 lookup must source qualified graph/index artifacts; missing '$required'"
        }
    }

    $coreMaterializer = Get-RustProductionContents (
        Read-PhysicalText 'crates/clearra-core-executor/src/backend/wasm_cpu/pc4_graph_materializer.rs'
    )
    foreach ($required in @(
        'pub fn materialize_pc4_ilc_transition(',
        'for realization in catalog.instantiations(',
        'placements.push(Pc4IlcPlacement {',
        'placements.sort_unstable();',
        'placements.dedup();'
    )) {
        if (-not $coreMaterializer.Contains($required)) {
            Add-ArchitectureError "Clearra core PC4 edge materializer must retain every exact reachable realization; missing '$required'"
        }
    }
    foreach ($pattern in @(
        '(?is)placements\s*\.\s*(?:first|last|pop|truncate)\s*\(',
        '(?is)catalog\s*\.\s*instantiations\s*\([^)]*\)\s*\.\s*(?:next|take)\s*\(\s*1?\s*\)'
    )) {
        if ($coreMaterializer -match $pattern) {
            Add-ArchitectureError 'Clearra core PC4 edge materializer must not select one preferred realization'
        }
    }

    $candidateAdapter = Get-RustProductionContents (
        Read-PhysicalText 'crates/clearra-app/src/pc4_graph_candidate_adapter.rs'
    )
    foreach ($required in @(
        'P: QualifiedCompleteAdjacencyProvider',
        'M: Pc4PlacementMaterializer',
        'prepare_fixed_queue_concrete_family(',
        'while observations.len() < limit.get()',
        'if !self.is_exhausted() {',
        'Pc4GraphCandidatePrepareError::IncompleteCannotFinalize',
        'PcCandidateCompletenessEvidence {'
    )) {
        if (-not $candidateAdapter.Contains($required)) {
            Add-ArchitectureError "App PC4 candidate adapter must exhaust graph paths and all concrete materializations before reducer authority; missing '$required'"
        }
    }

    $traversalFile = Read-PhysicalText 'crates/clearra-pc4-tablebase/src/fixed_queue_traversal.rs'
    $traversal = Get-RustProductionContents $traversalFile
    foreach ($required in @(
        'fn complete_outgoing_edges(',
        '.dedup_by_key(|edge| edge.target_field_id());',
        'for edge in adjacency.edges'
    )) {
        if (-not $traversal.Contains($required)) {
            Add-ArchitectureError "PC4 complete-graph traversal contract is missing '$required'"
        }
    }
    foreach ($requiredTest in @(
        'kat_traverses_every_outgoing_edge_in_canonical_order',
        'converging_paths_are_not_collapsed_by_visited_state',
        'repeated_raw_targets_are_one_transition_before_exact_materialization'
    )) {
        if (-not $traversalFile.Contains($requiredTest)) {
            Add-ArchitectureError "PC4 complete-graph traversal test contract is missing '$requiredTest'"
        }
    }
    foreach ($pattern in @(
        '(?is)adjacency\s*\.\s*edges\s*\.\s*(?:first|last|pop|truncate)\s*\(',
        '(?is)adjacency\s*\.\s*edges\s*\.\s*(?:iter|into_iter)\s*\(\s*\)\s*\.\s*(?:next|take)\s*\(\s*1?\s*\)'
    )) {
        if ($traversal -match $pattern) {
            Add-ArchitectureError 'PC4 traversal must not select a first/best subset from qualified complete adjacency'
        }
    }

    $graphRecordDecoderFile = Read-PhysicalText 'crates/clearra-pc4-tablebase/src/graph.rs'
    $graphRecordDecoder = Get-RustProductionContents $graphRecordDecoderFile
    foreach ($required in @(
        'pub fn decode_hydra_graph_record_v1(',
        'const HYDRA_GRAPH_PIECES:',
        'for piece in HYDRA_GRAPH_PIECES',
        'SourceFieldHashMismatch',
        'TargetOutsideFieldDomain',
        'TrailingBytes'
    )) {
        if (-not $graphRecordDecoder.Contains($required)) {
            Add-ArchitectureError "PC4 Hydra graph-record decoder is missing all-edge or fail-closed marker '$required'"
        }
    }
    if (-not $graphRecordDecoderFile.Contains('hydra_record_decodes_every_piece_group_without_selecting_an_edge')) {
        Add-ArchitectureError "PC4 Hydra graph-record decoder test contract is missing 'hydra_record_decodes_every_piece_group_without_selecting_an_edge'"
    }
    foreach ($pattern in @(
        '(?is)targets\s*\.\s*(?:first|last|pop|truncate)\s*\(',
        '(?is)targets\s*\.\s*(?:iter|into_iter)\s*\(\s*\)\s*\.\s*(?:next|take)\s*\(\s*1?\s*\)'
    )) {
        if ($graphRecordDecoder -match $pattern) {
            Add-ArchitectureError 'PC4 graph-record decoding must retain all outgoing targets rather than selecting one edge'
        }
    }

    $materializerFile = Read-PhysicalText 'crates/clearra-pc4-tablebase/src/materializer.rs'
    $materializer = Get-RustProductionContents $materializerFile
    foreach ($required in @(
        'let mut placements = output.placements;',
        'placements.sort_unstable();',
        'placements.dedup();'
    )) {
        if (-not $materializer.Contains($required)) {
            Add-ArchitectureError "PC4 all-realization materializer contract is missing '$required'"
        }
    }
    if (-not $materializerFile.Contains('multiple_realizations_are_sorted_and_deduped_only_by_stable_identity')) {
        Add-ArchitectureError "PC4 all-realization materializer test contract is missing 'multiple_realizations_are_sorted_and_deduped_only_by_stable_identity'"
    }
    foreach ($pattern in @(
        '(?is)placements\s*\.\s*(?:first|last|pop|truncate)\s*\(',
        '(?is)placements\s*\.\s*(?:iter|into_iter)\s*\(\s*\)\s*\.\s*(?:next|take)\s*\(\s*1?\s*\)'
    )) {
        if ($materializer -match $pattern) {
            Add-ArchitectureError 'PC4 materializer must preserve every distinct exact placement realization'
        }
    }

    $candidateBoundary = Read-PhysicalText 'crates/clearra-app/src/pc_candidate_page_boundary.rs'
    foreach ($required in @(
        '#[cfg(test)]',
        'fn from_verified_complete_source(',
        'PcCandidateCollectionCompleteness::CompleteRequestUniverse',
        'PcCandidateBoundaryError::IncompleteCannotReduce'
    )) {
        if (-not $candidateBoundary.Contains($required)) {
            Add-ArchitectureError "PC4 complete candidate-universe reducer boundary is missing '$required'"
        }
    }
    if ($candidateBoundary -match '(?m)^\s*pub(?:\([^)]*\))?\s+fn\s+from_verified_complete_source\s*\(') {
        Add-ArchitectureError 'PC4 completeness evidence constructor must remain unavailable to product/provider adapters until target-specific completeness is qualified'
    }

    $rangeAdmission = Read-Text 'crates/clearra-pc4-tablebase/src/range_admission/mod.rs'
    foreach ($required in @(
        '206 => self.admit_partial(request, http)',
        '200 => Err(RangeAdmissionError::WholeContentRejected)',
        'ensure_guard(guard, &self.snapshot)?;',
        'self.usage = RangeAdmissionUsage {'
    )) {
        if (-not $rangeAdmission.Contains($required)) {
            Add-ArchitectureError "PC4 bounded Range admission contract is missing '$required'"
        }
    }

    $onlineLookupSession = Read-Text 'crates/clearra-app/src/online_pc4_lookup_session.rs'
    foreach ($required in @(
        'range_admission: RangeAdmissionSession,',
        'pub fn admit_range<G>(',
        '.range_admission',
        '.admit(&request, attempt, input, guard)?'
    )) {
        if (-not $onlineLookupSession.Contains($required)) {
            Add-ArchitectureError "App PC4 lookup must pass host responses through bounded Range admission '$required'"
        }
    }
    if ($onlineLookupSession -match '(?m)^\s*pub\s+fn\s+(?:supply|reject_range)\s*\(') {
        Add-ArchitectureError 'App PC4 lookup must not expose a raw response or transport-failure supply bypass'
    }

    $inputDisclosure = Read-Text 'crates/clearra-app/src/pc4_input_disclosure_policy.rs'
    foreach ($required in @(
        'Pc4QueueDisclosure::FixedExplicit(queue)',
        'Pc4InputSurface::NonInteractiveCli',
        'Pc4InputDisclosureDecision::RequestBagRemainder',
        'Pc4InputDisclosureRejection::NonInteractiveBagDisclosureRequired',
        'target: QualifiedPc4TargetIdentity',
        'Pc4InputDisclosureStopReason::UserRefused'
    )) {
        if (-not $inputDisclosure.Contains($required)) {
            Add-ArchitectureError "App PC4 input-disclosure boundary is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'LookupMachine',
        'RangeRequest',
        'AppOnlinePc4LookupSession',
        'Pc4OfflineFallbackAuthorization'
    )) {
        if ($inputDisclosure.Contains($forbidden)) {
            Add-ArchitectureError "App PC4 input-disclosure boundary must not perform lookup or authorize fallback '$forbidden'"
        }
    }

    $setupAcceleration = Read-Text 'crates/clearra-app/src/setup_pc_candidate_acceleration.rs'
    foreach ($required in @(
        'target: Pc4TargetLines,',
        'target: QualifiedPc4TargetIdentity,',
        'Pc4TerminalUseCase::SetupSearch',
        'qualification.target.snapshot()'
    )) {
        if (-not $setupAcceleration.Contains($required)) {
            Add-ArchitectureError "Setup PC acceleration must consume shared target authority '$required'"
        }
    }
    if ($setupAcceleration.Contains('SetupPcAccelerationTarget') -or
        $setupAcceleration -match '(?m)^\s+snapshot:\s*QualifiedSnapshotIdentity,') {
        Add-ArchitectureError 'Setup PC acceleration must not duplicate target or snapshot qualification authority'
    }

    $manifest = Read-Text 'crates/clearra-pc4-tablebase/src/manifest.rs'
    foreach ($required in @(
        'pub struct Pc4TargetLines',
        'pub enum Pc4TerminalUseCase',
        'pub enum Pc4ArtifactRole',
        'FieldHashIndex',
        'GraphOffsets',
        'Graph',
        'pub struct ProfileTargetCompletenessQualification',
        'outgoing_edge_completeness_identity',
        'offline_exact_parity_identity',
        'pub struct QualifiedPc4TargetIdentity',
        'pub fn qualified_target('
    )) {
        if (-not $manifest.Contains($required)) {
            Add-ArchitectureError "PC4 profile-target completeness qualification is missing '$required'"
        }
    }
    if ($manifest -match '(?s)pub enum Pc4ArtifactRole\s*\{(?<Body>[^}]*)\}') {
        $artifactRoleBody = $Matches['Body']
        $artifactRoles = @(
            [regex]::Matches($artifactRoleBody, '(?m)^\s*([A-Za-z][A-Za-z0-9_]*)\s*,?\s*$') |
                ForEach-Object { $_.Groups[1].Value }
        )
        $expectedArtifactRoles = @('FieldHashIndex', 'GraphOffsets', 'Graph')
        if (($artifactRoles -join ',') -ne ($expectedArtifactRoles -join ',')) {
            Add-ArchitectureError "PC4 product artifact roles must remain graph/index-only; found '$($artifactRoles -join ',')'"
        }
    } else {
        Add-ArchitectureError 'PC4 product artifact-role enum could not be audited'
    }

    foreach ($required in @(
        'profiles: [ProfileAvailability; PC4_RULE_PROFILE_COUNT]',
        'ActivationError::NoQualifiedProfiles',
        '.any(|profile| matches!(profile, ProfileAvailability::Qualified(_)))',
        'pub fn profile_availability(',
        'ActivatedProfileError::NotQualified'
    )) {
        if (-not $manifest.Contains($required)) {
            Add-ArchitectureError "PC4 snapshot activation must preserve five independent qualified/not-qualified profile slots; missing '$required'"
        }
    }

    $profileCapability = Read-Text 'crates/clearra-app/src/pc4_profile_capability_projection.rs'
    foreach ($required in @(
        'profiles: [Pc4ProfileCapabilitySlot; 5]',
        'ProfileNotQualified { reason: UnsupportedProfileReason }',
        'pc_search_targets: [Pc4TargetCapabilitySlot; 4]',
        'setup_search_targets: [Pc4TargetCapabilitySlot; 4]',
        'Pc4RuleProfile::ALL.map(|profile| project_profile(snapshot, profile))'
    )) {
        if (-not $profileCapability.Contains($required)) {
            Add-ArchitectureError "App PC4 capability projection must expose every profile and PC/Setup target independently; missing '$required'"
        }
    }

    $fixedQueueRuntime = Get-RustProductionContents (
        Read-PhysicalText 'crates/clearra-app/src/pc4_fixed_queue_candidate_runtime.rs'
    )
    foreach ($required in @(
        'prepare_pc4_graph_candidate_stream(',
        'AppQualifiedPc4LookupHit',
        'NeedLookup(u32)',
        'pub const fn completed_reducer_input(',
        'Pc4FixedQueueCandidateRuntimeState::Complete'
    )) {
        if (-not $fixedQueueRuntime.Contains($required)) {
            Add-ArchitectureError "App PC4 fixed-queue runtime must resume qualified lookups and expose reducer input only after complete traversal; missing '$required'"
        }
    }
    if ($fixedQueueRuntime -match '(?m)^\s*pub(?:\([^)]*\))?\s+fn\s+(?:fetch|http|fallback)') {
        Add-ArchitectureError 'App PC4 fixed-queue runtime must not own HTTP or execute an offline fallback'
    }

    $onlineCandidateSession = Get-RustProductionContents (
        Read-PhysicalText 'crates/clearra-app/src/online_pc4_fixed_queue_candidate_session.rs'
    )
    foreach ($required in @(
        'AppOnlinePc4FixedQueueCandidateSession',
        'AppOnlinePc4LookupSession',
        'Pc4FixedQueueCandidateRuntime',
        'NeedRange(RangeRequest)',
        'pub fn completed_reducer_input(',
        'TerminalState::Complete'
    )) {
        if (-not $onlineCandidateSession.Contains($required)) {
            Add-ArchitectureError "App PC4 online candidate session must bind Range lookup to complete fixed-queue reduction; missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'Pc4OfflineFallbackAuthorization::ExplicitlyAuthorized',
        'reqwest',
        'fetch(',
        'Command::new'
    )) {
        if ($onlineCandidateSession.Contains($forbidden)) {
            Add-ArchitectureError "App PC4 online candidate session must remain no-I/O and cannot authorize or execute fallback '$forbidden'"
        }
    }

    foreach ($required in @(
        "target: &'a QualifiedPc4TargetIdentity",
        'let snapshot = request.target.snapshot();',
        'let profile = request.target.profile();'
    )) {
        if (-not $traversal.Contains($required)) {
            Add-ArchitectureError "PC4 traversal must require qualified profile/use-case/target authority '$required'"
        }
    }
    if ($traversal -match "(?s)pub const fn new\(\s*snapshot:\s*&'a QualifiedSnapshotIdentity") {
        Add-ArchitectureError 'PC4 public traversal request must not accept a raw snapshot in place of target completeness authority'
    }

    $partialGenerationManifest = Read-Text 'scripts/release/pc4/partial-generation-manifest.mjs'
    foreach ($required in @(
        'clearra.pc4.partial-generation-manifest.v1',
        '"srs",',
        '"srs-plus",',
        '"srs-x",',
        '"jstris-180",',
        '"no-kick",',
        'status === "not_qualified"',
        'qualifiedCount === 0',
        'cannot be borrowed across profiles',
        'resolved_revision'
    )) {
        if (-not $partialGenerationManifest.Contains($required)) {
            Add-ArchitectureError "PC4 release generation admission must preserve independent partial-profile qualification; missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'fetch(',
        'https://huggingface.co/',
        'process.env',
        'child_process',
        'writeFile'
    )) {
        if ($partialGenerationManifest.Contains($forbidden)) {
            Add-ArchitectureError "PC4 partial-generation validator must remain pure and cannot discover, sign, promote, or mutate '$forbidden'"
        }
    }

    $partialGenerationManifestTests = Read-Text 'scripts/release/pc4/partial-generation-manifest.test.mjs'
    foreach ($requiredTest in @(
        'one qualified profile activates while four retain exact not-qualified reasons',
        'qualified targets stay independent by use case and line count',
        'a generation with no qualified profile is rejected',
        'graph index and qualification evidence cannot be borrowed across profiles'
    )) {
        if (-not $partialGenerationManifestTests.Contains($requiredTest)) {
            Add-ArchitectureError "PC4 partial-generation release contract test is missing '$requiredTest'"
        }
    }
}
