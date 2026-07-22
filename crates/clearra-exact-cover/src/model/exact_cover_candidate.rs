#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactCoverCandidate {
    id: usize,
    columns: Vec<usize>,
}

impl ExactCoverCandidate {
    pub fn new(id: usize, columns: Vec<usize>) -> Self {
        Self { id, columns }
    }
}
impl ExactCoverCandidate {
    pub fn id(&self) -> usize {
        self.id
    }
}
impl ExactCoverCandidate {
    pub fn columns(&self) -> &[usize] {
        &self.columns
    }
}
