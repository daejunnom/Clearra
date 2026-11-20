use crate::product_contract_json_assert::{bool_value, fixture_json, number};

#[test]
fn path_reports_representative_trace() {
    let result = fixture_json("tests/fixtures/product/path_representative.json");
    assert_eq!(result["kind"], "path");
    let summary = &result["summary"];
    assert_eq!(summary["status"], "path-rendered");
    assert_eq!(summary["sample_trace_available"], true);
    assert_eq!(summary["retained_representative_trace"], true);
    assert!(bool_value(summary, "count_complete"));
    assert_eq!(number(summary, "unique_solution_count"), 4.0);
    assert_eq!(number(summary, "trace_steps"), 5.0);
    let retained = number(summary, "retained_trace_count");
    assert!(retained > 0.0 && retained <= number(summary, "total_solution_count"));
    let clears: f64 = (0..5)
        .map(|step| number(summary, &format!("step_{step}_cleared_lines")))
        .sum();
    assert_eq!(clears, 2.0);
}

#[test]
fn percent_reports_total_and_covered_pattern_count() {
    let result = fixture_json("tests/fixtures/product/percent_bag_pattern.json");
    assert_eq!(result["kind"], "percent");
    let summary = &result["summary"];
    assert_eq!(summary["actual_backend"], "wasm-cpu");
    assert_eq!(number(summary, "total_pattern_count"), 1.0);
    assert_eq!(number(summary, "covered_pattern_count"), 1.0);
    assert_eq!(number(summary, "coverage_probability"), 1.0);
    assert_eq!(number(summary, "failed_pattern_count"), 0.0);
    assert_eq!(summary["probability_complete"], true);
    assert_eq!(summary["renormalized"], false);
}

#[test]
fn cover_reports_build_union_probability() {
    let result = fixture_json("tests/fixtures/product/cover_template_basic.json");
    assert_eq!(result["kind"], "build_coverage");
    let summary = &result["summary"];
    assert_eq!(summary["status"], "cover-executed");
    assert_eq!(summary["coverage_scope"], "requested-materialized-ordinals");
    assert!(bool_value(summary, "assignment_exact_cover_complete"));
    assert!(bool_value(summary, "probability_complete"));
    // A one-cell slot has an abstract assignment but no buildable tetromino
    // witness. The actual union must therefore remain empty.
    assert_eq!(number(summary, "assignment_count"), 1.0);
    assert_eq!(number(summary, "union_covered_patterns"), 0.0);
    assert_eq!(number(summary, "build_coverage_probability"), 0.0);
}
