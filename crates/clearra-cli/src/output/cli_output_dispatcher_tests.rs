use super::*;

#[test]
fn success_output_carries_zero_exit_code() {
    let output = CliOutput::success("ready");

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert_eq!(output.stdout(), "ready");
    assert_eq!(output.stderr(), "");
}

#[test]
fn json_validation_failure_uses_stdout_json_contract() {
    use clearra_validation::diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport,
    };

    let mut report = DiagnosticReport::new();
    report.push(Diagnostic::new(
        DiagnosticCode::ECoreFfiBufferBounds,
        "native view exceeded the C ABI buffer bound",
    ));

    let output = CliOutput::validation_failed_with_format(&report, RenderFormat::Json);

    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output.stdout().contains("\"kind\":\"diagnostic\""));
    assert!(output.stdout().contains("\"E_CORE_FFI_BUFFER_BOUNDS\""));
    assert_eq!(output.stderr(), "");
}
