#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TwoLineTraceBridge {
    steps: Vec<String>,
}

impl TwoLineTraceBridge {
    pub fn new() -> Self {
        Self::default()
    }
}
impl TwoLineTraceBridge {
    pub fn push_step(&mut self, step: impl Into<String>) {
        self.steps.push(step.into());
    }
}
impl TwoLineTraceBridge {
    pub fn steps(&self) -> &[String] {
        &self.steps
    }
}
