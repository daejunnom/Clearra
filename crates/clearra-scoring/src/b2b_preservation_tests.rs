use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_replay::{RotationRequest, ScoringExecutionEdge, ScoringLockEvidence};

use super::{BackToBackPreservationPolicy, BeforeSpinEvidenceDecision};
use crate::{
    event::{clear_event::ClearEvent, score_event::ScoreEvent, SpinDetector},
    profile::{AllSpinScoreMapping, B2BChainRule, SpinAwardPolicy, SpinProfile, SpinProfileId},
    state::combo_state::ComboState,
};

const PROFILES: [SpinProfileId; 8] = [
    SpinProfileId::Disabled,
    SpinProfileId::TSpinSimple,
    SpinProfileId::TSpins,
    SpinProfileId::TSpinsPlus,
    SpinProfileId::AllSpin,
    SpinProfileId::AllSpinPlus,
    SpinProfileId::AllMini,
    SpinProfileId::AllMiniPlus,
];

#[test]
fn before_spin_evidence_matches_profile_domains_and_clear_exceptions() {
    use BeforeSpinEvidenceDecision::{Preserves, Rejects, RequiresSpinEvidence};

    for id in PROFILES {
        let policy = BackToBackPreservationPolicy::new(SpinProfile::builtin(id));
        for piece in PieceKind::STANDARD_TETROMINOES {
            // Cover the complete public u8 input domain, including values no
            // physical tetromino lock can generate. The preclassification
            // must still preserve the original predicate's exact semantics.
            for lines in 0..=u8::MAX {
                for perfect_clear in [false, true] {
                    let expected = if lines == 0 || lines == 4 || perfect_clear {
                        Preserves
                    } else {
                        match (id, piece) {
                            (SpinProfileId::Disabled, _) => Rejects,
                            (SpinProfileId::TSpinSimple, PieceKind::T) => Preserves,
                            (SpinProfileId::TSpinSimple, _)
                            | (
                                SpinProfileId::TSpins,
                                PieceKind::I
                                | PieceKind::O
                                | PieceKind::S
                                | PieceKind::Z
                                | PieceKind::J
                                | PieceKind::L,
                            )
                            | (
                                SpinProfileId::TSpinsPlus,
                                PieceKind::I
                                | PieceKind::O
                                | PieceKind::S
                                | PieceKind::Z
                                | PieceKind::J
                                | PieceKind::L,
                            ) => Rejects,
                            _ => RequiresSpinEvidence,
                        }
                    };
                    assert_eq!(
                        policy.before_spin_evidence(piece, lines, perfect_clear),
                        expected,
                        "{id:?}, {piece:?}, lines={lines}, perfect_clear={perfect_clear}"
                    );
                }
            }
        }
    }
}

#[test]
fn before_spin_evidence_is_sound_for_all_builtin_profiles_and_evidence_classes() {
    for id in PROFILES {
        let profile = SpinProfile::builtin(id);
        let policy = BackToBackPreservationPolicy::new(profile);
        for piece in PieceKind::STANDARD_TETROMINOES {
            for lines in 0..=4 {
                for perfect_clear in [false, true] {
                    let decision = policy.before_spin_evidence(piece, lines, perfect_clear);
                    for corners in 0..=4 {
                        for front_corners in 0..=2 {
                            for immobile in [false, true] {
                                let no_rotation =
                                    ScoringLockEvidence::no_rotation(RotationState::Zero)
                                        .with_immobile_before_clear(immobile);
                                assert_decision_matches_original(
                                    profile,
                                    decision,
                                    edge(
                                        piece,
                                        lines,
                                        perfect_clear,
                                        corners,
                                        front_corners,
                                        no_rotation,
                                    ),
                                );
                                for request in [
                                    RotationRequest::Clockwise,
                                    RotationRequest::CounterClockwise,
                                    RotationRequest::HalfTurn,
                                ] {
                                    for kick_index in [0, 4] {
                                        let rotation = ScoringLockEvidence::rotation(
                                            RotationState::Zero,
                                            request,
                                            kick_index,
                                            0,
                                            0,
                                            0,
                                            0,
                                        )
                                        .with_immobile_before_clear(immobile);
                                        assert_decision_matches_original(
                                            profile,
                                            decision,
                                            edge(
                                                piece,
                                                lines,
                                                perfect_clear,
                                                corners,
                                                front_corners,
                                                rotation,
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn before_spin_evidence_keeps_recognition_separate_from_award_mapping() {
    for id in PROFILES {
        let original = SpinProfile::builtin(id);
        for award in [
            SpinAwardPolicy::Disabled,
            SpinAwardPolicy::TSpinsOnly,
            SpinAwardPolicy::AllSpins,
            SpinAwardPolicy::AllMini,
            SpinAwardPolicy::AllSpinAsTSpinMini,
        ] {
            for mapping in [
                AllSpinScoreMapping::Disabled,
                AllSpinScoreMapping::NativeAllSpinTable,
                AllSpinScoreMapping::UseTSpinMiniTable,
            ] {
                let profile = original
                    .with_award_policy(award)
                    .with_all_spin_score_mapping(mapping);
                let policy = BackToBackPreservationPolicy::new(profile);
                for piece in PieceKind::STANDARD_TETROMINOES {
                    let decision = policy.before_spin_evidence(piece, 2, false);
                    assert_eq!(
                        decision,
                        BackToBackPreservationPolicy::new(original)
                            .before_spin_evidence(piece, 2, false)
                    );
                    for evidence in [
                        ScoringLockEvidence::no_rotation(RotationState::Zero),
                        ScoringLockEvidence::rotation(
                            RotationState::Zero,
                            RotationRequest::Clockwise,
                            4,
                            0,
                            0,
                            0,
                            0,
                        )
                        .with_immobile_before_clear(true),
                    ] {
                        assert_decision_matches_original(
                            profile,
                            decision,
                            edge(piece, 2, false, 3, 1, evidence),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn before_spin_evidence_keeps_simple_t_and_perfect_clear_save_semantics() {
    let no_rotation = ScoringLockEvidence::no_rotation(RotationState::Zero);
    let simple =
        BackToBackPreservationPolicy::new(SpinProfile::builtin(SpinProfileId::TSpinSimple));
    assert_eq!(
        simple.before_spin_evidence(PieceKind::T, 1, false),
        BeforeSpinEvidenceDecision::Preserves
    );
    let simple_edge = edge(PieceKind::T, 1, false, 0, 0, no_rotation);
    assert!(simple.allows(simple_edge));
    // A Boolean save decision does not loosen an existing consumer's
    // separate requirement for spin-labelled trace or finesse evidence.
    assert!(BackToBackPreservationPolicy::requires_recognized_spin(
        simple_edge
    ));

    let exact = BackToBackPreservationPolicy::new(SpinProfile::builtin(SpinProfileId::TSpins));
    assert_eq!(
        exact.before_spin_evidence(PieceKind::T, 1, false),
        BeforeSpinEvidenceDecision::RequiresSpinEvidence
    );
    assert!(!exact.allows(edge(PieceKind::T, 1, false, 0, 0, no_rotation)));

    for (lines, perfect_clear, decision, numeric_chain) in [
        (0, false, BeforeSpinEvidenceDecision::Preserves, 3),
        (2, false, BeforeSpinEvidenceDecision::Rejects, 0),
        (2, true, BeforeSpinEvidenceDecision::Preserves, 0),
        (4, false, BeforeSpinEvidenceDecision::Preserves, 4),
    ] {
        assert_eq!(
            exact.before_spin_evidence(PieceKind::O, lines, perfect_clear),
            decision
        );
        let event = ScoreEvent::from_classified_clear_with_b2b_chain(
            0,
            ClearEvent::new(lines, perfect_clear),
            None,
            ComboState::default(),
            3,
            B2BChainRule::UnderlyingDifficultClearOnly,
        );
        assert_eq!(event.b2b_after_chain(), numeric_chain);
    }
}

fn assert_decision_matches_original(
    profile: SpinProfile,
    decision: BeforeSpinEvidenceDecision,
    edge: ScoringExecutionEdge,
) {
    // Independent of the new preclassification: this is the complete old
    // edge predicate, including its distinct perfect-clear exception.
    let original = edge.cleared_lines() == 0
        || edge.cleared_lines() == 4
        || edge.perfect_clear()
        || SpinDetector::detect_scoring_edge_with_profile(edge, profile).is_some();
    match decision {
        BeforeSpinEvidenceDecision::Preserves => assert!(original, "{profile:?}: {edge:?}"),
        BeforeSpinEvidenceDecision::Rejects => assert!(!original, "{profile:?}: {edge:?}"),
        BeforeSpinEvidenceDecision::RequiresSpinEvidence => {}
    }
    assert_eq!(
        BackToBackPreservationPolicy::new(profile).allows(edge),
        original
    );
}

fn edge(
    piece: PieceKind,
    lines: u8,
    perfect_clear: bool,
    corners: u8,
    front_corners: u8,
    evidence: ScoringLockEvidence,
) -> ScoringExecutionEdge {
    ScoringExecutionEdge::new(
        0,
        0,
        piece,
        RotationState::Zero,
        0,
        0,
        lines,
        corners,
        front_corners,
        evidence,
    )
    .with_perfect_clear(perfect_clear)
}
