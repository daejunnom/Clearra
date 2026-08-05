function ConvertTo-ClearraPolicyState([AllowNull()][object]$Value) {
    if ($null -eq $Value) {
        return "unknown"
    }
    switch ([int]$Value) {
        0 { return "off" }
        1 { return "audit" }
        2 { return "enforced" }
        default { return "unknown" }
    }
}

function Get-ClearraApplicationControlStatus {
    if (-not (Test-StartTestsWindows)) {
        return [pscustomobject]@{
            schema_version = 1
            platform = "non-windows"
            query_status = "not-applicable"
            code_integrity_policy = "not-applicable"
            user_mode_code_integrity_policy = "not-applicable"
            local_source_build_policy = "not-applicable"
            generated_executable_policy = "not-applicable"
            policy_evidence_only = $true
            diagnostic_code = $null
            reason = $null
        }
    }

    try {
        $deviceGuard = Get-CimInstance `
            -Namespace "root\Microsoft\Windows\DeviceGuard" `
            -ClassName "Win32_DeviceGuard" `
            -ErrorAction Stop
        $codeIntegrity = ConvertTo-ClearraPolicyState `
            $deviceGuard.CodeIntegrityPolicyEnforcementStatus
        $userModeCodeIntegrity = ConvertTo-ClearraPolicyState `
            $deviceGuard.UsermodeCodeIntegrityPolicyEnforcementStatus
        $blocked = $userModeCodeIntegrity -eq "enforced"
        return [pscustomobject]@{
            schema_version = 1
            platform = "windows"
            query_status = "ok"
            code_integrity_policy = $codeIntegrity
            user_mode_code_integrity_policy = $userModeCodeIntegrity
            local_source_build_policy = if ($blocked) { "compile-only" } else { "allowed" }
            generated_executable_policy = if ($blocked) { "deny" } else { "allow" }
            policy_evidence_only = $false
            diagnostic_code = if ($blocked) {
                "W_WINDOWS_UMCI_SOURCE_BUILD_MAY_BE_BLOCKED"
            } else {
                $null
            }
            reason = if ($blocked) {
                "Windows user-mode code integrity requires an enterprise-approved executable."
            } else {
                $null
            }
        }
    } catch {
        return [pscustomobject]@{
            schema_version = 1
            platform = "windows"
            query_status = "failed"
            code_integrity_policy = "unknown"
            user_mode_code_integrity_policy = "unknown"
            local_source_build_policy = "unknown"
            generated_executable_policy = "deny"
            policy_evidence_only = $true
            diagnostic_code = "W_WINDOWS_APPLICATION_CONTROL_STATUS_UNKNOWN"
            reason = $_.Exception.Message
        }
    }
}

function Assert-ClearraWindowsGeneratedExecutionAllowed([string]$TaskName) {
    if (-not (Test-StartTestsWindows)) {
        return $null
    }

    $status = Get-ClearraApplicationControlStatus
    if ($status.query_status -ne 'ok') {
        throw (
            'E_WINDOWS_APPLICATION_CONTROL_PREFLIGHT_UNKNOWN: {0} cannot launch ' +
            'source-generated executables because the Windows UMCI state could not be ' +
            'established. Clearra did not launch, retry, sign, unblock, or substitute ' +
            'another runtime.'
        ) -f $TaskName
    }
    if ($status.generated_executable_policy -ne 'allow') {
        throw (New-ClearraGeneratedExecutionRequiresApprovedPackageMessage `
            $TaskName `
            $status)
    }
    return $status
}

function Assert-ClearraWindowsRuntimeArtifactAllowed(
    [string]$ArtifactPath,
    [string]$TaskName
) {
    if (-not (Test-StartTestsWindows)) {
        return $null
    }
    $report = Get-ClearraWindowsRuntimeArtifactTrustReport $ArtifactPath $TaskName
    if ($report.preflight_decision -eq 'unknown-fail-closed') {
        throw (
            'E_WINDOWS_APPLICATION_CONTROL_PREFLIGHT_UNKNOWN: {0} cannot launch {1} ' +
            'because the Windows application-control state could not be established. ' +
            'Clearra did not launch, retry, sign, unblock, or substitute another runtime.'
        ) -f $TaskName, $report.artifact_path
    }
    if ($report.preflight_decision -eq 'deny-unapproved-artifact') {
        throw (New-ClearraRuntimeArtifactBlockedMessage `
            $TaskName `
            $report.artifact_path `
            $report.policy `
            $report)
    }
    return $report
}

function New-ClearraGeneratedExecutionRequiresApprovedPackageMessage(
    [string]$TaskName,
    [AllowNull()][object]$Status
) {
    $policy = if ($null -eq $Status) {
        'unknown'
    } else {
        [string]$Status.user_mode_code_integrity_policy
    }
    return (
        'E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE: {0} requires ' +
        'source-generated executables, but Windows UMCI is {1}. Clearra stopped before ' +
        'building or launching another transient PE. Use ManagedLocal for process-free ' +
        'source validation and C static-library compilation, or run a release-pipeline-built, ' +
        'enterprise-approved package. ' +
        'Clearra did not retry, sign, unblock, change policy, or substitute WSL/WASM.'
    ) -f $TaskName, $policy
}

function Get-ClearraRecentGeneratedExecutableBlockEvidence(
    [int]$Minutes = 1440,
    [AllowNull()][Nullable[DateTime]]$Since = $null,
    [string[]]$ArtifactName = @(),
    [AllowNull()][string]$ParentProcessName = $null
) {
    if (-not (Test-StartTestsWindows)) {
        return [pscustomobject]@{
            query_status = "not-applicable"
            query_error = $null
            matched_event_count = 0
            latest_event_id = $null
            latest_event_time_utc = $null
            blocked_artifact_names = @()
            blocked_artifact_paths = @()
            parent_process_names = @()
            policy_ids = @()
            enterprise_signing_requirement = $false
        }
    }

    try {
        $startTime = if ($null -ne $Since -and $Since.HasValue) {
            $Since.Value
        } else {
            (Get-Date).AddMinutes(-1 * [Math]::Max(1, $Minutes))
        }
        $clearraArtifactPattern = '(?i)\\AppData\\Local\\Clearra\\build\\'
        $requestedArtifactNames = @($ArtifactName | Where-Object {
                -not [string]::IsNullOrWhiteSpace($_)
            } | ForEach-Object {
                [System.IO.Path]::GetFileName($_)
            })
        $events = @(Get-WinEvent -FilterHashtable @{
                LogName = "Microsoft-Windows-CodeIntegrity/Operational"
                StartTime = $startTime
                Id = @(3033, 3077)
            } -MaxEvents 200 -ErrorAction Stop | Where-Object {
                $event = $_
                $event.Message -match $clearraArtifactPattern -and
                ([string]::IsNullOrWhiteSpace($ParentProcessName) -or
                    $event.Message -match ('(?i)\\' + [regex]::Escape($ParentProcessName) + '\)')) -and
                ($requestedArtifactNames.Count -eq 0 -or @(
                        $requestedArtifactNames | Where-Object {
                            $event.Message -match ('(?i)\\' + [regex]::Escape($_) + '(?:\s|$)')
                        }
                    ).Count -gt 0)
            })
        $latest = $events | Select-Object -First 1
        $blockedArtifactNames = @($events | ForEach-Object {
                if ($_.Message -match '(?i)attempted to load (?<artifact>.+?\.(?:dll|exe)) that did not meet') {
                    [System.IO.Path]::GetFileName($Matches.artifact)
                }
            } | Sort-Object -Unique)
        $blockedArtifactPaths = @($events | ForEach-Object {
                if ($_.Message -match '(?i)attempted to load (?<artifact>.+?\.(?:dll|exe)) that did not meet') {
                    $Matches.artifact
                }
            } | Sort-Object -Unique)
        $parentProcessNames = @($events | ForEach-Object {
                if ($_.Message -match '(?i)a process \((?<process>.+?\.(?:exe|dll))\) attempted to load') {
                    [System.IO.Path]::GetFileName($Matches.process)
                }
            } | Sort-Object -Unique)
        $policyIds = @($events | ForEach-Object {
                if ($_.Message -match '(?i)Policy ID:\s*(?<policy>\{[0-9a-f-]+\})') {
                    $Matches.policy.ToLowerInvariant()
                }
            } | Sort-Object -Unique)
        return [pscustomobject]@{
            query_status = "ok"
            query_error = $null
            matched_event_count = $events.Count
            latest_event_id = if ($null -eq $latest) { $null } else { $latest.Id }
            latest_event_time_utc = if ($null -eq $latest -or $null -eq $latest.TimeCreated) {
                $null
            } else {
                $latest.TimeCreated.ToUniversalTime().ToString("o")
            }
            blocked_artifact_names = $blockedArtifactNames
            blocked_artifact_paths = $blockedArtifactPaths
            parent_process_names = $parentProcessNames
            policy_ids = $policyIds
            enterprise_signing_requirement = @($events | Where-Object {
                    $_.Message -match '(?i)Enterprise signing level requirements'
                }).Count -gt 0
        }
    } catch {
        $noEvents = $_.Exception.Message -match '(?i)no events were found|specified selection criteria|지정된 선택 조건'
        return [pscustomobject]@{
            query_status = if ($noEvents) { "ok" } else { "failed" }
            query_error = if ($noEvents) { $null } else { $_.Exception.Message }
            matched_event_count = 0
            latest_event_id = $null
            latest_event_time_utc = $null
            blocked_artifact_names = @()
            blocked_artifact_paths = @()
            parent_process_names = @()
            policy_ids = @()
            enterprise_signing_requirement = $false
        }
    }
}

function Wait-ClearraGeneratedExecutableBlockEvidence(
    [Parameter(Mandatory = $true)]
    [DateTime]$Since,
    [int]$TimeoutMilliseconds = 3000,
    [int]$PollIntervalMilliseconds = 150,
    [string[]]$ArtifactName = @(),
    [AllowNull()][string]$ParentProcessName = $null
) {
    if ($TimeoutMilliseconds -lt 0) {
        throw 'TimeoutMilliseconds must be zero or greater.'
    }
    if ($PollIntervalMilliseconds -lt 1) {
        throw 'PollIntervalMilliseconds must be at least 1.'
    }

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $evidence = Get-ClearraRecentGeneratedExecutableBlockEvidence `
            -Since $Since `
            -ArtifactName $ArtifactName `
            -ParentProcessName $ParentProcessName
        if ($evidence.query_status -ne 'ok' -or $evidence.matched_event_count -gt 0) {
            return $evidence
        }
        if ([DateTime]::UtcNow -ge $deadline) {
            return $evidence
        }
        Start-Sleep -Milliseconds $PollIntervalMilliseconds
    } while ($true)
}

function Get-ClearraWindowsRuntimeArtifactTrustReport(
    [string]$ArtifactPath,
    [string]$TaskName = 'Windows native runtime'
) {
    $path = [System.IO.Path]::GetFullPath($ArtifactPath)
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Windows runtime artifact does not exist: $path"
    }

    $item = Get-Item -LiteralPath $path
    $hash = Get-FileHash -LiteralPath $path -Algorithm SHA256
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    $policy = Get-ClearraApplicationControlStatus
    $evidence = Get-ClearraRecentGeneratedExecutableBlockEvidence `
        -ArtifactName @($item.Name)
    $decision = if ($policy.query_status -ne 'ok') {
        'unknown-fail-closed'
    } elseif ($policy.user_mode_code_integrity_policy -eq 'enforced' -and
        [string]$signature.Status -ne 'Valid') {
        'deny-unapproved-artifact'
    } else {
        'eligible-for-single-windows-verdict'
    }

    return [pscustomobject]@{
        schema_version = 1
        task_name = $TaskName
        artifact_path = $path
        artifact_size = $item.Length
        artifact_sha256 = $hash.Hash.ToLowerInvariant()
        signature_status = [string]$signature.Status
        signer_subject = if ($null -eq $signature.SignerCertificate) {
            $null
        } else {
            $signature.SignerCertificate.Subject
        }
        policy = $policy
        recent_block_evidence = $evidence
        preflight_decision = $decision
        windows_final_launch_verdict_required = $decision -eq 'eligible-for-single-windows-verdict'
        policy_or_signing_mutation = $false
        runtime_substitution = $false
    }
}

function Test-ClearraApplicationControlBlockOutput([AllowNull()][object[]]$Output) {
    if ($null -eq $Output) {
        return $false
    }
    $text = ($Output | ForEach-Object { $_.ToString() }) -join "`n"
    return $text -match '(?i)os error 4551' -or
        $text -match '(?i)application control policy.*blocked' -or
        $text -match '애플리케이션 제어 정책에서 .*차단'
}

function Test-ClearraCargoFailureExplainedByBlockEvidence(
    [AllowNull()][object[]]$Output,
    [AllowNull()][object]$Evidence
) {
    if ($null -eq $Output -or $null -eq $Evidence -or
        $Evidence.query_status -ne 'ok' -or $Evidence.matched_event_count -eq 0) {
        return $false
    }

    $text = ($Output | ForEach-Object { $_.ToString() }) -join "`n"
    $missingCrates = @([regex]::Matches(
            $text,
            "(?im)can't find crate for\s+\W*(?<crate>[A-Za-z0-9_-]+)"
        ) | ForEach-Object { $_.Groups['crate'].Value } | Sort-Object -Unique)
    if ($missingCrates.Count -eq 0) {
        return $false
    }

    $blockedNames = @($Evidence.blocked_artifact_names)
    return @($missingCrates | Where-Object { $blockedNames -contains $_ }).Count -gt 0
}

function New-ClearraLocalSourceBuildBlockedMessage(
    [string]$TaskName,
    [AllowNull()][object]$Status,
    [AllowNull()][object]$Evidence = $null
) {
    $policy = if ($null -eq $Status) {
        'unknown'
    } else {
        [string]$Status.user_mode_code_integrity_policy
    }
    $evidenceSuffix = if ($null -eq $Evidence -or $Evidence.matched_event_count -eq 0) {
        ''
    } else {
        $policyIds = @($Evidence.policy_ids) -join ','
        $artifacts = @($Evidence.blocked_artifact_names) -join ','
        " PolicyIds=$policyIds; Artifacts=$artifacts."
    }
    return ((
        'E_WINDOWS_LOCAL_SOURCE_BUILD_BLOCKED: {0} was attempted with the native Windows ' +
        'toolchain, but Windows Application Control blocked a source-generated native artifact ' +
        '(UMCI={1}). Clearra did not invoke WSL, change policy, unblock files, sign artifacts, ' +
        'or retry through a weaker execution surface. End-user Windows packages must be built ' +
        'and signed by the release pipeline; the installed product itself has no WSL dependency.'
    ) -f $TaskName, $policy) + $evidenceSuffix
}

function New-ClearraRuntimeArtifactBlockedMessage(
    [string]$TaskName,
    [string]$ArtifactPath,
    [AllowNull()][object]$Status,
    [AllowNull()][object]$TrustReport = $null
) {
    $policy = if ($null -eq $Status) {
        'unknown'
    } else {
        [string]$Status.user_mode_code_integrity_policy
    }
    $signature = if ($null -eq $TrustReport) {
        'unknown'
    } else {
        [string]$TrustReport.signature_status
    }
    $policyIds = if ($null -eq $TrustReport -or
        $null -eq $TrustReport.recent_block_evidence) {
        ''
    } else {
        @($TrustReport.recent_block_evidence.policy_ids) -join ','
    }
    return (
        'E_WINDOWS_RUNTIME_ARTIFACT_BLOCKED: Windows Application Control rejected the ' +
        'prebuilt runtime for {0} (UMCI={1}, signature={2}, policy_ids={3}, artifact={4}). ' +
        'Clearra did not rebuild, ' +
        'retry, change policy, unblock files, sign artifacts, or substitute another runtime.'
    ) -f `
        $TaskName, `
        $policy, `
        $signature, `
        $policyIds, `
        ([System.IO.Path]::GetFullPath($ArtifactPath))
}
