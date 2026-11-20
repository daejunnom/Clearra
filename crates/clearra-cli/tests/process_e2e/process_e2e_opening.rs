use super::*;

#[test]
fn process_e2e_pc_command_writes_stdout_and_zero_exit() {
    let output = clearra()
        .args([
            "pc",
            "--lines",
            "2",
            "--queue",
            "IJLOO",
            "--fixed",
            "--no-hold",
            "--backend",
            "cpu",
            "--workers",
            "1",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(!stdout.trim().is_empty());
}

#[test]
fn process_e2e_accepts_global_format_before_command() {
    let result = run_json(&[
        "pc",
        "--lines",
        "2",
        "--queue",
        "IJLOO",
        "--fixed",
        "--no-hold",
        "--backend",
        "cpu",
        "--workers",
        "1",
    ]);
    assert_eq!(result["schema_version"], 2);
    assert_eq!(result["kind"], "pc");
    assert_rust_cpu(&result["summary"]);
}

#[test]
fn process_e2e_opening_pc_json_accepts_duplicate_fixed_sequence() {
    let output = clearra()
        .args([
            "pc",
            "--lines",
            "2",
            "--queue",
            "IIOOO",
            "--fixed",
            "--no-hold",
            "--backend",
            "cpu",
            "--workers",
            "1",
            "--format",
            "json",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let result: Value = serde_json::from_slice(&output.stdout).expect("opening PC JSON");
    assert_eq!(result["schema_version"], 2);
    assert_eq!(result["kind"], "pc");
    assert_eq!(result["contract"]["command"]["kind"], "pc");
    let summary = &result["summary"];
    assert_rust_cpu(summary);
    assert_eq!(summary["count_complete"], true);
    assert_eq!(summary["normalized_unique_solution_count"], 4);
    let report = &result["contract"]["pc"]["execution_report"];
    assert_eq!(
        report["buildup"]["normalized_solution_set_hash"],
        "cts1:8dc81db9bcd4bab9"
    );
    assert_eq!(report["coverage"]["coverage_pattern_count"], "1");
    assert_eq!(report["coverage"]["covered_pattern_count"], 1);
}
