use crate::output::{number_field, string_field};

use super::*;

#[test]
fn delegates_command_rendering_to_output_crate() {
    let rendered = CommandRenderer::render("pc", [number_field("lines", 2)], RenderFormat::Json);

    assert!(rendered.contains("\"schema_version\":2"));
    assert!(rendered.contains("\"kind\":\"pc\""));
    assert!(rendered.contains("\"summary\":{\"lines\":2}"));
    assert!(rendered.contains("\"contract\":{\"command\""));
}

#[test]
fn command_renderer_does_not_infer_numeric_looking_strings() {
    let rendered = CommandRenderer::render(
        "pc",
        [string_field("queue_count", "001")],
        RenderFormat::Json,
    );

    assert!(rendered.contains("\"queue_count\":\"001\""));
}
