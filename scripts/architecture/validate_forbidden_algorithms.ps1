function Test-LogicalPathAllowlisted(
    [string]$RelativePath,
    [string[]]$AllowlistedRelativePaths
) {
    $normalized = $RelativePath.Replace("\", "/")
    foreach ($allowedPath in $AllowlistedRelativePaths) {
        $allowed = $allowedPath.Replace("\", "/")
        if ($normalized -eq $allowed) {
            return $true
        }

        $extension = [System.IO.Path]::GetExtension($allowed)
        if ([string]::IsNullOrEmpty($extension)) {
            continue
        }
        $ownerStem = $allowed.Substring(0, $allowed.Length - $extension.Length)
        if ($normalized.StartsWith("${ownerStem}_functions/") -or
            $normalized.StartsWith("${ownerStem}_types/") -or
            $normalized.StartsWith("${ownerStem}_impls/") -or
            $normalized.StartsWith("${ownerStem}_methods/")) {
            return $true
        }
    }
    return $false
}
function Test-PhysicalCompanionPath([System.IO.FileInfo]$File) {
    $normalized = $File.FullName.Replace("\", "/")
    return $normalized -match '/[^/]+_(functions|types|impls|methods)/'
}
function Read-ForbiddenAlgorithmLogicalOwnerText(
    [System.IO.FileInfo]$File,
    [System.Collections.Generic.HashSet[string]]$Visited = $null
) {
    if ($null -eq $Visited) {
        $Visited = [System.Collections.Generic.HashSet[string]]::new(
            [System.StringComparer]::OrdinalIgnoreCase
        )
    }
    if (-not $Visited.Add($File.FullName)) { return "" }

    $text = Get-Content -LiteralPath $File.FullName -Raw
    $parts = [System.Collections.Generic.List[string]]::new()
    $parts.Add($text)
    $companions = switch ($File.Extension.ToLowerInvariant()) {
        '.rs' { @(Get-RustIncludeCompanionPaths -FullPath $File.FullName -Text $text); break }
        '.c' { @(Get-CImplementationIncludeCompanionPaths -FullPath $File.FullName -Text $text); break }
        '.h' {
            @(Get-CLocalIncludeCompanionPaths -FullPath $File.FullName -Text $text | Where-Object {
                $_.Replace("\", "/") -match '/[^/]+_(api|types|functions)/'
            })
            break
        }
        default { @() }
    }
    foreach ($companion in $companions) {
        if (Test-Path -LiteralPath $companion) {
            $companionFile = Get-Item -LiteralPath $companion
            $parts.Add((Read-ForbiddenAlgorithmLogicalOwnerText $companionFile $Visited))
        }
    }
    return $parts -join "`n"
}function Assert-ForbiddenAlgorithmTextContains(
    [string]$Path,
    [string[]]$Markers,
    [string]$ErrorPrefix
) {
    $text = Read-Text $Path
    foreach ($marker in $Markers) {
        if ($text -notlike "*$marker*") {
            Add-ArchitectureError "$ErrorPrefix must contain marker '$marker' in $Path"
        }
    }
}function Get-ForbiddenAlgorithmProductFiles() {
    $files = New-Object System.Collections.Generic.List[System.IO.FileInfo]

    foreach ($file in Get-ProductionRustFiles) {
        if (-not (Test-PhysicalCompanionPath $file)) {
            [void]$files.Add($file)
        }
    }

    foreach ($relativeDir in @(
            "core-c/src",
            "core-c/include",
            "core-c/kernels"
        )) {
        $fullDir = Join-Path $Root $relativeDir
        if (-not (Test-Path -LiteralPath $fullDir)) {
            continue
        }
        foreach ($file in Get-ChildItem -LiteralPath $fullDir -Recurse -File -Include *.c,*.h,*.cl,*.cu) {
            if (-not (Test-PhysicalCompanionPath $file)) {
                [void]$files.Add($file)
            }
        }
    }

    return @($files.ToArray())
}function Assert-ForbiddenAlgorithmMarkersAbsent(
    [string[]]$Markers,
    [string]$Diagnostic,
    [string[]]$AllowlistedRelativePaths = @()
) {
    foreach ($file in Get-ForbiddenAlgorithmProductFiles) {
        $relativePath = Get-NormalizedRelativePath $file
        if (Test-LogicalPathAllowlisted $relativePath $AllowlistedRelativePaths) {
            continue
        }
        $contents = Read-ForbiddenAlgorithmLogicalOwnerText $file
        if ($file.Extension -eq ".rs") {
            $contents = Get-RustProductionContents $contents
        }
        foreach ($marker in $Markers) {
            if ($contents.Contains($marker)) {
                Add-ArchitectureError "$relativePath contains forbidden algorithm marker '$marker' ($Diagnostic)"
            }
        }
    }
}function Invoke-ForbiddenAlgorithmsValidation {
foreach ($requiredFile in @(
            "docs/algorithm-policy.md",
            "docs/pruning-policy.md",
            "scripts/architecture/validate_forbidden_algorithms.ps1",
            "crates/clearra-invariant-tests/tests/no_forbidden_algorithm_tests.rs"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "B forbidden algorithm contract required file missing: $requiredFile"
        }
    }
Assert-ForbiddenAlgorithmTextContains "docs/algorithm-policy.md" @(
        "Meet-in-the-middle PC search is not part of Clearra",
        "MeetInTheMiddlePacking",
        "mitm_pc_backend",
        "half_join_pc",
        "front_half_packing",
        "back_half_packing",
        "complement_join_pc",
        "mitm_static_tiling_in_search_path",
        "SmallComponentExactCover",
        "AreaFeasibilityChecker",
        "ComponentExactCoverVerifier",
        "must not turn a helper candidate into a product solution",
        "BuildOrders(P) intersection HoldReachableOrders(Q) is empty",
        "BuildOrders(P)_intersects_HoldReachableOrders(Q)_empty_proof",
        "architecture_validation_rejects_mitm_pc_backend",
        "architecture_validation_rejects_heuristic_prune_reason",
        "architecture_validation_rejects_representative_order_only_coverage",
        "architecture_validation_rejects_first_witness_coverage"
    ) "B forbidden algorithm contract"
Assert-ForbiddenAlgorithmTextContains "docs/pruning-policy.md" @(
        "collision",
        "bounds overflow",
        "target mask overflow",
        "area overflow",
        "piece count overflow",
        "row capacity overflow",
        "exact hash confirm dedupe",
        "coverage universe identity mismatch reject",
        "BuildUp full-key memo dedupe",
        "HoldAutomaton impossible",
        "Reachability impossible",
        "BuildOrders(P) intersection HoldReachableOrders(Q) is empty",
        "MCTS low score",
        "rare piece heuristic",
        "bad shape heuristic",
        "probably impossible",
        "no immediate placement",
        "target-frame floating",
        "spin classifier unknown",
        "score below threshold",
        "first witness missing",
        "representative order failed",
        "Bloom filter false positive",
        "resource cap reached",
        "verify_first",
        "representative-order replay",
        "All-state domain pruning is not connected",
        "clear_state_set_digest",
        "candidate_domain_table_digest",
        "cannot_promote_by_count_only_without_clear_state_set_digest"
    ) "B pruning policy contract"
Assert-ForbiddenAlgorithmTextContains "core-c/include/clr_pruning.h" @(
        "clr_prune_reason_has_connected_engine_factory",
        "clr_clear_state_domain_promote_if_all_reachable_clear_states"
    ) "B pruning engine-owned proof contract"
Assert-ForbiddenAlgorithmTextContains "core-c/src/pruning/domain_propagation.c" @(
        "CLR_PRUNE_PROOF_ALL_REACHABLE_CLEAR_STATES"
    ) "B pruning conditional all-state marker implementation"
foreach ($removedRawProofApi in @(
        "clr_global_forced_piece_family_proof",
        "clr_clear_state_domain_promote_with_global_proof",
        "clr_pruning_candidate_drop_allowed"
    )) {
    foreach ($path in @("core-c/include/clr_pruning.h", "core-c/src/pruning/domain_propagation.c", "core-c/src/pruning/pruning_proof_ledger.c")) {
        if ((Read-Text $path) -like "*$removedRawProofApi*") {
            Add-ArchitectureError "C pruning must not expose caller-constructed proof API '$removedRawProofApi'"
        }
    }
}
Assert-ForbiddenAlgorithmTextContains "core-c/tests/pruning_tests.c" @(
        "cannot_promote_by_count_only_without_clear_state_set_digest",
        "global_forced_piece_requires_complete_clear_state_set",
        "clear_state_set_truncated_keeps_candidate",
        "component_domain_digest_changes_with_operation_table",
        "target_frame_domain_never_global_safe_without_clear_state_set"
    ) "B pruning GlobalSafe proof tests"
$mitmMarkers = @(
        "MeetInTheMiddlePacking",
        "mitm_pc_backend",
        "half_join_pc",
        "front_half_packing",
        "back_half_packing",
        "complement_join_pc",
        "mitm_static_tiling_in_search_path"
    )
Assert-ForbiddenAlgorithmMarkersAbsent $mitmMarkers "architecture_validation_rejects_mitm_pc_backend"
$heuristicPruneMarkers = @(
        "mcts_low_score",
        "MctsLowScore",
        "MCTS low score",
        "rare_piece_heuristic",
        "RarePieceHeuristic",
        "rare piece heuristic",
        "bad_shape_heuristic",
        "BadShapeHeuristic",
        "bad shape heuristic",
        "probably_impossible",
        "ProbablyImpossible",
        "probably impossible",
        "no_immediate_placement",
        "NoImmediatePlacement",
        "no immediate placement",
        "target_frame_floating",
        "TargetFrameFloating",
        "target-frame floating",
        "spin_classifier_unknown",
        "SpinClassifierUnknown",
        "spin classifier unknown",
        "spin_unknown",
        "SpinUnknown",
        "score_below_threshold",
        "ScoreBelowThreshold",
        "score below threshold",
        "score_too_low",
        "ScoreTooLow",
        "first_witness_missing",
        "FirstWitnessMissing",
        "first witness missing",
        "representative_order_failed",
        "RepresentativeOrderFailed",
        "representative order failed",
        "bloom_filter_false_positive",
        "BloomFilterFalsePositive",
        "Bloom filter false positive",
        "resource_cap_reached",
        "ResourceCapReached",
        "resource cap reached"
    )
$heuristicPruneMarkerRegistryFiles = @(
        "crates/clearra-core-domain/src/pruning/prune_reason.rs",
        "core-c/src/pruning/prune_reason.c"
    )
Assert-ForbiddenAlgorithmMarkersAbsent $heuristicPruneMarkers "architecture_validation_rejects_heuristic_prune_reason" $heuristicPruneMarkerRegistryFiles
$coverageShortcutMarkers = @(
        "representative_order_only_coverage",
        "RepresentativeOrderOnlyCoverage",
        "first_witness_coverage",
        "FirstWitnessCoverage"
    )
Assert-ForbiddenAlgorithmMarkersAbsent $coverageShortcutMarkers "architecture_validation_rejects_representative_order_only_coverage"
$validatorText = Read-Text "scripts/architecture/validate_forbidden_algorithms.ps1"
foreach ($marker in @(
            "architecture_validation_rejects_mitm_pc_backend",
            "architecture_validation_rejects_heuristic_prune_reason",
            "architecture_validation_rejects_representative_order_only_coverage",
            "architecture_validation_rejects_first_witness_coverage"
        )) {
        if ($validatorText -notlike "*$marker*") {
            Add-ArchitectureError "B forbidden algorithm validator must expose marker '$marker'"
        }
    }
}
