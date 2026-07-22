use super::*;

#[test]
fn build_query_is_a_build_coverage_bridge_not_raw_template_parser() {
    let query = BuildQuery::coverage_bridge(
        BuildTemplateBridge::new("template-a", BoardSize::standard_10x20(), 3)
            .with_label("Template A"),
        64,
        BuildProblemLimits::new(100, 64),
    );

    assert_eq!(query.template().id(), "template-a");
    assert_eq!(query.template().label(), Some("Template A"));
    assert_eq!(query.template().slot_count(), 3);
    assert_eq!(query.pattern_count(), 64);
    assert_eq!(query.selected_pattern_id(), None);
    assert_eq!(query.limits().max_assignments(), 100);
}

#[test]
fn build_query_can_narrow_execution_to_one_materialized_pattern() {
    let query = BuildQuery::coverage_bridge(
        BuildTemplateBridge::new("template-a", BoardSize::standard_10x20(), 3),
        64,
        BuildProblemLimits::new(100, 64),
    )
    .with_selected_pattern_id(7);

    assert_eq!(query.pattern_count(), 64);
    assert_eq!(query.selected_pattern_id(), Some(7));
}
