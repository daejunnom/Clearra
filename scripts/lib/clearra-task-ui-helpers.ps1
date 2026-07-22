# This file is dot-sourced by clearra-start-helpers.ps1.

function Expand-ClearraTasks([string[]]$RequestedTasks) {
    $expanded = New-Object System.Collections.Generic.List[string]
    $allowed = New-Object 'System.Collections.Generic.Dictionary[string,string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($allowedTask in $script:ClearraAllowedTasks) {
        $allowed[$allowedTask] = $allowedTask
    }

    foreach ($taskValue in $RequestedTasks) {
        if ([string]::IsNullOrWhiteSpace($taskValue)) {
            continue
        }
        foreach ($rawTaskName in ([string]$taskValue -split ",")) {
            $taskName = $rawTaskName.Trim()
            if ([string]::IsNullOrWhiteSpace($taskName)) {
                continue
            }
            if (-not $allowed.ContainsKey($taskName)) {
                throw "Unknown Clearra task '$taskName'. Valid tasks: $($script:ClearraAllowedTasks -join ', ')"
            }
            $canonicalTaskName = $allowed[$taskName]
            if ($canonicalTaskName -eq "All") {
                $expanded.Add("Quick")
                $expanded.Add("Local")
                $expanded.Add("NativeLocal")
            } elseif ($canonicalTaskName -eq "Acceptance") {
                $expanded.Add("UXSmoke")
                $expanded.Add("DesktopHost")
                $expanded.Add("ProductE2E")
                $expanded.Add("Local")
                $expanded.Add("NativeLocal")
            } elseif ($canonicalTaskName -eq "ReleaseAcceptance") {
                $expanded.Add("NoProductDebt")
                $expanded.Add("AdversarialCorrectness")
                $expanded.Add("CSanitizer")
                $expanded.Add("RustExactTests")
                $expanded.Add("ProductE2E")
                $expanded.Add("WasmBuildTest")
                $expanded.Add("DesktopHost")
                $expanded.Add("RenderGolden")
            } elseif ($canonicalTaskName -eq "GpuWorkerRelease") {
                $expanded.Add("ReleaseAcceptance")
                $expanded.Add("GpuWorkerAcceptance")
            } elseif ($canonicalTaskName -eq "WorkerAcceptance") {
                $expanded.Add("WorkerE2E")
                $expanded.Add("WorkerE2EStress")
                $expanded.Add("ProductE2E")
                $expanded.Add("GpuWorkerAcceptance")
                $expanded.Add("Validate")
            } elseif ($canonicalTaskName -eq "WorkerRelease") {
                $expanded.Add("WorkerE2E")
                $expanded.Add("WorkerE2EStress")
                $expanded.Add("GpuWorkerRelease")
            } elseif ($canonicalTaskName -eq "Mvp2Acceptance") {
                $expanded.Add("Mvp2Acceptance")
            } elseif ($canonicalTaskName -eq "Mvp3Acceptance") {
                $expanded.Add("Mvp3Acceptance")
            } else {
                $expanded.Add($canonicalTaskName)
            }
        }
    }
    if ($expanded.Count -eq 0) {
        $expanded.Add("Local")
    }
    return [string[]]$expanded.ToArray()
}

