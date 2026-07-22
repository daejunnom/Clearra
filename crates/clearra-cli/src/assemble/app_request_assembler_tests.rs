use clearra_app::AppCommand;

use crate::args::{ParsedCliCommand, PcArgs, SetupArgs};

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
