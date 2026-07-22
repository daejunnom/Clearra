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
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(stdout.contains("kind: pc"));
    assert!(stdout.contains("lines: 2"));
    assert!(stdout.contains("queue_len: 5"));
}

#[test]
fn process_e2e_accepts_global_format_before_command() {
    let output = clearra()
        .args([
            "--format",
            "json",
            "pc",
            "--lines",
            "2",
            "--queue",
            "IJLOO",
            "--fixed",
            "--no-hold",
        ])
        .output()
        .expect("clearra process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(stdout.contains("\"schema_version\":2"));
    assert!(stdout.contains("\"kind\":\"pc\""));
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
            "--format",
            "json",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(stdout.contains("\"schema_version\":2"));
    assert!(stdout.contains("\"kind\":\"pc\""));
    assert!(stdout.contains("\"summary\":{"));
    assert!(stdout.contains("\"contract\":{\"command\""));
    assert!(stdout.contains("\"queue_mode\":\"fixed\""));
    assert!(stdout.contains("\"queue_len\":5"));
}
