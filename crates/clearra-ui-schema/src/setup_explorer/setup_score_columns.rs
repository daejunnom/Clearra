use super::setup_result_column_schema::{
    column, SetupResultColumnSchema, SetupResultColumnSource, SetupResultColumnType,
};

pub(crate) fn setup_score_columns() -> Vec<SetupResultColumnSchema> {
    vec![
        column(
            "score_expectation",
            "Score EV",
            SetupResultColumnType::Float,
            SetupResultColumnSource::ScoreAggregation,
        ),
        column(
            "attack_expectation",
            "Attack EV",
            SetupResultColumnType::Float,
            SetupResultColumnSource::ScoreAggregation,
        ),
        column(
            "score_evaluation_trace_count",
            "Score traces",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::ScoreAggregation,
        ),
        column(
            "score_evaluation_complete",
            "Score complete",
            SetupResultColumnType::Boolean,
            SetupResultColumnSource::ScoreAggregation,
        ),
        column(
            "score_evaluation_basis",
            "Score basis",
            SetupResultColumnType::Text,
            SetupResultColumnSource::ScoreAggregation,
        ),
        column(
            "score_basis",
            "Raw score basis",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "score_accuracy_level",
            "Score accuracy",
            SetupResultColumnType::Text,
            SetupResultColumnSource::ScoreAggregation,
        ),
    ]
}
