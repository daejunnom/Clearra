use crate::profile::ScoreProfile;

use super::{
    spin_accuracy::SpinAccuracy,
    spin_classification::{ClassificationConfidence, SpinClassification},
    spin_classification_input::SpinClassificationInput,
    spin_classifier::SpinClassifier,
    spin_result::{SpinKind, SpinResult},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllSpinRule;

impl SpinClassifier for AllSpinRule {
    fn classify(
        &self,
        input: SpinClassificationInput,
        _profile: &ScoreProfile,
    ) -> SpinClassification {
        if input.cleared_lines == 0 || !input.movement_info.rotation_used {
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
                    SpinKind::TSpin
                } else {
                    SpinKind::AllSpin
                },
                false,
                input.cleared_lines,
                input.has_kick_evidence(),
                SpinAccuracy::PlacementOnlyEstimate,
            ),
            ClassificationConfidence::estimated(),
        )
    }
}
