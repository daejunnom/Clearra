use super::*;

const EXPLICIT_GPU_SCENARIO: &[&str] = &[
    "pc-scenario",
    "--field",
    "0x00000000000003f0",
    // Six occupied cells and one I complete one row, not the default two.
    "--visible-height",
    "1",
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
fn process_e2e_retired_gpu_has_no_cpu_fallback() {
    for fallback in ["--allow-backend-fallback", "--no-backend-fallback"] {
        let denied = clearra()
            .args(EXPLICIT_GPU_SCENARIO)
            .arg(fallback)
            .output()
            .expect("launch the product CLI");
        assert_eq!(denied.status.code(), Some(3));
        assert!(denied.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&denied.stderr);
        assert!(stderr.contains("E_BACKEND_GPU_UNAVAILABLE"), "{stderr}");
        assert!(stderr.contains("webgpu_backend_retired"), "{stderr}");
    }
}
