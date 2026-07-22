#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedSpecialSpinId(String);

impl ImportedSpecialSpinId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSpecialSpinId(String);

impl CustomSpecialSpinId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpecialSpinCaseId {
    Fin,
    Iso,
    Neo,
    ImportedSpecialSpin(ImportedSpecialSpinId),
    CustomSpecialSpin(CustomSpecialSpinId),
}

impl SpecialSpinCaseId {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Fin => "fin",
            Self::Iso => "iso",
            Self::Neo => "neo",
            Self::ImportedSpecialSpin(_) => "imported-special-spin",
            Self::CustomSpecialSpin(_) => "custom-special-spin",
        }
    }
}
