use super::slot_assignment::SlotAssignment;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentTrace {
    assignment: SlotAssignment,
    labels: Vec<String>,
}

impl AssignmentTrace {
    pub fn new(assignment: SlotAssignment) -> Self {
        Self {
            assignment,
            labels: Vec::new(),
        }
    }
}
impl AssignmentTrace {
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.labels.push(label.into());
        self
    }
}
impl AssignmentTrace {
    pub fn assignment(&self) -> &SlotAssignment {
        &self.assignment
    }
}
impl AssignmentTrace {
    pub fn labels(&self) -> &[String] {
        &self.labels
    }
}
