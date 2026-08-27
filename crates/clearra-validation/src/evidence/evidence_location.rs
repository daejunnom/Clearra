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

    /// Returns only the heap payload retained by the path string, measured by
    /// `String` allocation capacity. The inline location is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(self.path.capacity() as u128)
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::EvidenceLocation;

    #[test]
    fn location_retained_capacity_counts_path_allocation_capacity() {
        let mut path = String::with_capacity(128);
        path.push_str("query.queue");
        let expected = path.capacity() as u128;
        let location = EvidenceLocation::with_index(path, 3);

        assert_eq!(location.checked_retained_capacity_bytes(), Some(expected));
    }
}
