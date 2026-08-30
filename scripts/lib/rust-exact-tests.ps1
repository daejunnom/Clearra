function Invoke-RustExactTestsGate {
    param(
        [string]$Root,
        [string]$CargoPath,
        [string]$CargoTargetDir,
        [int]$Workers
    )

    $packages = @(
        'clearra-core-domain',
        'clearra-core-ffi',
        'clearra-core-executor',
        'clearra-coverage',
        'clearra-objectives',
        'clearra-scoring',
        'clearra-postprocess',
        'clearra-webgpu',
        'clearra-app'
    )
    $arguments = New-Object System.Collections.Generic.List[string]
    $arguments.Add('test')
    foreach ($package in $packages) {
        $arguments.Add('--package')
        $arguments.Add($package)
    }
    $arguments.Add('--lib')
    $arguments.Add('--features')
    $arguments.Add('clearra-core-ffi/native-c-core,clearra-core-executor/native-c-core,clearra-core-executor/webgpu-search,clearra-app/native-c-core,clearra-app/webgpu-search')
    $arguments.Add('--')
    $arguments.Add('--test-threads=1')

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
        $result = Invoke-AdversarialCargoProcessOnce `
            -CargoPath $CargoPath `
            -Arguments @($arguments.ToArray())
        $result.Output | Write-Output
        if ($result.ExitCode -ne 0) {
            throw "Rust exact tests failed with exit code $($result.ExitCode)"
        }
        $output = $result.Output -join "`n"
        $summaries = [regex]::Matches($output, 'test result: ok\. (?<passed>[0-9]+) passed; 0 failed;')
        $passed = 0
        foreach ($summary in $summaries) {
            $passed += [int]$summary.Groups['passed'].Value
        }
        if ($passed -lt 1) {
            throw 'Rust exact test stage executed zero tests'
        }
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
        Write-Output "rust_exact_tests=passed tests=$passed packages=$($packages.Count)"
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
