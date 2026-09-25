function Get-RustExactPackageSpecs {
    return @(
        [pscustomobject]@{
            Package = 'clearra-app'
            Target = 'clearra_app'
            SerialWholePackage = $true
        },
        [pscustomobject]@{
            Package = 'clearra-core-executor'
            Target = 'clearra_core_executor'
            SerialWholePackage = $true
        },
        [pscustomobject]@{
            Package = 'clearra-core-ffi'
            Target = 'clearra_core_ffi'
            SerialWholePackage = $true
        },
        [pscustomobject]@{
            Package = 'clearra-webgpu'
            Target = 'clearra_webgpu'
            SerialWholePackage = $true
        },
        [pscustomobject]@{
            Package = 'clearra-core-domain'
            Target = 'clearra_core_domain'
            SerialWholePackage = $false
        },
        [pscustomobject]@{
            Package = 'clearra-coverage'
            Target = 'clearra_coverage'
            SerialWholePackage = $false
        },
        [pscustomobject]@{
            Package = 'clearra-objectives'
            Target = 'clearra_objectives'
            SerialWholePackage = $false
        },
        [pscustomobject]@{
            Package = 'clearra-scoring'
            Target = 'clearra_scoring'
            SerialWholePackage = $false
        },
        [pscustomobject]@{
            Package = 'clearra-postprocess'
            Target = 'clearra_postprocess'
            SerialWholePackage = $false
        }
    )
}

function New-RustExactCompileArguments {
    param([object[]]$PackageSpecs)

    $arguments = New-Object System.Collections.Generic.List[string]
    $arguments.Add('test')
    $arguments.Add('--no-run')
    # Compilation failures from every selected package remain visible in one run.
    $arguments.Add('--no-fail-fast')
    $arguments.Add('--message-format=json-render-diagnostics')
    foreach ($spec in $PackageSpecs) {
        $arguments.Add('--package')
        $arguments.Add($spec.Package)
    }
    $arguments.Add('--lib')
    $arguments.Add('--features')
    $arguments.Add('clearra-core-ffi/native-c-core,clearra-core-executor/native-c-core,clearra-core-executor/webgpu-search,clearra-app/native-c-core,clearra-app/webgpu-search')
    return @($arguments.ToArray())
}

function ConvertFrom-RustExactCompileOutput {
    param(
        [string[]]$Output,
        [object[]]$PackageSpecs
    )

    $expectedTargets = @{}
    foreach ($spec in $PackageSpecs) {
        $expectedTargets[$spec.Target] = $spec.Package
    }
    $executables = @{}
    $diagnostics = New-Object System.Collections.Generic.List[string]
    foreach ($chunk in $Output) {
        foreach ($line in ([regex]::Split([string]$chunk, "`r?`n"))) {
            if ([string]::IsNullOrWhiteSpace($line)) {
                continue
            }
            try {
                $message = $line | ConvertFrom-Json -ErrorAction Stop
            } catch {
                $diagnostics.Add($line)
                continue
            }
            if ($message.reason -eq 'compiler-message' -and
                -not [string]::IsNullOrWhiteSpace([string]$message.message.rendered)) {
                $diagnostics.Add(([string]$message.message.rendered).TrimEnd())
                continue
            }
            if ($message.reason -ne 'compiler-artifact' -or
                $null -eq $message.executable -or
                -not [bool]$message.profile.test -or
                @($message.target.kind) -notcontains 'lib') {
                continue
            }
            $target = [string]$message.target.name
            if (-not $expectedTargets.ContainsKey($target)) {
                continue
            }
            $package = $expectedTargets[$target]
            $executable = [string]$message.executable
            if ($executables.ContainsKey($package) -and
                $executables[$package] -ne $executable) {
                throw "Rust exact compile emitted multiple test harnesses for package '$package'."
            }
            $executables[$package] = $executable
        }
    }
    foreach ($spec in $PackageSpecs) {
        if (-not $executables.ContainsKey($spec.Package) -or
            -not (Test-Path -LiteralPath $executables[$spec.Package] -PathType Leaf)) {
            throw "Rust exact compile did not emit the library test harness for package '$($spec.Package)'."
        }
    }
    return [pscustomobject]@{
        Executables = $executables
        Diagnostics = @($diagnostics.ToArray())
    }
}

function Get-RustExactHarnessInventory {
    param(
        [string]$Package,
        [string]$Executable
    )

    $result = Invoke-AdversarialCargoProcessOnce `
        -CargoPath $Executable `
        -Arguments @('--list', '--format=terse')
    if ($result.ExitCode -ne 0) {
        throw "Rust exact test inventory failed for package '$Package' with exit code $($result.ExitCode)."
    }
    $output = $result.Output -join "`n"
    $names = @([regex]::Matches($output, '(?m)^(?<name>.+): test\s*$') | ForEach-Object {
        $_.Groups['name'].Value.Trim()
    })
    if ($names.Count -lt 1) {
        throw "Rust exact test inventory is empty for package '$Package'."
    }
    if (@($names | Sort-Object -Unique).Count -ne $names.Count) {
        throw "Rust exact test inventory contains duplicate names for package '$Package'."
    }
    return @($names)
}

function New-RustExactHarnessArguments {
    param(
        [int]$TestThreads,
        [AllowEmptyString()][string]$Filter = '',
        [string[]]$SkipFilters = @()
    )

    $arguments = New-Object System.Collections.Generic.List[string]
    $arguments.Add("--test-threads=$TestThreads")
    foreach ($skipFilter in $SkipFilters) {
        $arguments.Add('--skip')
        $arguments.Add($skipFilter)
    }
    if (-not [string]::IsNullOrWhiteSpace($Filter)) {
        $arguments.Add($Filter)
    }
    return @($arguments.ToArray())
}

function ConvertFrom-RustExactHarnessProcessResult {
    param(
        [string]$Package,
        [string]$Partition,
        [int]$TestThreads,
        [int]$ExpectedTests,
        [object]$Result,
        [long]$ElapsedMilliseconds
    )

    $output = $Result.Output -join "`n"
    $failure = $null
    $passed = 0
    $observed = 0
    if ($Result.ExitCode -ne 0) {
        $failure = "exit-code-$($Result.ExitCode)"
    } else {
        $summary = [regex]::Match(
            $output,
            'test result: ok\. (?<passed>[0-9]+) passed; 0 failed; (?<ignored>[0-9]+) ignored;'
        )
        if (-not $summary.Success) {
            $failure = 'missing-success-summary'
        } else {
            $passed = [int]$summary.Groups['passed'].Value
            $observed = $passed + [int]$summary.Groups['ignored'].Value
            if ($observed -ne $ExpectedTests) {
                $failure = "partition-count-mismatch-expected-$ExpectedTests-observed-$observed"
            }
        }
    }
    return [pscustomobject]@{
        Package = $Package
        Partition = $Partition
        TestThreads = $TestThreads
        ExpectedTests = $ExpectedTests
        Passed = $passed
        Observed = $observed
        ElapsedMilliseconds = $ElapsedMilliseconds
        Output = @($Result.Output)
        Failure = $failure
    }
}

function Invoke-RustExactHarnessPartition {
    param(
        [string]$Package,
        [string]$Executable,
        [string]$Partition,
        [int]$TestThreads,
        [int]$ExpectedTests,
        [AllowEmptyString()][string]$Filter = '',
        [string[]]$SkipFilters = @()
    )

    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    $result = Invoke-AdversarialCargoProcessOnce `
        -CargoPath $Executable `
        -Arguments @(New-RustExactHarnessArguments `
            -TestThreads $TestThreads `
            -Filter $Filter `
            -SkipFilters $SkipFilters)
    $timer.Stop()
    return ConvertFrom-RustExactHarnessProcessResult `
        -Package $Package `
        -Partition $Partition `
        -TestThreads $TestThreads `
        -ExpectedTests $ExpectedTests `
        -Result $result `
        -ElapsedMilliseconds $timer.ElapsedMilliseconds
}

function Start-RustExactHarnessPartitionProcess {
    param([object]$Request)

    $arguments = @(New-RustExactHarnessArguments `
        -TestThreads $Request.TestThreads `
        -Filter $Request.Filter `
        -SkipFilters @($Request.SkipFilters))
    $quotedArguments = $arguments | ForEach-Object {
        '"' + $_.Replace('"', '\"') + '"'
    }
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Request.Executable
    $startInfo.Arguments = $quotedArguments -join ' '
    $startInfo.WorkingDirectory = (Get-Location).Path
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    [void]$process.Start()
    return [pscustomobject]@{
        Request = $Request
        Process = $process
        StandardOutput = $process.StandardOutput.ReadToEndAsync()
        StandardError = $process.StandardError.ReadToEndAsync()
        Timer = [System.Diagnostics.Stopwatch]::StartNew()
    }
}

function Complete-RustExactHarnessPartitionProcess {
    param([object]$Running)

    try {
        $Running.Process.WaitForExit()
        $Running.Timer.Stop()
        $standardOutput = $Running.StandardOutput.GetAwaiter().GetResult()
        $standardError = $Running.StandardError.GetAwaiter().GetResult()
        $result = [pscustomobject]@{
            ExitCode = $Running.Process.ExitCode
            Output = @($standardError, $standardOutput) |
                Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
        }
        return ConvertFrom-RustExactHarnessProcessResult `
            -Package $Running.Request.Package `
            -Partition $Running.Request.Partition `
            -TestThreads $Running.Request.TestThreads `
            -ExpectedTests $Running.Request.ExpectedTests `
            -Result $result `
            -ElapsedMilliseconds $Running.Timer.ElapsedMilliseconds
    } finally {
        $Running.Process.Dispose()
    }
}

function Invoke-RustExactHarnessPartitionPool {
    param(
        [object[]]$Requests,
        [int]$MaxConcurrency
    )

    $results = New-Object System.Collections.Generic.List[object]
    for ($offset = 0; $offset -lt $Requests.Count; $offset += $MaxConcurrency) {
        $last = [Math]::Min($Requests.Count - 1, $offset + $MaxConcurrency - 1)
        $running = New-Object System.Collections.Generic.List[object]
        try {
            foreach ($request in @($Requests[$offset..$last])) {
                $running.Add((Start-RustExactHarnessPartitionProcess -Request $request))
            }
        } catch {
            foreach ($started in $running) {
                try {
                    if (-not $started.Process.HasExited) {
                        $started.Process.Kill()
                    }
                    $started.Process.WaitForExit()
                } catch {
                } finally {
                    $started.Process.Dispose()
                }
            }
            throw
        }
        foreach ($process in $running) {
            $results.Add((Complete-RustExactHarnessPartitionProcess -Running $process))
        }
    }
    return @($results.ToArray())
}

function Invoke-RustExactTestsGate {
    param(
        [string]$Root,
        [string]$CargoPath,
        [string]$CargoTargetDir,
        [int]$Workers
    )

    $packageSpecs = @(Get-RustExactPackageSpecs)
    $packages = @($packageSpecs.Package)
    $buildDir = Get-StartTestsPersistentBuildDir 'core-c-library-cache'
    $coreBuild = Invoke-CoreCBuild `
        -BuildDir $buildDir `
        -Configuration 'Debug' `
        -ConfigureArgs (Get-StartTestsCMakeConfigureArgs @(
            '-DBUILD_TESTING=OFF',
            '-DCLEARRA_BUILD_TEST_ORACLE=ON'
        )) `
        -BuildWorkers ([Math]::Max(1, $Workers))
    if ($coreBuild.Status -ne 'Passed') {
        throw "Rust exact tests could not build native C core: $($coreBuild.Reason)"
    }
    $libDir = Find-CoreCLibraryDir $buildDir
    if ([string]::IsNullOrWhiteSpace($libDir)) {
        throw "Rust exact tests could not find clearra_core under $buildDir"
    }

    $previousCargoTargetDir = $env:CARGO_TARGET_DIR
    $previousWindowsRustFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
    New-Item -ItemType Directory -Force -Path $CargoTargetDir | Out-Null
    try {
        $env:CARGO_TARGET_DIR = Assert-ClearraCanonicalCargoTargetDir $CargoTargetDir
        Sync-ClearraNativeCargoLinkState `
            -LibraryDirectory $libDir `
            -CargoTargetDirectory $env:CARGO_TARGET_DIR `
            -CargoPath $CargoPath `
            -WorkspaceRoot $Root
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS =
            Add-ClearraWindowsNativeRustLinkFlags $previousWindowsRustFlags $libDir

        # Compile every harness together so the first serial partition does not
        # turn package ordering into a new compilation critical path.
        $compileTimer = [System.Diagnostics.Stopwatch]::StartNew()
        $compileResult = Invoke-AdversarialCargoProcessOnce `
            -CargoPath $CargoPath `
            -Arguments @(New-RustExactCompileArguments $packageSpecs)
        $compileTimer.Stop()
        Write-Output "rust_exact_phase=compile exit_code=$($compileResult.ExitCode) elapsed_ms=$($compileTimer.ElapsedMilliseconds) packages=$($packages.Count)"
        if ($compileResult.ExitCode -ne 0) {
            $compileResult.Output | Write-Output
            throw "Rust exact test compilation failed with exit code $($compileResult.ExitCode)"
        }
        $compiled = ConvertFrom-RustExactCompileOutput `
            -Output @($compileResult.Output) `
            -PackageSpecs $packageSpecs
        $compiled.Diagnostics | Write-Output

        $inventories = @{}
        foreach ($spec in $packageSpecs) {
            $inventories[$spec.Package] = @(
                Get-RustExactHarnessInventory `
                    -Package $spec.Package `
                    -Executable $compiled.Executables[$spec.Package]
            )
        }

        $failures = New-Object System.Collections.Generic.List[string]
        $allOutput = New-Object System.Collections.Generic.List[string]
        $passed = 0
        $observedTests = 0
        $expectedInventoryTests = @($packageSpecs | ForEach-Object {
            @($inventories[$_.Package]).Count
        } | Measure-Object -Sum).Sum
        $parallelTestThreads = [Math]::Min(2, [Math]::Max(1, $Workers))

        # App and executor tests can reach the same process-global lease through
        # product code even when a test body has no explicit guard. Keep each
        # affected harness internally serial, but run independent processes in
        # a two-wide pool. This removes in-process lease convoys without turning
        # package order into a new tail.
        $resourceRequests = @($packageSpecs | Where-Object SerialWholePackage | ForEach-Object {
            [pscustomobject]@{
                Package = $_.Package
                Executable = $compiled.Executables[$_.Package]
                Partition = 'global-resource-isolated'
                TestThreads = 1
                ExpectedTests = @($inventories[$_.Package]).Count
                Filter = ''
                SkipFilters = @()
            }
        })
        $resourceRuns = @(Invoke-RustExactHarnessPartitionPool `
            -Requests $resourceRequests `
            -MaxConcurrency $parallelTestThreads)
        foreach ($run in $resourceRuns) {
            $run.Output | ForEach-Object { $allOutput.Add([string]$_) }
            Write-Output "rust_exact_phase=global-resource package=$($run.Package) tests=$($run.ExpectedTests) test_threads=1 process_pool=$parallelTestThreads elapsed_ms=$($run.ElapsedMilliseconds)"
            $passed += $run.Passed
            $observedTests += $run.Observed
            if ($null -ne $run.Failure) {
                $failures.Add("$($run.Package):global-resource-isolated:$($run.Failure)")
            }
        }

        # All remaining packages have no product path to the process-global
        # lease. Each harness owns one small pool; harnesses remain ordered so
        # independent packages do not oversubscribe the Windows runner.
        foreach ($spec in $packageSpecs) {
            if ($spec.SerialWholePackage) {
                continue
            }
            $inventory = @($inventories[$spec.Package])
            if ($inventory.Count -lt 1) {
                throw "Rust exact parallel-safe partition is empty for package '$($spec.Package)'."
            }
            $run = Invoke-RustExactHarnessPartition `
                -Package $spec.Package `
                -Executable $compiled.Executables[$spec.Package] `
                -Partition 'parallel-safe' `
                -TestThreads $parallelTestThreads `
                -ExpectedTests $inventory.Count
            $run.Output | ForEach-Object { $allOutput.Add([string]$_) }
            Write-Output "rust_exact_phase=parallel-safe package=$($spec.Package) tests=$($inventory.Count) test_threads=$parallelTestThreads elapsed_ms=$($run.ElapsedMilliseconds)"
            $passed += $run.Passed
            $observedTests += $run.Observed
            if ($null -ne $run.Failure) {
                $failures.Add("$($spec.Package):parallel-safe:$($run.Failure)")
            }
        }

        $allOutput | Write-Output
        if ($failures.Count -gt 0) {
            throw "Rust exact tests failed after collecting every partition: $($failures -join ', ')"
        }
        if ($passed -lt 1) {
            throw 'Rust exact test stage executed zero tests'
        }
        if ($observedTests -ne $expectedInventoryTests) {
            throw "Rust exact partition union differs from the compiled inventory: expected=$expectedInventoryTests observed=$observedTests"
        }
        $output = $allOutput -join "`n"
        Assert-AdversarialRustCasesInOutput `
            -Output $output `
            -RequiredCases @(Get-AdversarialRustCases) `
            -Owner 'RustExactTests'
        $completeRequiredCase = 'pruning::pruning_proof_ledger::tests::complete_required_capacity_keeps_candidate'
        if ($output -notmatch ('(?m)^test ' + [regex]::Escape($completeRequiredCase) + ' \.\.\. ok\s*$')) {
            throw 'RustExactTests did not execute the delegated NoProductDebt complete-required case'
        }
        Write-Output 'adversarial_rust_tests=executed owner=RustExactTests'
        Write-Output 'no_product_debt_evidence=complete_required_keeps_candidate status=passed source=rust-test owner=RustExactTests'
        Write-Output "rust_exact_tests=passed tests=$passed inventory=$observedTests packages=$($packages.Count) parallel_safe_threads=$parallelTestThreads"
    }
    finally {
        if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
            Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousCargoTargetDir
        }
        if ([string]::IsNullOrWhiteSpace($previousWindowsRustFlags)) {
            Remove-Item Env:\CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousWindowsRustFlags
        }
    }
}
