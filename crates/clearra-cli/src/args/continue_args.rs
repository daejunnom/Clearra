#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContinueArgs {
    token: Option<String>,
}

impl ContinueArgs {
    pub fn new(token: Option<String>) -> Self {
        Self { token }
    }
}
impl ContinueArgs {
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }
}
