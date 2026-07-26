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
fn srs_x_retains_the_extended_i_180_sequence() {
    let profile = SrsKicks::srs_x_profile();

    assert_eq!(profile.id(), KickTableProfileId::SrsX);
    assert_eq!(
        profile
            .sequence_for(KickTransition::new(
                PieceKind::I,
                RotationState::Zero,
                RotationState::Two
            ))
            .expect("SRS-X I 0->2")
            .offsets(),
        offsets([(0, 0), (0, 1), (1, 1), (-1, 1), (1, 0), (-1, 0)])
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
