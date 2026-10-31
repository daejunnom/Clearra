use super::*;

#[test]
fn jlstz_pieces_use_orientation_specific_srs_90_for_all_eight_transitions() {
    for piece in [
        PieceKind::J,
        PieceKind::L,
        PieceKind::S,
        PieceKind::T,
        PieceKind::Z,
    ] {
        for (from, to, expected) in jlstz_transition_fixtures() {
            assert_eq!(
                SrsKicks::offsets(piece, from, to).offsets(),
                expected,
                "{piece:?} {from:?}->{to:?}"
            );
        }
    }
}

#[test]
fn i_piece_uses_its_own_srs_90_table_for_all_eight_transitions() {
    for (from, to, expected) in i_transition_fixtures() {
        assert_eq!(
            SrsKicks::offsets(PieceKind::I, from, to).offsets(),
            expected,
            "I {from:?}->{to:?}"
        );
    }
}

#[test]
fn o_piece_is_no_kick_in_clearra_internal_srs_90_model() {
    for (from, to, _) in jlstz_transition_fixtures() {
        assert_eq!(
            SrsKicks::offsets(PieceKind::O, from, to).offsets(),
            &[KickOffset::new(0, 0)]
        );
    }
}

#[test]
fn srs_profile_exposes_transition_sequences_for_all_standard_pieces() {
    let profile = SrsKicks::profile();

    assert_eq!(profile.id(), KickTableProfileId::Srs90);
    assert_eq!(profile.source_rule(), RuleProfileId::Srs);
    assert_eq!(
        profile.entries().len(),
        PieceKind::STANDARD_TETROMINOES.len() * eight_direction_transitions().len()
    );
    assert_eq!(
        profile
            .sequence_for(KickTransition::new(
                PieceKind::T,
                RotationState::Zero,
                RotationState::Right
            ))
            .expect("T 0->R")
            .offsets(),
        offsets([(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)])
    );
}

#[test]
fn srs_plus_profile_preserves_symmetric_i_and_transition_specific_180_kicks() {
    let profile = SrsKicks::srs_plus_profile();

    assert_eq!(profile.id(), KickTableProfileId::SrsPlus);
    assert_eq!(profile.source_rule(), RuleProfileId::SrsPlus);
    assert!(profile.supports_180());
    assert_eq!(
        profile
            .sequence_for(KickTransition::new(
                PieceKind::I,
                RotationState::Zero,
                RotationState::Right
            ))
            .expect("I 0->R")
            .offsets(),
        offsets([(0, 0), (1, 0), (-2, 0), (-2, -1), (1, 2)])
    );
    assert_eq!(
        profile
            .sequence_for(KickTransition::new(
                PieceKind::T,
                RotationState::Zero,
                RotationState::Two
            ))
            .expect("T 0->2")
            .offsets(),
        offsets([(0, 0), (0, 1), (1, 1), (-1, 1), (1, 0), (-1, 0)])
    );
    assert_eq!(
        profile
            .sequence_for(KickTransition::new(
                PieceKind::I,
                RotationState::Zero,
                RotationState::Left
            ))
            .expect("I 0->L")
            .offsets(),
        offsets([(0, 0), (-1, 0), (2, 0), (2, -1), (-1, 2)])
    );
    assert_eq!(
        profile
            .sequence_for(KickTransition::new(
                PieceKind::T,
                RotationState::Right,
                RotationState::Left
            ))
            .expect("T R->L")
            .offsets(),
        offsets([(0, 0), (1, 0), (1, 2), (1, 1), (0, 2), (0, 1)])
    );
    assert_eq!(
        profile
            .sequence_for(KickTransition::new(
                PieceKind::I,
                RotationState::Zero,
                RotationState::Two
            ))
            .expect("I 0->2")
            .offsets(),
        offsets([(0, 0), (0, 1)])
    );
    assert!(profile
        .sequence_for(KickTransition::new(
            PieceKind::O,
            RotationState::Zero,
            RotationState::Two
        ))
        .is_none());
}

#[test]
fn srs_plus_matches_the_tetrio_source_fixture_with_redundant_o_half_turns_elided() {
    let profile = SrsKicks::srs_plus_profile();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/rules/tetrio_srs_plus_standard_tetromino_kicks.json"
    ))
    .expect("TETR.IO SRS+ fixture");
    let normalized = fixture["transitions_y_up"]
        .as_object()
        .expect("84 transitions");

    assert_eq!(fixture["authority"], "https://tetr.io/js/tetrio.js");
    assert_eq!(fixture["implicit_origin_attempt"], true);
    assert_eq!(fixture["standard_o"]["origin_rotation_allowed"], true);
    assert_eq!(fixture["standard_o"]["uses_oo_kicks"], false);
    assert_eq!(profile.transition_count(), 80);
    assert_eq!(normalized.len(), 84);
    assert_eq!(profile.duplicate_transitions(), []);

    for piece in PieceKind::STANDARD_TETROMINOES {
        for (from, to) in twelve_direction_transitions() {
            let source_transition = format!("{}{}", from.quarter_turns(), to.quarter_turns());
            let key = format!("{piece:?}:{source_transition}");
            let expected = normalized[&key]
                .as_array()
                .expect("ordered offsets")
                .iter()
                .map(|pair| {
                    KickOffset::new(
                        i8::try_from(pair[0].as_i64().expect("dx")).expect("i8 dx"),
                        i8::try_from(pair[1].as_i64().expect("dy")).expect("i8 dy"),
                    )
                })
                .collect::<Vec<_>>();
            let family = if piece == PieceKind::I { "i" } else { "jlstz" };
            let source_expected = if piece == PieceKind::O {
                offsets([(0, 0)])
            } else {
                tetrio_source_offsets_y_up(&fixture["families"][family][&source_transition])
            };
            assert_eq!(expected, source_expected, "source-axis conversion {key}");
            if piece == PieceKind::O && to == from.rotated_180() {
                assert!(profile
                    .sequence_for(KickTransition::new(piece, from, to))
                    .is_none());
                let middle = from.clockwise();
                // O's official half-turn is a redundant edge: the two
                // quarter-turn intermediates have exactly the same cells.
                for (a, b) in [(from, middle), (middle, to)] {
                    assert_eq!(
                        profile
                            .sequence_for(KickTransition::new(piece, a, b))
                            .expect("O quarter-turn substitute")
                            .offsets(),
                        expected
                    );
                    let shape = |rotation: RotationState| {
                        let mut cells = fixture["standard_geometry_y_down"]["O"]["cells"]
                            [usize::from(rotation.quarter_turns())]
                        .as_array()
                        .expect("O geometry")
                        .iter()
                        .map(|cell| (cell[0].as_i64().unwrap(), cell[1].as_i64().unwrap()))
                        .collect::<Vec<_>>();
                        cells.sort_unstable();
                        cells
                    };
                    assert_eq!(shape(a), shape(b), "O occupied-cell equivalence");
                }
                continue;
            }
            assert_eq!(
                profile
                    .sequence_for(KickTransition::new(piece, from, to))
                    .expect("complete SRS+ transition")
                    .offsets(),
                expected,
                "TETR.IO SRS+ {key}"
            );
        }
    }
}

#[test]
fn srs_plus_j_and_l_preserve_tetrio_ordered_180_candidates() {
    use RotationState::{Left, Right, Two, Zero};

    let profile = SrsKicks::srs_plus_profile();
    // TETR.IO uses one JLSTZ table. The first successful candidate wins, so
    // changing this order can make a mirrored J/L placement reach a new field.
    for piece in [PieceKind::J, PieceKind::L] {
        for (from, to, expected) in [
            (
                Zero,
                Two,
                offsets([(0, 0), (0, 1), (1, 1), (-1, 1), (1, 0), (-1, 0)]),
            ),
            (
                Two,
                Zero,
                offsets([(0, 0), (0, -1), (-1, -1), (1, -1), (-1, 0), (1, 0)]),
            ),
            (
                Right,
                Left,
                offsets([(0, 0), (1, 0), (1, 2), (1, 1), (0, 2), (0, 1)]),
            ),
            (
                Left,
                Right,
                offsets([(0, 0), (-1, 0), (-1, 2), (-1, 1), (0, 2), (0, 1)]),
            ),
        ] {
            assert_eq!(
                profile
                    .sequence_for(KickTransition::new(piece, from, to))
                    .expect("J/L SRS+ 180 transition")
                    .offsets(),
                expected.as_slice(),
                "{piece:?} {from:?}->{to:?}"
            );
        }
    }
}

#[test]
fn srs_x_matches_the_tetrio_standard_tetromino_fixture_for_all_84_transitions() {
    let profile = SrsKicks::srs_x_profile();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/rules/tetrio_srs_x_standard_tetromino_kicks.json"
    ))
    .expect("TETR.IO SRS-X fixture");

    assert_eq!(profile.id(), KickTableProfileId::SrsX);
    assert_eq!(profile.source_rule(), RuleProfileId::SrsX);
    assert_eq!(fixture["authority"], "https://tetr.io/js/tetrio.js");
    assert_eq!(fixture["implicit_origin_attempt"], true);
    assert_eq!(fixture["standard_o"]["disallow_kick"], true);
    assert_eq!(fixture["standard_o"]["uses_oo_kicks"], false);
    assert_eq!(fixture["oo_kicks_scope"]["standard_tetromino"], false);
    assert_eq!(profile.transition_count(), 84);
    assert_eq!(profile.duplicate_transitions(), []);

    for piece in PieceKind::STANDARD_TETROMINOES {
        for source_transition in [
            "01", "12", "23", "30", "03", "32", "21", "10", "02", "13", "20", "31",
        ] {
            let bytes = source_transition.as_bytes();
            let from = source_rotation(bytes[0]);
            let to = source_rotation(bytes[1]);
            let expected = if piece == PieceKind::O {
                vec![KickOffset::new(0, 0)]
            } else {
                let family = if piece == PieceKind::I { "i" } else { "jlstz" };
                tetrio_source_offsets_y_up(&fixture["families"][family][source_transition])
            };

            assert_eq!(
                profile
                    .sequence_for(KickTransition::new(piece, from, to))
                    .expect("complete SRS-X transition")
                    .offsets(),
                expected,
                "TETR.IO SRS-X {piece:?} {source_transition}"
            );
        }
    }

    assert_eq!(
        profile
            .sequence_for(KickTransition::new(
                PieceKind::I,
                RotationState::Zero,
                RotationState::Two
            ))
            .expect("SRS-X I 0->2")
            .offsets(),
        offsets([(0, 0), (-1, 0), (-2, 0), (1, 0), (2, 0), (0, -1)])
    );
    assert_eq!(
        profile
            .entries()
            .iter()
            .map(|entry| entry.sequence().len())
            .max(),
        Some(12)
    );
}

#[test]
fn jstris_180_uses_srs_quarter_turns_and_two_ordered_half_turn_kicks() {
    use RotationState::{Left, Right, Two, Zero};

    let profile = SrsKicks::jstris_180_profile();

    assert_eq!(profile.id(), KickTableProfileId::Jstris180);
    assert_eq!(profile.source_rule(), RuleProfileId::Jstris180);
    assert_eq!(profile.transition_count(), 72);
    assert_eq!(
        profile
            .sequence_for(KickTransition::new(PieceKind::I, Zero, Right))
            .expect("Jstris I 0->R")
            .offsets(),
        offsets([(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)])
    );

    for piece in [
        PieceKind::I,
        PieceKind::J,
        PieceKind::L,
        PieceKind::S,
        PieceKind::T,
        PieceKind::Z,
    ] {
        for (from, to, expected) in [
            (Zero, Two, offsets([(0, 0), (0, 1)])),
            (Right, Left, offsets([(0, 0), (1, 0)])),
            (Two, Zero, offsets([(0, 0), (0, -1)])),
            (Left, Right, offsets([(0, 0), (-1, 0)])),
        ] {
            assert_eq!(
                profile
                    .sequence_for(KickTransition::new(piece, from, to))
                    .expect("Jstris half turn")
                    .offsets(),
                expected,
                "{piece:?} {from:?}->{to:?}"
            );
        }
    }

    assert!(profile
        .entries()
        .iter()
        .all(|entry| entry.transition().piece() != PieceKind::O));
}

fn jlstz_transition_fixtures() -> Vec<(RotationState, RotationState, Vec<KickOffset>)> {
    use RotationState::{Left, Right, Two, Zero};

    vec![
        (
            Zero,
            Right,
            offsets([(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)]),
        ),
        (
            Right,
            Two,
            offsets([(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)]),
        ),
        (
            Two,
            Left,
            offsets([(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)]),
        ),
        (
            Left,
            Zero,
            offsets([(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)]),
        ),
        (
            Zero,
            Left,
            offsets([(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)]),
        ),
        (
            Left,
            Two,
            offsets([(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)]),
        ),
        (
            Two,
            Right,
            offsets([(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)]),
        ),
        (
            Right,
            Zero,
            offsets([(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)]),
        ),
    ]
}

fn i_transition_fixtures() -> Vec<(RotationState, RotationState, Vec<KickOffset>)> {
    use RotationState::{Left, Right, Two, Zero};

    vec![
        (
            Zero,
            Right,
            offsets([(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)]),
        ),
        (
            Right,
            Two,
            offsets([(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)]),
        ),
        (
            Two,
            Left,
            offsets([(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)]),
        ),
        (
            Left,
            Zero,
            offsets([(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)]),
        ),
        (
            Zero,
            Left,
            offsets([(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)]),
        ),
        (
            Left,
            Two,
            offsets([(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)]),
        ),
        (
            Two,
            Right,
            offsets([(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)]),
        ),
        (
            Right,
            Zero,
            offsets([(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)]),
        ),
    ]
}

fn offsets<const N: usize>(values: [(i8, i8); N]) -> Vec<KickOffset> {
    values
        .into_iter()
        .map(|(dx, dy)| KickOffset::new(dx, dy))
        .collect()
}

fn source_rotation(value: u8) -> RotationState {
    match value {
        b'0' => RotationState::Zero,
        b'1' => RotationState::Right,
        b'2' => RotationState::Two,
        b'3' => RotationState::Left,
        _ => panic!("invalid TETR.IO rotation digit"),
    }
}

fn tetrio_source_offsets_y_up(value: &serde_json::Value) -> Vec<KickOffset> {
    let mut expected = vec![KickOffset::new(0, 0)];
    expected.extend(
        value
            .as_array()
            .expect("TETR.IO transition array")
            .iter()
            .map(|offset| {
                let offset = offset.as_array().expect("TETR.IO offset pair");
                let dx = offset[0].as_i64().expect("TETR.IO dx") as i8;
                let source_dy = offset[1].as_i64().expect("TETR.IO dy") as i8;
                KickOffset::new(dx, -source_dy)
            }),
    );
    expected
}
