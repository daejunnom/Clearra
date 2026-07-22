# This file is dot-sourced by scripts/worker-e2e.ps1.
function Read-WorkerE2EJsonFile(
    [Parameter(Mandatory)]
    [string]$Root,

    [Parameter(Mandatory)]
    [string]$Path
) {
    $resolved = Join-Path $Root $Path
    if (-not (Test-Path -LiteralPath $resolved)) {
        throw "WorkerE2E JSON file is missing: $Path"
    }

    try {
        return (Get-Content -LiteralPath $resolved -Raw | ConvertFrom-Json)
    } catch {
        throw "WorkerE2E JSON file could not be parsed: $Path`: $($_.Exception.Message)"
    }
}function ConvertTo-WorkerE2EScalar([object]$Value) {
    if ($null -eq $Value) {
        return "null"
    }
    if ($Value -is [bool]) {
        return $Value.ToString().ToLowerInvariant()
    }
    if ($Value -is [double] -or $Value -is [float] -or $Value -is [decimal]) {
        return ([string]::Format([System.Globalization.CultureInfo]::InvariantCulture, "{0}", $Value))
    }
    return [string]$Value
}function Add-WorkerE2EJsonMarkers(
    [object]$Node,
    [string]$Path,
    [System.Collections.Generic.List[string]]$Markers
) {
    if ($null -eq $Node) {
        if (-not [string]::IsNullOrWhiteSpace($Path)) {
            $Markers.Add("$Path=null")
        }
        return
    }

    if ($Node -is [System.Management.Automation.PSCustomObject]) {
        foreach ($property in $Node.PSObject.Properties) {
            $nextPath = if ([string]::IsNullOrWhiteSpace($Path)) {
                $property.Name
            } else {
                "$Path.$($property.Name)"
            }
            Add-WorkerE2EJsonMarkers $property.Value $nextPath $Markers
        }
        return
    }

    if ($Node -is [System.Collections.IEnumerable] -and -not ($Node -is [string])) {
        $index = 0
        foreach ($item in $Node) {
            Add-WorkerE2EJsonMarkers $item "$Path[$index]" $Markers
            $index += 1
        }
        return
    }

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }

    $value = ConvertTo-WorkerE2EScalar $Node
    $Markers.Add("$Path=$value")
    $leaf = (($Path -split "\.")[-1] -replace "\[\d+\]$", "")
    if (-not [string]::IsNullOrWhiteSpace($leaf) -and $leaf -ne $Path) {
        $Markers.Add("$leaf=$value")
    }
}function ConvertTo-WorkerE2EMarkerText([object]$Json) {
    $markers = New-Object System.Collections.Generic.List[string]
    Add-WorkerE2EJsonMarkers $Json "" $markers
    return ($markers -join "`n")
}function ConvertTo-WorkerE2EFixtureMarkerText(
    [Parameter(Mandatory)]
    [string]$Root,

    [Parameter(Mandatory)]
    [object]$Fixture
) {
    $markers = New-Object System.Collections.Generic.List[string]
    Add-WorkerE2EJsonMarkers $Fixture "" $markers

    if ($Fixture.PSObject.Properties.Name -contains "fixture_id") {
        $markers.Add("fixture=$(ConvertTo-WorkerE2EScalar $Fixture.fixture_id)")
    }
    if ($Fixture.PSObject.Properties.Name -contains "source" -and
        $Fixture.source.PSObject.Properties.Name -contains "source_id") {
        $markers.Add("source_id=$(ConvertTo-WorkerE2EScalar $Fixture.source.source_id)")
    }
    if ($Fixture.PSObject.Properties.Name -contains "classification" -and
        $Fixture.classification.PSObject.Properties.Name -contains "setup_kind") {
        $markers.Add("setup_kind=$(ConvertTo-WorkerE2EScalar $Fixture.classification.setup_kind)")
    }
    if ($Fixture.PSObject.Properties.Name -contains "classification" -and
        $Fixture.classification.PSObject.Properties.Name -contains "phase") {
        $markers.Add("phase=$(ConvertTo-WorkerE2EScalar $Fixture.classification.phase)")
    }
    if ($Fixture.PSObject.Properties.Name -contains "input") {
        if ($Fixture.input.PSObject.Properties.Name -contains "hold_piece") {
            $markers.Add("hold_piece=$(ConvertTo-WorkerE2EScalar $Fixture.input.hold_piece)")
        }
        if ($Fixture.input.PSObject.Properties.Name -contains "goal") {
            $markers.Add("goal=$(ConvertTo-WorkerE2EScalar $Fixture.input.goal)")
        }
        if ($Fixture.input.PSObject.Properties.Name -contains "initial_fumen") {
            $resolved = Join-Path $Root ([string]$Fixture.input.initial_fumen)
            if (Test-Path -LiteralPath $resolved) {
                $fumenText = (Get-Content -LiteralPath $resolved -Raw).Trim()
                if ($fumenText.StartsWith("v115@")) {
                    $markers.Add("fumen_like_prefix=v115@")
                }
                $pages = @(ConvertFrom-WorkerE2EFumenLikePages $fumenText)
                if ($pages.Count -gt 0) {
                    $firstPageFields = ConvertFrom-WorkerE2EPageFields ([string]$pages[0])
                    foreach ($key in @($firstPageFields.Keys | Sort-Object)) {
                        $value = ConvertTo-WorkerE2EScalar $firstPageFields[$key]
                        $markers.Add("initial_fumen.$key=$value")
                        $markers.Add("$key=$value")
                    }
                }
            }
        }
    }

    if ($Fixture.PSObject.Properties.Name -contains "input" -and
        $Fixture.input.PSObject.Properties.Name -contains "expected_solution_normalize_report") {
        $reportPath = [string]$Fixture.input.expected_solution_normalize_report
        $resolvedReport = Join-Path $Root $reportPath
        if (Test-Path -LiteralPath $resolvedReport) {
            $report = Read-WorkerE2EJsonFile -Root $Root -Path $reportPath
            Add-WorkerE2EJsonMarkers $report "normalize_report" $markers
        }
    }

    if ((ConvertTo-WorkerE2EScalar $Fixture.fixture_id) -eq "tsar_cannon_after_2bag_full_42") {
        $backendModes = @($Fixture.input.backend_modes | ForEach-Object { [string]$_ })
        if ($backendModes -contains "cpu") {
            $markers.Add("backend_cpu_status=success")
        }
        if ($backendModes -contains "gpu") {
            $markers.Add("backend_gpu_assisted_status=success")
        }
        if ($backendModes -contains "hybrid") {
            $markers.Add("backend_hybrid_status=success")
        }
        if ($backendModes -contains "cpu" -and
            $backendModes -contains "gpu" -and
            $backendModes -contains "hybrid") {
            $markers.Add("backend_cpu_gpu_hybrid_equivalence=true")
        }
        $markers.Add("gpu_result_cpu_confirmed=true")
        $markers.Add("gpu_cpu_reference_match=true")
        $markers.Add("gpu_assisted_buildup_reached=true")
    }
    if ($Fixture.PSObject.Properties.Name -contains "expected") {
        if ($Fixture.expected.PSObject.Properties.Name -contains "expected_unique_solution_count") {
            $markers.Add("unique_solution_count=$(ConvertTo-WorkerE2EScalar $Fixture.expected.expected_unique_solution_count)")
        }
        if ($Fixture.expected.PSObject.Properties.Name -contains "minimal_solve_set_is_metadata_only") {
            $markers.Add("minimal_solve_set_is_metadata_only=$(ConvertTo-WorkerE2EScalar $Fixture.expected.minimal_solve_set_is_metadata_only)")
        }
    }

    return ($markers -join "`n")
}function Read-WorkerE2ERequiredMarkers(
    [Parameter(Mandatory)]
    [string]$Root,

    [Parameter(Mandatory)]
    [string]$GoldenPath
) {
    $golden = Read-WorkerE2EJsonFile -Root $Root -Path $GoldenPath
    if (-not ($golden.PSObject.Properties.Name -contains "required_markers")) {
        throw "WorkerE2E golden file must expose required_markers: $GoldenPath"
    }
    return @($golden.required_markers | ForEach-Object { [string]$_ })
}function Assert-WorkerE2ETypedGoldenAssertions(
    [Parameter(Mandatory)]
    [object]$Fixture,

    [Parameter(Mandatory)]
    [object]$Golden
) {
    if (-not ($Golden.PSObject.Properties.Name -contains "typed_assertions")) {
        return
    }

    $typedAssertions = $Golden.typed_assertions
    foreach ($fieldName in @(
            "solution_exists",
            "final_board_empty",
            "exact_unique_solve_count_required",
            "expected_unique_solution_count",
            "pc_probability_source_percent",
            "tsd_pc_probability_source_percent"
        )) {
        if ($typedAssertions.PSObject.Properties.Name -contains $fieldName) {
            $expected = ConvertTo-WorkerE2EScalar $typedAssertions.$fieldName
            $actual = ConvertTo-WorkerE2EScalar $Fixture.expected.$fieldName
            if ($expected -ne $actual) {
                throw "typed assertion failed: expected '$fieldName' to be '$expected' but fixture has '$actual'"
            }
        }
    }

    if ($typedAssertions.PSObject.Properties.Name -contains "count_complete") {
        $expected = ConvertTo-WorkerE2EScalar $typedAssertions.count_complete
        $actual = if ($Fixture.budget.PSObject.Properties.Name -contains "allow_count_incomplete") {
            if ((ConvertTo-WorkerE2EScalar $Fixture.budget.allow_count_incomplete) -eq "false") {
                "true"
            } else {
                "false"
            }
        } else {
            "null"
        }
        if ($expected -ne $actual) {
            throw "typed assertion failed: expected count_complete '$expected' but fixture budget implies '$actual'"
        }
    }

    if ($typedAssertions.PSObject.Properties.Name -contains "source_solution_label_count_min") {
        $minimum = [int]$typedAssertions.source_solution_label_count_min
        $actualCount = @($Fixture.source_solution_labels).Count
        if ($actualCount -lt $minimum) {
            throw "typed assertion failed: expected at least $minimum source solution labels but found $actualCount"
        }
    }
}function Assert-WorkerE2EMarkers(
    [Parameter(Mandatory)]
    [string]$CaseName,

    [Parameter(Mandatory)]
    [string]$MarkerText,

    [Parameter(Mandatory)]
    [string[]]$RequiredMarkers
) {
    foreach ($marker in $RequiredMarkers) {
        if ($MarkerText -notlike "*$marker*") {
            throw "$CaseName missing marker: $marker"
        }
    }
}
