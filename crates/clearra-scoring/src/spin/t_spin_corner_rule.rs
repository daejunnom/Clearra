use crate::profile::ScoreProfile;

use super::{
    spin_accuracy::SpinAccuracy,
    spin_classification::{ClassificationConfidence, SpinClassification},
    spin_classification_input::{RotationRequest, SpinClassificationInput},
    spin_classifier::SpinClassifier,
    spin_result::{SpinKind, SpinResult},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TSpinCornerRule;

impl SpinClassifier for TSpinCornerRule {
    fn classify(
        &self,
        input: SpinClassificationInput,
        _profile: &ScoreProfile,
    ) -> SpinClassification {
        if input.piece != 'T' {
            return SpinClassification::none(
                input.piece,
                input.cleared_lines,
                SpinAccuracy::PlacementOnlyEstimate,
            );
        }

        if !input.trace_completeness.is_full() || !input.movement_info.evidence_complete {
            return SpinClassification::none(
                input.piece,
                input.cleared_lines,
                SpinAccuracy::Incomplete,
            );
        }
        let final_kick_override = input.kick_evidence.as_ref().is_some_and(|evidence| {
            fifth_kick_test_regular_override(
                evidence.first_success_confirmed,
                evidence.kick_index,
                matches!(
                    evidence.rotation_request,
                    RotationRequest::Clockwise | RotationRequest::CounterClockwise
                ),
            )
        });
        let Some(mini) = exact_t_spin_mini(
            input.piece,
            input.movement_info.rotation_used,
            input.blocked_corners,
            input.front_corners,
            final_kick_override,
        ) else {
            return SpinClassification::none(input.piece, input.cleared_lines, SpinAccuracy::Exact);
        };

        SpinClassification::new(
            SpinResult::new(
                'T',
                if mini {
                    SpinKind::TSpinMini
                } else {
                    SpinKind::TSpin
                },
                mini,
                input.cleared_lines,
                input.has_kick_evidence(),
                SpinAccuracy::Exact,
            ),
            ClassificationConfidence::exact(),
        )
    }
}

pub(crate) const fn fifth_kick_test_regular_override(
    first_success_confirmed: bool,
    kick_index: u8,
    quarter_turn_requested: bool,
) -> bool {
    first_success_confirmed && kick_index == 4 && quarter_turn_requested
}

pub(crate) const fn exact_t_spin_mini(
    piece: char,
    rotation_used: bool,
    blocked_corners: u8,
    front_corners: u8,
    final_kick_override: bool,
) -> Option<bool> {
    if piece != 'T' || !rotation_used || blocked_corners < 3 {
        None
    } else {
        Some(front_corners < 2 && !final_kick_override)
    }
}
