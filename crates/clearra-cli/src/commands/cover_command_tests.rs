use crate::exit::ExitCode;

use super::*;

#[test]
fn cover_command_assembles_and_validates_canonical_query() {
    let output = CoverCommand::run(
        &CoverArgs::new(Some("basic".to_owned())),
        RenderFormat::TextVerbose,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("template: basic"));
    assert!(output.stdout().contains("status: cover-executed"));
    assert!(output
        .stdout()
        .contains("execution_scope: m21-build-coverage-product-path"));
    assert!(output
        .stdout()
        .contains("coverage_row_source: C BuildUp coverage row"));
    assert!(output.stdout().contains("build_coverage_probability:"));
    assert!(output
        .stdout()
        .contains("route: search-problem-core-executor"));
}

#[test]
fn cover_command_exports_native_template_json() {
    let output = CoverCommand::run(
        &CoverArgs::new(Some("basic".to_owned())).with_export_template_json(true),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("\"schema_version\": 2"));
    assert!(output.stdout().contains("\"id\": \"basic\""));
}

#[test]
fn cover_command_reports_invalid_native_template_json() {
    let output = CoverCommand::run(
        &CoverArgs::new(None).with_template_json(Some("v115@vhAAgH".to_owned())),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output.stderr().contains("E_COVER_QUERY_INVALID"));
}
