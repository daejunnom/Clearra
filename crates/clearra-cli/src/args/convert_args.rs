#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConvertArgs {
    input: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

impl ConvertArgs {
    pub fn new(input: Option<String>, from: Option<String>, to: Option<String>) -> Self {
        Self { input, from, to }
    }
}
impl ConvertArgs {
    pub fn input(&self) -> Option<&str> {
        self.input.as_deref()
    }
}
impl ConvertArgs {
    pub fn from(&self) -> Option<&str> {
        self.from.as_deref()
    }
}
impl ConvertArgs {
    pub fn to(&self) -> Option<&str> {
        self.to.as_deref()
    }
}
