#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScoreEvaluationScope {
    #[default]
    RetainedTraceSample,
    CoveredPatternsConditional,
    FullPatternUniverseExpected,
}
