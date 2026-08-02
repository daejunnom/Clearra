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
fn cli_setup_assembly_applies_the_shared_host_worker_policy() {
    let hardware = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
    let default_workers = clearra_pc_graph::request::WorkerPolicy::default_worker_limit();

    let default_request = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Setup(SetupArgs::default()),
        RenderFormat::Text,
    )
    .expect("default setup request")
    .request();
    assert_eq!(
        usize::from(default_request.resource_budget().workers()),
        default_workers.min(usize::from(u16::MAX))
    );

    let all_request = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Setup(
            SetupArgs::default()
                .with_workers(Some(0))
                .with_use_all_logical_processors(true),
        ),
        RenderFormat::Text,
    )
    .expect("all-logical-processors setup request")
    .request();
    assert_eq!(
        usize::from(all_request.resource_budget().workers()),
        hardware.min(usize::from(u16::MAX))
    );

    let cap = default_workers.min(3);
    let capped_request = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Setup(SetupArgs::default().with_automatic_worker_limit(Some(cap))),
        RenderFormat::Text,
    )
    .expect("capped setup request")
    .request();
    assert_eq!(usize::from(capped_request.resource_budget().workers()), cap);
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
