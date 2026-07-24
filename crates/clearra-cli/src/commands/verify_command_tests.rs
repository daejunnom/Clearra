use crate::error::CliErrorCode;
use crate::exit::ExitCode;

use super::*;

#[test]
fn verify_command_runs_validation_before_native_execution() {
    let output = VerifyCommand::run(&VerifyArgs::default(), RenderFormat::Text);

    assert_eq!(output.exit_code(), ExitCode::Unsupported);
    assert!(output
        .stderr()
        .contains(CliErrorCode::ProductRuntimeUnsupported.as_str()));
}

#[test]
fn verify_command_default_reaches_native_execution_after_capability_validation() {
    let output = VerifyCommand::run(&VerifyArgs::default(), RenderFormat::Json);

    assert_eq!(output.exit_code(), ExitCode::Unsupported);
    assert!(output
        .stderr()
        .contains(CliErrorCode::ProductRuntimeUnsupported.as_str()));
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
    assert!(output.stdout().contains("kick_verification_cases: 192"));
    assert!(output.stdout().contains("kick_verification_failures: 0"));
    assert!(!output.stdout().contains("srs_plus_extension_reason"));
}
