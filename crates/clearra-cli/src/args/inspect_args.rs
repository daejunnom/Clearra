#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InspectArgs {
    subject: Option<String>,
}

impl InspectArgs {
    pub fn new(subject: Option<String>) -> Self {
        Self { subject }
    }
}
impl InspectArgs {
    pub fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }
}
