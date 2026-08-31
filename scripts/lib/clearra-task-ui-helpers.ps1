# This file is dot-sourced by clearra-start-helpers.ps1.

function Get-ClearraReleaseAcceptanceTasks([string]$Shard = "Full") {
    switch ($Shard) {
        "Full" {
            return [string[]]@(
                "NoProductDebt",
                "AdversarialCorrectness",
                "CSanitizer",
                "RustExactTests",
                "ProductE2E",
                "WasmBuildTest",
                "DesktopHost",
                "RenderGolden"
            )
        }
        "Foundation" {
            # NoProductDebt owns the architecture pass consumed by DesktopHost.
            # Keep AdversarialCorrectness here so its delegated Rust evidence is
            # paired with the same release-mode authority as the full path.
            return [string[]]@(
                "NoProductDebt",
                "AdversarialCorrectness",
                "DesktopHost"
            )
        }
        "Sanitizer" {
            return [string[]]@("CSanitizer")
        }
        "Rust" {
            # These stages share the native-link fingerprint, Cargo target, and
            # the NoProductDebt complete/render delegated evidence owners.
            return [string[]]@(
                "RustExactTests",
                "ProductE2E",
                "RenderGolden"
            )
        }
        "Pages" {
            return [string[]]@("WasmBuildTest")
        }
        default {
            throw "Unknown ReleaseAcceptance shard '$Shard'."
        }
    }
}

function Expand-ClearraTasks {
    param(
        [string[]]$RequestedTasks,
        [ValidateSet("Full", "Foundation", "Sanitizer", "Rust", "Pages")]
        [string]$ReleaseAcceptanceShard = "Full"
    )

    $expanded = New-Object System.Collections.Generic.List[string]
    $allowed = New-Object 'System.Collections.Generic.Dictionary[string,string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($allowedTask in $script:ClearraAllowedTasks) {
        $allowed[$allowedTask] = $allowedTask
    }

    $canonicalRequests = New-Object System.Collections.Generic.List[string]
    foreach ($taskValue in $RequestedTasks) {
        if ([string]::IsNullOrWhiteSpace($taskValue)) {
            continue
        }
        foreach ($rawTaskName in ([string]$taskValue -split ",")) {
            $taskName = $rawTaskName.Trim()
            if (-not [string]::IsNullOrWhiteSpace($taskName)) {
                $canonicalRequests.Add($taskName)
            }
        }
    }
    if ($ReleaseAcceptanceShard -ne "Full" -and
        ($canonicalRequests.Count -ne 1 -or
            -not $canonicalRequests[0].Equals(
                "ReleaseAcceptance",
                [System.StringComparison]::OrdinalIgnoreCase
            ))) {
        throw "-ReleaseAcceptanceShard may only select one ReleaseAcceptance task."
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
                foreach ($releaseTask in @(Get-ClearraReleaseAcceptanceTasks $ReleaseAcceptanceShard)) {
                    $expanded.Add($releaseTask)
                }
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

