use clearra_app::AppCommand;

use crate::args::{FailedQueueArgs, ParsedCliCommand, PcArgs, SetupArgs};

use super::*;

#[test]
fn cli_pc_builds_app_request() {
    let assembly =
        CliAppRequestAssembler::assemble(ParsedCliCommand::Pc(PcArgs::new(2)), RenderFormat::Text)
            .expect("pc app request");

    assert!(matches!(assembly.request().command(), AppCommand::Pc(_)));
}

#[test]
fn cli_pc_command_assembles_app_request() {
    let assembly =
        CliAppRequestAssembler::assemble(ParsedCliCommand::Pc(PcArgs::new(2)), RenderFormat::Text)
            .expect("pc app request");
    let (command, _, _, _) = assembly.request().into_parts();
    assert!(matches!(command, AppCommand::Pc(_)));
}

#[test]
fn cli_setup_command_assembles_app_request() {
    let assembly = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Setup(SetupArgs::default()),
        RenderFormat::Text,
    )
    .expect("setup app request");
    let (command, _, _, _) = assembly.request().into_parts();
    assert!(matches!(command, AppCommand::Setup(_)));
}

#[test]
fn cli_failed_queue_command_assembles_coverage_complement_request() {
    let assembly = CliAppRequestAssembler::assemble(
        ParsedCliCommand::FailedQueue(FailedQueueArgs::new(PcArgs::new(2), None, 9)),
        RenderFormat::Text,
    )
    .expect("failed-queue app request");
    let request = assembly.request();
    let AppCommand::Percent(command) = request.command() else {
        panic!("expected percent-backed failed-queue request");
    };
    assert!(command.is_failed_queue());
    assert_eq!(command.failed_pattern_limit(), 9);
}
