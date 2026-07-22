#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbabilityError {
    NotFinite,
    OutOfRange,
    TotalWeightExceedsOne,
    MissingWeight,
}
