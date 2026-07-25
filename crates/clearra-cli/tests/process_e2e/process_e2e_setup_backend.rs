use super::*;

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
