use std::collections::BTreeSet;

use super::{
    normalized_fumen_page::NormalizedFumenPage, normalized_solution_key::NormalizedSolutionKey,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedFumenDocument {
    pages: Vec<NormalizedFumenPage>,
    solution_keys: BTreeSet<NormalizedSolutionKey>,
}

impl NormalizedFumenDocument {
    pub fn new(pages: Vec<NormalizedFumenPage>) -> Self {
        let solution_keys = pages
            .iter()
            .filter_map(NormalizedFumenPage::solution_key)
            .collect();
        Self {
            pages,
            solution_keys,
        }
    }
}
impl NormalizedFumenDocument {
    pub fn pages(&self) -> &[NormalizedFumenPage] {
        &self.pages
    }
}
impl NormalizedFumenDocument {
    pub fn solution_keys(&self) -> &BTreeSet<NormalizedSolutionKey> {
        &self.solution_keys
    }
}
impl NormalizedFumenDocument {
    pub fn solution_key_count(&self) -> usize {
        self.solution_keys.len()
    }
}
