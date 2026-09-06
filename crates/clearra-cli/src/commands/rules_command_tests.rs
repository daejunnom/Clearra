use clearra_rules::{
    kicks::{KickImport, KickTableProfile, KickTableProfileId, NoKick, SrsKicks},
    profile::rule_profile::RuleProfileId,
};

use crate::{
    args::{RulesAction, RulesArgs},
    commands::RulesCommand,
    error::CliErrorCode,
    exit::ExitCode,
    output::RenderFormat,
};

#[test]
fn rules_command_lists_canonical_registry_profiles() {
    let output = RulesCommand::run(&RulesArgs::new(RulesAction::List), RenderFormat::Json);

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("\"kind\":\"rules\""));
    assert!(output.stdout().contains("\"profile_0_kick_profile\""));
    assert!(output.stdout().contains("source_kind"));
}

#[test]
fn rules_command_inspects_and_verifies_builtin_profiles() {
    let inspect = RulesCommand::run(
        &RulesArgs::new(RulesAction::Inspect).with_profile(Some("srs".to_owned())),
        RenderFormat::Text,
    );
    let verify = RulesCommand::run(&RulesArgs::new(RulesAction::Verify), RenderFormat::Text);

    assert_eq!(inspect.exit_code(), ExitCode::Success);
    assert!(inspect.stdout().contains("effective_kick_model: srs-90"));
    assert!(inspect.stdout().contains("source_kind: built-in-exact"));
    assert_eq!(verify.exit_code(), ExitCode::Success);
    assert!(verify.stdout().contains("kick_verification_failures: 0"));
}

#[test]
fn rules_command_discloses_exact_connected_srs_plus_and_srs_x_backends() {
    let srs_plus = RulesCommand::run(
        &RulesArgs::new(RulesAction::Inspect).with_profile(Some("srs-plus".to_owned())),
        RenderFormat::Text,
    );
    let srs_x = RulesCommand::run(
        &RulesArgs::new(RulesAction::Inspect).with_profile(Some("srs-x".to_owned())),
        RenderFormat::Text,
    );

    assert_eq!(srs_plus.exit_code(), ExitCode::Success);
    assert!(srs_plus.stdout().contains("source_kind: built-in-exact"));
    assert!(srs_plus.stdout().contains("TETR.IO SRS+"));
    assert!(srs_plus.stdout().contains("search_backend_supported: true"));
    assert!(srs_plus.stdout().contains("supports_exact_180: true"));
    assert!(srs_plus
        .stdout()
        .contains("c_compact_descriptor_ready: true"));
    assert_eq!(srs_x.exit_code(), ExitCode::Success);
    assert!(srs_x.stdout().contains("source_kind: built-in-exact"));
    assert!(srs_x.stdout().contains("search_backend_supported: true"));
    assert!(srs_x.stdout().contains("supports_exact_180: true"));
    assert!(srs_x.stdout().contains("c_compact_descriptor_ready: true"));
    assert!(srs_x.stdout().contains("unsupported_backend_reason: none"));
}

#[test]
fn rules_command_imports_and_exports_json_profiles() {
    let exported = RulesCommand::run(
        &RulesArgs::new(RulesAction::Export).with_profile(Some("no-kick".to_owned())),
        RenderFormat::Json,
    );
    let import_json = KickImport::to_json(&NoKick::profile()).expect("export no-kick json");
    let imported = RulesCommand::run(
        &RulesArgs::new(RulesAction::Import).with_input(Some(import_json)),
        RenderFormat::Text,
    );

    assert_eq!(exported.exit_code(), ExitCode::Success);
    assert!(exported.stdout().contains("\"action\":\"export\""));
    assert!(exported.stdout().contains("\\\"id\\\": \\\"no-kick\\\""));
    assert_eq!(imported.exit_code(), ExitCode::Success);
    assert!(imported.stdout().contains("action: import"));
    assert!(imported.stdout().contains("issue_count: 0"));
    assert!(imported.stdout().contains("verified_profile: true"));
    assert!(imported
        .stdout()
        .contains("c_compact_descriptor_ready: true"));
}

#[test]
fn rules_command_exports_and_round_trips_the_canonical_srs_x_profile() {
    let exported = RulesCommand::run(
        &RulesArgs::new(RulesAction::Export).with_profile(Some("srs-x".to_owned())),
        RenderFormat::Json,
    );

    assert_eq!(
        exported.exit_code(),
        ExitCode::Success,
        "{}",
        exported.stderr()
    );
    let envelope: serde_json::Value =
        serde_json::from_str(exported.stdout()).expect("rules export envelope");
    let profile_json = envelope["summary"]["json"]
        .as_str()
        .expect("embedded SRS-X profile");
    let profile = KickImport::from_json(profile_json).expect("round-tripped SRS-X profile");

    assert_eq!(profile.id(), KickTableProfileId::SrsX);
    assert_eq!(profile.source_rule(), RuleProfileId::SrsX);
    assert!(profile.supports_180());
    assert_eq!(profile.transition_count(), 84);
    assert_eq!(profile, SrsKicks::srs_x_profile());
}

#[test]
fn rules_import_marks_verified_exact_180_profile_as_c_descriptor_ready() {
    let import_json = KickImport::to_json(&KickTableProfile::new(
        KickTableProfileId::Imported,
        RuleProfileId::SrsX,
        SrsKicks::srs_plus_profile().entries().to_vec(),
    ))
    .expect("imported srs-x json");
    let imported = RulesCommand::run(
        &RulesArgs::new(RulesAction::Import).with_input(Some(import_json)),
        RenderFormat::Text,
    );

    assert_eq!(imported.exit_code(), ExitCode::Success);
    assert!(imported.stdout().contains("source_rule: srs-x"));
    assert!(imported.stdout().contains("supports_exact_180: true"));
    assert!(imported
        .stdout()
        .contains("c_compact_descriptor_ready: true"));
    assert!(imported
        .stdout()
        .contains("unsupported_backend_reason: none"));
}

#[test]
fn rules_verify_input_reports_issues_without_failing() {
    let output = RulesCommand::run(
        &RulesArgs::new(RulesAction::Verify).with_input(Some(incomplete_import_json())),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("action: verify"));
    assert!(output.stdout().contains("issue_count:"));
    assert!(output.stdout().contains("transition_complete: false"));
    assert!(output.stdout().contains("verification_status: issues"));
    assert!(output
        .stdout()
        .contains("unsupported_backend_reason: kick_profile_verification_failed"));
}

#[test]
fn rules_import_rejects_unverified_imported_profiles() {
    let output = RulesCommand::run(
        &RulesArgs::new(RulesAction::Import).with_input(Some(incomplete_import_json())),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output
        .stderr()
        .contains(CliErrorCode::RulesInputInvalid.as_str()));
    assert!(output
        .stderr()
        .contains("imported kick profile is not verified"));
    assert!(output.stderr().contains("missing_transition_count="));
}

fn incomplete_import_json() -> String {
    r#"{
        "id": "imported",
        "source_rule": "custom",
        "entries": [
            {
                "piece": "T",
                "from": "0",
                "to": "R",
                "offsets": [{ "dx": 0, "dy": 0 }]
            }
        ]
    }"#
    .to_owned()
}
