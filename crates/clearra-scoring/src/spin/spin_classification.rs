use super::{spin_accuracy::SpinAccuracy, spin_result::SpinResult};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClassificationConfidence(f32);

impl ClassificationConfidence {
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }
}
impl ClassificationConfidence {
    pub fn exact() -> Self {
        Self(1.0)
    }
}
impl ClassificationConfidence {
    pub fn estimated() -> Self {
        Self(0.5)
    }
}
impl ClassificationConfidence {
    pub fn get(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpinClassification {
    result: SpinResult,
    confidence: ClassificationConfidence,
}

impl SpinClassification {
    pub fn new(result: SpinResult, confidence: ClassificationConfidence) -> Self {
        Self { result, confidence }
    }
}
impl SpinClassification {
    pub fn none(piece: char, cleared_lines: u8, accuracy: SpinAccuracy) -> Self {
        Self::new(
            SpinResult::none(piece, cleared_lines, accuracy),
            ClassificationConfidence::new(0.0),
        )
    }
}
impl SpinClassification {
    pub fn result(self) -> SpinResult {
        self.result
    }
}
impl SpinClassification {
    pub fn confidence(self) -> ClassificationConfidence {
        self.confidence
    }
}
