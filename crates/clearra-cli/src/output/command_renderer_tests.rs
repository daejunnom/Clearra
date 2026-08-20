use crate::output::{number_field, string_field};

use super::*;

#[test]
fn delegates_command_rendering_to_output_crate() {
    let rendered = CommandRenderer::render("pc", [number_field("lines", 2)], RenderFormat::Json)
        .expect("JSON");

    assert!(rendered.contains("\"schema_version\":2"));
    assert!(rendered.contains("\"kind\":\"pc\""));
    assert!(rendered.contains("\"summary\":{\"lines\":2}"));
    assert!(rendered.contains("\"contract\":{\"command\""));
}

#[test]
fn every_cli_json_root_carries_the_compiled_five_field_runtime_identity() {
    let rendered = CommandRenderer::render("pc", [], RenderFormat::Json).expect("CLI JSON");
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("valid CLI JSON");
    let expected = ProductBuildIdentity::current();
    let identity = value["runtime_identity"]
        .as_object()
        .expect("runtime_identity object");

    assert_eq!(identity.len(), 5);
    assert_eq!(identity["engine_build_id"], expected.engine_build_id());
    assert_eq!(identity["source_commit"], expected.source_commit());
    assert_eq!(
        identity["contract_schema_version"],
        expected.contract_schema_version()
    );
    assert_eq!(
        identity["supply_semantics_id"],
        expected.supply_semantics_id()
    );
    assert_eq!(
        identity["artifact_schema_version"],
        expected.artifact_schema_version()
    );
}

#[test]
fn command_renderer_does_not_infer_numeric_looking_strings() {
    let rendered = CommandRenderer::render(
        "pc",
        [string_field("queue_count", "001")],
        RenderFormat::Json,
    )
    .expect("JSON");

    assert!(rendered.contains("\"queue_count\":\"001\""));
}

#[test]
fn oversized_fumen_output_is_a_typed_cli_failure_instead_of_a_panic() {
    let output = CommandRenderer::render_output(
        "x".repeat(4096),
        std::iter::empty(),
        RenderFormat::FumenLike,
    );

    assert_eq!(output.exit_code(), crate::exit::ExitCode::ValidationFailed);
    assert!(output.stdout().is_empty());
    assert!(output.stderr().contains("E_CLI_OUTPUT_LIMIT_EXCEEDED"));
    assert!(output.stderr().contains("fumen page 0 comment"));
}
