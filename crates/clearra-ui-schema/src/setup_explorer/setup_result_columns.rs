use super::{
    scenario_result_columns::scenario_result_columns_impl,
    setup_backend_columns::setup_backend_columns, setup_column_group::append_column_groups,
    setup_continuation_columns::setup_continuation_columns,
    setup_diagnostic_columns::setup_diagnostic_columns,
    setup_probability_columns::setup_probability_columns,
    setup_result_column_schema::SetupResultColumnSchema, setup_score_columns::setup_score_columns,
};

pub(crate) fn setup_result_columns() -> Vec<SetupResultColumnSchema> {
    append_column_groups([
        setup_probability_columns(),
        setup_backend_columns(),
        setup_score_columns(),
        setup_diagnostic_columns(),
        setup_continuation_columns(),
    ])
}

pub(crate) fn scenario_result_columns() -> Vec<SetupResultColumnSchema> {
    scenario_result_columns_impl()
}
