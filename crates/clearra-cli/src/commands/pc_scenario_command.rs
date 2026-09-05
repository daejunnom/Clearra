use clearra_app::{AppCommand, AppContext, AppRenderModel, AppRequest, ScenarioAppCommand};

use crate::{
    args::pc_scenario_args::PcScenarioArgs,
    assemble::PcScenarioQueryAssembler,
    error::CliErrorCode,
    fixture::{PcScenarioExpectedVerifier, PcScenarioUnsupportedVerifier},
    output::{CliOutput, CommandRenderer, RenderFormat},
};

#[cfg(all(test, feature = "native-c-core"))]
#[path = "pc_scenario_command_tests.rs"]
mod pc_scenario_command_tests;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcScenarioCommand;

impl PcScenarioCommand {
    pub fn run(args: &PcScenarioArgs, format: RenderFormat) -> CliOutput {
        let assembly = match PcScenarioQueryAssembler::assemble(args) {
            Ok(assembly) => assembly,
            Err(error) => {
                return CliOutput::error(CliErrorCode::PcScenarioFixtureInvalid, error.message());
            }
        };
        let response = AppContext::default().run(AppRequest::new(AppCommand::Scenario(
            ScenarioAppCommand::new(assembly.query().clone()),
        )));
        if response.status() == clearra_app::AppStatus::ValidationFailed {
            let report = response.diagnostics().validation();
            if let Some(output) = expected_unsupported_output(args, &assembly, report, format) {
                return output;
            }
            return CliOutput::validation_failed_with_format(report, format);
        }
        match response.render_model() {
            Some(AppRenderModel::Scenario(result)) => {
                render_success(args, &assembly, result.summary_fields(), format)
            }
            _ => {
                let message = response
                    .error()
                    .map(|error| error.message().to_owned())
                    .unwrap_or_else(|| "scenario app response missing result".to_owned());
                CliOutput::error(CliErrorCode::PcScenarioSearchInternal, message)
            }
        }
    }
}

fn render_success(
    args: &PcScenarioArgs,
    assembly: &crate::assemble::PcScenarioAssembly,
    result_fields: Vec<(String, String)>,
    format: RenderFormat,
) -> CliOutput {
    let expected_fields = match PcScenarioExpectedVerifier::verify(
        args.verify_expected(),
        assembly.fixture(),
        &result_fields,
    ) {
        Ok(fields) => fields,
        Err(error) => return expected_mismatch_output(assembly, error),
    };
    let mut fields = assembly.input_fields();
    fields.extend(expected_fields);
    fields.extend(result_fields);
    CommandRenderer::render_output(
        "pc-scenario",
        crate::output::SummaryRenderContract::render_fields(fields),
        format,
    )
}

fn expected_unsupported_output(
    args: &PcScenarioArgs,
    assembly: &crate::assemble::PcScenarioAssembly,
    report: &clearra_validation::diagnostic::diagnostic_report::DiagnosticReport,
    format: RenderFormat,
) -> Option<CliOutput> {
    if !args.verify_expected() {
        return None;
    }
    let fixture = assembly.fixture()?;
    match PcScenarioUnsupportedVerifier::verify_validation(fixture.expected(), report) {
        Ok(expected_fields) => {
            let mut fields = assembly.input_fields();
            fields.extend(expected_fields);
            fields.extend(PcScenarioUnsupportedVerifier::validation_fields(report));
            Some(CommandRenderer::render_output(
                "pc-scenario",
                crate::output::SummaryRenderContract::render_fields(fields),
                format,
            ))
        }
        Err(error) => Some(expected_mismatch_output(assembly, error)),
    }
}

fn expected_mismatch_output(
    assembly: &crate::assemble::PcScenarioAssembly,
    error: String,
) -> CliOutput {
    CliOutput::error(
        CliErrorCode::PcScenarioExpectedMismatch,
        format!(
            "scenario fixture '{}' expected result mismatch: {error}",
            assembly.fixture_path().unwrap_or("<inline>")
        ),
    )
}
