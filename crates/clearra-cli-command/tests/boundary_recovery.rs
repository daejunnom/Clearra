use clearra_app::{AppCommand, AppContext, AppStatus};
use clearra_cli_command::CliCommandParser;

#[test]
fn fixed_queue_boundary_recovery_uses_one_continuous_app_request() {
    let parsed = CliCommandParser::parse(
        "clearra recovery boundary --initial-board-mask 0x3f0 --target-board-mask 0xc030 --height 4 --queue IO --stage-one-count 1 --placements 2 --borrow-source-position 2 --borrow-placement-mask 0x300c000 --no-hold --preserve-b2b-stage-one --spin-profile all-spin-plus",
    )
    .unwrap();
    let request = parsed.to_app_request().unwrap();
    let AppCommand::BoundaryRecovery(command) = request.command() else {
        panic!("expected typed boundary recovery command");
    };
    assert_eq!(command.query().queue.len(), 2);
    assert_eq!(command.query().stage_one_queue_len, 1);
    assert_eq!(command.query().borrow_source_index, 1);
    assert_eq!(
        command.query().borrow_placement_mask.words(),
        [0x300c000, 0, 0, 0]
    );
    assert_eq!(command.query().preserve_b2b_by_stage, [true, false]);
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let message = response.render_model().unwrap().message().unwrap();
    assert!(message
        .fields()
        .iter()
        .any(|field| field.key() == "status" && field.value().as_text() == "normal"));
}

#[test]
fn boundary_recovery_rejects_ambiguous_or_missing_supply() {
    assert!(CliCommandParser::parse(
        "clearra recovery boundary --initial-board-mask 0x3f0 --target-board-mask 0xc030 --height 4 --queue IO --stage-one-count 1 --placements 2 --borrow-source-position 2 --borrow-placement-mask 0x300c000 --hold --no-hold"
    )
    .is_err());
    assert!(CliCommandParser::parse(
        "clearra recovery boundary --initial-board-mask 0x3f0 --target-board-mask 0xc030 --height 4 --stage-one-count 1 --placements 2"
    )
    .is_err());
}

#[test]
fn zero_early_placements_does_not_require_a_borrow_role() {
    let parsed = CliCommandParser::parse(
        "clearra recovery boundary --initial-board-mask 0x3f0 --target-board-mask 0xc030 --height 4 --queue IO --stage-one-count 1 --placements 2 --max-early-placements 0 --no-hold",
    ).unwrap();
    let request = parsed.to_app_request().unwrap();
    let AppCommand::BoundaryRecovery(command) = request.command() else {
        panic!("expected typed boundary recovery command");
    };
    assert_eq!(command.query().max_early_placements, 0);
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
}

#[test]
fn complete_diagram_roles_bind_each_source_token_to_its_lock_mask() {
    let command = "clearra recovery boundary --initial-board-mask 0x3f0 --target-board-mask 0xc030 --height 4 --queue IO --stage-one-count 1 --placements 2 --max-early-placements 0 --no-hold --role-mask 2:0xc030 --role-mask 1:0xf";
    let parsed = CliCommandParser::parse(command).unwrap();
    let request = parsed.to_app_request().unwrap();
    let AppCommand::BoundaryRecovery(recovery) = request.command() else {
        panic!("expected boundary recovery");
    };
    assert_eq!(recovery.query().placement_role_masks.len(), 2);
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert!(CliCommandParser::parse(&command.replace(" --role-mask 1:0xf", "")).is_err());
    assert!(
        CliCommandParser::parse(&command.replace("--role-mask 1:0xf", "--role-mask 2:0xf"))
            .is_err()
    );

    let borrowed = command
        .replace(
            "--max-early-placements 0",
            "--max-early-placements 1 --borrow-source-position 2",
        )
        .replace("--role-mask 2:0xc030", "--role-mask 2:0x300c000");
    let borrowed = CliCommandParser::parse(&borrowed)
        .unwrap()
        .to_app_request()
        .unwrap();
    let AppCommand::BoundaryRecovery(recovery) = borrowed.command() else {
        panic!("expected boundary recovery");
    };
    assert_eq!(
        recovery.query().borrow_placement_mask.words(),
        [0x300c000, 0, 0, 0]
    );
}
