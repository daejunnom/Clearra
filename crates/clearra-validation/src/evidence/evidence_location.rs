#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceLocation {
    path: String,
    index: Option<usize>,
}

impl EvidenceLocation {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            index: None,
        }
    }
}
impl EvidenceLocation {
    pub fn with_index(path: impl Into<String>, index: usize) -> Self {
        Self {
            path: path.into(),
            index: Some(index),
        }
    }
}
impl EvidenceLocation {
    pub fn path(&self) -> &str {
        &self.path
    }
}
impl EvidenceLocation {
    pub fn index(&self) -> Option<usize> {
        self.index
    }
}
