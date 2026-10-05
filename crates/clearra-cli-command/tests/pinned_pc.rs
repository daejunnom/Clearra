use clearra_app::AppCommand;
use clearra_cli_command::CliCommandParser;
use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{NormalizedTilingSolutionKey, PiecePlacementMask},
};

#[test]
fn pc_minimals_accepts_a_second_canonical_solution_selection() {
    let key = NormalizedTilingSolutionKey::from_placements(
        0x3f,
        [PiecePlacementMask::new(PieceKind::I, 0x3c0)],
    )
    .unwrap()
    .as_str()
    .to_owned();
    let command = format!(
        "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --hold empty --pin-key \"{key}\""
    );
    let request = CliCommandParser::parse(&command)
        .unwrap()
        .to_app_request()
        .unwrap();
    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected scenario-backed PC minimals");
    };
    assert_eq!(command.pinned_minimum_keys(), [key.clone()]);
    assert!(CliCommandParser::parse(&command_string_with_invalid_pin()).is_err());

    let repeated = format!(
        "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --pin-key \"{key}\" --pin-key \"{key}\" --no-hold"
    );
    let parsed = CliCommandParser::parse(&repeated).unwrap();
    let repeated_request = parsed.to_app_request().unwrap();
    let AppCommand::Scenario(repeated_command) = repeated_request.command() else {
        panic!("expected scenario-backed PC minimals");
    };
    assert_eq!(repeated_command.pinned_minimum_keys(), [key]);
}

fn command_string_with_invalid_pin() -> String {
    "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --hold empty --pin-key invalid"
        .to_owned()
}

#[test]
fn common_extended_codec_does_not_enable_compact_pc_minimum_pins() {
    let key = format!(
        "ctk2|height=1|initial={:064x}|placements=I:{:064x}",
        0x3f, 0x3c0
    );
    assert!(NormalizedTilingSolutionKey::parse_canonical(&key).is_ok());
    let base =
        "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --no-hold";
    assert!(CliCommandParser::parse(&format!("{base} --pin-key \"{key}\"")).is_err());
    let request = CliCommandParser::parse(base)
        .unwrap()
        .to_app_request()
        .unwrap();
    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected PC scenario")
    };
    assert!(command
        .clone()
        .with_pinned_minimum_keys(vec![key])
        .validate_result_projection()
        .is_err());
}
