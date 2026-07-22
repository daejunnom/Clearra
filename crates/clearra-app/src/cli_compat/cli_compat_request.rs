#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliCompatRequest {
    args: Vec<String>,
}

impl CliCompatRequest {
    pub fn new<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}
impl CliCompatRequest {
    pub fn args(&self) -> &[String] {
        &self.args
    }
}
