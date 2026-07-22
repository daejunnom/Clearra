use super::post_pc_evaluation_summary::PostPcEvaluationSummary;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostPcEvaluation {
    Evaluated(PostPcEvaluationSummary),
    Unsupported { reason: &'static str },
}

impl PostPcEvaluation {
    pub fn solution_found(&self) -> bool {
        matches!(self, Self::Evaluated(summary) if summary.solution_found())
    }
}
impl PostPcEvaluation {
    pub fn status(&self) -> &'static str {
        match self {
            Self::Evaluated(_) => "evaluated",
            Self::Unsupported { .. } => "unsupported",
        }
    }
}
impl PostPcEvaluation {
    pub fn unsupported_reason(&self) -> Option<&'static str> {
        match self {
            Self::Evaluated(_) => None,
            Self::Unsupported { reason } => Some(reason),
        }
    }
}
impl PostPcEvaluation {
    pub fn summary(&self) -> Option<&PostPcEvaluationSummary> {
        match self {
            Self::Evaluated(summary) => Some(summary),
            Self::Unsupported { .. } => None,
        }
    }
}
