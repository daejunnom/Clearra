use crate::profile::ScoreProfile;

use super::{
    special_spin_case_registry::SpecialSpinCaseRegistry,
    spin_accuracy::SpinAccuracy,
    spin_classification::{ClassificationConfidence, SpinClassification},
    spin_classification_input::{RotationRequest, SpinClassificationInput},
    spin_classifier::SpinClassifier,
    spin_result::{SpinKind, SpinResult},
    t_spin_corner_rule::{exact_t_spin_mini, fifth_kick_test_regular_override},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KickSensitiveSpinRule<'a> {
    special_case_registry: Option<&'a SpecialSpinCaseRegistry>,
}

impl<'a> KickSensitiveSpinRule<'a> {
    pub fn new(special_case_registry: &'a SpecialSpinCaseRegistry) -> Self {
        Self {
            special_case_registry: Some(special_case_registry),
        }
    }
}
impl<'a> KickSensitiveSpinRule<'a> {
    pub fn without_special_cases() -> Self {
        Self {
            special_case_registry: None,
        }
    }
}

impl SpinClassifier for KickSensitiveSpinRule<'_> {
    fn classify(
        &self,
        input: SpinClassificationInput,
        profile: &ScoreProfile,
    ) -> SpinClassification {
        if input.cleared_lines == 0 {
            return SpinClassification::none(input.piece, 0, SpinAccuracy::Incomplete);
        }
        let Some(kick_evidence) = input
            .kick_evidence
            .as_ref()
            .filter(|_| input.has_kick_evidence())
        else {
            return SpinClassification::new(
                SpinResult::none(
                    input.piece,
                    input.cleared_lines,
                    SpinAccuracy::KickSensitiveUnavailable,
                ),
                ClassificationConfidence::new(0.0),
            );
        };

        if let Some(registry) = self.special_case_registry {
            for special_case in registry.cases_for_piece(input.piece) {
                if !special_case.exact_enabled(true) {
                    continue;
                }
                if !special_case.allowed_for_profile(profile.id()) {
                    continue;
                }
                if !special_case.required_kick_signature_matches(kick_evidence) {
                    continue;
                }
                if !special_case.board_signature_matches(&input) {
                    continue;
                }

                return special_case.classify(&input);
            }
        }

        fallback_to_corner_or_immobility_rule(input)
    }
}

fn fallback_to_corner_or_immobility_rule(input: SpinClassificationInput) -> SpinClassification {
    let t_spin_mini = input.kick_evidence.as_ref().and_then(|evidence| {
        exact_t_spin_mini(
            input.piece,
            input.movement_info.rotation_used || input.has_kick_evidence(),
            input.blocked_corners,
            input.front_corners,
            fifth_kick_test_regular_override(
                evidence.first_success_confirmed,
                evidence.kick_index,
                matches!(
                    evidence.rotation_request,
                    RotationRequest::Clockwise | RotationRequest::CounterClockwise
                ),
            ),
        )
    });
    if let Some(mini) = t_spin_mini {
        SpinClassification::new(
            SpinResult::new(
                input.piece,
                if mini {
                    SpinKind::TSpinMini
                } else {
                    SpinKind::TSpin
                },
                mini,
                input.cleared_lines,
                true,
                SpinAccuracy::Exact,
            ),
            ClassificationConfidence::exact(),
        )
    } else if input.piece != 'T'
        && input.movement_info.immobile
        && input.movement_info.rotation_used
    {
        SpinClassification::new(
            SpinResult::new(
                input.piece,
                SpinKind::AllSpin,
                false,
                input.cleared_lines,
                true,
                SpinAccuracy::Exact,
            ),
            ClassificationConfidence::exact(),
        )
    } else {
        SpinClassification::new(
            SpinResult::none(input.piece, input.cleared_lines, SpinAccuracy::Incomplete),
            ClassificationConfidence::new(0.0),
        )
    }
}
