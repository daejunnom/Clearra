use clearra_output::fumen_like::{FumenLikeTrace, FumenLikeWriter};

use crate::{error::CliErrorCode, exit::ExitCode};

use super::*;

#[test]
fn converts_fumen_like_trace_to_json_contract() {
    let input = FumenLikeWriter::write(&FumenLikeTrace::new(vec!["kind=pc\nlines=2".to_owned()]));
    let output = ConvertCommand::run(
        &ConvertArgs::new(
            Some(input),
            Some("fumen-like".to_owned()),
            Some("json".to_owned()),
        ),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::Success);
    assert!(output.stdout().contains("\"kind\":\"convert\""));
    assert!(output.stdout().contains("\"page_count\":1"));
    assert!(output.stdout().contains("\"page_0\":\"kind=pc\\nlines=2\""));
}

#[test]
fn rejects_reverse_or_encode_direction_for_mvp1() {
    let output = ConvertCommand::run(
        &ConvertArgs::new(
            Some("{\"page_0\":\"kind=pc\"}".to_owned()),
            Some("json".to_owned()),
            Some("fumen-like".to_owned()),
        ),
        RenderFormat::Text,
    );

    assert_eq!(output.exit_code(), ExitCode::Unsupported);
    assert!(output
        .stderr()
        .contains(CliErrorCode::ConvertDirectionUnsupported.as_str()));
}
