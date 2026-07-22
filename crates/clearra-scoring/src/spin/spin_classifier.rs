use crate::profile::ScoreProfile;

use super::{
    spin_classification::SpinClassification, spin_classification_input::SpinClassificationInput,
};

pub trait SpinClassifier {
    fn classify(
        &self,
        input: SpinClassificationInput,
        profile: &ScoreProfile,
    ) -> SpinClassification;
}
