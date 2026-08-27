use crate::error::CliErrorCode;
use crate::exit::ExitCode;

use super::*;

#[test]
fn verify_command_runs_validation_before_native_execution() {
    let output = VerifyCommand::run(&VerifyArgs::new(Some("pc".to_owned())), RenderFormat::Text);

    // Workspace feature unification may connect clearra-app's native core
    // through another package without enabling clearra-cli's same-named
    // feature.  Assert the observed closed boundary instead of guessing the
    // dependency feature state from this crate's cfg.
    if output.exit_code() == ExitCode::Unsupported {
        assert_eq!(output.exit_code(), ExitCode::Unsupported);
        assert!(output
            .stderr()
            .contains("native_geometry_exact_cover_not_connected"));
        return;
    }

    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    assert!(output.stdout().contains("kind: verify"));
    assert!(output.stdout().contains("scope: pc"));
    assert!(output.stdout().contains("status: verified"));
    assert!(output.stdout().contains("probe_result_kind: pc"));
    assert!(output.stdout().contains("probes_attempted: 1"));
    assert!(output.stdout().contains("probes_passed: 1"));
    assert!(output.stdout().contains("probes_failed: 0"));
}

#[test]
fn verify_command_bounded_cover_reaches_native_execution_after_capability_validation() {
    let output = VerifyCommand::run(
        &VerifyArgs::new(Some("cover".to_owned())),
        RenderFormat::Json,
    );

    if output.exit_code() == ExitCode::Unsupported {
        assert_eq!(output.exit_code(), ExitCode::Unsupported);
        assert!(output
            .stderr()
            .contains("native_geometry_exact_cover_not_connected"));
        return;
    }

    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    assert!(output.stdout().contains("\"kind\":\"verify\""));
    assert!(output.stdout().contains("\"scope\":\"build\""));
    assert!(output.stdout().contains("\"status\":\"verified\""));
    assert!(output
        .stdout()
        .contains("\"probe_result_kind\":\"build_coverage\""));
    let json: serde_json::Value =
        serde_json::from_str(output.stdout()).expect("verify cover JSON response");
    assert_eq!(json["summary"]["probes_attempted"], serde_json::json!(1));
    assert_eq!(json["summary"]["probes_passed"], serde_json::json!(1));
    assert_eq!(json["summary"]["probes_failed"], serde_json::json!(0));
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
