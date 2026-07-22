use super::*;

#[test]
fn process_e2e_x3_setup_reports_raw_metrics_without_condition_summary() {
    let setup = clearra()
        .args(["--verbose", "setup", "--queue", "IOTSZJL", "--fixed"])
        .output()
        .expect("clearra-cli process runs");
    assert!(setup.status.success());
    assert!(setup.stderr.is_empty());
    let stdout = String::from_utf8(setup.stdout).expect("setup stdout utf8");

    for marker in [
        "shape_family_id: setup-family-0",
        expected_setup_tiling_variant_count(),
        expected_setup_build_variant_count(),
        expected_setup_covered_pattern_count(),
        expected_setup_coverage_probability(),
        "post_pc_solution_count: 0",
        "score_basis: none",
        "backend_report: attached",
        "raw_coverage_export_path: inline://clearra/setup/raw-coverage/setup-family-0/union",
    ] {
        assert!(stdout.contains(marker), "missing setup raw metric {marker}");
    }
    assert!(!stdout.contains("condition_summary"));
}

#[test]
fn process_e2e_m19_backend_policy_reports_fallback_and_backend_split() {
    let output = clearra()
        .args([
            "pc-scenario",
            "--field",
            "0x00000000000003f0",
            "--queue",
            "I",
            "--max-pieces",
            "1",
            "--exact-pieces",
            "1",
            "--backend",
            "gpu",
            "--gpu-device",
            "0",
            "--allow-backend-fallback",
            "--max-candidates",
            "128",
            "--max-patterns",
            "64",
            "--max-memory-mib",
            "256",
            "--format",
            "json",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    for marker in [
        "\"backend_requested\":\"gpu\"",
        "\"backend_selected\":\"cpu\"",
        "\"backend_fallback_reason\":\"gpu_kernel_unavailable\"",
        "\"gpu_confirmed\":false",
        "\"execution_max_candidates\":128",
        "\"execution_max_patterns\":64",
        "\"execution_max_memory_mib\":256",
    ] {
        assert!(stdout.contains(marker), "missing marker {marker}");
    }
    assert!(stdout.contains(&format!(
        "\"cpu_confirmed\":{}",
        expected_cpu_confirmed_json()
    )));
    assert!(stdout.contains(&format!(
        "\"candidate_backend\":\"{}\"",
        expected_candidate_backend()
    )));
    assert!(stdout.contains(&format!(
        "\"buildup_backend\":\"{}\"",
        expected_buildup_backend()
    )));
    assert!(stdout.contains(&format!(
        "\"native_c_core_executed\":{}",
        expected_native_c_core_executed_json()
    )));
}
