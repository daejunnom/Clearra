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
    return $File.FullName -match '[\\/](node_modules|dist|dist-server|build|coverage|target|fixtures|tests?|__tests__)[\\/]' -or
        $File.Name -match '(?i)(^test[._-]|[._-]test\.|[._-]tests\.)'
}

function Get-Pc4ProductAuthoritySourceFiles() {
    $roots = @(
        'crates/clearra-pc4-tablebase/src',
        'crates/clearra-app/src',
        'crates/clearra-host-contract/src',
        'crates/clearra-ui-schema/src',
        'crates/clearra-gui-host/src',
        'crates/clearra-cli-command/src',
        'crates/clearra-wasm/src',
        'crates/clearra-cli/src',
        'apps/clearra-web/src',
        'apps/clearra-desktop/src',
        'apps/clearra-discord-bot'
    )
    $extensions = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($extension in @('.rs', '.ts', '.tsx', '.js', '.mjs', '.cjs')) {
        [void]$extensions.Add($extension)
    }

    foreach ($relativeRoot in $roots) {
        $absoluteRoot = Join-Path $Root $relativeRoot
        if (-not (Test-Path -LiteralPath $absoluteRoot)) { continue }
        foreach ($file in Get-ChildItem -LiteralPath $absoluteRoot -Recurse -File) {
            if ($extensions.Contains($file.Extension) -and
                -not (Test-Pc4ProductSourceExcluded $file)) {
                $file
            }
        }
    }
}

function Invoke-Pc4FullSolutionAuthorityContractValidation($WorkspaceDependencyGraph) {
    $productRoots = @(
        'clearra-pc4-tablebase',
        'clearra-app',
        'clearra-host-contract',
        'clearra-ui-schema',
        'clearra-gui-host',
        'clearra-cli-command',
        'clearra-wasm',
        'clearra-cli'
    )
    foreach ($productRoot in $productRoots) {
        Assert-Pc4ProductDependencyClosure $WorkspaceDependencyGraph $productRoot
    }

    $probabilityManifest = Get-CargoDependencyNames 'crates/clearra-pc-next-probability/Cargo.toml'
    if ($probabilityManifest.Count -ne 0) {
        Add-ArchitectureError 'Dormant n-PC probability seam must remain dependency-free and unable to acquire an I/O implementation'
    }
    $probabilityLib = Read-Text 'crates/clearra-pc-next-probability/src/lib.rs'
    if (-not $probabilityLib.Contains('#![no_std]')) {
        Add-ArchitectureError 'Dormant n-PC probability seam must remain no_std'
    }
    Assert-ProductionImportAbsence 'crates/clearra-pc-next-probability/src' @(
        'std::fs',
        'std::net',
        'extern crate std',
        'include_bytes!',
        'include_str!',
        'File::open',
        'OpenOptions',
        'Command::new',
        'http://',
        'https://'
    ) 'dormant n-PC probability seam performs no I/O and owns no asset'

    $forbiddenDecisionPatterns = @(
        '(?i)\bclearra[_-]pc[_-]next[_-]probability\b',
        '(?i)\bpc[_-]?krylov\b',
        '(?i)\bkrylov\b',
        '(?i)\bv[_-]?star\b',
        '(?i)\bv\*',
        '(?i)\bpolicy[_-]?value\b',
        '(?i)\bpolicy(?:[_-]?(?:action|array|asset)|\.bin)\b',
        '(?i)\bvalue(?:[_-]?(?:array|asset)|\.bin)\b',
        '(?i)\b(?:optimal|best|recommended)[_-]?(?:action|edge|transition)\b',
        '(?i)\b(?:single|one)[_-]?best\b'
    )
    foreach ($file in Get-Pc4ProductAuthoritySourceFiles) {
        $contents = Get-Content -LiteralPath $file.FullName -Raw
        foreach ($pattern in $forbiddenDecisionPatterns) {
            if ($contents -match $pattern) {
                $relative = Get-RepositoryRelativePath $file.FullName
                Add-ArchitectureError "$relative contains forbidden PC4 decision-source semantics '$($Matches[0])'; product authority is the complete outgoing graph plus exact Clearra materialization"
            }
        }
    }

    $traversal = Read-Text 'crates/clearra-pc4-tablebase/src/fixed_queue_traversal.rs'
    foreach ($required in @(
        'fn complete_outgoing_edges(',
        '.dedup_by_key(|edge| edge.target_field_id());',
        'for edge in adjacency.edges',
        'kat_traverses_every_outgoing_edge_in_canonical_order',
        'converging_paths_are_not_collapsed_by_visited_state',
        'repeated_raw_targets_are_one_transition_before_exact_materialization'
    )) {
        if (-not $traversal.Contains($required)) {
            Add-ArchitectureError "PC4 complete-graph traversal contract is missing '$required'"
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

    $materializer = Read-Text 'crates/clearra-pc4-tablebase/src/materializer.rs'
    foreach ($required in @(
        'let mut placements = output.placements;',
        'placements.sort_unstable();',
        'placements.dedup();',
        'multiple_realizations_are_sorted_and_deduped_only_by_stable_identity'
    )) {
        if (-not $materializer.Contains($required)) {
            Add-ArchitectureError "PC4 all-realization materializer contract is missing '$required'"
        }
    }
    foreach ($pattern in @(
        '(?is)placements\s*\.\s*(?:first|last|pop|truncate)\s*\(',
        '(?is)placements\s*\.\s*(?:iter|into_iter)\s*\(\s*\)\s*\.\s*(?:next|take)\s*\(\s*1?\s*\)'
    )) {
        if ($materializer -match $pattern) {
            Add-ArchitectureError 'PC4 materializer must preserve every distinct exact placement realization'
        }
    }

    $candidateBoundary = Read-Text 'crates/clearra-app/src/pc_candidate_page_boundary.rs'
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
}
