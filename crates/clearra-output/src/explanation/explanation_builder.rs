#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExplanationBuilder {
    lines: Vec<String>,
}

impl ExplanationBuilder {
    pub fn push(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }
}
impl ExplanationBuilder {
    pub fn build(self) -> String {
        self.lines.join("\n")
    }
}
