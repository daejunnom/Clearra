use crate::{evidence::SpinEvidence, special::SpecialSpinCaseId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpinInterpretation {
    NormalPlacement,
    MiniTSpin,
    RegularTSpin,
    AllSpin,
    AllMini,
    SpecialSpin(SpecialSpinCaseId),
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinInterpretationSet {
    pub interpretations: Vec<SpinInterpretation>,
    pub evidence: SpinEvidence,
}

impl SpinInterpretationSet {
    pub fn new(
        interpretations: impl IntoIterator<Item = SpinInterpretation>,
        evidence: SpinEvidence,
    ) -> Self {
        Self {
            interpretations: interpretations.into_iter().collect(),
            evidence,
        }
    }
}
impl SpinInterpretationSet {
    pub fn contains_unknown(&self) -> bool {
        self.interpretations
            .iter()
            .any(|interpretation| matches!(interpretation, SpinInterpretation::Unknown))
    }
}
impl SpinInterpretationSet {
    pub fn contains_normal_and_special(&self) -> bool {
        self.interpretations
            .iter()
            .any(|interpretation| matches!(interpretation, SpinInterpretation::NormalPlacement))
            && self
                .interpretations
                .iter()
                .any(|interpretation| matches!(interpretation, SpinInterpretation::SpecialSpin(_)))
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use crate::{
        evidence::{LastActionEvidence, SpinEvidence},
        resolution::{SpinInterpretation, SpinInterpretationSet},
        special::SpecialSpinCaseId,
    };

    #[test]
    fn normal_placement_and_special_spin_can_both_be_preserved() {
        let evidence = SpinEvidence::new(LastActionEvidence::new(PieceKind::T, true));
        let set = SpinInterpretationSet::new(
            [
                SpinInterpretation::NormalPlacement,
                SpinInterpretation::SpecialSpin(SpecialSpinCaseId::Fin),
            ],
            evidence,
        );

        assert!(set.contains_normal_and_special());
    }
}
