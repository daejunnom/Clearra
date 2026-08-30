function Invoke-RenderGoldenGate {
    param(
        [string]$CargoPath,
        [string]$CargoTargetDir
    )

    $previousCargoTargetDir = $env:CARGO_TARGET_DIR
    New-Item -ItemType Directory -Force -Path $CargoTargetDir | Out-Null
    try {
        $env:CARGO_TARGET_DIR = Assert-ClearraCanonicalCargoTargetDir $CargoTargetDir
        $result = Invoke-AdversarialCargoProcessOnce `
            -CargoPath $CargoPath `
            -Arguments @('test', '--package', 'clearra-render', '--', '--test-threads=1')
        $result.Output | Write-Output
        if ($result.ExitCode -ne 0) {
            throw "render golden gate failed with exit code $($result.ExitCode)"
        }
        $output = $result.Output -join "`n"
        foreach ($required in @(
            'png_board_render_golden',
            'png_lock_frame_render_golden',
            'gif_timeline_render_golden'
        )) {
            if ($output -notmatch ('(?m)^test .*' + [regex]::Escape($required) + ' \.\.\. ok\s*$')) {
                throw "render golden gate did not execute '$required'"
            }
        }
        Write-Output 'no_product_debt_evidence=renderer_png_artifact status=passed source=rust-test owner=RenderGolden'
        Write-Output 'no_product_debt_evidence=renderer_gif_artifact status=passed source=rust-test owner=RenderGolden'
        Write-Output 'render_golden=passed capability=connected-exact artifacts=png,gif'
    }
    finally {
        if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
            Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousCargoTargetDir
        }
    }
}
