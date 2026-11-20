//! Check exact public behavior at the Rust product executable boundary.
//! The independent C oracle is exercised in App and Core tests.

use std::{path::Path, process::Command};

use clearra_app::ProductBuildIdentity;
use clearra_pc_graph::request::WorkerPolicy;
use serde_json::Value;

fn run_json(args: &[&str]) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO_BIN_EXE_clearra"))
        .current_dir(root)
        .args(["--format", "json"])
        .args(args)
        .output()
        .expect("launch the compiled Rust product");
    assert!(
        output.status.success(),
        "{args:?}: {}, stdout={}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).expect("product CLI JSON");
    let build = ProductBuildIdentity::current();
    assert_eq!(
        result["runtime_identity"]["source_commit"],
        build.source_commit()
    );
    assert_eq!(
        result["runtime_identity"]["engine_build_id"],
        build.engine_build_id()
    );
    result
}

fn assert_exact_cpu_pc(result: &Value, expected_kind: &str, solution_count: u64) {
    assert_eq!(result["kind"], expected_kind);
    let summary = &result["summary"];
    assert_eq!(summary["actual_backend"], "wasm-cpu", "{summary}");
    assert_ne!(summary["native_c_core_executed"], true);
    assert_ne!(summary["native_c_core_linked"], true);
    assert_eq!(summary["solution_found"], true);
    assert_eq!(summary["count_complete"], true);
    assert_eq!(summary["probability_complete"], true);
    assert_eq!(summary["covered_pattern_count"], 1);
    assert_eq!(summary["coverage_probability"].as_f64(), Some(1.0));
    assert_eq!(summary["normalized_unique_solution_count"], solution_count);
}

fn assert_reported_true(summary: &Value, field: &str) {
    let value = &summary[field];
    // Continue can relay a core field directly rather than projecting the PC
    // summary DTO. Both forms carry the same typed completion fact.
    assert!(
        value.as_bool() == Some(true) || value.as_str() == Some("true"),
        "{field}: {value}"
    );
}

#[test]
fn fixed_queue_continuation_matches_the_exact_direct_suffix() {
    let opening = |queue| {
        run_json(&[
            "pc",
            "--lines",
            "2",
            "--queue",
            queue,
            "--fixed",
            "--no-hold",
            "--backend",
            "cpu",
            "--workers",
            "1",
        ])
    };
    let result = opening("IIOOOIIOOO");
    // Two horizontal Is occupy one of four aligned pairs of columns. Three
    // Os tile the complement. Build variants are not extra solution fields.
    assert_exact_cpu_pc(&result, "pc", 4);
    let summary = &result["summary"];
    assert_eq!(summary["next_pc_available"], true);
    assert_eq!(summary["continuation_token_version"], "pc2");
    let token = summary["continuation_token"].as_str().expect("pc2 token");
    assert!(token.starts_with("pc2:"));

    let direct = opening("IIOOO");
    assert_exact_cpu_pc(&direct, "pc", 4);
    let expected_hash = direct["summary"]["normalized_solution_set_hash"]
        .as_str()
        .filter(|hash| hash.starts_with("cts1:"))
        .expect("canonical solution-family hash");
    let continued = run_json(&["continue", token]);
    assert_eq!(continued["kind"], "continue");
    assert_reported_true(&continued["summary"], "solution_found");
    assert_reported_true(&continued["summary"], "count_complete");
    assert_eq!(
        continued["summary"]["normalized_solution_set_hash"],
        expected_hash
    );
}

#[test]
fn patterned_pc_uses_two_workers_without_changing_the_exact_result() {
    let hardware_limit = WorkerPolicy::hardware_worker_limit();
    let search = |workers| {
        let mut args = vec![
            "pc",
            "--lines",
            "4",
            "--height",
            "4",
            "--board-mask",
            "0x3c0f03c0f",
            "--pieces",
            "6",
            "--patterns",
            "P7",
            "--hold",
            "empty",
            "--rule",
            "srs-plus",
            "--objective",
            "unique",
            "--count",
            "unique",
            "--backend",
            "cpu",
            "--workers",
            workers,
            "--no-tablebase",
            "--no-legal-board",
            "--no-conditioned-reachability",
            "--no-build-dependency-dag",
        ];
        if workers == "2" && WorkerPolicy::default_worker_limit_for_hardware(hardware_limit) < 2 {
            args.push("--use-all-cpu-threads");
        }
        let result = run_json(&args);
        assert_eq!(result["kind"], "pc-scenario");
        let summary = &result["summary"];
        assert_eq!(summary["actual_backend"], "wasm-cpu", "{summary}");
        assert_eq!(summary["solution_found"], true);
        assert_eq!(summary["count_complete"], true);
        assert_eq!(summary["probability_complete"], true);
        assert_eq!(summary["normalized_unique_solution_count"], 246);
        assert_eq!(
            summary["normalized_solution_set_hash"],
            "cts1:cb0b19c391d5003e"
        );
        result
    };

    let serial = search("1");
    let serial = &serial["summary"];
    assert_eq!(serial["workers_used"], 1);
    assert_eq!(serial["materialized_pattern_count"], 5040);
    assert_eq!(serial["covered_pattern_count"], 5040);
    assert_eq!(serial["coverage_probability"].as_f64(), Some(1.0));

    // Keep the fixture assertions on single-processor hosts without treating
    // a serial run as successful parallel coverage.
    if hardware_limit < 2 {
        eprintln!("two-worker PC comparison unavailable on this host");
        return;
    }

    let parallel = search("2");
    let parallel = &parallel["summary"];
    assert_eq!(parallel["workers_requested"], "2");
    assert_eq!(parallel["workers_used"], 2, "{parallel}");
    // Active counts only workers that processed at least one candidate;
    // the pool width alone would accept a serial execution.
    assert_eq!(parallel["parallel_active_workers"], "2", "{parallel}");
    for key in [
        "normalized_solution_set_hash",
        "materialized_pattern_count",
        "covered_pattern_count",
        "coverage_probability",
    ] {
        assert_eq!(parallel[key], serial[key], "{key}");
    }
}

#[test]
fn verified_scenario_preserves_replay_without_a_c_link() {
    let result = run_json(&[
        "pc-scenario",
        "--fixture",
        "tests/fixtures/pc/scenario_simple_4l.json",
        "--verify-expected",
        "--backend",
        "cpu",
        "--workers",
        "1",
    ]);
    assert_exact_cpu_pc(&result, "pc-scenario", 1);
    let summary = &result["summary"];
    assert_eq!(summary["expected_checked"], true);
    assert_eq!(summary["expected_match"], true);
    assert_eq!(summary["scenario_replay_token_version"], "sr2");
    assert_eq!(summary["retained_trace_count"], 1);
}

#[test]
fn legacy_percent_uses_the_same_pattern_denominator_and_exact_row_union() {
    // The Percent input has one horizontal-I target. Fixed IOT covers it,
    // while fixed OJT cannot. Do not mistake a missing row for a missing
    // pattern in the original denominator.
    for (queue, covered) in [("IOT", 1), ("OJT", 0)] {
        let result = run_json(&["percent", "--queue", queue, "--fixed", "--min-len", "3"]);
        assert_eq!(result["kind"], "percent");
        let summary = &result["summary"];
        assert_eq!(summary["actual_backend"], "wasm-cpu", "{summary}");
        assert_eq!(summary["total_pattern_count"], 1);
        assert_eq!(summary["covered_pattern_count"], covered);
        assert_eq!(summary["probability_complete"], true);
        assert_eq!(
            summary["coverage_probability"].as_f64(),
            Some(f64::from(covered))
        );
        assert_eq!(summary["renormalized"], false);
        assert_eq!(summary["failed_pattern_count"], 1 - covered);
    }
}

#[test]
fn limited_rust_search_does_not_claim_complete_count_or_probability() {
    let result = run_json(&[
        "pc",
        "--lines",
        "4",
        "--queue",
        "IIOOOIIOOO",
        "--fixed",
        "--no-hold",
        "--backend",
        "cpu",
        "--workers",
        "1",
        "--max-patterns",
        "1",
        "--max-candidates",
        "1",
    ]);
    let summary = &result["summary"];
    assert_eq!(summary["actual_backend"], "wasm-cpu", "{summary}");
    assert_eq!(summary["count_complete"], false);
    assert_eq!(summary["probability_complete"], false);
    assert_eq!(
        summary["resource_truncation_reason"],
        "candidate_budget_exceeded"
    );
}
