#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceCountConstraint {
    required_count: usize,
}

impl PieceCountConstraint {
    pub fn new(required_count: usize) -> Self {
        Self { required_count }
    }
}
impl PieceCountConstraint {
    pub fn required_count(self) -> usize {
        self.required_count
    }
}
impl PieceCountConstraint {
    pub fn accepts(self, selected_count: usize) -> bool {
        selected_count == self.required_count
    }
}
