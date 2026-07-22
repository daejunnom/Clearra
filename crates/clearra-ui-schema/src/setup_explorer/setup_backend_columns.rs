use super::setup_result_column_schema::{
    column, SetupResultColumnSchema, SetupResultColumnSource, SetupResultColumnType,
};

pub(crate) fn setup_backend_columns() -> Vec<SetupResultColumnSchema> {
    vec![
        column(
            "total_solution_count",
            "Solutions",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "count_complete",
            "Count complete",
            SetupResultColumnType::Boolean,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "solution_trace_mode",
            "Trace mode",
            SetupResultColumnType::Text,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "backend_selection_reason",
            "Backend reason",
            SetupResultColumnType::Text,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "backend_fallback_reason",
            "Fallback reason",
            SetupResultColumnType::Text,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "gpu_status",
            "GPU status",
            SetupResultColumnType::Text,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "gpu_trust_state",
            "GPU trust",
            SetupResultColumnType::Text,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "hybrid_backpressure_active",
            "Backpressure",
            SetupResultColumnType::Boolean,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "hybrid_memory_pressure_level",
            "Memory pressure",
            SetupResultColumnType::Text,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "state_count",
            "States",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::PostPcEvaluation,
        ),
        column(
            "multiplicity_count",
            "Multiplicity",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::PostPcEvaluation,
        ),
    ]
}
