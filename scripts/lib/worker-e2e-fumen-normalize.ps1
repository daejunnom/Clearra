# This file is dot-sourced by scripts/worker-e2e.ps1.
function Read-WorkerE2EFumenValue(
    [System.Collections.Generic.List[int]]$Values,
    [ref]$Cursor,
    [int]$Count
) {
    $value = 0
    $base = 64
    for ($digit = 0; $digit -lt $Count; $digit++) {
        if ($Cursor.Value -ge $Values.Count) {
            throw "WorkerE2E fumen fixture ended unexpectedly"
        }
        $value += $Values[$Cursor.Value] * [Math]::Pow($base, $digit)
        $Cursor.Value += 1
    }
    return [int]$value
}function ConvertFrom-WorkerE2EFumenLikePages([string]$Text) {
    $withoutParams = ($Text -split "&", 2)[0].Trim()
    $markerIndex = -1
    foreach ($marker in @("v115@", "m115@", "d115@")) {
        $index = $withoutParams.IndexOf($marker, [System.StringComparison]::Ordinal)
        if ($index -ge 0 -and ($markerIndex -lt 0 -or $index -lt $markerIndex)) {
            $markerIndex = $index
        }
    }
    if ($markerIndex -lt 0) {
        throw "WorkerE2E fumen fixture must use fumen-like v115 payload"
    }

    $data = ($withoutParams.Substring($markerIndex + 5).ToCharArray() |
        Where-Object { $_ -ne '?' -and -not [char]::IsWhiteSpace($_) }) -join ""
    if ([string]::IsNullOrWhiteSpace($data)) {
        throw "WorkerE2E fumen fixture has empty v115 data"
    }

    $encodeTable = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    $values = New-Object System.Collections.Generic.List[int]
    for ($index = 0; $index -lt $data.Length; $index++) {
        $value = $encodeTable.IndexOf($data[$index])
        if ($value -lt 0) {
            throw "WorkerE2E fumen fixture contains invalid v115 character '$($data[$index])'"
        }
        $values.Add($value)
    }

    $cursor = 0
    $fieldBlocks = 10 * 24
    $pages = New-Object System.Collections.Generic.List[string]
    $repeatCount = 0
    $lastComment = ""
    $pageIndex = 0

    while ($cursor -lt $values.Count) {
        if ($repeatCount -gt 0) {
            $repeatCount -= 1
        } else {
            $fieldIndex = 0
            $changed = $true
            while ($fieldIndex -lt $fieldBlocks) {
                $diffBlock = Read-WorkerE2EFumenValue -Values $values -Cursor ([ref]$cursor) -Count 2
                $diff = [int][Math]::Floor($diffBlock / $fieldBlocks)
                $blockCount = $diffBlock % $fieldBlocks
                if ($diff -gt 16) {
                    throw "WorkerE2E fumen fixture has invalid field diff"
                }
                if ($diff -eq 8 -and $blockCount -eq ($fieldBlocks - 1)) {
                    $changed = $false
                }
                $fieldIndex += $blockCount + 1
                if ($fieldIndex -gt $fieldBlocks) {
                    throw "WorkerE2E fumen fixture has invalid field run"
                }
            }
            if (-not $changed) {
                $repeatCount = Read-WorkerE2EFumenValue -Values $values -Cursor ([ref]$cursor) -Count 1
            }
        }

        $action = Read-WorkerE2EFumenValue -Values $values -Cursor ([ref]$cursor) -Count 3
        $action = [int][Math]::Floor($action / 8)
        $action = [int][Math]::Floor($action / 4)
        $action = [int][Math]::Floor($action / $fieldBlocks)
        $action = [int][Math]::Floor($action / 2)
        $action = [int][Math]::Floor($action / 2)
        $action = [int][Math]::Floor($action / 2)
        $hasComment = ($action % 2) -ne 0

        if ($hasComment) {
            $commentLength = Read-WorkerE2EFumenValue -Values $values -Cursor ([ref]$cursor) -Count 2
            $escaped = New-Object System.Text.StringBuilder
            for ($chunk = 0; $chunk -lt [Math]::Ceiling($commentLength / 4); $chunk++) {
                $chunkValue = Read-WorkerE2EFumenValue -Values $values -Cursor ([ref]$cursor) -Count 5
                for ($offset = 0; $offset -lt 4; $offset++) {
                    $charIndex = [int]($chunkValue % 96)
                    if ($charIndex -ge 95) {
                        throw "WorkerE2E fumen fixture has invalid comment character"
                    }
                    [void]$escaped.Append([char]([int](32 + $charIndex)))
                    $chunkValue = [int][Math]::Floor($chunkValue / 96)
                }
            }
            $comment = ConvertFrom-WorkerE2EFumenEscapedComment ($escaped.ToString().Substring(0, $commentLength))
            $lastComment = $comment
            $pages.Add($comment)
        } elseif ($pageIndex -eq 0) {
            $pages.Add("")
        } else {
            $pages.Add($lastComment)
        }
        $pageIndex += 1
    }

    return @($pages.ToArray())
}function ConvertFrom-WorkerE2EFumenEscapedComment([string]$Escaped) {
    $builder = New-Object System.Text.StringBuilder
    $index = 0
    while ($index -lt $Escaped.Length) {
        if ($Escaped[$index] -ne '%') {
            [void]$builder.Append($Escaped[$index])
            $index += 1
            continue
        }
        if ($index + 1 -lt $Escaped.Length -and $Escaped[$index + 1] -eq 'u') {
            $hex = $Escaped.Substring($index + 2, 4)
            [void]$builder.Append([char][Convert]::ToInt32($hex, 16))
            $index += 6
        } else {
            $hex = $Escaped.Substring($index + 1, 2)
            [void]$builder.Append([char][Convert]::ToInt32($hex, 16))
            $index += 3
        }
    }
    return $builder.ToString()
}function ConvertFrom-WorkerE2EPageFields([string]$Page) {
    $fields = @{}
    foreach ($line in @($Page -split "`r?`n")) {
        $parts = $line.Split("=", 2)
        if ($parts.Count -ne 2) {
            continue
        }
        $fields[$parts[0].Trim().ToLowerInvariant()] = (($parts[1].Trim() -split "\s+") -join " ")
    }
    return $fields
}function Get-WorkerE2ENormalizedSolutionKeys(
    [string]$Root,
    [string]$Path
) {
    $resolved = Join-Path $Root $Path
    $text = (Get-Content -LiteralPath $resolved -Raw).Trim()
    $keys = New-Object System.Collections.Generic.HashSet[string]
    foreach ($page in ConvertFrom-WorkerE2EFumenLikePages $text) {
        $fields = ConvertFrom-WorkerE2EPageFields $page
        $kind = if ($fields.ContainsKey("kind")) { [string]$fields["kind"] } else { "" }
        $isSolution = $kind -eq "normalized-solution" -or $kind.EndsWith("-solution") -or
            ($fields.ContainsKey("normalized_solution_page") -and [string]$fields["normalized_solution_page"] -eq "true")
        if (-not $isSolution) {
            continue
        }
        $key = @(
            $(if ($fields.ContainsKey("initial_board_mask")) { $fields["initial_board_mask"] } else { "0x0" }),
            $(if ($fields.ContainsKey("final_board_mask")) { $fields["final_board_mask"] } else { "0x0" }),
            $(if ($fields.ContainsKey("piece_sequence")) { $fields["piece_sequence"] } else { "" }),
            $(if ($fields.ContainsKey("hold_decision_sequence")) { $fields["hold_decision_sequence"] } else { "" }),
            $(if ($fields.ContainsKey("operation_sequence")) { $fields["operation_sequence"] } else { "" }),
            $(if ($fields.ContainsKey("cleared_line_sequence")) { $fields["cleared_line_sequence"] } else { "" }),
            $(if ($fields.ContainsKey("normalized_shape_key")) { $fields["normalized_shape_key"] } else { "" }),
            $(if ($fields.ContainsKey("normalized_tiling_key")) { $fields["normalized_tiling_key"] } else { "" })
        ) -join "|"
        [void]$keys.Add($key)
    }
    return $keys
}function Get-WorkerE2ENormalizedSolutionSetHash(
    [string]$Root,
    [string]$Path
) {
    $keys = Get-WorkerE2ENormalizedSolutionKeys -Root $Root -Path $Path
    $sortedKeys = @($keys | Sort-Object)
    $modulus = [System.Numerics.BigInteger]::Pow([System.Numerics.BigInteger]2, 64)
    $prime = [System.Numerics.BigInteger]::Parse("1099511628211")
    $hash = [System.Numerics.BigInteger]0

    foreach ($key in $sortedKeys) {
        if ($hash -eq 0) {
            $hash = [System.Numerics.BigInteger]::Parse("14695981039346656037")
        }

        foreach ($byte in [System.Text.Encoding]::UTF8.GetBytes([string]$key)) {
            $hash = (($hash -bxor [System.Numerics.BigInteger]$byte) * $prime) % $modulus
        }
    }

    $hash64 = [UInt64]$hash
    return ("wes1:{0:x16}" -f $hash64)
}function Assert-WorkerE2EFumenFileContract(
    [Parameter(Mandatory)]
    [string]$Root,

    [Parameter(Mandatory)]
    [string]$Path
) {
    $resolved = Join-Path $Root $Path
    if (-not (Test-Path -LiteralPath $resolved)) {
        throw "WorkerE2E fumen fixture is missing: $Path"
    }

    $text = (Get-Content -LiteralPath $resolved -Raw).Trim()
    if ([string]::IsNullOrWhiteSpace($text)) {
        throw "WorkerE2E fumen fixture must not be empty: $Path"
    }
    if ($text -notlike "v115@*") {
        throw "WorkerE2E fumen fixture must use fumen-like v115 payload: $Path"
    }
    foreach ($forbiddenMarker in @(
            "raw_fumen_string_exact_equality=true",
            "expected_fumen_string == actual_fumen_string",
            "expected_fumen_string==actual_fumen_string"
        )) {
        if ($text -like "*$forbiddenMarker*") {
            throw "WorkerE2E fumen fixture must not opt into raw fumen exact equality: $Path"
        }
    }
    [void](ConvertFrom-WorkerE2EFumenLikePages $text)
}function Assert-WorkerE2EExternalPcFumenContracts(
    [Parameter(Mandatory)]
    [string]$Root
) {
    foreach ($path in @(
            "tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_setup.fumen",
            "tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_expected_any.fumen",
            "tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_setup.fumen",
            "tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_full_42.fumen"
        )) {
        Assert-WorkerE2EFumenFileContract -Root $Root -Path $path
    }
}function Assert-WorkerE2EFixtureFumenSolutionSet(
    [Parameter(Mandatory)]
    [string]$Root,

    [Parameter(Mandatory)]
    [object]$Fixture
) {
    if ($Fixture.input.PSObject.Properties.Name -contains "initial_fumen") {
        Assert-WorkerE2EFumenFileContract -Root $Root -Path ([string]$Fixture.input.initial_fumen)
    }

    if (-not ($Fixture.input.PSObject.Properties.Name -contains "expected_solution_fumen")) {
        return
    }

    $keys = Get-WorkerE2ENormalizedSolutionKeys -Root $Root -Path ([string]$Fixture.input.expected_solution_fumen)
    if ($Fixture.expected.PSObject.Properties.Name -contains "expected_unique_solution_count") {
        $expected = [int](ConvertTo-WorkerE2EScalar $Fixture.expected.expected_unique_solution_count)
        if ($keys.Count -ne $expected) {
            throw "WorkerE2E normalized solution set count mismatch: expected=$expected actual=$($keys.Count)"
        }
    }

    if (-not ($Fixture.input.PSObject.Properties.Name -contains "expected_solution_normalize_report")) {
        return
    }

    $reportPath = [string]$Fixture.input.expected_solution_normalize_report
    $report = Read-WorkerE2EJsonFile -Root $Root -Path $reportPath
    if ((ConvertTo-WorkerE2EScalar $report.kind) -ne "external-pc-normalize-report") {
        throw "WorkerE2E normalize report must use kind=external-pc-normalize-report: $reportPath"
    }
    if ((ConvertTo-WorkerE2EScalar $report.fixture_id) -ne (ConvertTo-WorkerE2EScalar $Fixture.fixture_id)) {
        throw "WorkerE2E normalize report fixture_id mismatch: $reportPath"
    }
    if ([int](ConvertTo-WorkerE2EScalar $report.source_unique_solution_count) -ne $keys.Count) {
        throw "WorkerE2E normalize report source count mismatch: $reportPath"
    }
    if ([int](ConvertTo-WorkerE2EScalar $report.normalized_unique_solution_count) -ne $keys.Count) {
        throw "WorkerE2E normalize report normalized count mismatch: $reportPath"
    }
    if ([int](ConvertTo-WorkerE2EScalar $report.page_count) -ne $keys.Count) {
        throw "WorkerE2E normalize report page_count must track solution pages: $reportPath"
    }
    if ((ConvertTo-WorkerE2EScalar $report.comment_ignored) -ne "true") {
        throw "WorkerE2E normalize report must declare comment_ignored=true: $reportPath"
    }
    if ((ConvertTo-WorkerE2EScalar $report.mirror_policy) -ne "none") {
        throw "WorkerE2E normalize report must pin mirror_policy=none: $reportPath"
    }

    $actualHash = Get-WorkerE2ENormalizedSolutionSetHash -Root $Root -Path ([string]$Fixture.input.expected_solution_fumen)
    if ((ConvertTo-WorkerE2EScalar $report.solution_set_hash) -ne $actualHash) {
        throw "WorkerE2E normalize report hash mismatch: expected=$($report.solution_set_hash) actual=$actualHash"
    }
}