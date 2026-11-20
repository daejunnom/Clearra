use super::*;

#[test]
fn process_e2e_m26_percent_and_path_report_product_contract() {
    let percent = run_json(&["percent", "--queue", "IOT", "--fixed", "--min-len", "3"]);
    assert_eq!(percent["kind"], "percent");
    let summary = &percent["summary"];
    assert_rust_cpu(summary);
    assert_eq!(summary["total_pattern_count"], 1);
    assert_eq!(summary["covered_pattern_count"], 1);
    assert_eq!(summary["coverage_probability"].as_f64(), Some(1.0));
    assert_eq!(summary["probability_complete"], true);
    assert_eq!(summary["renormalized"], false);

    let path = run_json(&[
        "path",
        "--lines",
        "2",
        "--queue",
        "IIOOO",
        "--fixed",
        "--no-hold",
        "--backend",
        "cpu",
        "--workers",
        "1",
    ]);
    assert_eq!(path["kind"], "path");
    let summary = &path["summary"];
    assert_eq!(summary["status"], "path-rendered");
    assert_eq!(summary["retained_representative_trace"], true);
    assert_eq!(summary["total_solution_count"], "4");
    assert_eq!(summary["unique_solution_count"], "4");
    assert_eq!(summary["count_complete"], "true");
    assert_eq!(summary["retained_trace_count"], "1");
    assert_eq!(summary["trace_steps"], 5);
    assert_eq!(
        summary["path_distinguishes_retained_trace_from_total_count"],
        true
    );
}
