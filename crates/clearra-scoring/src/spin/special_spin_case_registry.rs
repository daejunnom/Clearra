use std::collections::BTreeMap;

use super::{special_spin_case::SpecialSpinCase, special_spin_case_id::SpecialSpinCaseId};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpecialSpinCaseRegistry {
    cases: BTreeMap<SpecialSpinCaseId, SpecialSpinCase>,
}

impl SpecialSpinCaseRegistry {
    pub fn new(cases: impl IntoIterator<Item = SpecialSpinCase>) -> Self {
        let cases = cases
            .into_iter()
            .map(|case| (case.id().clone(), case))
            .collect();
        Self { cases }
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
    pub fn get(&self, id: &SpecialSpinCaseId) -> Option<&SpecialSpinCase> {
        self.cases.get(id)
    }
}
impl SpecialSpinCaseRegistry {
    pub fn cases_for_piece(&self, piece: char) -> impl Iterator<Item = &SpecialSpinCase> {
        let piece = piece.to_ascii_uppercase();
        self.cases
            .values()
            .filter(move |case| case.piece() == piece)
    }
}
impl SpecialSpinCaseRegistry {
    pub fn len(&self) -> usize {
        self.cases.len()
    }
}
impl SpecialSpinCaseRegistry {
    pub fn is_empty(&self) -> bool {
        self.cases.is_empty()
    }
}
