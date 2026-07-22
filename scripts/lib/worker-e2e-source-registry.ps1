# This file is dot-sourced by scripts/worker-e2e.ps1.
function Test-WorkerE2EObjectProperty(
    [Parameter(Mandatory)]
    [object]$Object,

    [Parameter(Mandatory)]
    [string]$Name
) {
    return ($Object.PSObject.Properties.Name -contains $Name)
}function Get-WorkerE2ESourceField(
    [Parameter(Mandatory)]
    [object]$Source,

    [Parameter(Mandatory)]
    [string]$FieldName,

    [Parameter(Mandatory)]
    [string]$DiagnosticCode
) {
    if (-not (Test-WorkerE2EObjectProperty -Object $Source -Name $FieldName)) {
        throw "${DiagnosticCode}: external PC source is missing '$FieldName'"
    }
    $value = $Source.$FieldName
    if ($null -eq $value -or [string]::IsNullOrWhiteSpace([string]$value)) {
        throw "${DiagnosticCode}: external PC source has empty '$FieldName'"
    }
    return $value
}function Assert-WorkerE2ESourceRegistryShape(
    [Parameter(Mandatory)]
    [object]$Registry
) {
    if ((ConvertTo-WorkerE2EScalar $Registry.schema_version) -ne "1" -or
        (ConvertTo-WorkerE2EScalar $Registry.kind) -ne "external-pc-source-registry") {
        throw "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID: source registry must use schema_version=1 and kind=external-pc-source-registry"
    }
    if (-not (Test-WorkerE2EObjectProperty -Object $Registry -Name "sources")) {
        throw "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID: source registry must expose sources"
    }
    if (@($Registry.sources).Count -eq 0) {
        throw "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID: source registry must contain at least one source"
    }
}function Assert-WorkerE2ESourceEntry(
    [Parameter(Mandatory)]
    [object]$Source,

    [System.Collections.Generic.HashSet[string]]$SeenSourceIds
) {
    $sourceId = [string](Get-WorkerE2ESourceField `
            -Source $Source `
            -FieldName "source_id" `
            -DiagnosticCode "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID")
    if (-not $SeenSourceIds.Add($sourceId)) {
        throw "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID: duplicate external PC source_id '$sourceId'"
    }

    [void](Get-WorkerE2ESourceField `
            -Source $Source `
            -FieldName "source_url" `
            -DiagnosticCode "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID")
    [void](Get-WorkerE2ESourceField `
            -Source $Source `
            -FieldName "retrieved_at" `
            -DiagnosticCode "E_EXTERNAL_PC_SOURCE_MISSING_RETRIEVED_AT")
    [void](Get-WorkerE2ESourceField `
            -Source $Source `
            -FieldName "redistribution_note" `
            -DiagnosticCode "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID")

    $sourceKind = [string](Get-WorkerE2ESourceField `
            -Source $Source `
            -FieldName "source_kind" `
            -DiagnosticCode "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID")
    if ($sourceKind -notin @("external-reference-metadata-only", "external-fumen-reference")) {
        throw "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID: unknown external PC source_kind '$sourceKind'"
    }

    if ($sourceKind -eq "external-fumen-reference") {
        $label = [string](Get-WorkerE2ESourceField `
                -Source $Source `
                -FieldName "preferred_fumen_link_label" `
                -DiagnosticCode "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID")
        if ([string]::IsNullOrWhiteSpace($label)) {
            throw "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID: external fumen source '$sourceId' must name the preferred fumen link label"
        }

        $preferredUrl = [string](Get-WorkerE2ESourceField `
                -Source $Source `
                -FieldName "preferred_fumen_source_url" `
                -DiagnosticCode "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID")
        if (-not $preferredUrl.StartsWith("https://fumen.zui.jp/?", [System.StringComparison]::Ordinal)) {
            throw "E_EXTERNAL_PC_SOURCE_REGISTRY_INVALID: external fumen source '$sourceId' must pin a fumen.zui.jp redirect URL"
        }

        [void](Get-WorkerE2ESourceField `
                -Source $Source `
                -FieldName "source_link_retrieved_at" `
                -DiagnosticCode "E_EXTERNAL_PC_SOURCE_MISSING_RETRIEVED_AT")
    }

    if (-not (Test-WorkerE2EObjectProperty -Object $Source -Name "human_verified_required")) {
        throw "E_EXTERNAL_PC_SOURCE_REQUIRES_HUMAN_VERIFICATION: external PC source '$sourceId' must declare human_verified_required"
    }
    if ((ConvertTo-WorkerE2EScalar $Source.human_verified_required) -ne "true") {
        throw "E_EXTERNAL_PC_SOURCE_REQUIRES_HUMAN_VERIFICATION: external PC source '$sourceId' must require human verification"
    }
}function Assert-WorkerE2ESourceRegistryContract(
    [Parameter(Mandatory)]
    [string]$Root
) {
    $registry = Read-WorkerE2EJsonFile -Root $Root -Path "tests/fixtures/external-pc/source_registry.json"
    Assert-WorkerE2ESourceRegistryShape -Registry $registry

    $seenSourceIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($source in @($registry.sources)) {
        Assert-WorkerE2ESourceEntry -Source $source -SeenSourceIds $seenSourceIds
    }

    $markers = ConvertTo-WorkerE2EMarkerText $registry

    foreach ($requiredMarker in @(
            "kind=external-pc-source-registry",
            "source_id=pcinfo-korea-pco-6p-i-hold",
            "source_url=https://sites.google.com/view/pcinfokorea/",
            "retrieved_at=2026-07-07",
            "source_kind=external-reference-metadata-only",
            "contains_images=true",
            "contains_fumen_link=false",
            "human_verified_required=true",
            "source_id=four-pco-opener-full-63",
            "source_url=https://four.lol/perfect-clears/opener/",
            "preferred_solution_set_file=tests/fixtures/external-pc/pco_opener_full_63.source_solutions.json",
            "source_id=hse30-tsar-cannon-full-42",
            "source_url=https://hse30.tistory.com/1224",
            "source_kind=external-fumen-reference",
            "contains_fumen_link=true",
            "preferred_fumen_link_label=전체 42개",
            "preferred_fumen_source_url=https://fumen.zui.jp/?D115@",
            "source_link_retrieved_at=2026-07-07",
            "redistribution_note=metadata-only",
            "redistribution_note=store normalized fumen fixture"
        )) {
        if ($markers -notlike "*$requiredMarker*") {
            throw "source registry missing marker: $requiredMarker"
        }
    }
}