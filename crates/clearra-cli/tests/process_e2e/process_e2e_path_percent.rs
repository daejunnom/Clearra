use super::*;

#[test]
fn process_e2e_m26_percent_and_path_report_product_contract() {
    let percent = clearra()
        .args([
            "--verbose",
            "percent",
            "--queue",
            "IOT",
            "--fixed",
            "--min-len",
            "3",
        ])
        .output()
        .expect("clearra-cli process runs");
    assert!(percent.status.success());
    assert!(percent.stderr.is_empty());
    let percent_stdout = String::from_utf8(percent.stdout).expect("percent stdout utf8");
    assert!(percent_stdout.contains("kind: percent"));
    assert!(percent_stdout.contains("total_pattern_count: 1"));
    assert!(percent_stdout.contains(&format!(
        "covered_pattern_count: {}",
        expected_percent_covered_pattern_count()
    )));
    assert!(percent_stdout.contains(&format!("probability: {}", expected_percent_probability())));
    assert!(percent_stdout.contains(&format!(
        "c_buildup_coverage_row_count: {}",
        expected_percent_covered_pattern_count()
    )));

    let path = clearra()
        .args([
            "--verbose",
            "path",
            "--lines",
            "2",
            "--queue",
            "IIOOO",
            "--fixed",
            "--no-hold",
        ])
        .output()
        .expect("clearra-cli process runs");
    assert!(path.status.success());
    assert!(path.stderr.is_empty());
    let path_stdout = String::from_utf8(path.stdout).expect("path stdout utf8");
    assert!(path_stdout.contains("kind: path"));
    assert!(path_stdout.contains("retained_representative_trace: true"));
    assert!(path_stdout.contains(&format!(
        "total_solution_count: {}",
        expected_path_solution_count()
    )));
    assert!(path_stdout.contains(&format!(
        "retained_trace_count: {}",
        expected_path_retained_trace_count()
    )));
    assert!(path_stdout.contains("path_distinguishes_retained_trace_from_total_count: true"));
}
