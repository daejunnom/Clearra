#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VerifyArgs {
    target: Option<String>,
}

impl VerifyArgs {
    pub fn new(target: Option<String>) -> Self {
        Self { target }
    }
}
impl VerifyArgs {
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }
}
