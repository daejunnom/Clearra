use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::special::{SpecialSpinCase, SpecialSpinCaseId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialSpinCaseRegistryId(String);

impl SpecialSpinCaseRegistryId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl SpecialSpinCaseRegistryId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialSpinCaseRegistry {
    id: SpecialSpinCaseRegistryId,
    cases: Vec<SpecialSpinCase>,
}

impl SpecialSpinCaseRegistry {
    pub fn new(cases: impl IntoIterator<Item = SpecialSpinCase>) -> Self {
        Self {
            id: SpecialSpinCaseRegistryId::new("custom-special-spin-case-registry"),
            cases: cases.into_iter().collect(),
        }
    }
}
impl SpecialSpinCaseRegistry {
    pub fn with_builtin_descriptors() -> Self {
        Self::new([
            SpecialSpinCase::fin_descriptor(),
            SpecialSpinCase::iso_descriptor(),
            SpecialSpinCase::neo_descriptor(),
        ])
    }
}
impl SpecialSpinCaseRegistry {
    pub fn id(&self) -> &SpecialSpinCaseRegistryId {
        &self.id
    }
}
impl SpecialSpinCaseRegistry {
    pub fn get(&self, id: &SpecialSpinCaseId) -> Option<&SpecialSpinCase> {
        self.cases.iter().find(|case| case.id == *id)
    }
}
impl SpecialSpinCaseRegistry {
    pub fn cases_for_piece(&self, piece: PieceKind) -> impl Iterator<Item = &SpecialSpinCase> {
        self.cases.iter().filter(move |case| case.piece == piece)
    }
}

#[cfg(test)]
#[path = "special_spin_case_registry_tests.rs"]
mod tests;
