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

impl SolutionTrace {
    /// Heap storage retained by the step owner, excluding the inline trace.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.steps.capacity() as u128).checked_mul(core::mem::size_of::<PlacementStep>() as u128)
    }

    /// Heap storage requested by cloning the step owner.
    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        (self.steps.len() as u128).checked_mul(core::mem::size_of::<PlacementStep>() as u128)
    }
}

#[cfg(test)]
mod retained_memory_projection_tests {
    use super::*;

    #[test]
    fn solution_trace_projection_uses_capacity_and_clone_uses_length() {
        let steps = Vec::with_capacity(9);
        let capacity = steps.capacity();
        let trace = SolutionTrace::new(steps);
        assert_eq!(
            trace.checked_nested_retained_bytes(),
            Some(capacity as u128 * core::mem::size_of::<PlacementStep>() as u128)
        );
        assert_eq!(trace.checked_clone_nested_bytes(), Some(0));
    }
}
