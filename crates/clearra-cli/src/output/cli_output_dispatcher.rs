use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    error::CliErrorCode,
    exit::ExitCode,
    output::{diagnostic_printer::DiagnosticPrinter, RenderFormat},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliOutput {
    exit_code: ExitCode,
    stdout: String,
    stderr: String,
    warning_before: String,
    warning_after: String,
}

impl CliOutput {
    pub fn new(exit_code: ExitCode, stdout: impl Into<String>, stderr: impl Into<String>) -> Self {
        Self {
            exit_code,
            stdout: stdout.into(),
            stderr: stderr.into(),
            warning_before: String::new(),
            warning_after: String::new(),
        }
    }
}
impl CliOutput {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self::new(ExitCode::Success, stdout, "")
    }
}
impl CliOutput {
    pub fn error(code: CliErrorCode, message: impl Into<String>) -> Self {
        Self::new(
            code.default_exit_code(),
            "",
            format!("error {} {}", code.as_str(), message.into()),
        )
    }
}
impl CliOutput {
    pub fn validation_failed(report: &DiagnosticReport) -> Self {
        Self::new(
            ExitCode::ValidationFailed,
            "",
            DiagnosticPrinter::render(report),
        )
    }
}
impl CliOutput {
    pub fn validation_failed_with_format(report: &DiagnosticReport, format: RenderFormat) -> Self {
        match format {
            RenderFormat::Json => Self::new(
                ExitCode::ValidationFailed,
                DiagnosticPrinter::render_json(report),
                "",
            ),
            RenderFormat::Text
            | RenderFormat::TextVerbose
            | RenderFormat::TextDiagnostics
            | RenderFormat::FumenLike => Self::validation_failed(report),
        }
    }

    pub fn with_surrounding_warning(mut self, warning: impl Into<String>) -> Self {
        let warning = warning.into();
        self.warning_before = warning.clone();
        self.warning_after = warning;
        self
    }
}
impl CliOutput {
    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }
}
impl CliOutput {
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
}
impl CliOutput {
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    pub fn warning_before(&self) -> &str {
        &self.warning_before
    }

    pub fn warning_after(&self) -> &str {
        &self.warning_after
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CliOutputDispatcher;

impl CliOutputDispatcher {
    pub fn dispatch(output: &CliOutput) -> i32 {
        if !output.warning_before().is_empty() {
            eprintln!("{}", output.warning_before());
        }
        if !output.stdout().is_empty() {
            println!("{}", output.stdout());
        }
        if !output.stderr().is_empty() {
            eprintln!("{}", output.stderr());
        }
        if !output.warning_after().is_empty() {
            eprintln!("{}", output.warning_after());
        }
        output.exit_code().code()
    }
}

#[cfg(test)]
#[path = "cli_output_dispatcher_tests.rs"]
mod tests;
