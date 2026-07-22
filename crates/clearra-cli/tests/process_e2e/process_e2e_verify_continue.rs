use super::*;

#[test]
fn process_e2e_default_verify_runs_kick_contracts_before_reporting_ok() {
    let output = clearra()
        .arg("verify")
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(stdout.contains("kind: verify"));
    assert!(stdout.contains("pc: ok"));
    assert!(stdout.contains("setup: ok"));
    assert!(stdout.contains("build_coverage: ok"));
    assert!(stdout.contains("kicks: ok"));
}

#[test]
fn process_e2e_unknown_command_writes_stderr_and_validation_exit() {
    let output = clearra()
        .arg("wat")
        .output()
        .expect("clearra-cli process runs");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(stderr.contains("E_CLI_COMMAND_UNKNOWN"));
    assert!(stderr.contains("unknown command 'wat'"));
}

#[test]
fn process_e2e_inspect_reports_reserved_unsupported_command() {
    let output = clearra()
        .arg("inspect")
        .output()
        .expect("clearra process runs");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(stderr.contains("E_CLI_COMMAND_UNSUPPORTED"));
    assert!(stderr.contains("inspect is unsupported"));
}

#[test]
fn process_e2e_continue_command_accepts_token_without_prompting() {
    let output = clearra()
        .args([
            "continue",
            "pc2:l2:bdstandard-10:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:oall:e0:hnone:qIIOOO",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(stdout.contains("kind: continue"));
    assert!(stdout.contains("status: continued-searched"));
    assert!(stdout.contains("interactive_prompt: false"));
    assert!(stdout.contains("rule_profile: srs-plus"));
    assert!(stdout.contains("objective: all"));
}

#[test]
fn process_e2e_continue_command_accepts_scenario_token_without_prompting() {
    let output = clearra()
        .args([
            "--verbose",
            "continue",
            "sc2:w10:v2:m0x00000000000003f0:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:hnone:qI:p1:x1:n0:a1:z0:gclear-to-empty:ccount-all:t1",
        ])
        .output()
        .expect("clearra-cli process runs");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(stdout.contains("kind: continue"));
    assert!(stdout.contains("status: scenario-continued-searched"));
    assert!(stdout.contains("interactive_prompt: false"));
    assert!(stdout.contains("exact_pieces: 1"));
    assert!(stdout.contains("retained_trace_limit: 1"));
}
