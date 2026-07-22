#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SpecialSpinCaseId {
    Fin,
    Iso,
    Neo,
    ImportedSpecialSpin(String),
    CustomSpecialSpin(String),
}

impl SpecialSpinCaseId {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Fin => "fin",
            Self::Iso => "iso",
            Self::Neo => "neo",
            Self::ImportedSpecialSpin(id) | Self::CustomSpecialSpin(id) => id.as_str(),
        }
    }
}
impl SpecialSpinCaseId {
    pub fn profile_specific_kind_id(&self) -> &'static str {
        match self {
            Self::Fin => "fin",
            Self::Iso => "iso",
            Self::Neo => "neo",
            Self::ImportedSpecialSpin(_) => "imported-special-spin",
            Self::CustomSpecialSpin(_) => "custom-special-spin",
        }
    }
}
