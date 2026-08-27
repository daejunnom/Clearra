use std::fmt;

use crate::model::is_json_number;

const COMMENT_SCHEMA: &str = "clearra.solution-annotation.v1";
const COMMENT_AUTHORITY: &str = "annotation-only";
const MAX_METRIC_LENGTH: usize = 128;
const MAX_LAYOUT_BYTES: usize = 4_095;

/// Optional per-solution presentation values.
///
/// These values are deliberately separate from the canonical solution key and
/// set hash. A codec adapter may place the rendered layout in a document
/// comment, but the comment never establishes solution or score authority.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SolutionArtifactAnnotation {
    pc_probability: Option<String>,
    average_score: Option<String>,
}

impl SolutionArtifactAnnotation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pc_probability(
        mut self,
        value: impl Into<String>,
    ) -> Result<Self, SolutionCommentLayoutError> {
        let value = validate_metric(value.into(), MetricKind::Probability)?;
        self.pc_probability = Some(value);
        Ok(self)
    }

    pub fn with_average_score(
        mut self,
        value: impl Into<String>,
    ) -> Result<Self, SolutionCommentLayoutError> {
        let value = validate_metric(value.into(), MetricKind::AverageScore)?;
        self.average_score = Some(value);
        Ok(self)
    }

    pub fn pc_probability(&self) -> Option<&str> {
        self.pc_probability.as_deref()
    }

    pub fn average_score(&self) -> Option<&str> {
        self.average_score.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.pc_probability.is_none() && self.average_score.is_none()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SolutionCommentLayout;

impl SolutionCommentLayout {
    pub const fn schema() -> &'static str {
        COMMENT_SCHEMA
    }

    pub const fn authority() -> &'static str {
        COMMENT_AUTHORITY
    }

    pub fn render(annotation: &SolutionArtifactAnnotation) -> Option<String> {
        if annotation.is_empty() {
            return None;
        }
        let mut lines = vec![
            COMMENT_SCHEMA.to_owned(),
            format!("authority={COMMENT_AUTHORITY}"),
        ];
        if let Some(probability) = annotation.pc_probability() {
            lines.push(format!("pc_probability={probability}"));
        }
        if let Some(score) = annotation.average_score() {
            lines.push(format!("average_score={score}"));
        }
        let rendered = lines.join("\n");
        debug_assert!(rendered.len() <= MAX_LAYOUT_BYTES);
        Some(rendered)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MetricKind {
    Probability,
    AverageScore,
}

fn validate_metric(value: String, kind: MetricKind) -> Result<String, SolutionCommentLayoutError> {
    if value.is_empty() || value.len() > MAX_METRIC_LENGTH || !is_json_number(&value) {
        return Err(SolutionCommentLayoutError::InvalidMetric);
    }
    let parsed = value
        .parse::<f64>()
        .map_err(|_| SolutionCommentLayoutError::InvalidMetric)?;
    if !parsed.is_finite()
        || matches!(kind, MetricKind::Probability) && !(0.0..=1.0).contains(&parsed)
    {
        return Err(SolutionCommentLayoutError::InvalidMetric);
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionCommentLayoutError {
    InvalidMetric,
}

impl fmt::Display for SolutionCommentLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("solution annotation metric is invalid")
    }
}

impl std::error::Error for SolutionCommentLayoutError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_has_fixed_order_and_marks_comments_non_authoritative() {
        let annotation = SolutionArtifactAnnotation::new()
            .with_average_score("1250.5")
            .expect("score")
            .with_pc_probability("0.25")
            .expect("probability");

        assert_eq!(
            SolutionCommentLayout::render(&annotation).as_deref(),
            Some(concat!(
                "clearra.solution-annotation.v1\n",
                "authority=annotation-only\n",
                "pc_probability=0.25\n",
                "average_score=1250.5"
            ))
        );
    }

    #[test]
    fn invalid_or_out_of_range_metrics_fail_closed() {
        for value in ["", "NaN", "inf", "1.01", "value\nidentity=fake"] {
            assert!(SolutionArtifactAnnotation::new()
                .with_pc_probability(value)
                .is_err());
        }
        assert!(SolutionArtifactAnnotation::new()
            .with_average_score("NaN")
            .is_err());
    }
}
