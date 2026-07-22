use std::collections::HashMap;

use super::two_line_suffix_key::TwoLineSuffixKey;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TwoLineSuffixCache {
    entries: HashMap<TwoLineSuffixKey, bool>,
}

impl TwoLineSuffixCache {
    pub fn insert(&mut self, key: TwoLineSuffixKey, solvable: bool) {
        self.entries.insert(key, solvable);
    }
}
impl TwoLineSuffixCache {
    pub fn get(&self, key: &TwoLineSuffixKey) -> Option<bool> {
        self.entries.get(key).copied()
    }
}
impl TwoLineSuffixCache {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
impl TwoLineSuffixCache {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
