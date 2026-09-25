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
