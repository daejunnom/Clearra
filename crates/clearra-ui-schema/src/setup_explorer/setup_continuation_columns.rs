use super::setup_result_column_schema::{
    column, SetupResultColumnSchema, SetupResultColumnSource, SetupResultColumnType,
};

pub(crate) fn setup_continuation_columns() -> Vec<SetupResultColumnSchema> {
    vec![
        column(
            "continuation_available",
            "Continue",
            SetupResultColumnType::Boolean,
            SetupResultColumnSource::Continuation,
        ),
        column(
            "continuation_available_complete",
            "Continue complete",
            SetupResultColumnType::Boolean,
            SetupResultColumnSource::Continuation,
        ),
    ]
}
