use crate::profile::ScoreProfile;

use super::{
    spin_accuracy::SpinAccuracy,
    spin_classification::{ClassificationConfidence, SpinClassification},
    spin_classification_input::SpinClassificationInput,
    spin_classifier::SpinClassifier,
    spin_result::{SpinKind, SpinResult},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllMiniRule;

impl SpinClassifier for AllMiniRule {
    fn classify(
        &self,
        input: SpinClassificationInput,
        _profile: &ScoreProfile,
    ) -> SpinClassification {
        if !input.movement_info.rotation_used || !input.movement_info.immobile {
            return SpinClassification::none(
                input.piece,
                input.cleared_lines,
                SpinAccuracy::PlacementOnlyEstimate,
            );
        }

        SpinClassification::new(
            SpinResult::new(
                input.piece,
                if input.piece == 'T' {
                    SpinKind::TSpinMini
                } else {
                    SpinKind::AllSpinMini
                },
                true,
                input.cleared_lines,
                input.has_kick_evidence(),
                SpinAccuracy::PlacementOnlyEstimate,
            ),
            ClassificationConfidence::estimated(),
        )
    }
}
