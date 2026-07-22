use super::*;

#[test]
fn kick_transition_preserves_piece_and_rotation_direction() {
    let transition = KickTransition::new(PieceKind::T, RotationState::Zero, RotationState::Right);

    assert_eq!(transition.piece(), PieceKind::T);
    assert_eq!(transition.from(), RotationState::Zero);
    assert_eq!(transition.to(), RotationState::Right);
}

#[test]
fn kick_transition_reports_180_rotation_requests() {
    let transition = KickTransition::new(PieceKind::T, RotationState::Zero, RotationState::Two);

    assert!(transition.is_180());
    assert!(transition.rotation_request().is_180());
}

#[test]
fn kick_table_profile_finds_offset_sequence_by_transition() {
    let transition = KickTransition::new(PieceKind::I, RotationState::Zero, RotationState::Right);
    let sequence = KickOffsetSequence::new(vec![KickOffset::new(0, 0), KickOffset::new(-2, 0)]);
    let profile = KickTableProfile::new(
        KickTableProfileId::Srs90,
        RuleProfileId::Srs,
        vec![KickTableEntry::new(transition, sequence.clone())],
    );

    assert_eq!(profile.id().as_str(), "srs-90");
    assert_eq!(profile.source_rule(), RuleProfileId::Srs);
    assert_eq!(profile.sequence_for(transition), Some(&sequence));
    assert_eq!(
        profile.sequence_for(KickTransition::new(
            PieceKind::O,
            RotationState::Zero,
            RotationState::Right
        )),
        None
    );
}

#[test]
fn kick_table_wraps_offset_sequence_without_losing_order() {
    let sequence = KickOffsetSequence::new(vec![KickOffset::new(0, 0), KickOffset::new(1, -2)]);
    let table = KickTable::from_sequence(sequence.clone());

    assert_eq!(table.sequence(), &sequence);
    assert_eq!(table.offsets(), sequence.offsets());
}

#[test]
fn kick_table_profile_reports_180_support_and_duplicates() {
    let transition = KickTransition::new(PieceKind::T, RotationState::Zero, RotationState::Two);
    let sequence = KickOffsetSequence::new(vec![KickOffset::new(0, 0)]);
    let profile = KickTableProfile::new(
        KickTableProfileId::Imported,
        RuleProfileId::Custom,
        vec![
            KickTableEntry::new(transition, sequence.clone()),
            KickTableEntry::new(transition, sequence),
        ],
    );

    assert!(profile.supports_180());
    assert_eq!(profile.duplicate_transitions(), vec![transition]);
}

#[test]
fn fin_iso_neo_are_not_kick_table_profile_ids() {
    assert_eq!(KickTableProfileId::parse("fin"), None);
    assert_eq!(KickTableProfileId::parse("iso"), None);
    assert_eq!(KickTableProfileId::parse("neo"), None);
    assert_eq!(KickTableProfileId::parse("fin-special"), None);
    assert_eq!(KickTableProfileId::parse("iso-special"), None);
    assert_eq!(KickTableProfileId::parse("neo-special"), None);
}

#[test]
fn special_spin_case_is_not_kick_table_profile() {
    for id in [
        "fin",
        "iso",
        "neo",
        "fin-special",
        "iso-special",
        "neo-special",
    ] {
        assert_eq!(KickTableProfileId::parse(id), None);
    }
}
