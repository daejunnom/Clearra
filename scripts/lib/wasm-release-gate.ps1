function Invoke-WasmReleaseCommand {
    param(
        [string]$FileName,
        [string[]]$Arguments,
        [string]$Label
    )

    $result = Invoke-AdversarialCargoProcess -CargoPath $FileName -Arguments $Arguments
    if ($result.ExitCode -ne 0) {
        throw "$Label failed with exit code $($result.ExitCode)`n$($result.Output -join "`n")"
    }
    Write-Output "wasm_stage=$Label status=passed"
}

function Assert-WasmReleaseArtifacts([string]$WasmDirectory) {
    $artifacts = @(
        (Join-Path $WasmDirectory 'clearra_wasm.js'),
        (Join-Path $WasmDirectory 'clearra_wasm_bg.wasm'),
        (Join-Path $WasmDirectory 'clearra_wasm.manifest.json')
    )
    foreach ($artifact in $artifacts) {
        if (-not (Test-Path -LiteralPath $artifact -PathType Leaf) -or
            (Get-Item -LiteralPath $artifact).Length -le 0) {
            throw "Bound WASM release artifact is missing or empty: $artifact"
        }
    }
}

function Invoke-WasmProductArtifactBuild {
    param(
        [string]$Root,
        [string]$CargoPath,
        [string]$NodePath,
        [string]$Destination
    )

    Invoke-WasmReleaseCommand $CargoPath @(
        'test', '--locked', '-p', 'clearra-wasm',
        '--test', 'terminal_supply_public_contract',
        '--', '--test-threads=1'
    ) 'clearra-wasm terminal-supply public contract'
    Invoke-WasmReleaseCommand $NodePath @(
        '--test',
        (Join-Path $Root 'scripts/tools/wasm-product-terminal-contract.test.mjs')
    ) 'clearra-wasm product terminal contract'
    Invoke-WasmReleaseCommand $NodePath @(
        (Join-Path $Root 'scripts/tools/build-clearra-wasm.mjs'),
        '--verify',
        '--destination', $Destination
    ) 'clearra-wasm verified product build'
    Assert-WasmReleaseArtifacts $Destination
}

function Invoke-WasmBuildProducerGate {
    param(
        [string]$Root,
        [string]$CargoPath,
        [string]$CargoTargetDir
    )

    $outputDirectory = $env:CLEARRA_ACCEPTED_WASM_OUTPUT_DIR
    if ([string]::IsNullOrWhiteSpace($outputDirectory)) {
        throw 'WASM producer requires CLEARRA_ACCEPTED_WASM_OUTPUT_DIR'
    }
    foreach ($entry in @(
            @{ Name = 'CLEARRA_SOURCE_COMMIT'; Value = $env:CLEARRA_SOURCE_COMMIT },
            @{ Name = 'CLEARRA_ACCEPTED_RUN_ID'; Value = $env:CLEARRA_ACCEPTED_RUN_ID },
            @{ Name = 'CLEARRA_ACCEPTED_RUN_ATTEMPT'; Value = $env:CLEARRA_ACCEPTED_RUN_ATTEMPT }
        )) {
        if ([string]::IsNullOrWhiteSpace($entry.Value)) {
            throw "WASM producer requires $($entry.Name)"
        }
    }

    $nodeCommand = Get-Command 'node' -ErrorAction SilentlyContinue
    if ($null -eq $nodeCommand) {
        throw 'WASM producer requires node on PATH'
    }
    New-Item -ItemType Directory -Force -Path $CargoTargetDir | Out-Null
    $outputDirectory = [System.IO.Path]::GetFullPath($outputDirectory)
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
    if (@(Get-ChildItem -LiteralPath $outputDirectory -Force).Count -ne 0) {
        throw "WASM producer output must start empty: $outputDirectory"
    }

    $previousCargoTargetDir = $env:CARGO_TARGET_DIR
    try {
        $env:CARGO_TARGET_DIR = Assert-ClearraCanonicalCargoTargetDir $CargoTargetDir
        Invoke-WasmProductArtifactBuild `
            -Root $Root `
            -CargoPath $CargoPath `
            -NodePath $nodeCommand.Source `
            -Destination $outputDirectory
        Invoke-WasmReleaseCommand $nodeCommand.Source @(
            (Join-Path $Root 'scripts/release/accepted-wasm-build.mjs'),
            '--seal', $outputDirectory,
            '--source-commit', $env:CLEARRA_SOURCE_COMMIT,
            '--run-id', $env:CLEARRA_ACCEPTED_RUN_ID,
            '--run-attempt', $env:CLEARRA_ACCEPTED_RUN_ATTEMPT
        ) 'clearra-wasm accepted producer seal'
        Invoke-WasmReleaseCommand $nodeCommand.Source @(
            (Join-Path $Root 'scripts/release/accepted-wasm-build.mjs'),
            '--verify', $outputDirectory,
            '--expected-source-commit', $env:CLEARRA_SOURCE_COMMIT,
            '--expected-run-id', $env:CLEARRA_ACCEPTED_RUN_ID,
            '--expected-run-attempt', $env:CLEARRA_ACCEPTED_RUN_ATTEMPT
        ) 'clearra-wasm accepted producer verification'
        Write-Output 'wasm_build_producer=passed build_count=1 artifact=sealed source_bound=true run_bound=true'
    }
    finally {
        if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
            Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousCargoTargetDir
        }
    }
}

function Import-AcceptedWasmBuild {
    param(
        [string]$Root,
        [string]$NodePath,
        [string]$SourceDirectory,
        [string]$DestinationDirectory
    )

    foreach ($entry in @(
            @{ Name = 'CLEARRA_SOURCE_COMMIT'; Value = $env:CLEARRA_SOURCE_COMMIT },
            @{ Name = 'CLEARRA_ACCEPTED_RUN_ID'; Value = $env:CLEARRA_ACCEPTED_RUN_ID },
            @{ Name = 'CLEARRA_ACCEPTED_RUN_ATTEMPT'; Value = $env:CLEARRA_ACCEPTED_RUN_ATTEMPT }
        )) {
        if ([string]::IsNullOrWhiteSpace($entry.Value)) {
            throw "Accepted WASM consumer requires $($entry.Name)"
        }
    }
    $source = [System.IO.Path]::GetFullPath($SourceDirectory)
    Invoke-WasmReleaseCommand $NodePath @(
        (Join-Path $Root 'scripts/release/accepted-wasm-build.mjs'),
        '--verify', $source,
        '--expected-source-commit', $env:CLEARRA_SOURCE_COMMIT,
        '--expected-run-id', $env:CLEARRA_ACCEPTED_RUN_ID,
        '--expected-run-attempt', $env:CLEARRA_ACCEPTED_RUN_ATTEMPT
    ) 'clearra-wasm accepted producer input'

    Get-ChildItem -LiteralPath $source -File | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $DestinationDirectory -Force
    }
    Invoke-WasmReleaseCommand $NodePath @(
        (Join-Path $Root 'scripts/release/accepted-wasm-build.mjs'),
        '--verify', $DestinationDirectory,
        '--expected-source-commit', $env:CLEARRA_SOURCE_COMMIT,
        '--expected-run-id', $env:CLEARRA_ACCEPTED_RUN_ID,
        '--expected-run-attempt', $env:CLEARRA_ACCEPTED_RUN_ATTEMPT
    ) 'clearra-wasm accepted staged input'
}

function Invoke-WasmBuildTestGate {
    param(
        [string]$Root,
        [string]$CargoPath,
        [string]$CargoTargetDir
    )

    $previousCargoTargetDir = $env:CARGO_TARGET_DIR
    $previousWebPublicDir = $env:CLEARRA_WEB_PUBLIC_DIR
    $webPublicDir = Join-Path $CargoTargetDir 'clearra-web-public'
    New-Item -ItemType Directory -Force -Path $CargoTargetDir | Out-Null
    try {
        $env:CARGO_TARGET_DIR = Assert-ClearraCanonicalCargoTargetDir $CargoTargetDir
        $nodeCommand = Get-Command 'node' -ErrorAction SilentlyContinue
        if ($null -eq $nodeCommand) {
            throw 'WASM release requires node on PATH'
        }
        if (Test-Path -LiteralPath $webPublicDir) {
            Remove-Item -LiteralPath $webPublicDir -Recurse -Force
        }
        New-Item -ItemType Directory -Force -Path $webPublicDir | Out-Null
        $repositoryStatic = Join-Path $Root 'apps/clearra-web/static'
        if (Test-Path -LiteralPath $repositoryStatic) {
            Get-ChildItem -LiteralPath $repositoryStatic | ForEach-Object {
                Copy-Item -LiteralPath $_.FullName -Destination $webPublicDir -Recurse -Force
            }
        }
        $stagedWasm = Join-Path $webPublicDir 'wasm'
        New-Item -ItemType Directory -Force -Path $stagedWasm | Out-Null
        Get-ChildItem -LiteralPath $stagedWasm -File -ErrorAction SilentlyContinue |
            Remove-Item -Force
        $acceptedWasmDirectory = $env:CLEARRA_ACCEPTED_WASM_DIR
        $acceptedProducer = -not [string]::IsNullOrWhiteSpace($acceptedWasmDirectory)
        if ($acceptedProducer) {
            Import-AcceptedWasmBuild `
                -Root $Root `
                -NodePath $nodeCommand.Source `
                -SourceDirectory $acceptedWasmDirectory `
                -DestinationDirectory $stagedWasm
        } else {
            Invoke-WasmProductArtifactBuild `
                -Root $Root `
                -CargoPath $CargoPath `
                -NodePath $nodeCommand.Source `
                -Destination $stagedWasm
        }
        $wasmBindings = Join-Path $stagedWasm 'clearra_wasm.js'
        $boundWasm = Join-Path $stagedWasm 'clearra_wasm_bg.wasm'
        $wasmManifest = Join-Path $stagedWasm 'clearra_wasm.manifest.json'
        Assert-WasmReleaseArtifacts $stagedWasm
        $expectedSourceCommit = if ([string]::IsNullOrWhiteSpace($env:CLEARRA_SOURCE_COMMIT)) {
            'unverified-local-build'
        } else {
            $env:CLEARRA_SOURCE_COMMIT
        }
        Invoke-WasmReleaseCommand $nodeCommand.Source @(
            (Join-Path $Root 'scripts/tools/wasm-pc-environment-probe.mjs'),
            $wasmBindings,
            'clearra pc --board-mask 0x000000e0f87e3f87 --height 4 --pieces 4 --hold I --count all --max-patterns 840 --max-candidates 250000 --backend cpu',
            '8192',
            'summary',
            '--manifest',
            $wasmManifest,
            '--expected-source-commit',
            $expectedSourceCommit
        ) 'clearra-wasm exact worker probe'
        $env:CLEARRA_WEB_PUBLIC_DIR = $webPublicDir

        $npmName = if ($env:OS -eq 'Windows_NT') { 'npm.cmd' } else { 'npm' }
        $npmCommand = Get-Command $npmName -ErrorAction SilentlyContinue
        if ($null -eq $npmCommand) {
            throw 'WASM release requires npm on PATH'
        }
        Invoke-WasmReleaseCommand $npmCommand.Source @(
            'test', '--workspace', '@clearra/ui'
        ) 'clearra-ui runtime contracts'
        Invoke-WasmReleaseCommand $npmCommand.Source @(
            'test', '--workspace', '@clearra/web'
        ) 'clearra-web worker contracts'
        Invoke-WasmReleaseCommand $npmCommand.Source @(
            'exec', '--workspace', '@clearra/web', '--', 'vite', 'build'
        ) 'clearra-web frontend build'
        Invoke-WasmReleaseCommand $nodeCommand.Source @(
            (Join-Path $Root 'apps/clearra-web/scripts/prepare-pages-fallback.mjs')
        ) 'clearra-web Pages fallback'
        $buildSource = if ($acceptedProducer) { 'accepted-producer' } else { 'inline' }
        Write-Output "wasm_build_test=passed host_tests=executed runtime_tests=executed wasm32=compiled bindgen_runtime=staged frontend=built build_source=$buildSource"
    }
    finally {
        if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
            Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousCargoTargetDir
        }
        if ([string]::IsNullOrWhiteSpace($previousWebPublicDir)) {
            Remove-Item Env:\CLEARRA_WEB_PUBLIC_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CLEARRA_WEB_PUBLIC_DIR = $previousWebPublicDir
        }
    }
}
