use clearra_core_domain::{board::board_size::BoardSize, piece::piece_kind::PieceKind};
use clearra_problem::{BuildProblemLimits, BuildTemplateBridge};

use super::*;

#[test]
fn cover_service_runs_build_coverage_through_c_buildup_rows() {
    let bridge_query = BuildQuery::coverage_bridge(
        BuildTemplateBridge::new("template-a", BoardSize::new(10, 4).expect("board"), 1),
        4,
        BuildProblemLimits::new(12, 4),
    );
    let problem = clearra_problem::ProblemCompiler::compile_build(&bridge_query).expect("problem");
    let coverage_query = build_coverage_query_from_bridge(&bridge_query);

    let result = CoverService::execute_build_coverage(&problem, &coverage_query).expect("cover");
    let fields = result.summary_fields();

    assert!(fields.contains(&(
        "execution_scope".to_owned(),
        "m21-build-coverage-product-path".to_owned()
    )));
    assert!(fields.contains(&(
        "coverage_row_source".to_owned(),
        "C BuildUp coverage row".to_owned()
    )));
    assert!(fields.contains(&(
        "union_probability_reducer".to_owned(),
        "BuildCoverageResult uses union probability".to_owned()
    )));
    assert_eq!(result.field("assignment_count"), Some("1"));
    assert_eq!(
        result.field("c_buildup_coverage_row_generated"),
        Some(expected_c_buildup_coverage_row_generated())
    );
    assert_eq!(
        result.field("coverage_row_identity_validated"),
        Some("true")
    );
    assert_eq!(
        result.field("slot_domain_policy"),
        Some("bridge-repeated-standard-pieces")
    );
    assert_eq!(
        result.field("build_coverage_probability"),
        Some(expected_build_coverage_probability())
    );
    assert_eq!(
        result.field("cover_reports_union_probability"),
        Some("true")
    );
    assert_eq!(
        result.field("cover_reports_c_coverage_row_count"),
        Some("true")
    );
    assert_eq!(
        result.field("slot_assignment_count_is_not_success_probability"),
        Some("true")
    );
    assert_eq!(
        result.field("success_probability_source"),
        Some("UnionProbability")
    );
}

fn expected_build_coverage_probability() -> &'static str {
    "0.0"
}

fn expected_c_buildup_coverage_row_generated() -> &'static str {
    if cfg!(feature = "native-c-core") {
        "false"
    } else {
        "false"
    }
}

#[test]
fn bridge_query_synthesizes_slot_domains_without_raw_template_parsing() {
    let bridge_query = BuildQuery::coverage_bridge(
        BuildTemplateBridge::new("template-a", BoardSize::new(10, 4).expect("board"), 2),
        8,
        BuildProblemLimits::new(12, 8),
    );

    let query = build_coverage_query_from_bridge(&bridge_query);

    assert_eq!(query.template().slots().len(), 2);
    assert_eq!(query.domains().len(), 2);
    assert_eq!(query.domains()[0].pieces(), &[PieceKind::I]);
    assert_eq!(query.domains()[1].pieces(), &[PieceKind::O]);
}
