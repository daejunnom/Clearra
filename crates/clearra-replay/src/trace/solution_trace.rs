use crate::trace::placement_step::PlacementStep;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SolutionTrace {
    steps: Vec<PlacementStep>,
}

impl SolutionTrace {
    pub fn new(steps: Vec<PlacementStep>) -> Self {
        Self { steps }
    }
}
impl SolutionTrace {
    pub fn empty() -> Self {
        Self::default()
    }
}
impl SolutionTrace {
    pub fn push(&mut self, step: PlacementStep) {
        self.steps.push(step);
    }
}
impl SolutionTrace {
    pub fn pop(&mut self) -> Option<PlacementStep> {
        self.steps.pop()
    }
}
impl SolutionTrace {
    pub fn steps(&self) -> &[PlacementStep] {
        &self.steps
    }
}
impl SolutionTrace {
    pub fn len(&self) -> usize {
        self.steps.len()
    }
}
impl SolutionTrace {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}
