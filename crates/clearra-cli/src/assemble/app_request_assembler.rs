use clearra_app::{
    AppCommand, AppRequest, ContinueAppCommand, ConvertAppCommand, CoverAppCommand,
    InspectUnsupportedAppCommand, PathAppCommand, PcAppCommand, PercentAppCommand,
    ScenarioAppCommand, SetupAppCommand, VerifyAppCommand,
};

use crate::{
    args::ParsedCliCommand,
    assemble::{
        app_request_error_render::{pc_assembly_error, percent_assembly_error},
        app_request_format::{default_format_name, target_render_format},
        app_request_rules::{rules_command, scoring_command},
        app_request_scenario_contract::scenario_render_contract,
        CoverQueryAssembler, PcQueryAssembler, PcScenarioQueryAssembler, PercentQueryAssembler,
        SetupQueryAssembler,
    },
    error::CliErrorCode,
    output::{CliOutput, RenderFormat},
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CliAppRequestAssembly {
    request: AppRequest,
    render_format: RenderFormat,
    default_error: CliErrorCode,
}

impl CliAppRequestAssembly {
    fn new(request: AppRequest, render_format: RenderFormat, default_error: CliErrorCode) -> Self {
        Self {
            request,
            render_format,
            default_error,
        }
    }
}
impl CliAppRequestAssembly {
    pub(crate) fn request(self) -> AppRequest {
        self.request
    }
}
impl CliAppRequestAssembly {
    pub(crate) fn render_format(&self) -> RenderFormat {
        self.render_format
    }
}
impl CliAppRequestAssembly {
    pub(crate) fn default_error(&self) -> CliErrorCode {
        self.default_error
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CliAppRequestAssembler;

impl CliAppRequestAssembler {
    pub(crate) fn assemble(
        command: ParsedCliCommand,
        default_format: RenderFormat,
    ) -> Result<CliAppRequestAssembly, CliOutput> {
        match command {
            ParsedCliCommand::Pc(args) => {
                let query = PcQueryAssembler::assemble(&args).map_err(pc_assembly_error)?;
                Ok(CliAppRequestAssembly::new(
                    AppRequest::new(AppCommand::Pc(PcAppCommand::new(query))),
                    default_format,
                    CliErrorCode::PcSearchInternal,
                ))
            }
            ParsedCliCommand::PcScenario(args) => {
                let assembly = PcScenarioQueryAssembler::assemble(&args).map_err(|error| {
                    CliOutput::error(CliErrorCode::PcScenarioFixtureInvalid, error.message())
                })?;
                let contract = scenario_render_contract(&args, &assembly);
                Ok(CliAppRequestAssembly::new(
                    AppRequest::new(AppCommand::Scenario(
                        ScenarioAppCommand::new(assembly.query().clone())
                            .with_render_contract(contract),
                    )),
                    default_format,
                    CliErrorCode::PcScenarioSearchInternal,
                ))
            }
            ParsedCliCommand::Path(args) => {
                let query = PcQueryAssembler::assemble(args.pc()).map_err(pc_assembly_error)?;
                Ok(CliAppRequestAssembly::new(
                    AppRequest::new(AppCommand::Path(PathAppCommand::new(query))),
                    default_format,
                    CliErrorCode::PathSearchInternal,
                ))
            }
            ParsedCliCommand::Percent(args) => {
                let assembly =
                    PercentQueryAssembler::assemble(&args).map_err(percent_assembly_error)?;
                Ok(CliAppRequestAssembly::new(
                    AppRequest::new(AppCommand::Percent(
                        PercentAppCommand::new(assembly.query().clone())
                            .with_failed_pattern_limit(assembly.failed_pattern_limit()),
                    )),
                    default_format,
                    CliErrorCode::PercentQueryInvalid,
                ))
            }
            ParsedCliCommand::FailedQueue(args) => {
                let mut query = PcQueryAssembler::assemble(args.pc()).map_err(pc_assembly_error)?;
                if let Some(patterns) = args.patterns() {
                    let pattern = clearra_supply::queue::queue_pattern_expression::QueuePatternExpression::parse(
                        patterns,
                        query.execution_policy().max_patterns(),
                    )
                    .map_err(|error| {
                        CliOutput::error(
                            CliErrorCode::PercentQueryInvalid,
                            format!("invalid failed-queue pattern: {error}"),
                        )
                    })?;
                    query = query.with_queue(
                        clearra_pc_graph::request::PcQueueInput::pattern_expression(pattern),
                    );
                }
                Ok(CliAppRequestAssembly::new(
                    AppRequest::new(AppCommand::Percent(
                        PercentAppCommand::failed_queue_opening(query)
                            .with_failed_pattern_limit(args.failed_pattern_limit()),
                    )),
                    default_format,
                    CliErrorCode::PercentQueryInvalid,
                ))
            }
            ParsedCliCommand::Setup(args) => {
                let query = SetupQueryAssembler::assemble(&args).map_err(|error| {
                    CliOutput::error(CliErrorCode::SetupQueryInvalid, format!("{error:?}"))
                })?;
                Ok(CliAppRequestAssembly::new(
                    AppRequest::new(AppCommand::Setup(SetupAppCommand::new(query))),
                    default_format,
                    CliErrorCode::SetupQueryInvalid,
                ))
            }
            ParsedCliCommand::Cover(args) => {
                let query = CoverQueryAssembler::assemble(&args).map_err(|error| {
                    CliOutput::error(CliErrorCode::CoverQueryInvalid, error.to_string())
                })?;
                Ok(CliAppRequestAssembly::new(
                    AppRequest::new(AppCommand::Cover(
                        CoverAppCommand::new(query)
                            .with_export_template_json(args.export_template_json()),
                    )),
                    default_format,
                    CliErrorCode::CoverQueryInvalid,
                ))
            }
            ParsedCliCommand::Rules(args) => Ok(CliAppRequestAssembly::new(
                AppRequest::new(AppCommand::Rules(rules_command(&args))),
                default_format,
                CliErrorCode::RulesInputInvalid,
            )),
            ParsedCliCommand::Scoring(args) => Ok(CliAppRequestAssembly::new(
                AppRequest::new(AppCommand::Scoring(scoring_command(&args))),
                default_format,
                CliErrorCode::ScoringInputInvalid,
            )),
            ParsedCliCommand::Convert(args) => {
                let source = args.from().unwrap_or("fumen-like").to_owned();
                let target = args
                    .to()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| default_format_name(default_format).to_owned());
                let render_format = target_render_format(&target).unwrap_or(default_format);
                Ok(CliAppRequestAssembly::new(
                    AppRequest::new(AppCommand::Convert(ConvertAppCommand::from_parts(
                        args.input().map(ToOwned::to_owned),
                        source,
                        target,
                    ))),
                    render_format,
                    CliErrorCode::ConvertInputInvalid,
                ))
            }
            ParsedCliCommand::Continue(args) => Ok(CliAppRequestAssembly::new(
                AppRequest::new(AppCommand::Continue(ContinueAppCommand::new(
                    args.token().map(ToOwned::to_owned),
                ))),
                default_format,
                CliErrorCode::ContinueSearchInternal,
            )),
            ParsedCliCommand::Verify(args) => {
                let command = if matches!(args.target(), Some("kicks")) {
                    AppCommand::VerifyKicks(VerifyAppCommand::kicks())
                } else {
                    AppCommand::Verify(VerifyAppCommand::with_scope(
                        args.target().map(ToOwned::to_owned),
                    ))
                };
                Ok(CliAppRequestAssembly::new(
                    AppRequest::new(command),
                    default_format,
                    CliErrorCode::VerifyKicksFailed,
                ))
            }
            ParsedCliCommand::Product(tokens) => {
                let request = clearra_web_command::WebCommandParser::parse_tokens(&tokens)
                    .and_then(|request| request.to_app_request())
                    .map_err(|error| {
                        CliOutput::error(
                            CliErrorCode::CliInvalidValue,
                            format!("{}: {}", error.code().as_diagnostic_code(), error.message()),
                        )
                    })?;
                Ok(CliAppRequestAssembly::new(
                    request,
                    default_format,
                    CliErrorCode::ProductRuntimeUnsupported,
                ))
            }
            ParsedCliCommand::Unsupported(command) => Ok(CliAppRequestAssembly::new(
                AppRequest::new(AppCommand::InspectUnsupported(
                    InspectUnsupportedAppCommand::new(command),
                )),
                default_format,
                CliErrorCode::CliCommandUnsupported,
            )),
            ParsedCliCommand::Help(_) => unreachable!("help is rendered before app assembly"),
        }
    }
}

#[cfg(test)]
#[path = "app_request_assembler_tests.rs"]
mod tests;
