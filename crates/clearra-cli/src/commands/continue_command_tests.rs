use crate::exit::ExitCode;

use super::*;

#[test]
fn continue_command_runs_next_pc_from_token_without_prompting() {
    let output = ContinueCommand::run(
        &ContinueArgs::new(Some(
            "pc2:l2:bdstandard-10:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:oall:e0:hnone:qIIOOO".to_owned(),
        )),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("kind: continue"));
    assert!(output.stdout().contains("status: continued-searched"));
    assert!(output.stdout().contains("interactive_prompt: false"));
    assert!(output.stdout().contains("queue_mode: fixed"));
    assert!(output.stdout().contains("queue_len: 5"));
    assert!(output.stdout().contains("rule_profile: srs-plus"));
    assert!(output.stdout().contains("objective: all"));
}

#[test]
fn continue_command_can_emit_another_continue_hint() {
    let output = ContinueCommand::run(
        &ContinueArgs::new(Some(
            "pc2:l2:bdstandard-10:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:oall:e0:hnone:qIIOOOIIOOO".to_owned(),
        )),
        RenderFormat::TextVerbose,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("kind: continue"));
    assert!(output.stdout().contains("next_pc_available: true"));
    assert!(output.stdout().contains("continuation_token_version: pc2"));
    assert!(output.stdout().contains(
        "continue_hint: clearra continue pc2:l2:bdstandard-10:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:oall:e0:hnone:qIIOOO:qkoracle"
    ));
}

#[test]
fn continue_command_accepts_scenario_continuation_token() {
    let output = ContinueCommand::run(
        &ContinueArgs::new(Some(
            "sc2:w10:v2:m0x00000000000003f0:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:hnone:qI:p1:x1:n0:a1:z0:gclear-to-empty:ccount-all:t1".to_owned(),
        )),
        RenderFormat::TextVerbose,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("kind: continue"));
    assert!(output
        .stdout()
        .contains("status: scenario-continued-searched"));
    assert!(output.stdout().contains("continuation_kind: scenario"));
    assert!(output.stdout().contains("solution_found: true"));
    assert!(output.stdout().contains("exact_pieces: 1"));
    assert!(output.stdout().contains("min_remaining_queue: 0"));
    assert!(output.stdout().contains("allow_hold: true"));
    assert!(output.stdout().contains("requires_180: false"));
    assert!(output.stdout().contains("count_policy: count-all"));
    assert!(output.stdout().contains("retained_trace_limit: 1"));
    assert!(output.stdout().contains("continuation_token_version: none"));
    assert!(output
        .stdout()
        .contains("scenario_replay_token_version: sr2"));
}

#[test]
fn continue_command_labels_scenario_replay_token_as_replay() {
    let output = ContinueCommand::run(
        &ContinueArgs::new(Some(
            "sr2:w10:v2:m0x00000000000003f0:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:hnone:qI:p1:x1:n0:a1:z0:gclear-to-empty:ccount-all:t1".to_owned(),
        )),
        RenderFormat::TextVerbose,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("kind: continue"));
    assert!(output
        .stdout()
        .contains("status: scenario-replayed-searched"));
    assert!(output
        .stdout()
        .contains("continuation_kind: scenario-replay"));
    assert!(output.stdout().contains("interactive_prompt: false"));
    assert!(output.stdout().contains("solution_found: true"));
    assert!(output
        .stdout()
        .contains("scenario_replay_token_version: sr2"));
}

#[test]
fn continue_command_rejects_missing_or_invalid_token() {
    let missing = ContinueCommand::run(&ContinueArgs::default(), RenderFormat::Text);
    assert_eq!(missing.exit_code(), ExitCode::ValidationFailed);
    assert!(missing
        .stderr()
        .contains(CliErrorCode::ContinueTokenRequired.as_str()));

    let invalid = ContinueCommand::run(
        &ContinueArgs::new(Some("not-a-token".to_owned())),
        RenderFormat::Text,
    );
    assert_eq!(invalid.exit_code(), ExitCode::ValidationFailed);
    assert!(invalid
        .stderr()
        .contains(CliErrorCode::ContinueTokenInvalid.as_str()));
}
