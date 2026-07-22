use crate::{
    args::{ScoringAction, ScoringArgs},
    commands::ScoringCommand,
    exit::ExitCode,
    output::RenderFormat,
};

#[test]
fn scoring_command_lists_and_inspects_canonical_profiles() {
    let list = ScoringCommand::run(&ScoringArgs::new(ScoringAction::List), RenderFormat::Text);
    let inspect = ScoringCommand::run(
        &ScoringArgs::new(ScoringAction::Inspect).with_profile(Some("tetrio".to_owned())),
        RenderFormat::Text,
    );

    assert_eq!(list.exit_code(), ExitCode::Success);
    assert!(list.stdout().contains("profile_0_id: jstris-ultra"));
    assert_eq!(inspect.exit_code(), ExitCode::Success);
    assert!(inspect.stdout().contains("score_model: tetrio"));
    assert!(inspect.stdout().contains("attack_model: tetrio"));
    assert!(inspect
        .stdout()
        .contains("accuracy_level: basic-approximation"));
    assert!(inspect.stdout().contains("profile_specific_exact: false"));
    assert!(inspect.stdout().contains("spin_rule: t-spins"));
    assert!(inspect.stdout().contains("configurable spin"));
}

#[test]
fn scoring_command_imports_and_exports_json_profiles() {
    let json = r#"{"id":"x","display_name":"X","attack_model":"guideline"}"#;
    let import = ScoringCommand::run(
        &ScoringArgs::new(ScoringAction::Import).with_input(Some(json.to_owned())),
        RenderFormat::Json,
    );
    let export = ScoringCommand::run(
        &ScoringArgs::new(ScoringAction::Export).with_profile(Some("jstris-ultra".to_owned())),
        RenderFormat::Json,
    );

    assert_eq!(import.exit_code(), ExitCode::Success);
    assert!(import.stdout().contains("\"id\":\"x\""));
    assert_eq!(export.exit_code(), ExitCode::Success);
    assert!(export.stdout().contains("\\\"id\\\": \\\"jstris-ultra\\\""));
}
