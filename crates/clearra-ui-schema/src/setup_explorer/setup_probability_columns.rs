use super::setup_result_column_schema::{
    column, SetupResultColumnSchema, SetupResultColumnSource, SetupResultColumnType,
};

pub(crate) fn setup_probability_columns() -> Vec<SetupResultColumnSchema> {
    vec![
        column(
            "family",
            "Family",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupFamily,
        ),
        column(
            "shape_family_id",
            "Shape family",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "tiling_variant",
            "Tiling variant",
            SetupResultColumnType::Text,
            SetupResultColumnSource::TilingVariant,
        ),
        column(
            "tiling_variant_count",
            "Tiling variants",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "build_variant",
            "Build variant",
            SetupResultColumnType::Text,
            SetupResultColumnSource::BuildVariant,
        ),
        column(
            "packing_candidate_count",
            "Packing candidates",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::BuildVariantMetrics,
        ),
        column(
            "build_variant_count",
            "Build variants",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::BuildVariant,
        ),
        column(
            "coverage_probability",
            "Coverage",
            SetupResultColumnType::Probability,
            SetupResultColumnSource::ScoreAggregation,
        ),
        column(
            "covered_pattern_count",
            "Covered patterns",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "post_pc_probability",
            "Post-PC",
            SetupResultColumnType::Probability,
            SetupResultColumnSource::ScoreAggregation,
        ),
        column(
            "post_pc_solution_count",
            "Post-PC solutions",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::SetupRawMetrics,
        ),
    ]
}
