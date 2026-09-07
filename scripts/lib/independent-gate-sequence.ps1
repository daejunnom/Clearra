# Diagnostic orchestration only. A failed/blocked stage never earns evidence;
# the aggregate throws after independent stages have had one execution attempt.
function Invoke-ClearraIndependentGateSequence {
    param(
        [Parameter(Mandatory)] [object[]]$Stages,
        [Parameter(Mandatory)] $Scope
    )

    $known = @{}
    foreach ($stage in $Stages) {
        if ([string]::IsNullOrWhiteSpace($stage.Name) -or $known.ContainsKey($stage.Name) -or
            $stage.Body -isnot [scriptblock]) {
            throw 'Independent gate plan contains a missing/duplicate name or invalid body.'
        }
        foreach ($dependency in $stage.Requires) {
            if (-not $known.ContainsKey($dependency)) {
                throw "Gate '$($stage.Name)' requires an absent or later stage '$dependency'."
            }
        }
        $known[$stage.Name] = $true
    }

    $outcomes = @{}
    $failures = New-Object 'System.Collections.Generic.List[string]'
    foreach ($stage in $Stages) {
        $blockedBy = @($stage.Requires | Where-Object { $outcomes[$_] -ne 'passed' })
        if ($blockedBy.Count -gt 0) {
            $outcomes[$stage.Name] = 'blocked'
            $reason = "blocked by $($blockedBy -join ',')"
            $failures.Add("$($stage.Name): $reason")
            Write-Output "release_gate_stage=$($stage.Name) status=blocked reason=$reason"
        } else {
            try {
                Invoke-ClearraProgressCase -Scope $Scope -Name $stage.Name -PreserveOutput -Body {
                    & $stage.Body $stage.Name
                }
                $outcomes[$stage.Name] = 'passed'
            } catch [System.Management.Automation.PipelineStoppedException], [System.OperationCanceledException] {
                # An explicit cancellation is not permission to start more work.
                throw
            } catch {
                $outcomes[$stage.Name] = 'failed'
                $reason = ($_.Exception.Message -split '\r?\n', 2)[0]
                $failures.Add("$($stage.Name): $reason")
                Write-Output "release_gate_stage=$($stage.Name) status=failed reason=$reason"
            }
        }
        # A failed stage was attempted, but must not be counted as a pass.
        $Scope.Pending = [Math]::Max(0, $Stages.Count - $outcomes.Count)
        $Scope.Running = 0
    }
    Complete-ClearraProgressLine $Scope
    Write-Output "release_gate_diagnostics scope=$($Scope.Name) release_authority=false"
    foreach ($stage in $Stages) {
        Write-Output "release_gate_result stage=$($stage.Name) status=$($outcomes[$stage.Name])"
    }
    if ($failures.Count -gt 0) {
        throw "Release gate has $($failures.Count) failed/blocked stage(s):`n$($failures -join "`n")"
    }
}
