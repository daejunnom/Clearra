# This file is dot-sourced by scripts/worker-e2e.ps1.
function Assert-WorkerE2EMinimalSolveSetIsMetadataOnly(
    [Parameter(Mandatory)]
    [object]$Fixture,

    [Parameter(Mandatory)]
    [string]$FixturePath
) {
    $minimal = if ($Fixture.PSObject.Properties.Name -contains "minimal_solve_set") {
        $Fixture.minimal_solve_set
    } else {
        $null
    }
    if ($null -eq $minimal) {
        if ((ConvertTo-WorkerE2EScalar $Fixture.fixture_id) -eq "pco_i_hold_6p_second_bag_pc" -and
            @($Fixture.source_solution_labels).Count -ge 15 -and
            (ConvertTo-WorkerE2EScalar $Fixture.expected.oracle_kind) -eq "metadata-only-source-labels") {
            return
        }
        if ((ConvertTo-WorkerE2EScalar $Fixture.fixture_id) -eq "tsar_cannon_after_2bag_full_42" -and
            (ConvertTo-WorkerE2EScalar $Fixture.clearra_count_policy.worker_correctness_basis) -eq "unique_solve_set" -and
            (ConvertTo-WorkerE2EScalar $Fixture.clearra_count_policy.expected_unique_solution_count) -eq "42") {
            return
        }
        throw "WorkerE2E fixture must define minimal_solve_set policy: $FixturePath"
    }

    foreach ($propertyName in @(
            "forbidden_as_total_solution_count",
            "forbidden_as_unique_solution_count",
            "forbidden_as_worker_correctness"
        )) {
        if ((ConvertTo-WorkerE2EScalar $minimal.$propertyName) -ne "true") {
            throw "WorkerE2E fixture '$FixturePath' must keep minimal_solve_set.$propertyName=true"
        }
    }
}function Assert-WorkerE2ETsarUniqueSolveSetContract(
    [Parameter(Mandatory)]
    [object]$Fixture
) {
    if ((ConvertTo-WorkerE2EScalar $Fixture.fixture_id) -ne "tsar_cannon_after_2bag_full_42") {
        return
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.source_counts.minimal_solve_set) -ne "18") {
        throw "Tsar Cannon external PC fixture must pin hse30 minimal_solve_set=18 as metadata"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.source_counts.minimal_plus_tspin_extra) -ne "25") {
        throw "Tsar Cannon external PC fixture must pin minimal_plus_tspin_extra=25 as 18+7 metadata"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.source_counts.unique_solve_set) -ne "42") {
        throw "Tsar Cannon external PC fixture must pin source_counts.unique_solve_set=42"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.clearra_count_policy.worker_correctness_basis) -ne "unique_solve_set") {
        throw "Tsar Cannon external PC fixture must use unique_solve_set as worker correctness basis"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.clearra_count_policy.expected_unique_solution_count) -ne "42") {
        throw "Tsar Cannon external PC fixture must pin clearra_count_policy.expected_unique_solution_count=42"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.clearra_count_policy.minimal_solve_set_is_metadata_only) -ne "true") {
        throw "Tsar Cannon external PC fixture must keep minimal solve set as metadata only"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.expected.pc_probability_source_percent) -ne "98.69") {
        throw "Tsar Cannon external PC fixture must pin source PC probability 98.69"
    }
    if ((ConvertTo-WorkerE2EScalar $Fixture.expected.tsd_pc_probability_source_percent) -ne "73.2") {
        throw "Tsar Cannon external PC fixture must pin source TSD-PC probability 73.2"
    }
}
