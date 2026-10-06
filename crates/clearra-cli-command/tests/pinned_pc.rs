use clearra_app::{AppCommand, PcResultProjection, PcTilingIngressOrigin};
use clearra_cli_command::CliCommandParser;
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{NormalizedTilingSolutionKey, PiecePlacementMask},
};

#[test]
fn extended_empty_opening_cli_keeps_its_target_and_typed_ingress_without_search() {
    for lines in (8..=24).step_by(2) {
        for product in ["pc", "pc tiling"] {
            let request = CliCommandParser::parse(&format!(
                "clearra {product} --lines {lines} --queue {} --workers 1 --backend cpu --no-hold",
                "I".repeat(lines * 10 / 4),
            ))
            .unwrap()
            .to_app_request()
            .unwrap();
            let AppCommand::Pc(command) = request.command() else {
                panic!("empty opening must retain its own query authority");
            };
            assert_eq!(usize::from(command.query().target().lines()), lines);
            assert_eq!(command.query().execution_policy().workers(), 1);
            if product == "pc tiling" {
                assert_eq!(
                    command.result_projection(),
                    PcResultProjection::TilingFamilyV1(PcTilingIngressOrigin::CanonicalPcTiling)
                );
            }
            command.validate_result_projection().unwrap();
        }
    }
}

#[test]
fn extended_tiling_cli_keeps_the_actual_target_board_and_explicit_worker_policy() {
    for height in [7_u8, 8, 12, 24] {
        let starts = if height == 7 {
            vec![0, 3]
        } else {
            (0..u16::from(height)).step_by(4).collect()
        };
        let pieces = starts.len();
        let mut holes = Board256Mask::EMPTY;
        for (column, start) in starts.into_iter().enumerate() {
            for row in start..start + 4 {
                holes = holes.union(Board256Mask::singleton(row * 10 + column as u16).unwrap());
            }
        }
        let initial = Board256Mask::all_cells(u16::from(height) * 10)
            .unwrap()
            .without(holes);
        let words = initial.words();
        let command = format!(
            "clearra pc tiling --lines {height} --height {height} --board-mask 0x{:016x}{:016x}{:016x}{:016x} --pieces {pieces} --queue {} --workers 1 --backend cpu --no-hold",
            words[3], words[2], words[1], words[0], "I".repeat(pieces)
        );
        let request = CliCommandParser::parse(&command)
            .unwrap()
            .to_app_request()
            .unwrap();
        let AppCommand::Scenario(command) = request.command() else {
            panic!("expected full-height PC scenario");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::TilingFamilyV1(PcTilingIngressOrigin::CanonicalPcTiling)
        );
        assert_eq!(
            command.query().initial_board().visible_height(),
            u16::from(height)
        );
        assert_eq!(command.query().initial_board().occupied_words(), words);
        assert_eq!(command.query().exact_pieces(), Some(pieces));
        assert_eq!(command.query().execution_policy().workers(), 1);
        command.validate_result_projection().unwrap();
    }
}

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

#[test]
fn pc_tiling_rejects_high_words_outside_the_declared_target_instead_of_truncating_them() {
    for (height, invalid_cell) in [(7, 70), (24, 240)] {
        let words = Board256Mask::singleton(invalid_cell).unwrap().words();
        let command = format!(
            "clearra pc tiling --lines {height} --height {height} --board-mask 0x{:016x}{:016x}{:016x}{:016x} --pieces 1 --queue I --workers 1 --backend cpu --no-hold",
            words[3], words[2], words[1], words[0]
        );
        assert!(CliCommandParser::parse(&command).is_err());
    }
}
