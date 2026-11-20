use super::*;

const EXPLICIT_GPU_SCENARIO: &[&str] = &[
    "pc-scenario",
    "--field",
    "0x00000000000003f0",
    "--queue",
    "I",
    "--max-pieces",
    "1",
    "--exact-pieces",
    "1",
    "--workers",
    "1",
    "--backend",
    "gpu",
    "--gpu-device",
    "0",
    "--max-candidates",
    "128",
    "--max-patterns",
    "64",
    "--max-memory-mib",
    "256",
];

#[test]
fn process_e2e_m19_backend_policy_reports_fallback_and_backend_split() {
    // This executable has no compiled WebGPU kernel. Hardware inventory
    // cannot turn this fallback-policy probe into a GPU availability test.
    let mut args = EXPLICIT_GPU_SCENARIO.to_vec();
    args.push("--allow-backend-fallback");
    let result = run_json(&args);
    assert_eq!(result["kind"], "pc-scenario");
    let summary = &result["summary"];
    assert_rust_cpu(summary);
    assert_eq!(summary["backend_requested"], "gpu");
    assert_eq!(summary["backend_selected"], "wasm-cpu");
    assert_eq!(summary["backend_fallback_used"], true);
    assert_eq!(summary["fallback_backend"], "wasm-cpu");
    assert_eq!(summary["backend_fallback_reason"], "gpu_kernel_unavailable");
    assert_eq!(summary["gpu_trust_state"], "fallback-used");
    assert_eq!(summary["solution_found"], true);
    assert_eq!(summary["count_complete"], true);

    let denied = clearra()
        .args(EXPLICIT_GPU_SCENARIO)
        .arg("--no-backend-fallback")
        .output()
        .expect("launch the product CLI without fallback");
    assert_eq!(denied.status.code(), Some(3));
    assert!(denied.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&denied.stderr);
    assert!(stderr.contains("E_BACKEND_GPU_UNAVAILABLE"), "{stderr}");
}
