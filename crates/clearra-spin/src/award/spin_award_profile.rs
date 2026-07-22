use crate::resolution::SpinInterpretation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinAwardClass {
    None,
    Mini,
    Regular,
    AllSpin,
    AllMini,
    Special,
    Unknown,
}

impl SpinAwardClass {
    pub const ALL: [Self; 7] = [
        Self::None,
        Self::Mini,
        Self::Regular,
        Self::AllSpin,
        Self::AllMini,
        Self::Special,
        Self::Unknown,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinAwardProfileId(String);

impl SpinAwardProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl SpinAwardProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinAwardProfile {
    id: SpinAwardProfileId,
    all_spin_award: SpinAwardClass,
    all_mini_award: SpinAwardClass,
}

impl SpinAwardProfile {
    pub fn standard() -> Self {
        Self {
            id: SpinAwardProfileId::new("standard-spin-award"),
            all_spin_award: SpinAwardClass::AllSpin,
            all_mini_award: SpinAwardClass::AllMini,
        }
    }

    pub fn all_piece_as_t_spin_mini() -> Self {
        Self {
            id: SpinAwardProfileId::new("all-piece-as-t-spin-mini-award"),
            all_spin_award: SpinAwardClass::Mini,
            all_mini_award: SpinAwardClass::Mini,
        }
    }
}
impl SpinAwardProfile {
    pub fn id(&self) -> &SpinAwardProfileId {
        &self.id
    }
}
impl SpinAwardProfile {
    pub fn award_class(&self, interpretation: &SpinInterpretation) -> SpinAwardClass {
        match interpretation {
            SpinInterpretation::NormalPlacement => SpinAwardClass::None,
            SpinInterpretation::MiniTSpin => SpinAwardClass::Mini,
            SpinInterpretation::RegularTSpin => SpinAwardClass::Regular,
            SpinInterpretation::AllSpin => self.all_spin_award,
            SpinInterpretation::AllMini => self.all_mini_award,
            SpinInterpretation::SpecialSpin(_) => SpinAwardClass::Special,
            SpinInterpretation::Unknown => SpinAwardClass::Unknown,
        }
    }
}
