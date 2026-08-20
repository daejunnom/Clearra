use crate::error::CliErrorCode;
use crate::exit::ExitCode;

use super::*;

#[test]
fn verify_command_runs_validation_before_native_execution() {
    let output = VerifyCommand::run(&VerifyArgs::new(Some("pc".to_owned())), RenderFormat::Text);

    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    assert!(output.stdout().contains("kind: pc"));
    assert!(output.stdout().contains("queue_len: 5"));
    assert!(output.stdout().contains("hold_enabled: false"));
}

#[test]
fn verify_command_bounded_cover_reaches_native_execution_after_capability_validation() {
    let output = VerifyCommand::run(
        &VerifyArgs::new(Some("cover".to_owned())),
        RenderFormat::Json,
    );

    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    assert!(output.stdout().contains("\"kind\":\"build_coverage\""));
    assert!(output.stdout().contains("\"template\":\"cli-default\""));
}

#[test]
fn verify_command_rejects_unknown_target() {
    let output = VerifyCommand::run(
        &VerifyArgs::new(Some("fixtures".to_owned())),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output
        .stderr()
        .contains(CliErrorCode::VerifyTargetUnknown.as_str()));
}

#[test]
fn verify_command_reports_builtin_kick_contracts() {
    let output = VerifyCommand::run(
        &VerifyArgs::new(Some("kicks".to_owned())),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("kind: verify-kicks"));
    assert!(output.stdout().contains("srs_jlstz_transitions: 8"));
    assert!(output.stdout().contains("srs_i_transitions: 8"));
    assert!(output.stdout().contains("srs_profile_id: srs-90"));
    assert!(output.stdout().contains("no_kick_profile_id: no-kick"));
    assert!(output.stdout().contains("srs_plus_profile_id: srs-plus"));
    assert!(output
        .stdout()
        .contains("srs_plus_effective_kick_model: srs-plus-180"));
    assert!(output.stdout().contains("srs_plus_180_transitions: 24"));
    assert!(output.stdout().contains("kick_verification_cases: 264"));
    assert!(output.stdout().contains("kick_verification_failures: 0"));
    assert!(!output.stdout().contains("srs_plus_extension_reason"));
}
