function Get-ClearraMsvcAsanRuntimeDirectory {
    $roots = @(
        (Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio'),
        (Join-Path $env:ProgramFiles 'Microsoft Visual Studio')
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) -and (Test-Path -LiteralPath $_) }
    foreach ($root in $roots) {
        $runtime = Get-ChildItem -LiteralPath $root -Recurse `
            -Filter 'clang_rt.asan_dynamic-x86_64.dll' -File -ErrorAction SilentlyContinue |
            Where-Object { $_.DirectoryName -match 'Hostx64[\\/]x64$' } |
            Sort-Object -Property FullName -Descending |
            Select-Object -First 1
        if ($null -ne $runtime) {
            return $runtime.DirectoryName
        }
    }
    return $null
}

function Invoke-ClearraCSanitizerGate {
    param([string]$Root)

    $previousPath = $env:PATH
    try {
        if ($env:OS -eq 'Windows_NT') {
            $asanRuntime = Get-ClearraMsvcAsanRuntimeDirectory
            if ([string]::IsNullOrWhiteSpace($asanRuntime)) {
                throw 'MSVC ASan runtime DLL was not found; release sanitizer execution is unavailable'
            }
            $env:PATH = "$asanRuntime;$previousPath"
            Write-Output "c_sanitizer_runtime=$asanRuntime"
        }

        Invoke-CoreCTestStartMode `
            -Root $Root `
            -ModeName 'ReleaseAsan' `
            -ConfigureArgs (Get-StartTestsCMakeConfigureArgs @('-DCLEARRA_CORE_ENABLE_ASAN=ON')) `
            -PersistentBuildName 'core-c-asan-cache'
        Write-Output 'c_sanitizer_asan=passed'
    }
    finally {
        $env:PATH = $previousPath
    }

    if ($env:OS -eq 'Windows_NT') {
        Write-Output 'c_sanitizer_ubsan=unavailable reason=msvc_ubsan_unavailable'
        return
    }

    Invoke-CoreCTestStartMode `
        -Root $Root `
        -ModeName 'ReleaseUbsan' `
        -ConfigureArgs (Get-StartTestsCMakeConfigureArgs @('-DCLEARRA_CORE_ENABLE_UBSAN=ON')) `
        -PersistentBuildName 'core-c-ubsan-cache'
    Write-Output 'c_sanitizer_ubsan=passed'
}
